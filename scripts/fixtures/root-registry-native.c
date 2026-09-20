#define _DARWIN_C_SOURCE
#include "idapi_v16.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>
static IdApi *api;
static const RootRegistryV1 *roots;
static Oop kept;
static unsigned active,calls[4],call_count,registered,later_registered;
static void later(MarkFn mark) {
    mark(kept);
    if(active) { assert(call_count<4);calls[call_count++]=2; }
}
static void first(MarkFn mark) {
    mark(kept);
    if(active) {
        assert(call_count<4);calls[call_count++]=1;
        assert(api->gc_collect()==0);
        if(!later_registered) { later_registered=1;root_registry_register_v1(roots,later); }
    }
}
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==1 && args && args[0] && registered);
    kept=api->alloc(api->object_vt,0);assert(kept);
    unsigned already=later_registered;
    call_count=0;active=1;api->gc_collect();active=0;
    assert(call_count==2+already && calls[0]==1 && calls[1]==1);
    if(already)assert(calls[2]==2);
    assert(api->vtable_of(kept)==api->object_vt);
    call_count=0;active=1;api->gc_collect();active=0;
    assert(call_count==3 && calls[0]==1 && calls[1]==1 && calls[2]==2);
    assert(api->vtable_of(kept)==api->object_vt);kept=NULL;
    return args[0];
}
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *settings) { (void)settings;
    (void)presence;(void)exports;(void)vtables;(void)observation;
    assert(selected && !prior && imports && root_registry);
    if(api)assert(api==selected && roots==root_registry);
    else { api=selected;roots=root_registry; }
    assert(roots->version==1 && roots->byte_size==sizeof(*roots));
    Dl_info owner;assert(dladdr((void *)roots->register_root,&owner));
    assert(strstr(owner.dli_fname,"libbirnam_root_registry.dylib"));
    if(!registered) { root_registry_register_v1(roots,first);root_registry_register_v1(roots,first);registered=1; }
    Oop system=namespace_import_v1(imports,"SystemVt");assert(system);
    api->method(system,api->intern("rootRegistryExercise:"),exercise);return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
