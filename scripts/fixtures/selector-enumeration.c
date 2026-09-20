/* Run against each revision's own independent header and prerequisite helpers. */
#ifdef FIXTURE_IDAPI_V12
#include "idapi_v12.h"
#else
#include "idapi_v16.h"
#endif
#include "namespace-support.h"
#include "object-model-support.h"
#include "trace-support.h"
#include "../../abi/fixtures/idapi-v16/storage_v1.h"
#include <pthread.h>
static IdApi *api;
static Oop roots[70],candidate;
static size_t checked_index,root_calls;
static Oop at(Oop vtable,size_t index) {
#ifdef FIXTURE_IDAPI_V12
    return api->vtable_selector_at(vtable,index);
#else
    return vtable_selector_at_v2(fixture_vtable_enumeration(),vtable,index);
#endif
}
static Oop answer(Oop self,const Oop *args,size_t count) { (void)args;(void)count;return self; }
static Oop forbidden(Oop self,const Oop *args,size_t count) { (void)self;(void)args;(void)count;assert(!"enumeration invoked lookup");return NULL; }
static void mark_roots(MarkFn mark) { for(size_t i=0;i<70;i++)mark(roots[i]); }
static void check(void) { assert(at(candidate,checked_index)==roots[2]); }
static void *worker(void *context) { (void)context;check();return NULL; }
static void probe_roots(MarkFn mark) { (void)mark;root_calls++;check(); }
static int inspect(void *context,const StorageViewV1 *view) { (void)context;(void)view;check();return 1; }
__attribute__((noinline)) static void collect_now(void) {
    uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));api->gc_collect();
}
int main(int argc,char **argv) {
    assert(argc==3);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);fixture_prepare_namespace(floor,argv[1]);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);fixture_set_auto(0);
    fixture_mount_object_model(api,argv[1],fixture_root_registry());fixture_mount_trace(api,argv[1],fixture_root_registry());
    fixture_register_root(mark_roots);
    roots[0]=api->proto(api->object_vt);roots[1]=api->proto(roots[0]);
    for(size_t i=0;i<64;i++) { char name[48];snprintf(name,sizeof(name),"selector-enumeration-%zu",i);roots[i+2]=api->intern(name); }
    candidate=roots[1];api->method(candidate,roots[2],answer);
    const char *mode=argv[2];
    if(!strcmp(mode,"owner")) { pthread_t thread;assert(!pthread_create(&thread,NULL,worker,NULL));assert(!pthread_join(thread,NULL));return 0; }
    if(!strcmp(mode,"observation")) {
        StorageObserveV1 observe=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(observe);
        StorageRequestV1 request={1,sizeof(request),NULL,inspect};observe(&request);assert(!"missing observation callback");
    }
    if(!strcmp(mode,"collection")) { fixture_register_root(probe_roots);collect_now();assert(root_calls);puts("selector observation during root collection passed");return 0; }
    if(!strncmp(mode,"invalid-",8)) {
        if(strstr(mode,"null"))candidate=NULL;
        else if(strstr(mode,"instance"))candidate=api->alloc(roots[0],0);
        else if(strstr(mode,"stale")) { candidate=api->proto(api->object_vt);fixture_set_precise(1);collect_now(); }
        else candidate=(Oop)(uintptr_t)0x40;
        if(strstr(mode,"max"))checked_index=SIZE_MAX;
        (void)at(candidate,checked_index);assert(!"invalid vtable was accepted");
    }
    assert(!strcmp(mode,"ordered"));
#ifndef FIXTURE_IDAPI_V12
    Dl_info owner;assert(dladdr((void *)fixture_vtable_enumeration()->selector_at,&owner));
    assert(strstr(owner.dli_fname,"libbirnam_object_model.dylib"));
#endif
    api->method(roots[0],roots[65],answer);
    for(size_t i=1;i<64;i++)api->method(candidate,roots[i+2],answer);
    for(size_t i=0;i<64;i++)assert(at(candidate,i)==roots[i+2]);
    assert(!at(candidate,64) && !at(candidate,SIZE_MAX));
    for(size_t i=0;i<64;i++)api->method(candidate,roots[i+2],forbidden);
    for(size_t i=0;i<64;i++)assert(at(candidate,i)==roots[i+2]);
    Oop remove=api->intern("removeMethod:");
    for(size_t i=0;i<64;i+=2)api->send(candidate,remove,&roots[i+2],1);
    for(size_t i=0;i<32;i++)assert(at(candidate,i)==roots[i*2+3]);
    for(size_t i=0;i<64;i+=2)api->method(candidate,roots[i+2],answer);
    for(size_t i=0;i<32;i++)assert(at(candidate,i+32)==roots[i*2+2]);
    fixture_set_precise(1);collect_now();
    for(size_t i=0;i<32;i++)assert(at(candidate,i)==roots[i*2+3]);
    /* Raw reflection does not send to the behavior's replaceable lookup. */
    Oop lookup=api->intern("lookup");
    api->method(api->vtable_vt,lookup,forbidden);
    for(size_t i=0;i<32;i++)assert(at(candidate,i+32)==roots[i*2+2]);
    assert(!at(candidate,64));
    puts("selector enumeration: ordered growth, overwrite, removal, append, bounds, GC and raw observation passed");
}
