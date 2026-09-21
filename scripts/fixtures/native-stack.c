/* Independent pre/post extraction oracle for the host's native root anchor. */
#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "../../abi/fixtures/idapi-v20/storage_v1.h"
#include "../../abi/fixtures/native-stack-v1/native_stack_v1.h"
static const NativeStackV1 *stack;
#define SET(address) native_stack_set_v1(stack,address)
#include "native-fixture.h"
#include <dlfcn.h>
#include <stdio.h>
#include <limits.h>
#include <pthread.h>
#include <stdlib.h>

static const RootRegistryV1 *registry;
static const GcSettingsV1 *settings;
static const FinalizersV1 *finalizers;
static IdApi *api;
static const char *mode;
static unsigned collections,reset_during_roots,finished;
static Oop kept;
static void roots(MarkFn mark) {
    collections++;mark(kept);
    if(reset_during_roots) { SET(0);assert(api->gc_collect()==0); }
}
static void finalize(Oop object) {
    assert(!api->vtable_of(object));finished++;
    SET(0);assert(api->gc_collect()==0);
}
__attribute__((noinline)) static void allocate_dead(void) {
    Oop dead=api->alloc(api->object_vt,8);finalizers_set_v1(finalizers,dead,finalize);
}
__attribute__((noinline)) static void collect_after_return(void) {
    volatile uintptr_t scratch[8192];for(size_t i=0;i<8192;i++)scratch[i]=0;
    api->gc_collect();assert(scratch[8191]==0);
}
static void mutate(void) { SET(strstr(mode,"-zero")?0:1); }
static void *wrong_thread(void *context) { (void)context;mutate();return NULL; }
static int observe(void *context,const StorageViewV1 *view) { (void)context;(void)view;mutate();abort(); }
static BirnamOutcome exercise(const BirnamProtocol *id,const BirnamOperand *args,size_t count) {
    (void)id;assert(count==7);
    api=fixture_word(args[0]);registry=fixture_word(args[1]);settings=fixture_word(args[2]);
    stack=fixture_word(args[3]);finalizers=fixture_word(args[4]);
    char *path=fixture_text(args[5]);void *floor=dlopen(path,RTLD_NOW|RTLD_LOCAL);free(path);assert(floor);
    mode=fixture_text(args[6]);
    Dl_info owner;assert(dladdr((void *)stack->set,&owner) && strstr(owner.dli_fname,"libbirnam_native_stack.dylib"));
    if(strstr(mode,"owner")) { pthread_t thread;assert(!pthread_create(&thread,NULL,wrong_thread,NULL));assert(!pthread_join(thread,NULL));abort(); }
    if(strstr(mode,"observation")) {
        StorageObserveV1 inspect=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(inspect);
        StorageRequestV1 request={1,sizeof(request),NULL,observe};inspect(&request);abort();
    }
    gc_settings_auto_v1(settings,0);gc_settings_precise_v1(settings,1);root_registry_register_v1(registry,roots);
    // Production activation already set the thread anchor; exercise explicit suppression.
    SET(0);
    size_t before=api->gc_live_count();
    assert(api->gc_collect()==0 && !collections);
    for(size_t i=0;i<6;i++) { const uintptr_t addresses[]={0,1,3,UINTPTR_MAX,7,0};SET(addresses[i]); }
    assert(api->gc_live_count()==before && !collections && api->gc_collect()==0);
    puts("zero anchor and arbitrary assignments neither collect nor change allocation count");
    (void)api->alloc(api->object_vt,16u<<20);gc_settings_auto_v1(settings,1);
    (void)api->alloc(api->object_vt,0);assert(!collections);
    struct { volatile Oop native_only; uintptr_t anchor; } boundary={api->alloc(api->object_vt,0),0};
    uintptr_t bottom=(uintptr_t)&boundary+sizeof(boundary);
    kept=api->alloc(api->object_vt,0);SET(bottom);assert(!collections);
    (void)api->alloc(api->object_vt,0);assert(collections==1);
    assert(api->vtable_of(boundary.native_only)==api->object_vt && api->vtable_of(kept)==api->object_vt);
    puts("pending automatic collection waits for the next allocation and retains native/provider roots");
    gc_settings_auto_v1(settings,0);reset_during_roots=1;api->gc_collect();assert(collections==2);
    assert(api->vtable_of(boundary.native_only)==api->object_vt && api->vtable_of(kept)==api->object_vt);
    assert(api->gc_collect()==0 && collections==2);reset_during_roots=0;
    SET(bottom);api->gc_collect();assert(collections==3);
    puts("callback-time reset preserves this collection and suppresses the next until reassigned");
    allocate_dead();collect_after_return();assert(finished==1 && collections==4);
    assert(api->gc_collect()==0 && collections==4);SET(bottom);api->gc_collect();assert(finished==1 && collections==5);
    puts("finalizer-time assignment is allowed and nested collection stays suppressed");
    SET(0);(void)api->alloc(api->object_vt,16u<<20);gc_settings_auto_v1(settings,1);
    (void)api->alloc(api->object_vt,0);assert(collections==5);
    SET(bottom);assert(collections==5);(void)api->alloc(api->object_vt,0);assert(collections==6);
    puts("anchor reset and restoration preserve accumulated allocation budget");
    return (BirnamOutcome){.status=BIRNAM_LOADED};
}
FIXTURE_NATIVE_ENTRY(exercise)
