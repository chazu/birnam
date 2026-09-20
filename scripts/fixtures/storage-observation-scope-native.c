#define _DARWIN_C_SOURCE
#include "../../abi/fixtures/idapi-v20/storage_v1.h"
#include "../../abi/fixtures/storage-observation-scope-v1/storage_observation_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>

static IdApi *api;
static const StorageObservationFactoryV1 *factory;
static const GcSettingsV1 *settings;
static StorageObserveV1 observe;
static Oop retained;
static unsigned collections,registered;

static StorageObservationStateV1 read_scope(void *context) {
    StorageObservationStateV1 state;factory->read(context,&state);return state;
}
static void roots(MarkFn mark) {
    collections++;mark(retained);
    assert(observe(NULL)==STORAGE_BUSY && api->gc_collect()==0);
}
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==1 && args[0]);retained=args[0];
    /* These are independent native owners, not the Runtime's authorization. */
    void *a=factory->create(),*b=factory->create();assert(a && b && a!=b);
    assert(!read_scope(a).epoch && !read_scope(a).active && !read_scope(a).permit);
    for(unsigned i=1;i<=3;i++) {
        factory->set_permit(a,-1);factory->set_active(a,17);
        uint64_t epoch=0;assert(factory->advance_epoch(a,&epoch)==1 && epoch==i);
        StorageObservationStateV1 state=read_scope(a);
        assert(state.epoch==i && state.active==1 && state.permit==1);
        assert(observe(NULL)==STORAGE_BUSY);
        factory->set_permit(a,0);factory->set_active(a,0);
        assert(!read_scope(b).epoch && !read_scope(b).active && !read_scope(b).permit);
    }
    factory->destroy(a);uint64_t epoch=0;
    assert(factory->advance_epoch(b,&epoch)==1 && epoch==1);factory->destroy(b);
    gc_settings_auto_v1(settings,0);gc_settings_precise_v1(settings,1);
    collections=0;api->gc_collect();assert(collections==1);
    api->alloc(api->object_vt,16u<<20);assert(collections==1);
    gc_settings_auto_v1(settings,1);assert(collections==1);
    api->alloc(api->object_vt,0);assert(collections==2);
    assert(observe(NULL)==STORAGE_BUSY && api->vtable_of(retained));
    Oop result=retained;retained=NULL;return result;
}
__attribute__((visibility("default")))
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *selected_settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,const FinalizersV1 *finalizers) {
    (void)prior;(void)presence;(void)exports;(void)vtables;(void)observation;(void)layouts;(void)hooks;(void)finalizers;
    if(api)assert(api==selected);api=selected;settings=selected_settings;
    Dl_info owner;assert(dladdr((void *)api->dlsym_fn,&owner));
    void *floor=dlopen(owner.dli_fname,RTLD_NOW|RTLD_NOLOAD);assert(floor);
    observe=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(observe);
    const char *slash=strrchr(owner.dli_fname,'/');assert(slash);char path[PATH_MAX];
    int length=snprintf(path,sizeof(path),"%.*s/libbirnam_storage.dylib",(int)(slash-owner.dli_fname),owner.dli_fname);
    assert(length>0 && (size_t)length<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_NOLOAD);assert(image);
    StorageObservationFactoryProviderV1 provide=(StorageObservationFactoryProviderV1)dlsym(image,"birnam_storage_observation_factory_v1");assert(provide);
    factory=provide();assert(factory && factory->version==1 && factory->byte_size==sizeof(*factory));
    assert(dladdr((void *)factory->advance_epoch,&owner) && strstr(owner.dli_fname,"libbirnam_storage.dylib"));
    if(!registered) { root_registry_register_v1(root_registry,roots);registered=1; }
    Oop system=namespace_import_v1(imports,"System");assert(system);
    api->method(api->vtable_of(system),api->intern("observationScopeExercise:"),exercise);
    return NULL;
}
__attribute__((visibility("default"))) int birnam_symbol_borrow_v1(void) { return 1; }
