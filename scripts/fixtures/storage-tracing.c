#include "../../abi/fixtures/idapi-v20/storage_v1.h"
#include "../../abi/fixtures/trace-v1/trace_v1.h"
#include "namespace-support.h"
#include "integer-support.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static IdApi *api;
static StorageObserveV1 observe;
static StorageDescribeV1 describe;
static TraceDescribedV1 trace;
static Oop nodes[9], pair_vt;
static uint64_t identities[9], runtime_identity, last_epoch;
static int scenario, report_calls, report_status, provider_status, missing_provider;
static int wrapper_calls, malformed_checks, legacy_root_calls, legacy_hook_calls;
static Method original;

static size_t index_of(const StorageViewV1 *view, Oop object) {
    for(size_t i=0; i<view->allocation_count; i++) if(view->allocations[i].object==object) return i;
    abort();
}
static void legacy_roots(MarkFn mark) { legacy_root_calls++; mark(nodes[6]); }
static void legacy_hook(Oop self, MarkFn mark) { (void)self; legacy_hook_calls++; mark(nodes[6]); }
static void finalizer(Oop self) { (void)self; abort(); }
static Oop wrapper(Oop self, const Oop *args, size_t count) {
    wrapper_calls++;
    return original(self,args,count);
}
static int root_enabled(size_t node) {
    return node==5 || node==8 || (node==0 && scenario!=2) || (node==3 && scenario==1);
}
static int provide(void *context, const StorageViewV1 *view, const StorageSinkV1 *sink) {
    (void)context;
    if(provider_status) return provider_status;
    assert(sink->root(sink->context,pair_vt)==STORAGE_OK);
    for(size_t i=0; i<9; i++) {
        assert(view->allocations[index_of(view,nodes[i])].identity==identities[i]);
        if(root_enabled(i)) assert(sink->root(sink->context,nodes[i])==STORAGE_OK);
    }
    const size_t pair_indices[]={0,1,3,4};
    const size_t offsets[]={0,sizeof(Oop)};
    for(size_t i=0; i<4; i++) {
        size_t node=pair_indices[i];
        StorageDescriptionV1 claim={runtime_identity,identities[node],nodes[node],2*sizeof(Oop),offsets,2};
        assert(sink->describe(sink->context,&claim)==STORAGE_OK);
    }
    StorageDescriptionV1 raw={runtime_identity,identities[5],nodes[5],sizeof(int64_t),NULL,0};
    assert(sink->describe(sink->context,&raw)==STORAGE_OK);
    return STORAGE_OK;
}
static int add_edge(const StorageViewV1 *view, unsigned char *reached, Oop from, Oop to) {
    if(!to) return 0;
    size_t f=index_of(view,from), t=index_of(view,to);
    if(reached[f] && !reached[t]) { reached[t]=1; return 1; }
    return 0;
}
static int report(void *context, const TraceReportV1 *result) {
    (void)context;
    report_calls++;
    const StorageViewV1 *view=result->described->storage;
    assert(result->version==1 && result->byte_size==sizeof(*result));
    assert(result->runtime_identity==runtime_identity && result->observation_identity==view->observation_identity);
    assert(result->coverage_gaps==(view->coverage_gaps|STORAGE_MISSING));
    assert(result->coverage_gaps & STORAGE_NATIVE_ROOTS);
    assert(result->coverage_gaps & STORAGE_LEGACY_ROOTS);
    assert(result->coverage_gaps & STORAGE_LEGACY_HOOKS);
    assert(result->reached_count+result->not_reached_count==view->allocation_count);
    assert(!legacy_root_calls && !legacy_hook_calls);

    /* Independent oracle: repeatedly scan edges, without the tracer's adjacency
       index or worklist. Payload edges come directly from known fixture slots,
       not from the description validator's output. Raw Integer bytes are absent. */
    unsigned char *expected=calloc(view->allocation_count,1), *seen=calloc(view->allocation_count,1);
    assert(expected && seen);
    for(size_t i=0; i<view->root_count; i++) expected[index_of(view,view->roots[i])]=1;
    expected[index_of(view,pair_vt)]=1;
    for(size_t i=0; i<9; i++) if(root_enabled(i)) expected[index_of(view,nodes[i])]=1;
    int changed;
    do {
        changed=0;
        for(size_t i=0; i<view->edge_count; i++) changed |= add_edge(view,expected,view->edges[i].from,view->edges[i].to);
        const size_t pair_indices[]={0,1,3,4};
        for(size_t i=0; i<4; i++) for(size_t slot=0; slot<2; slot++) {
            Oop from=nodes[pair_indices[i]];
            changed |= add_edge(view,expected,from,((Oop *)from)[slot]);
        }
    } while(changed);
    for(int reached=0; reached<=1; reached++) {
        const TraceAllocationRefV1 *list=reached?result->reached:result->not_reached;
        size_t count=reached?result->reached_count:result->not_reached_count;
        uint64_t previous=0;
        for(size_t i=0; i<count; i++) {
            size_t index=index_of(view,list[i].object);
            assert(!seen[index]); seen[index]=1;
            assert(expected[index]==reached);
            assert(list[i].allocation_identity==view->allocations[index].identity);
            assert(list[i].allocation_identity>previous); previous=list[i].allocation_identity;
        }
    }
    for(size_t i=0; i<view->allocation_count; i++) assert(seen[i]);
    assert(expected[index_of(view,nodes[5])]);
    assert(expected[index_of(view,nodes[7])]); /* default-vtable owner edge */
    assert(!expected[index_of(view,nodes[6])]); /* omitted legacy roots stay omitted */
    assert(expected[index_of(view,nodes[0])]==(scenario!=2));
    assert(expected[index_of(view,nodes[1])]==(scenario<2));
    assert(expected[index_of(view,nodes[3])]==(scenario==1 || scenario==3));
    assert(expected[index_of(view,nodes[4])]==(scenario==1 || scenario==3));
    free(seen); free(expected);
    StorageRequestV1 nested={1,sizeof(nested),NULL,NULL};
    assert(observe(&nested)==STORAGE_BUSY);
    return report_status;
}
static int trace_description(void *context, const StorageDescribedViewV1 *view) {
    (void)context;
    TraceRequestV1 request={1,sizeof(request),0,NULL,report};
    int before=report_calls;
    assert(trace(view,&request)==STORAGE_INCOMPLETE);
    assert(report_calls==before);
    request.accept_gaps=STORAGE_NATIVE_ROOTS|STORAGE_LEGACY_ROOTS|STORAGE_LEGACY_HOOKS;
    assert(trace(view,&request)==STORAGE_INCOMPLETE); /* missing owners are explicit */
    request.accept_gaps |= STORAGE_MISSING;
    if(missing_provider) return trace(view,&(TraceRequestV1){1,sizeof(request),0,NULL,report});
    if(!malformed_checks) {
        malformed_checks=1;
        StorageViewV1 bad_storage=*view->storage;
        StorageDescribedViewV1 bad=*view; bad.storage=&bad_storage;
        StorageEdgeV1 foreign={nodes[6],(Oop)(uintptr_t)8};
        bad_storage.edges=&foreign; bad_storage.edge_count=1;
        assert(trace(&bad,&request)==STORAGE_INVALID); /* even unreached source */
        bad_storage=*view->storage;
        bad.payload_edges=&foreign; bad.payload_edge_count=1;
        assert(trace(&bad,&request)==STORAGE_INVALID);
        bad=*view;
        Oop foreign_root=(Oop)(uintptr_t)8;
        bad.roots=&foreign_root; bad.root_count=1;
        assert(trace(&bad,&request)==STORAGE_INVALID);
        bad=*view; bad.storage=&bad_storage;
        StorageAllocationV1 *allocations=malloc(bad_storage.allocation_count*sizeof(*allocations)); assert(allocations);
        memcpy(allocations,bad_storage.allocations,bad_storage.allocation_count*sizeof(*allocations));
        bad_storage.allocations=allocations;
        allocations[1].identity=allocations[0].identity;
        assert(trace(&bad,&request)==STORAGE_INVALID);
        free(allocations);
        bad=*view; bad.coverage_gaps=0; bad.missing_description_count=0;
        TraceRequestV1 strict={1,sizeof(strict),0,NULL,report};
        assert(trace(&bad,&strict)==STORAGE_INCOMPLETE); /* cannot clear floor gaps */
        assert(report_calls==before);
    }
    return trace(view,&request);
}
static int inspect(void *context, const StorageViewV1 *view) {
    (void)context;
    assert(view->observation_identity>last_epoch); last_epoch=view->observation_identity;
    if(!runtime_identity) {
        runtime_identity=view->runtime_identity;
        for(size_t i=0; i<9; i++) identities[i]=view->allocations[index_of(view,nodes[i])].identity;
    }
    assert(view->runtime_identity==runtime_identity);
    StorageProviderV1 provider={NULL,provide};
    StorageDescribeRequestV1 request={1,sizeof(request),&provider,missing_provider?0:1,0,NULL,trace_description};
    return describe(view,&request);
}
int main(int argc, char **argv) {
    assert(argc==5);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); if(!floor) { fputs(dlerror(),stderr); abort(); }
    api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api);
    fixture_set_auto(0);
    observe=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1"); assert(observe);
    void *storage=api->dlopen_fn(argv[2],RTLD_NOW|RTLD_LOCAL); assert(storage);
    describe=(StorageDescribeV1)api->dlsym_fn(storage,"birnam_storage_describe_v1"); assert(describe);
    ((ModuleInitFn)api->dlsym_fn(storage,"module_init"))(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers());
    fixture_mount_integer(api, argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());
    IntegerApiV1 *integer=fixture_integer;
    pair_vt=api->proto(api->object_vt);
    for(size_t i=0; i<8; i++) if(i!=5) nodes[i]=api->alloc(pair_vt,(i==0||i==1||i==3||i==4)?2*sizeof(Oop):0);
    nodes[5]=integer->make(integer->context,(int64_t)(uintptr_t)nodes[3]);
    nodes[8]=api->proto(api->object_vt);
    ((Oop *)nodes[0])[0]=nodes[1]; ((Oop *)nodes[0])[1]=nodes[2];
    ((Oop *)nodes[1])[0]=nodes[0]; ((Oop *)nodes[1])[1]=nodes[2];
    ((Oop *)nodes[3])[0]=nodes[4]; ((Oop *)nodes[4])[0]=nodes[3];
    Oop owner_args[]={nodes[8],nodes[7]};
    api->send(api->vtable_vt,api->intern("ownerOf:put:"),owner_args,2);
    Oop reassociate[]={nodes[0],integer->behavior};
    api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),reassociate,2);
    fixture_register_root(legacy_roots);
    fixture_set_trace_hook(pair_vt,legacy_hook);
    fixture_set_finalizer(nodes[3],finalizer);
    Oop selector=api->intern("withStorageView:");
    original=(Method)api->send(api->vtable_vt,api->intern("lookup"),&selector,1); assert(original);
    api->method(api->vtable_vt,selector,wrapper);
    /* Load the tracer after the objects exist. It acquires no heap or Id state. */
    void *tracer=api->dlopen_fn(argv[4],RTLD_NOW|RTLD_LOCAL); assert(tracer);
    trace=(TraceDescribedV1)api->dlsym_fn(tracer,"birnam_trace_described_v1"); assert(trace);
    assert(!((ModuleInitFn)api->dlsym_fn(tracer,"module_init"))(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()));
    StorageRequestV1 request={1,sizeof(request),NULL,inspect};
    size_t count=api->gc_live_count();
    for(scenario=0; scenario<4; scenario++) {
        if(scenario==3) ((Oop *)nodes[0])[0]=nodes[3];
        assert(observe(&request)==STORAGE_OK);
        assert(api->gc_live_count()==count);
    }
    scenario=0; ((Oop *)nodes[0])[0]=nodes[1];
    report_status=-77;
    assert(observe(&request)==-77);
    assert(api->gc_live_count()==count);
    report_status=0; provider_status=-78;
    assert(observe(&request)==-78);
    provider_status=0; missing_provider=1;
    assert(observe(&request)==STORAGE_INCOMPLETE);
    missing_provider=0;
    assert(observe(&request)==STORAGE_OK);
    assert(api->gc_live_count()==count);
    assert(wrapper_calls==8 && report_calls==6);
    assert(!legacy_root_calls && !legacy_hook_calls);
    puts("storage-tracing: exact graph oracle, cycles, roots, raw bytes, reassociation, identities, gaps, malformed graphs and error cleanup passed");
    return 0;
}
