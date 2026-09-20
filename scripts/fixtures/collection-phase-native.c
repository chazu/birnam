#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "../../abi/fixtures/collection-phase-v1/collection_phase_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
#include "../../abi/fixtures/collect-v1/collect_v1.h"
#include "../../abi/fixtures/trace-live-v1/trace_live_v1.h"

static IdApi *api;
static const CollectionPhaseFactoryV1 *factory;
static const GcSettingsV1 *settings;
static Oop retained;
static unsigned collections,registered;
static CollectDependencyV1 attach_collect;
static TraceLiveDependencyV1 attach_trace;
static const CollectApiV1 *collect_api;
static const TraceLiveApiV1 *trace_api;
static void roots(MarkFn mark) {
    collections++;mark(retained);assert(api->gc_collect()==0);
    assert(!attach_collect(collect_api) && !attach_trace(trace_api));
    void *independent=factory->create();assert(independent && factory->read(independent)==0);
    assert(factory->set(independent,2)==1 && factory->read(independent)==2);
    assert(factory->set(independent,0)==1 && api->gc_collect()==0);
    assert(!attach_collect(collect_api) && !attach_trace(trace_api));factory->destroy(independent);
}
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==1 && args[0]);retained=args[0];
    void *a=factory->create(),*b=factory->create();assert(a && b && a!=b);
    assert(factory->read(a)==0 && factory->read(b)==0);
    for(uint32_t before=0;before<3;before++)for(uint32_t after=0;after<3;after++) {
        assert(factory->set(a,before)==1 && factory->set(a,after)==1);
        assert(factory->read(a)==after && factory->read(b)==0);
        assert(factory->set(a,UINT32_MAX)==0 && factory->read(a)==after);
    }
    factory->destroy(a);assert(factory->set(b,1)==1 && factory->read(b)==1);factory->destroy(b);
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
    int length=snprintf(path,sizeof(path),"%.*s/libbirnam_collect.dylib",(int)(slash-owner.dli_fname),owner.dli_fname);
    assert(length>0 && (size_t)length<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_NOLOAD);assert(image);
    CollectionPhaseFactoryProviderV1 provide=(CollectionPhaseFactoryProviderV1)dlsym(image,"birnam_collection_phase_factory_v1");assert(provide);
    factory=provide();assert(factory && factory->version==1 && factory->byte_size==sizeof(*factory));
    assert(dladdr((void *)factory->set,&owner) && strstr(owner.dli_fname,"libbirnam_collect.dylib"));
    /* Recover Id from its already selected function, not the operation owner. */
    assert(dladdr((void *)api->dlsym_fn,&owner));void *floor=dlopen(owner.dli_fname,RTLD_NOW|RTLD_LOCAL);assert(floor);
    attach_collect=(CollectDependencyV1)dlsym(floor,"birnam_collect_dependency_v1");
    attach_trace=(TraceLiveDependencyV1)dlsym(floor,"birnam_trace_dependency_v1");assert(attach_collect && attach_trace);
    collect_api=((CollectProviderV1)dlsym(image,"birnam_collect_provider_v1"))();assert(collect_api);
    slash=strrchr(owner.dli_fname,'/');assert(slash);
    length=snprintf(path,sizeof(path),"%.*s/libbirnam_trace.dylib",(int)(slash-owner.dli_fname),owner.dli_fname);
    assert(length>0 && (size_t)length<sizeof(path));void *tracer=dlopen(path,RTLD_NOW|RTLD_NOLOAD);assert(tracer);
    trace_api=((TraceLiveProviderV1)dlsym(tracer,"birnam_trace_provider_v1"))();assert(trace_api);
    if(!registered) { root_registry_register_v1(root_registry,roots);registered=1; }
    Oop system=namespace_import_v1(imports,"System");assert(system);
    api->method(api->vtable_of(system),api->intern("collectionPhaseExercise:"),exercise);
    return NULL;
}
__attribute__((visibility("default"))) int birnam_symbol_borrow_v1(void) { return 1; }
