#define _DARWIN_C_SOURCE
#include "idapi_v18.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>
static IdApi *api;
static const NamespaceImportV1 *imports;
static const MethodObservationV1 *observation;
static Oop rooted,basis_root;
static int rooted_registered;
static void mark_root(MarkFn mark) { mark(rooted);mark(basis_root); }
static Oop answer(Oop self,const Oop *args,size_t count) { (void)args;(void)count;return self; }
static Oop another(Oop self,const Oop *args,size_t count) { (void)self;(void)args;(void)count;return api->true_obj; }
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==1 && args && args[0]);
    Oop store=namespace_import_v1(imports,"Store");assert(store);
    rooted=api->proto(api->object_vt);assert(rooted);
    Oop selector=api->intern("observationImageMethod"),get=api->intern("rawCurrentValue:attr:"),fill=api->intern("backfillMirror");
    Oop pair[]={rooted,selector};
    basis_root=api->send(store,api->intern("basis"),NULL,0);
    api->method(rooted,selector,answer);api->send(store,fill,NULL,0);
    assert(api->send(store,get,pair,2)==api->intern("primitive"));
    api->method(rooted,selector,answer); // Same-address replacement still retracts.
    assert(!api->send(store,get,pair,2));api->send(store,fill,NULL,0);
    api->method(rooted,selector,another);assert(!api->send(store,get,pair,2));
    api->send(store,fill,NULL,0);api->gc_collect();
    assert(api->send(store,get,pair,2)==api->intern("primitive"));
    api->send(rooted,api->intern("removeMethod:"),&selector,1);assert(!api->send(store,get,pair,2));
    Oop basis_after=api->send(store,api->intern("basis"),NULL,0);
    assert(api->send(basis_root,api->intern("="),&basis_after,1)==api->true_obj);
    rooted=basis_root=NULL;return args[0];
}
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *selected_imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *selected_observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks) { (void)hooks; (void)layouts; (void)settings;
    (void)presence;(void)exports;(void)vtables;assert(selected && !prior && selected_imports && selected_observation);
    if(api)assert(api==selected && imports==selected_imports && observation==selected_observation);
    else { api=selected;imports=selected_imports;observation=selected_observation; }
    assert(observation->version==1 && observation->byte_size==sizeof(*observation));
    Dl_info owner;assert(dladdr((void *)observation->replace,&owner));assert(strstr(owner.dli_fname,"libbirnam_method_observation.dylib"));
    if(!rooted_registered) { root_registry_register_v1(root_registry,mark_root);rooted_registered=1; }
    Oop system=namespace_import_v1(imports,"SystemVt");assert(system);
    api->method(system,api->intern("observationExercise:"),exercise);return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
