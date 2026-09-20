#define _DARWIN_C_SOURCE
#ifdef TRACE_HOOKS_BASELINE
#include "idapi_v17.h"
#define SET(v,h) api->set_trace_hook(v,h)
#else
#include "idapi_v18.h"
#define SET(v,h) fixture_set_trace_hook(v,h)
#endif
#include "storage_v1.h"
#include "namespace-support.h"
#include <pthread.h>
#include <stdlib.h>

static IdApi *api;
static Oop parent,child,object;
static TraceHook expected;
static const char *mode;
static void first(Oop self,MarkFn mark) { (void)self;(void)mark;assert(!"observation invoked hook"); }
static void second(Oop self,MarkFn mark) { (void)self;(void)mark;assert(!"observation invoked hook"); }
static void *wrong_thread(void *context) { (void)context;SET(NULL,first);return NULL; }
static int observe(void *context,const StorageViewV1 *view) {
    (void)context;
    if(!strcmp(mode,"observation")) SET(NULL,first);
    unsigned found=0;
    for(size_t i=0;i<view->allocation_count;i++) if(view->allocations[i].object==object) {
        assert(view->allocations[i].trace_hook==expected);found++;
    }
    assert(found==1);return 0;
}
static Oop answer(Oop self,const Oop *args,size_t count) { (void)args;(void)count;return self; }
#ifndef TRACE_HOOKS_BASELINE
static unsigned reads;
static int read_entry(const void *context,void *vt,TraceHookEntryV1 *output) {
    assert(context==&reads);reads++;
    if(vt==(void *)(uintptr_t)1) { *output=(TraceHookEntryV1){(void *)(uintptr_t)2,{NULL}};return 1; }
    if(vt==(void *)(uintptr_t)2) { *output=(TraceHookEntryV1){NULL,{first}};return 1; }
    return 0;
}
static void factory_probe(void) {
    TraceHooksV1 *hooks=fixture_trace_hooks();Dl_info owner;
    assert(dladdr((void *)hooks->set,&owner) && strstr(owner.dli_fname,"libbirnam_trace_hooks.dylib"));
    void *image=dlopen(owner.dli_fname,RTLD_NOW|RTLD_LOCAL);assert(image);
    TraceHooksFactoryProviderV1 provide=(TraceHooksFactoryProviderV1)dlsym(image,"birnam_trace_hooks_factory_v1");assert(provide);
    const TraceHooksFactoryV1 *factory=provide();assert(factory && factory->version==1 && factory->byte_size==sizeof(*factory));
    assert(factory->slot(first).hook==first);
    TraceHookSourceV1 source={&reads,2,read_entry};
    assert(factory->resolve(&source,(void *)(uintptr_t)1)==first && reads==2);
    reads=0;source.budget=1;assert(!factory->resolve(&source,(void *)(uintptr_t)1) && reads==1);
    reads=0;source.budget=3;assert(!factory->resolve(&source,(void *)(uintptr_t)3) && reads==1);
    assert(!factory->create(NULL));
}
#endif
int main(int argc,char **argv) {
    assert(argc==3);mode=argv[2];
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);
#ifndef TRACE_HOOKS_BASELINE
    factory_probe();
#endif
    fixture_set_auto(0);parent=api->proto(api->object_vt);child=api->proto(parent);object=api->alloc(child,0);
    if(!strcmp(mode,"owner")) { pthread_t thread;assert(!pthread_create(&thread,NULL,wrong_thread,NULL));assert(!pthread_join(thread,NULL));abort(); }
    if(!strcmp(mode,"invalid")) { SET(NULL,first);abort(); }
    StorageObserveV1 inspect=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(inspect);
    StorageRequestV1 request={1,sizeof(request),NULL,observe};
    assert(inspect(&request)==0);
    SET(parent,first);expected=first;assert(inspect(&request)==0);
    assert(*(TraceHook *)((char *)parent+40)==first && !*(TraceHook *)((char *)child+40));
    SET(parent,second);expected=second;assert(inspect(&request)==0);
    api->method(parent,api->intern("traceHookMirrorProbe"),answer);
    fixture_set_layout(parent,NULL,0,0);assert(*(TraceHook *)((char *)parent+40)==second);
    set_parent(api,child,api->object_vt);expected=NULL;assert(inspect(&request)==0);
    set_parent(api,child,parent);expected=second;assert(inspect(&request)==0);
    SET(child,first);expected=first;assert(inspect(&request)==0);
    puts("unset, inherited, replacement, raw mirror, live parent and nearest override agree without invoking hooks");
}
