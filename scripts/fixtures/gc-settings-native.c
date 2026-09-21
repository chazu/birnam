#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "native-fixture.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>
static IdApi *api;
static const NamespaceImportV1 *imports;
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
static BirnamOutcome initialize(const BirnamProtocol *id,const BirnamOperand *args,size_t count) {
    (void)id;
    if(count) {
        assert(count==5);IdApi *selected=fixture_word(args[0]);
        if(api)assert(api==selected);api=selected;imports=fixture_word(args[1]);
        const RootRegistryV1 *root_registry=fixture_word(args[2]);
        settings=fixture_word(args[3]);factory=fixture_word(args[4]);
        if(!registered) { root_registry_register_v1(root_registry,roots);registered=1; }
    }
    assert(api && imports && registered);
    Dl_info owner;
    assert(settings->version==1 && settings->byte_size==sizeof(*settings));
    assert(factory->version==1 && factory->byte_size==sizeof(*factory));
    assert(dladdr((void *)settings->set_auto,&owner) && strstr(owner.dli_fname,"libbirnam_gc_settings.dylib"));
    Oop system=namespace_import_v1(imports,"System");assert(system);
    api->method(api->vtable_of(system),api->intern("gcSettingsExercise:"),exercise);
    return (BirnamOutcome){.status=BIRNAM_LOADED};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
