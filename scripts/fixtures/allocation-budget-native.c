#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "../../abi/fixtures/allocation-budget-v1/allocation_budget_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>

static IdApi *api;
static const AllocationBudgetFactoryV1 *factory;
static const GcSettingsV1 *settings;
static Oop retained;
static unsigned collections,registered;
static void roots(MarkFn mark) {
    collections++;mark(retained);assert(api->gc_collect()==0);
}
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==1 && args[0]);retained=args[0];
    void *a=factory->create(),*b=factory->create();assert(a && b && a!=b);
    assert(factory->read(a)==0 && factory->read(b)==0);
    factory->replace(a,SIZE_MAX-8);size_t projected=19;
    assert(factory->preview(a,8,&projected)==1 && projected==SIZE_MAX);
    assert(factory->read(a)==SIZE_MAX-8 && factory->read(b)==0);
    assert(factory->charge(a,8)==1 && factory->charge(a,1)==0);
    projected=23;assert(factory->preview(a,1,&projected)==0 && projected==23);
    assert(factory->read(a)==SIZE_MAX);factory->destroy(a);
    assert(factory->charge(b,8)==1 && factory->read(b)==8);factory->destroy(b);
    gc_settings_auto_v1(settings,0);gc_settings_precise_v1(settings,1);
    collections=0;api->gc_collect();assert(collections==1);
    api->alloc(api->object_vt,16u<<20);assert(collections==1);
    gc_settings_auto_v1(settings,1);assert(collections==1);
    api->alloc(api->object_vt,0);assert(collections==2);
    assert(api->vtable_of(retained));
    Oop result=retained;retained=NULL;return result;
}
__attribute__((visibility("default")))
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *selected_settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,const FinalizersV1 *finalizers) {
    (void)prior;(void)presence;(void)exports;(void)vtables;(void)observation;(void)layouts;(void)hooks;(void)finalizers;
    if(api)assert(api==selected);api=selected;settings=selected_settings;
    Dl_info owner;assert(dladdr((void *)api->dlsym_fn,&owner));
    const char *slash=strrchr(owner.dli_fname,'/');assert(slash);char path[PATH_MAX];
    int length=snprintf(path,sizeof(path),"%.*s/libbirnam_allocation_budget.dylib",(int)(slash-owner.dli_fname),owner.dli_fname);
    assert(length>0 && (size_t)length<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_NOLOAD);assert(image);
    AllocationBudgetFactoryProviderV1 provide=(AllocationBudgetFactoryProviderV1)dlsym(image,"birnam_allocation_budget_factory_v1");assert(provide);
    factory=provide();assert(factory && factory->version==1 && factory->byte_size==sizeof(*factory));
    assert(dladdr((void *)factory->charge,&owner) && strstr(owner.dli_fname,"libbirnam_allocation_budget.dylib"));
    if(!registered) { root_registry_register_v1(root_registry,roots);registered=1; }
    Oop system=namespace_import_v1(imports,"System");assert(system);
    api->method(api->vtable_of(system),api->intern("allocationBudgetExercise:"),exercise);
    return NULL;
}
__attribute__((visibility("default"))) int birnam_symbol_borrow_v1(void) { return 1; }
