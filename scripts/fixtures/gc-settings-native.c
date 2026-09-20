#define _DARWIN_C_SOURCE
#include "idapi_v18.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>
static IdApi *api;
static const GcSettingsV1 *settings;
static const GcSettingsFactoryV1 *factory;
static Oop kept;
static unsigned active,calls,registered;
static void roots(MarkFn mark) {
    mark(kept);
    if(active) {
        calls++;
        gc_settings_auto_v1(settings,0);
        gc_settings_precise_v1(settings,-3);
        assert(api->gc_collect()==0);
    }
}
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(args && count==1 && args[0]);kept=args[0];
    int was_auto=factory->automatic_due(settings,1,16u<<20),was_precise=factory->precise(settings);
    const int values[]={0,1,-1};
    for(size_t i=0;i<3;i++) {
        gc_settings_precise_v1(settings,values[i]);assert(factory->precise(settings)==(values[i]!=0));
        gc_settings_auto_v1(settings,values[i]);
        assert(factory->automatic_due(settings,1,16u<<20)==(values[i]!=0));
        assert(!factory->automatic_due(settings,0,SIZE_MAX));
        assert(!factory->automatic_due(settings,1,(16u<<20)-1));
    }
    gc_settings_auto_v1(settings,0);
    (void)api->alloc(api->object_vt,16u<<20);
    calls=0;active=1;gc_settings_auto_v1(settings,1);assert(!calls);
    (void)api->alloc(api->object_vt,0);active=0;
    assert(calls==1 && factory->precise(settings)==1 && !factory->automatic_due(settings,1,SIZE_MAX));
    assert(api->vtable_of(kept));
    Oop result=kept;kept=NULL;
    gc_settings_precise_v1(settings,was_precise);gc_settings_auto_v1(settings,was_auto);
    return result;
}
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *selected_settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks) { (void)hooks; (void)layouts;
    (void)presence;(void)exports;(void)vtables;(void)observation;
    assert(selected && !prior && imports && root_registry && selected_settings);
    if(api)assert(api==selected && settings==selected_settings);
    else { api=selected;settings=selected_settings; }
    assert(settings->version==1 && settings->byte_size==sizeof(*settings));
    Dl_info owner;assert(dladdr((void *)settings->set_auto,&owner));
    assert(strstr(owner.dli_fname,"libbirnam_gc_settings.dylib"));
    void *image=api->dlopen_fn(owner.dli_fname,RTLD_NOW|RTLD_LOCAL);assert(image);
    GcSettingsFactoryProviderV1 provide=(GcSettingsFactoryProviderV1)api->dlsym_fn(image,"birnam_gc_settings_factory_v1");assert(provide);
    factory=provide();assert(factory);
    if(!registered) { root_registry_register_v1(root_registry,roots);registered=1; }
    Oop system=namespace_import_v1(imports,"SystemVt");assert(system);
    api->method(system,api->intern("gcSettingsExercise:"),exercise);return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
