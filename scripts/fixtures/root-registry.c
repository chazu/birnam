#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "../../abi/fixtures/idapi-v20/storage_v1.h"
#define REGISTER(provider) root_registry_register_v1(registry,provider)
#include "native-fixture.h"
#include "../../abi/fixtures/root-registry-owner-v1/root_registry_owner_v1.h"
#include "../../abi/fixtures/native-stack-v1/native_stack_v1.h"
#include <dlfcn.h>
#include <stdio.h>
#include <limits.h>
#include <pthread.h>
#include <stdlib.h>

static IdApi *api;
static const RootRegistryV1 *registry;
static const GcSettingsV1 *settings;
static const NativeStackV1 *stack;
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
static unsigned admissions;
static void admit(void *context) { assert(context==&admissions);admissions++; }
static void overflow(void *context) { (void)context;abort(); }
static void factory_probe(const BirnamProtocol *id,BirnamOperand path) {
    const RootRegistryOwnedV1 *native=fixture_load_word(id,path);
    assert(native && native->version==1 && native->byte_size==sizeof(*native));
    const RootRegistryFactoryV1 *factory=native->factory;
    assert(factory && factory->version==1 && factory->byte_size==sizeof(*factory));
    RootRegistryFloorV1 floor={1,sizeof(floor),&admissions,admit,overflow};
    RootRegistryV1 *owned=factory->create(&floor);assert(owned && !admissions);
    root_registry_register_v1(owned,first);assert(admissions==1);
    RootRegistrySnapshotV1 before=factory->snapshot(owned);assert(before.count==1 && before.entries[0]==first);
    owned->context=NULL;owned->register_root=NULL;
    assert(factory->append(owned,later)==1);
    RootRegistrySnapshotV1 after=factory->snapshot(owned);assert(after.count==2 && after.entries[0]==first && after.entries[1]==later);
    factory->destroy(owned);assert(admissions==1 && before.count==1 && before.entries[0]==first && !before.entries[1]);
    for(size_t i=after.count;i<32;i++)assert(!after.entries[i]);
    Dl_info owner;assert(dladdr((void *)registry->register_root,&owner));
    assert(strstr(owner.dli_fname,"libbirnam_root_registry.dylib"));
    factory->destroy(native->api);
}
static BirnamOutcome exercise(const BirnamProtocol *id,const BirnamOperand *args,size_t count) {
    assert(count==7);setvbuf(stdout,NULL,_IONBF,0);api=fixture_word(args[0]);registry=fixture_word(args[1]);
    settings=fixture_word(args[2]);stack=fixture_word(args[3]);
    char *path=fixture_text(args[4]);void *floor=dlopen(path,RTLD_NOW|RTLD_LOCAL);free(path);assert(floor);
    char *mode=fixture_text(args[6]);gc_settings_auto_v1(settings,0);
    StorageObserveV1 inspect=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(inspect);
    factory_probe(id,args[5]);
    snapshot(inspect);size_t initial=provider_count;assert(initial<30);
    if(!strcmp(mode,"overflow")) {
        for(size_t i=initial;i<32;i++)REGISTER(unused);
        snapshot(inspect);assert(provider_count==32);puts("32 entries including duplicates");REGISTER(unused);abort();
    }
    if(!strcmp(mode,"observation")) { reject_observation=1;snapshot(inspect);abort(); }
    if(!strcmp(mode,"owner")) { pthread_t thread;assert(!pthread_create(&thread,NULL,wrong_thread,NULL));pthread_join(thread,NULL);abort(); }
    assert(!strcmp(mode,"normal"));
    REGISTER(first);REGISTER(first);snapshot(inspect);
    assert(provider_count==initial+2 && !call_count && !appended);
    puts("diagnostics copy ordered providers without invoking them");
    kept=api->alloc(api->object_vt,0);assert(kept);
    uintptr_t anchor=0;native_stack_set_v1(stack,(uintptr_t)&anchor+sizeof(anchor));
    api->gc_collect();assert(call_count==2 && calls[0]==1 && calls[1]==1 && api->vtable_of(kept)==api->object_vt);
    puts("first GC invokes duplicate entries; callback append waits");
    snapshot(inspect);assert(provider_count==initial+3 && call_count==2);
    call_count=0;api->gc_collect();assert(call_count==3 && calls[0]==1 && calls[1]==1 && calls[2]==2);
    puts("next GC invokes the appended provider in order; nested GC remains guarded");
    return (BirnamOutcome){.status=BIRNAM_LOADED};
}

FIXTURE_NATIVE_ENTRY(exercise)
