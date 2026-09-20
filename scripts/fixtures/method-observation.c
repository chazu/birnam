/* Compile against each revision's own independent floor header/helpers. */
#ifdef FIXTURE_IDAPI_V13
#include "idapi_v13.h"
#else
#include "idapi_v19.h"
#endif
#include "namespace-support.h"
#include "object-model-support.h"
#include "trace-support.h"
#include "../../abi/fixtures/idapi-v19/storage_v1.h"
#include <pthread.h>
static IdApi *api;
static Oop roots[5],target,expected;
static size_t calls;
static void mark_roots(MarkFn mark) { for(size_t i=0;i<5;i++)mark(roots[i]); }
static Oop first(Oop self,const Oop *args,size_t count) { (void)self;(void)args;(void)count;return api->true_obj; }
static Oop second(Oop self,const Oop *args,size_t count) { (void)self;(void)args;(void)count;return api->false_obj; }
static void hook(Oop vtable,Oop selector) {
    assert(vtable==target && selector==roots[2]);calls++;
    Method method=(Method)api->send(vtable,api->intern("lookup"),&selector,1);
    assert(method && method(vtable,NULL,0)==expected);
    assert(vtable_selector_at_v2(fixture_vtable_enumeration(),vtable,0)==selector);
    printf("before mutation %zu %s\n",calls,expected==api->true_obj?"first":"second");
}
static void alternate(Oop vtable,Oop selector) { puts("alternate hook");hook(vtable,selector); }
static void replace(void (*callback)(Oop,Oop)) {
#ifdef FIXTURE_IDAPI_V13
    api->set_method_overwrite_hook(callback);
#else
    method_observation_replace_v1(fixture_method_observation(),callback);
#endif
}
static void *worker(void *context) { (void)context;replace(NULL);return NULL; }
static int inspect(void *context,const StorageViewV1 *view) { (void)context;(void)view;replace(NULL);return 1; }
__attribute__((noinline)) static void collect_now(void) { uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));api->gc_collect(); }
int main(int argc,char **argv) {
    assert(argc==3);setvbuf(stdout,NULL,_IONBF,0);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);fixture_prepare_namespace(floor,argv[1]);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);fixture_set_auto(0);
    fixture_mount_object_model(api,argv[1],fixture_root_registry());fixture_mount_trace(api,argv[1],fixture_root_registry());fixture_register_root(mark_roots);
    roots[0]=api->proto(api->object_vt);roots[1]=api->proto(roots[0]);roots[2]=api->intern("observedMethod");
    roots[3]=api->alloc(roots[1],0);target=roots[1];expected=api->true_obj;
    const char *mode=argv[2];
    if(!strcmp(mode,"owner")) { pthread_t thread;assert(!pthread_create(&thread,NULL,worker,NULL));assert(!pthread_join(thread,NULL));return 0; }
    if(!strcmp(mode,"observation")) {
        StorageObserveV1 observe=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(observe);
        StorageRequestV1 request={1,sizeof(request),NULL,inspect};observe(&request);assert(!"missing observation callback");
    }
    if(!strncmp(mode,"invalid-",8)) {
        if(strstr(mode,"null"))target=NULL;
        else if(strstr(mode,"instance"))target=roots[3];
        else if(strstr(mode,"stale")) { target=api->proto(api->object_vt);fixture_set_precise(1);collect_now(); }
        else target=(Oop)(uintptr_t)0x40;
        if(strstr(mode,"hook"))replace(hook);
        api->method(target,roots[2],first);assert(!"invalid target accepted");
    }
    assert(!strcmp(mode,"normal"));
#ifndef FIXTURE_IDAPI_V13
    Dl_info owner;assert(dladdr((void *)fixture_method_observation()->replace,&owner));assert(strstr(owner.dli_fname,"libbirnam_method_observation.dylib"));
    MethodObservationFactoryProviderV1 provide=(MethodObservationFactoryProviderV1)dlsym(dlopen(owner.dli_fname,RTLD_NOW|RTLD_LOCAL),"birnam_method_observation_factory_v1");assert(provide);
    assert(dladdr((void *)provide()->notify,&owner));assert(strstr(owner.dli_fname,"libbirnam_method_observation.dylib"));
#endif
    replace(hook);replace(hook);assert(!calls);
    api->method(roots[0],roots[2],first); // New parent binding, no callback.
    api->send(target,api->intern("removeMethod:"),&roots[2],1); // Inherited only.
    api->method(target,roots[2],first);assert(!calls); // Inherited replacement is new here.
    api->method(target,roots[2],first);assert(calls==1); // Same-address direct replacement.
    api->method(target,roots[2],second);assert(calls==2);
    expected=api->false_obj;replace(alternate);
    api->send(target,api->intern("removeMethod:"),&roots[2],1);assert(calls==3);
    api->send(target,api->intern("removeMethod:"),&roots[2],1);assert(calls==3);
    replace(NULL);replace(NULL);api->method(target,roots[2],first);api->method(target,roots[2],second);assert(calls==3);
    replace(hook);fixture_set_precise(1);collect_now();
    api->send(target,api->intern("removeMethod:"),&roots[2],1);assert(calls==4);
    replace(NULL);puts("method observation: replacement, clearing, old dictionary, exact bindings, identities and GC passed");
}
