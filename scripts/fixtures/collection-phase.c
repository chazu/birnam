/* Observe the real Runtime owner without exposing a production state getter. */
#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "../../abi/fixtures/idapi-v20/storage_v1.h"
#include "../../abi/fixtures/collection-phase-v1/collection_phase_v1.h"
#include "../../abi/fixtures/collect-v1/collect_v1.h"
#include "../../abi/fixtures/trace-live-v1/trace_live_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>

static const CollectionPhaseFactoryV1 *factory;
static CollectionPhaseFactoryV1 watched_factory;
static void *runtime_phase;
static unsigned creates,sets[64],set_count;
static void *capture_create(void) {
    assert(!creates++);runtime_phase=factory->create();assert(runtime_phase);return runtime_phase;
}
static int capture_set(void *context,uint32_t value) {
    assert(context==runtime_phase && set_count<64);sets[set_count++]=value;
    return factory->set(context,value);
}
static const CollectionPhaseFactoryV1 *provide_watched(void) { return &watched_factory; }
static void *capture_dlsym(void *image,const char *name) {
    void *symbol=dlsym(image,name);
    if(strcmp(name,"birnam_collection_phase_factory_v1"))return symbol;
    assert(symbol);factory=((CollectionPhaseFactoryProviderV1)symbol)();
    assert(factory && factory->version==1 && factory->byte_size==sizeof(*factory));
    watched_factory=*factory;watched_factory.create=capture_create;watched_factory.set=capture_set;
    return (void *)provide_watched;
}
#define dlsym capture_dlsym
#include "namespace-support.h"
#undef dlsym

static IdApi *api;
static const char *mode;
static TraceLiveDependencyV1 attach_trace;
static CollectDependencyV1 attach_collect;
static StorageObserveV1 observe;
static const TraceLiveApiV1 *trace_api;
static CollectV1 actual_collect;
static CollectApiV1 selected_collect;
static CollectSourceV1 original_source;
static Oop retained;
static unsigned roots_seen,finalized,pruned;
static void check_admission(uint32_t phase) {
    assert(factory->read(runtime_phase)==phase);
    assert(api->gc_collect()==0);
    assert(!attach_trace(trace_api) && !attach_collect(&selected_collect));
    assert(observe(NULL)==2);assert(api->vtable_of(retained)==api->object_vt);
}
static void roots(MarkFn mark) {
    roots_seen++;check_admission(COLLECTION_PHASE_MARKING);mark(retained);
    if(!strcmp(mode,"allocate-marking"))api->alloc((Oop)1,0);
}
static void finalize(Oop object) {
    assert(!api->vtable_of(object));finalized++;check_admission(COLLECTION_PHASE_FINALIZING);
    if(!strcmp(mode,"allocate-finalizing"))api->alloc((Oop)1,0);
}
static void prune(void *context) {
    assert(factory->read(runtime_phase)==COLLECTION_PHASE_IDLE);
    assert(actual_collect(NULL,NULL)==2);pruned++;
    original_source.prune(context);
}
static int collect(const CollectSourceV1 *source,size_t *out) {
    original_source=*source;CollectSourceV1 watched=*source;watched.prune=prune;
    if(!strcmp(mode,"invalid-source-phase"))source->phase(source->context,UINT32_MAX);
    return actual_collect(&watched,out);
}
static void *mount(void *floor,const char *floor_path,const char *name) {
    (void)floor;const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_%s.dylib",(int)(slash-floor_path),floor_path,name);
    assert(n>0 && (size_t)n<sizeof(path));void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    ModuleInitFn init=(ModuleInitFn)dlsym(image,"module_init");assert(init);
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers());
    return image;
}
__attribute__((noinline)) static void allocate_dead(void) {
    Oop dead=api->alloc(api->object_vt,0);fixture_set_finalizer(dead,finalize);
}
__attribute__((noinline)) static void collect_after_return(void) {
    volatile uintptr_t scratch[8192];for(size_t i=0;i<8192;i++)scratch[i]=0;
    api->gc_collect();assert(!scratch[8191]);
}
int main(int argc,char **argv) {
    assert(argc==3);mode=argv[2];void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);assert(!creates);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api && creates==1 && !factory->read(runtime_phase));
    Dl_info owner;assert(dladdr((void *)factory->set,&owner) && strstr(owner.dli_fname,"libbirnam_collect.dylib"));
    fixture_set_auto(0);fixture_set_precise(1);uintptr_t anchor=0;fixture_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    if(!strcmp(mode,"unmounted")) { api->gc_collect();assert(!"unmounted coordinator returned"); }
    void *tracer=mount(floor,argv[1],"trace");
    trace_api=((TraceLiveProviderV1)dlsym(tracer,"birnam_trace_provider_v1"))();
    attach_trace=(TraceLiveDependencyV1)dlsym(floor,"birnam_trace_dependency_v1");assert(attach_trace(trace_api)==1);
    void *driver=mount(floor,argv[1],"collect");
    selected_collect=*((CollectProviderV1)dlsym(driver,"birnam_collect_provider_v1"))();actual_collect=selected_collect.collect;selected_collect.collect=collect;
    attach_collect=(CollectDependencyV1)dlsym(floor,"birnam_collect_dependency_v1");assert(attach_collect(&selected_collect)==1);
    observe=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(observe);
    retained=api->alloc(api->object_vt,0);fixture_register_root(roots);allocate_dead();
    set_count=0;collect_after_return();
    assert(roots_seen==1 && finalized==1 && pruned==1);
    assert(set_count==4 && sets[0]==1 && sets[1]==2 && sets[2]==0 && sets[3]==0);
    assert(factory->read(runtime_phase)==0 && creates==1);
    puts("one Runtime owner: Marking, Finalizing, Idle, then guard reset");
    puts("callbacks preserve nested GC, attachment, storage refusal and allowed reads");
    puts("Runtime is Idle during pruning while the independent coordinator remains busy");
}
