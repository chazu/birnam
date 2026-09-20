#define _DARWIN_C_SOURCE
#ifdef ROOT_REGISTRY_BASELINE
#include "idapi_v14.h"
#include "../../abi/fixtures/storage-v1/storage_v1.h"
#define ROOTS_ARGUMENT
#define REGISTER(provider) api->gc_register_root_provider(provider)
#else
#include "idapi_v15.h"
#include "../../abi/fixtures/idapi-v15/storage_v1.h"
#define ROOTS_ARGUMENT ,fixture_root_registry()
#define REGISTER(provider) fixture_register_root(provider)
#endif
#include "namespace-support.h"
#include "trace-support.h"
#include <pthread.h>
#include <stdlib.h>

static IdApi *api;
static Oop kept;
static unsigned calls[16],call_count,appended,observations;
static size_t provider_count;
static int reject_observation;
static void later(MarkFn mark) { assert(call_count<16);calls[call_count++]=2;mark(kept); }
static void first(MarkFn mark) {
    assert(call_count<16);calls[call_count++]=1;mark(kept);
    assert(api->gc_collect()==0);
    if(!appended++) REGISTER(later);
}
static void unused(MarkFn mark) { (void)mark;abort(); }
static int observe(void *context,const StorageViewV1 *view) {
    assert(context==&observations);observations++;
    assert(view && view->legacy_root_provider_count && (view->coverage_gaps & STORAGE_LEGACY_ROOTS));
    provider_count=view->legacy_root_provider_count;
    assert(view->legacy_root_providers[provider_count-1]);
    if(reject_observation) REGISTER(unused);
    return 0;
}
static void snapshot(StorageObserveV1 inspect) {
    StorageRequestV1 request={1,sizeof(request),&observations,observe};
    assert(inspect(&request)==0);
}
static void *wrong_thread(void *unused_context) { (void)unused_context;REGISTER(unused);return NULL; }
#ifndef ROOT_REGISTRY_BASELINE
static unsigned admissions;
static void admit(void *context) { assert(context==&admissions);admissions++; }
static void overflow(void *context) { (void)context;abort(); }
static void factory_probe(const char *floor_path) {
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    assert(snprintf(path,sizeof(path),"%.*s/libbirnam_root_registry.dylib",(int)(slash-floor_path),floor_path)>0);
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    RootRegistryFactoryProviderV1 provide=(RootRegistryFactoryProviderV1)dlsym(image,"birnam_root_registry_factory_v1");assert(provide);
    const RootRegistryFactoryV1 *factory=provide();assert(factory && factory->version==1 && factory->byte_size==sizeof(*factory));
    RootRegistryFloorV1 floor={1,sizeof(floor),&admissions,admit,overflow};
    RootRegistryV1 *owned=factory->create(&floor);assert(owned && !admissions);
    root_registry_register_v1(owned,first);assert(admissions==1);
    RootRegistrySnapshotV1 before=factory->snapshot(owned);assert(before.count==1 && before.entries[0]==first);
    owned->context=NULL;owned->register_root=NULL;
    assert(factory->append(owned,later)==1);
    RootRegistrySnapshotV1 after=factory->snapshot(owned);assert(after.count==2 && after.entries[0]==first && after.entries[1]==later);
    factory->destroy(owned);assert(admissions==1 && before.count==1 && before.entries[0]==first && !before.entries[1]);
    for(size_t i=after.count;i<32;i++)assert(!after.entries[i]);
    Dl_info owner;assert(dladdr((void *)fixture_root_registry()->register_root,&owner));
    assert(strstr(owner.dli_fname,"libbirnam_root_registry.dylib"));
    ModuleInitFn initialize=(ModuleInitFn)dlsym(image,"module_init");assert(initialize);
    initialize(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry());
}
#endif
int main(int argc,char **argv) {
    assert(argc==3);setvbuf(stdout,NULL,_IONBF,0);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1] ROOTS_ARGUMENT);
    StorageObserveV1 inspect=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(inspect);
#ifndef ROOT_REGISTRY_BASELINE
    factory_probe(argv[1]);
#endif
    snapshot(inspect);size_t initial=provider_count;assert(initial<30);
    if(!strcmp(argv[2],"overflow")) {
        for(size_t i=initial;i<32;i++)REGISTER(unused);
        snapshot(inspect);assert(provider_count==32);puts("32 entries including duplicates");REGISTER(unused);abort();
    }
    if(!strcmp(argv[2],"observation")) { reject_observation=1;snapshot(inspect);abort(); }
    if(!strcmp(argv[2],"owner")) { pthread_t thread;assert(!pthread_create(&thread,NULL,wrong_thread,NULL));pthread_join(thread,NULL);abort(); }
    assert(!strcmp(argv[2],"normal"));
    REGISTER(first);REGISTER(first);snapshot(inspect);
    assert(provider_count==initial+2 && !call_count && !appended);
    puts("diagnostics copy ordered providers without invoking them");
    kept=api->alloc(api->object_vt,0);assert(kept);
    uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    api->gc_collect();assert(call_count==2 && calls[0]==1 && calls[1]==1 && api->vtable_of(kept)==api->object_vt);
    puts("first GC invokes duplicate entries; callback append waits");
    snapshot(inspect);assert(provider_count==initial+3 && call_count==2);
    call_count=0;api->gc_collect();assert(call_count==3 && calls[0]==1 && calls[1]==1 && calls[2]==2);
    puts("next GC invokes the appended provider in order; nested GC remains guarded");
    return 0;
}
