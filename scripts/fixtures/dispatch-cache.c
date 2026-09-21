/* Capture real Runtime ownership and cache calls without production test exports. */
#define _DARWIN_C_SOURCE
#include "../../abi/fixtures/idapi-v20/idapi_v20.h"
#include "../../abi/fixtures/dispatch-cache-v2/dispatch_cache_v2.h"
#include "../../abi/fixtures/collection-phase-v1/collection_phase_v1.h"
#include "../../abi/fixtures/trace-live-v1/trace_live_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>

static const DispatchCacheFactoryV2 *factory;
static DispatchCacheFactoryV2 watched;
static const CollectionPhaseFactoryV1 *phases;
static CollectionPhaseFactoryV1 watched_phases;
static void *cache,*phase;
static unsigned creates,lookups,hits,inserts,clears,providers,admitted_before_clear;
static uintptr_t target_shape,target_selector;
static uint64_t saved_epoch;
static int collecting;
static void *create(void) { assert(!creates++);cache=factory->create();assert(cache);return cache; }
static void *phase_create(void) { phase=phases->create();assert(phase);return phase; }
static BirnamNativeEntry lookup(const void *context,uint64_t epoch,uintptr_t shape,uintptr_t selector) {
    assert(context==cache);BirnamNativeEntry method=factory->lookup(context,epoch,shape,selector);
    if(shape==target_shape && selector==target_selector) { lookups++;hits+=method.method!=NULL;saved_epoch=epoch; }
    return method;
}
static void insert(void *context,uint64_t epoch,uintptr_t shape,uintptr_t selector,BirnamNativeEntry method) {
    assert(context==cache && method.method);
    if(shape==target_shape && selector==target_selector)inserts++;
    factory->insert(context,epoch,shape,selector,method);
}
static void clear(void *context) {
    assert(context==cache);
    if(collecting)assert(phases->read(phase)==1 && admitted_before_clear && !providers);
    clears++;factory->clear(context);
}
static const DispatchCacheFactoryV2 *provide(void) { return &watched; }
static const CollectionPhaseFactoryV1 *provide_phases(void) { return &watched_phases; }
static void *capture_dlsym(void *image,const char *name) {
    void *symbol=dlsym(image,name);
    if(!strcmp(name,"birnam_dispatch_cache_factory_v2")) {
        assert(symbol);factory=((DispatchCacheFactoryProviderV2)symbol)();watched=*factory;
        watched.create=create;watched.lookup=lookup;watched.insert=insert;watched.clear=clear;return (void *)provide;
    }
    if(!strcmp(name,"birnam_collection_phase_factory_v1")) {
        assert(symbol);phases=((CollectionPhaseFactoryProviderV1)symbol)();watched_phases=*phases;
        watched_phases.create=phase_create;return (void *)provide_phases;
    }
    return symbol;
}
#define dlsym capture_dlsym
#include "namespace-support.h"
#undef dlsym
#include "collect-support.h"

static IdApi *api;
static Oop receiver;
static TraceLiveV1 actual_trace;
static TraceLiveSourceV1 original_source;
static int admit(void *context,uintptr_t address) {
    if(!clears)admitted_before_clear++;
    return original_source.admit(context,address);
}
static int trace(const TraceLiveSourceV1 *source) {
    original_source=*source;TraceLiveSourceV1 observed=*source;observed.admit=admit;
    return actual_trace(&observed);
}
static Oop identity(Oop self,const Oop *args,size_t count) { (void)args;assert(!count);return self; }
static Oop nil(Oop self,const Oop *args,size_t count) { (void)self;(void)args;assert(!count);return api->nil_obj; }
static void roots(MarkFn mark) {
    assert(collecting && clears==1 && phases->read(phase)==1);providers++;
    assert(api->send(receiver,(Oop)target_selector,NULL,0)==receiver);
    assert(!lookups && !inserts && !hits);mark(receiver);
}
static void reset(void) { lookups=hits=inserts=0; }
int main(int argc,char **argv) {
    assert(argc==3);void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);assert(!creates);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api && creates==1 && cache && phase);
    Dl_info owner;assert(dladdr((void *)factory->lookup,&owner) && strstr(owner.dli_fname,"libbirnam_dispatch_cache.dylib"));
    fixture_set_auto(0);
    Oop parent=api->proto(api->object_vt),shape=api->proto(parent),selector=api->intern("cacheLoadedAnswer");
    receiver=api->alloc(shape,0);Oop direct=api->alloc(parent,0);
    api->method(parent,selector,identity);target_shape=(uintptr_t)shape;target_selector=(uintptr_t)selector;
    reset();
    assert(api->send(direct,selector,NULL,0)==direct);assert(!lookups && !inserts);
    assert(api->send(receiver,selector,NULL,0)==receiver);
    assert(api->send(receiver,selector,NULL,0)==receiver);
    assert(lookups==2 && hits==1 && inserts==1);
    for(uint32_t value=1;value<=2;value++) {
        assert(phases->set(phase,value)==1);reset();
        assert(api->send(receiver,selector,NULL,0)==receiver);assert(!lookups && !inserts);
        assert(phases->set(phase,0)==1);
        assert(api->send(receiver,selector,NULL,0)==receiver);assert(lookups==1 && hits==1 && !inserts);
    }
    api->method(parent,selector,nil);reset();
    assert(api->send(receiver,selector,NULL,0)==api->nil_obj);assert(lookups==1 && !hits && inserts==1);
    api->method(parent,selector,identity);reset();
    assert(api->send(receiver,selector,NULL,0)==receiver);assert(lookups==1 && !hits && inserts==1);
    puts("Runtime cache: one owner, direct bypass, inherited miss/hit, both collection-phase bypasses and warm mutation");

    fixture_mount_collect(api,argv[1],fixture_root_registry());
    void *image=dlopen(argv[2],RTLD_NOW|RTLD_LOCAL);assert(image);
    ((ModuleInitFn)dlsym(image,"module_init"))(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers());
    const TraceLiveApiV1 *implementation=((TraceLiveProviderV1)dlsym(image,"birnam_trace_provider_v1"))();
    actual_trace=implementation->trace;TraceLiveApiV1 tracing=*implementation;tracing.trace=trace;
    assert(((TraceLiveDependencyV1)dlsym(floor,"birnam_trace_dependency_v1"))(&tracing)==1);
    fixture_register_root(roots);fixture_set_precise(1);
    uintptr_t anchor=0;fixture_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    assert(api->send(receiver,selector,NULL,0)==receiver);
    uint64_t epoch=saved_epoch;assert(factory->lookup(cache,epoch,target_shape,target_selector).method);
    clears=providers=admitted_before_clear=0;reset();collecting=1;api->gc_collect();collecting=0;
    assert(clears==1 && providers==1 && !factory->lookup(cache,epoch,target_shape,target_selector).method);
    assert(api->send(receiver,selector,NULL,0)==receiver);assert(lookups==1 && !hits && inserts==1);
    puts("Runtime GC clears after native admission and before providers; callbacks bypass the cache; next send repopulates");
}
