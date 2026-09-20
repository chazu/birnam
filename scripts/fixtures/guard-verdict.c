/* Observe the loaded Runtime's two actual owners without production test exports. */
#define _DARWIN_C_SOURCE
#include "../../abi/fixtures/idapi-v20/idapi_v20.h"
#include "../../abi/fixtures/guard-verdict-v1/guard_verdict_v1.h"
#include "../../abi/fixtures/trace-live-v1/trace_live_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>

static const GuardVerdictFactoryV1 *factory;
static GuardVerdictFactoryV1 watched;
static void *owners[2];
static unsigned creates,reads[2],writes[2];
static unsigned index_of(const void *context) {
    for(unsigned i=0;i<creates;i++)if(owners[i]==context)return i;
    assert(0);return 0;
}
static GuardVerdictStateV1 state(unsigned index) {
    GuardVerdictStateV1 out;factory->read(owners[index],&out);return out;
}
static void *create(void) {
    assert(creates<2);void *context=factory->create();assert(context);
    owners[creates++]=context;GuardVerdictStateV1 out=state(creates-1);
    assert(out.epoch==UINT64_MAX && out.verified==0);return context;
}
static void read(const void *context,GuardVerdictStateV1 *out) {
    reads[index_of(context)]++;factory->read(context,out);
}
static void write(void *context,uint64_t epoch,int verdict) {
    writes[index_of(context)]++;factory->write(context,epoch,verdict);
}
static const GuardVerdictFactoryV1 *provide(void) { return &watched; }
static void *capture_dlsym(void *image,const char *name) {
    void *symbol=dlsym(image,name);
    if(!strcmp(name,"birnam_guard_verdict_factory_v1")) {
        assert(symbol);factory=((GuardVerdictFactoryProviderV1)symbol)();watched=*factory;
        watched.create=create;watched.read=read;watched.write=write;return (void *)provide;
    }
    return symbol;
}
#define dlsym capture_dlsym
#include "namespace-support.h"
#undef dlsym
#include "collect-support.h"
static IdApi *api;
static Oop machine,receiver,shape,selector;
static Method canonical_read,canonical_lookup;
static unsigned overridden_reads,overridden_lookups;
static Oop identity(Oop self,const Oop *args,size_t count) { (void)args;assert(!count);return self; }
static Oop read_override(Oop self,const Oop *args,size_t count) {
    overridden_reads++;return canonical_read(self,args,count);
}
static Oop lookup_override(Oop self,const Oop *args,size_t count) {
    if(self==shape && count==1 && args[0]==selector)overridden_lookups++;
    return canonical_lookup(self,args,count);
}
static void roots(MarkFn mark) { mark(machine);mark(receiver); }
static void reset(void) { memset(reads,0,sizeof(reads));memset(writes,0,sizeof(writes)); }
int main(int argc,char **argv) {
    assert(argc==3);void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);assert(!creates);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api && creates==2 && owners[0]!=owners[1]);
    Dl_info owner;assert(dladdr((void *)factory->read,&owner) && strstr(owner.dli_fname,"libbirnam_dispatch_cache.dylib"));
    fixture_set_auto(0);
    Oop parent=api->proto(api->object_vt);shape=api->proto(parent);receiver=api->alloc(shape,0);
    selector=api->intern("guardLoadedAnswer");api->method(parent,selector,identity);
    reset();assert(api->send(receiver,selector,NULL,0)==receiver);assert(api->send(receiver,selector,NULL,0)==receiver);
    assert(!reads[0] && !writes[0] && reads[1]>=2 && writes[1]==1 && state(1).verified);
    puts("verdict loaded lookup: actual Runtime owner, one cold write and warm reads");
    Oop prototype=fixture_namespace_import("Machine");assert(prototype);
    machine=api->send(prototype,api->intern("new"),NULL,0);assert(machine && machine!=api->nil_obj);
    Oop machine_vt=api->vtable_of(machine),read_selector=api->intern("oopAt:"),lookup_selector=api->intern("lookup");
    canonical_read=(Method)api->send(machine_vt,lookup_selector,&read_selector,1);assert(canonical_read);
    canonical_lookup=(Method)api->send(api->vtable_vt,lookup_selector,&lookup_selector,1);assert(canonical_lookup);
    reset();
    for(unsigned i=0;i<2;i++)assert(api->send(machine,read_selector,&api->nil_obj,1)==api->nil_obj);
    assert(reads[0]==2 && writes[0]==1 && state(0).verified);
    puts("verdict loaded slot guard: separate actual owner, one cold write and warm reads");
    api->method(machine_vt,read_selector,read_override);reset();overridden_reads=0;
    for(unsigned i=0;i<2;i++)assert(api->send(machine,read_selector,&api->nil_obj,1)==api->nil_obj);
    assert(overridden_reads==2 && reads[0]==2 && writes[0]==1 && !state(0).verified && state(1).verified);
    assert(state(0).epoch==state(1).epoch);
    api->method(api->vtable_vt,lookup_selector,lookup_override);reset();overridden_lookups=0;
    for(unsigned i=0;i<2;i++)assert(api->send(receiver,selector,NULL,0)==receiver);
    assert(overridden_lookups==2 && reads[1]>=2 && writes[1]==1 && !state(1).verified);
    puts("verdict loaded warm false: opposite cell facts, ordinary overrides once per send");
    api->method(machine_vt,read_selector,canonical_read);api->method(api->vtable_vt,lookup_selector,canonical_lookup);
    assert(api->send(machine,read_selector,&api->nil_obj,1)==api->nil_obj);
    assert(api->send(receiver,selector,NULL,0)==receiver);assert(state(0).verified && state(1).verified);
    fixture_mount_collect(api,argv[1],fixture_root_registry());
    void *image=dlopen(argv[2],RTLD_NOW|RTLD_LOCAL);assert(image);
    ((ModuleInitFn)dlsym(image,"module_init"))(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers());
    const TraceLiveApiV1 *trace=((TraceLiveProviderV1)dlsym(image,"birnam_trace_provider_v1"))();
    assert(((TraceLiveDependencyV1)dlsym(floor,"birnam_trace_dependency_v1"))(trace)==1);
    fixture_register_root(roots);fixture_set_precise(1);uintptr_t anchor=0;fixture_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    GuardVerdictStateV1 before[2]={state(0),state(1)};reset();api->gc_collect();
    for(unsigned i=0;i<2;i++) { GuardVerdictStateV1 after=state(i);assert(after.epoch==before[i].epoch && after.verified==before[i].verified && !writes[i]); }
    assert(api->send(receiver,selector,NULL,0)==receiver);
    puts("verdict loaded GC preserves both guard cells and rooted identity");
}
