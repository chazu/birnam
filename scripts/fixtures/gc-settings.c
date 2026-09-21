#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "../../abi/fixtures/idapi-v20/storage_v1.h"
static GcSettingsV1 *settings;
#define SET_AUTO(value) gc_settings_auto_v1(settings,value)
#define SET_PRECISE(value) gc_settings_precise_v1(settings,value)
#include "native-fixture.h"
#include "../../abi/fixtures/native-stack-v1/native_stack_v1.h"
#include <dlfcn.h>
#include <stdio.h>
#include <limits.h>
#include <pthread.h>
#include <stdlib.h>

static IdApi *api;
static const RootRegistryV1 *registry;
static const NativeStackV1 *stack;
static const char *mode;
static unsigned collections,observations;
static uint32_t observed_precise;
static void roots(MarkFn mark) { (void)mark;collections++;assert(api->gc_collect()==0); }
static void mutate(void) {
    if(strstr(mode,"precise")) SET_PRECISE(-1);
    else SET_AUTO(-1);
}
static int observe(void *context,const StorageViewV1 *view) {
    assert(context==&observations);observations++;
    observed_precise=view->gc_precise;
    if(strstr(mode,"observation")) mutate();
    return 0;
}
static void snapshot(StorageObserveV1 inspect) {
    StorageRequestV1 request={1,sizeof(request),&observations,observe};
    assert(inspect(&request)==0);
}
static void *wrong_thread(void *context) { (void)context;mutate();return NULL; }

static unsigned admissions;
static void admit(void *context) { assert(context==&admissions);admissions++; }
static void factory_probe(const BirnamProtocol *id,BirnamOperand path) {
    const GcSettingsFactoryV1 *factory=fixture_load_word(id,path);
    assert(factory && factory->version==1 && factory->byte_size==sizeof(*factory));
    GcSettingsFloorV1 floor={1,sizeof(floor),&admissions,admit};
    GcSettingsV1 *a=factory->create(&floor),*b=factory->create(&floor);assert(a && b && a!=b && !admissions);
    assert(!factory->precise(a));
    const size_t threshold=16u<<20;
    for(int ready=0;ready<2;ready++) {
        assert(!factory->automatic_due(a,ready,threshold-1));
        assert(factory->automatic_due(a,ready,threshold)==ready);
        assert(factory->automatic_due(a,ready,SIZE_MAX)==ready);
    }
    gc_settings_auto_v1(a,0);gc_settings_precise_v1(a,-9);assert(admissions==2);
    a->context=NULL;a->set_auto=NULL;a->set_precise=NULL;
    assert(!factory->automatic_due(a,1,SIZE_MAX) && factory->precise(a));
    assert(factory->automatic_due(b,1,threshold) && !factory->precise(b));
    factory->set_auto(a,-2);factory->set_precise(a,0);
    assert(factory->automatic_due(a,1,threshold) && !factory->precise(a));
    factory->destroy(a);factory->destroy(b);assert(admissions==2);
    Dl_info owner;assert(dladdr((void *)settings->set_auto,&owner));
    assert(strstr(owner.dli_fname,"libbirnam_gc_settings.dylib"));
    assert(dladdr((void *)settings->set_precise,&owner));
    assert(strstr(owner.dli_fname,"libbirnam_gc_settings.dylib"));

}

static BirnamOutcome exercise(const BirnamProtocol *id,const BirnamOperand *args,size_t count) {
    assert(count==7);api=fixture_word(args[0]);registry=fixture_word(args[1]);
    settings=fixture_word(args[2]);stack=fixture_word(args[3]);
    char *path=fixture_text(args[4]);void *floor=dlopen(path,RTLD_NOW|RTLD_LOCAL);free(path);assert(floor);
    mode=fixture_text(args[6]);factory_probe(id,args[5]);
    StorageObserveV1 inspect=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(inspect);
    if(strstr(mode,"owner")) { pthread_t thread;assert(!pthread_create(&thread,NULL,wrong_thread,NULL));assert(!pthread_join(thread,NULL));abort(); }
    if(strstr(mode,"observation")) { snapshot(inspect);abort(); }
    snapshot(inspect);assert(observed_precise==0);
    for(size_t i=0;i<5;i++) {
        const int values[]={0,1,-1,INT_MIN,INT_MAX};SET_PRECISE(values[i]);
        snapshot(inspect);assert(observed_precise==(uint32_t)(values[i]!=0));
    }
    SET_PRECISE(0);snapshot(inspect);assert(!observed_precise);
    puts("precise defaults, nonzero conversion and copied observation agree");
    SET_AUTO(0);root_registry_register_v1(registry,roots);
    native_stack_set_v1(stack,0);
    (void)api->alloc(api->object_vt,16u<<20);assert(!collections);
    SET_AUTO(-1);assert(!collections);
    (void)api->alloc(api->object_vt,0);assert(!collections);
    uintptr_t anchor=0;native_stack_set_v1(stack,(uintptr_t)&anchor+sizeof(anchor));
    (void)api->alloc(api->object_vt,0);assert(collections==1);
    puts("automatic collection waits for stack readiness and the next allocation");
    SET_AUTO(0);(void)api->alloc(api->object_vt,16u<<20);assert(collections==1);
    SET_AUTO(1);assert(collections==1);(void)api->alloc(api->object_vt,0);assert(collections==2);
    puts("suppression retains accumulated bytes; enabling does not collect immediately");
    return (BirnamOutcome){.status=BIRNAM_LOADED};
}
FIXTURE_NATIVE_ENTRY(exercise)
