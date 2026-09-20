#define _DARWIN_C_SOURCE
#ifdef GC_SETTINGS_BASELINE
#include "idapi_v15.h"
#include "../../abi/fixtures/idapi-v15/storage_v1.h"
#define SET_AUTO(value) api->gc_set_auto(value)
#define SET_PRECISE(value) api->gc_set_precise(value)
#else
#include "idapi_v17.h"
#include "../../abi/fixtures/idapi-v17/storage_v1.h"
static GcSettingsV1 *settings;
#define SET_AUTO(value) gc_settings_auto_v1(settings,value)
#define SET_PRECISE(value) gc_settings_precise_v1(settings,value)
#endif
#include "namespace-support.h"
#include "trace-support.h"
#include <pthread.h>
#include <stdlib.h>

static IdApi *api;
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

#ifndef GC_SETTINGS_BASELINE
static unsigned admissions;
static void admit(void *context) { assert(context==&admissions);admissions++; }
static void factory_probe(const char *floor_path) {
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    assert(snprintf(path,sizeof(path),"%.*s/libbirnam_gc_settings.dylib",(int)(slash-floor_path),floor_path)>0);
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    GcSettingsFactoryProviderV1 provide=(GcSettingsFactoryProviderV1)dlsym(image,"birnam_gc_settings_factory_v1");assert(provide);
    const GcSettingsFactoryV1 *factory=provide();assert(factory && factory->version==1 && factory->byte_size==sizeof(*factory));
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
    ModuleInitFn initialize=(ModuleInitFn)dlsym(image,"module_init");assert(initialize);
    assert(!initialize(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),settings,fixture_payload_layout()));
}
#endif

int main(int argc,char **argv) {
    assert(argc==3);mode=argv[2];
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);
#ifndef GC_SETTINGS_BASELINE
    settings=fixture_gc_settings();factory_probe(argv[1]);
#endif
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
    SET_AUTO(0);fixture_mount_trace(api,argv[1],fixture_root_registry());fixture_register_root(roots);
    api->gc_set_stack_bottom(0);
    (void)api->alloc(api->object_vt,16u<<20);assert(!collections);
    SET_AUTO(-1);assert(!collections);
    (void)api->alloc(api->object_vt,0);assert(!collections);
    uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    (void)api->alloc(api->object_vt,0);assert(collections==1);
    puts("automatic collection waits for stack readiness and the next allocation");
    SET_AUTO(0);(void)api->alloc(api->object_vt,16u<<20);assert(collections==1);
    SET_AUTO(1);assert(collections==1);(void)api->alloc(api->object_vt,0);assert(collections==2);
    puts("suppression retains accumulated bytes; enabling does not collect immediately");
}
