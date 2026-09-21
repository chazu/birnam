/* Independent C client of the actual native graph before Runtime and after GC. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/root-registry-owner-v1/root_registry_owner_v1.h"
#include "../../abi/fixtures/lifetime-v1/lifetime_v1.h"
#include "../../abi/fixtures/object-model-owner-v1/object_model_owner_v1.h"
#include "../../abi/fixtures/trace-graph-v2/trace_graph_v2.h"
#include <assert.h>
#include <string.h>
static BirnamProtocol id;
static RootRegistryOwnedV1 roots;
static LifetimeOwnedV1 lifetime;
static ObjectModelOwnedV1 model;
static TraceGraphV2 graph;
static void *objects[5], *behavior, *parent;
static size_t hooks, providers, finalizations;
static int keep_raw;
static BirnamOperand oop(void *p) { return (BirnamOperand){.kind=BIRNAM_OBJECT,.data.object=p}; }
static BirnamOutcome send(void *receiver,void *selector,const BirnamOperand *args,size_t count) {
    BirnamOutcome result=id.message(id.context,receiver,selector,args,count);
    assert(result.status==BIRNAM_LOADED); return result;
}
static void *name(const char *text) {
    BirnamOperand arg={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    return send(id.receiver,id.intern,&arg,1).value.data.object;
}
static void *allocate(void *vt,size_t bytes) {
    BirnamOperand arg={.kind=BIRNAM_WORD,.data.word=bytes};
    return send(vt,name("allocate"),&arg,1).value.data.object;
}
static unsigned bit(void *p) {
    for (unsigned i=0;i<5;i++) if (p==objects[i]) return 1u<<i;
    return p==behavior ? 32u : p==parent ? 64u : 0;
}
static int emit(void *context,uintptr_t address) { *(unsigned *)context |= bit((void *)address); return 0; }
static int reject(void *context,uintptr_t address) { (void)context;(void)address;return -77; }
static void hook(void *object,void (*mark)(void *)) { assert(object==objects[0]);hooks++;mark(objects[3]); }
static void replacement_hook(void *object,void (*mark)(void *)) { assert(object==objects[0]);mark(objects[2]); }
static BirnamOutcome forbidden_fact(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args;(void)count;
    assert(!"graph performed application lookup after construction");
    return (BirnamOutcome){0};
}
static void provider(void (*mark)(void *)) { providers++;mark(objects[0]);if(keep_raw) mark(objects[2]); }
static void finalizer(void *object) { assert(object==objects[1]);finalizations++; }
static int source_roots(void *context,const TraceLiveSinkV1 *sink) { (void)context;return graph.roots(graph.context,sink); }
static int admit(void *context,uintptr_t address) {
    unsigned b=bit((void *)address),*marked=context;
    if (!b || (*marked & b)) return 0;
    *marked |= b;return 1;
}
static int source_edges(void *context,uintptr_t address,const TraceLiveSinkV1 *sink) {
    (void)context;return graph.edges(graph.context,(void *)address,TRACE_GRAPH_PRECISE,sink);
}
static int inspect(void *context,const TraceSnapshotV1 *snapshot) {
    assert(context==&graph);
    unsigned found=0, core_edges=0; uint64_t previous=0;
    for(size_t i=0;i<snapshot->allocation_count;i++) {
        const StorageAllocationV1 *entry=&snapshot->allocations[i];
        assert(entry->identity>previous);previous=entry->identity;
        assert(entry->padded_size==((entry->requested_size+7)&~(size_t)7));
        assert(entry->allocation_bytes==entry->padded_size+sizeof(void *));
        assert(!(entry->flags&STORAGE_PERMANENT)); /* upper collection policy */
        found|=bit(entry->object);
        if(entry->object==objects[0]) {
            assert(entry->requested_size==2*sizeof(void *) && entry->behavior==behavior);
            assert(entry->flags&STORAGE_HAS_GC_LAYOUT);
            assert(!(entry->flags&STORAGE_CORE_PAYLOAD));
            assert(entry->trace_hook==hook);
        }
        if(entry->object==behavior) assert(entry->flags&STORAGE_CORE_PAYLOAD);
        if(entry->object==objects[1]) assert(entry->flags&STORAGE_HAS_FINALIZER);
    }
    assert(found==127);
    for(size_t i=0;i<snapshot->edge_count;i++) {
        StorageEdgeV1 edge=snapshot->edges[i];
        if(edge.from==objects[0]) assert(edge.to==behavior); /* never read payload */
        if(edge.from==behavior) core_edges|=bit(edge.to);
    }
    assert((core_edges&(64|16))==(64|16));
    int id_root=0, intern_symbol=0;
    for(size_t i=0;i<snapshot->root_count;i++) id_root|=snapshot->roots[i]==id.receiver;
    for(size_t i=0;i<snapshot->symbol_count;i++) intern_symbol|=snapshot->symbols[i]==id.intern;
    assert(id_root && intern_symbol);
    return 0;
}
static int inspect_failure(void *context,const TraceSnapshotV1 *snapshot) {
    assert(inspect(context,snapshot)==0);return -79;
}
static void check_inspection(void) {
    size_t before_hooks=hooks,before_providers=providers,before_finalizations=finalizations;
    assert(graph.inspect(graph.context,&graph,NULL)==STORAGE_INVALID);
    assert(graph.inspect(graph.context,&graph,inspect)==0);
    assert(graph.inspect(graph.context,&graph,inspect_failure)==-79);
    assert(graph.inspect(graph.context,&graph,inspect)==0);
    assert(hooks==before_hooks && providers==before_providers && finalizations==before_finalizations);
}
static void check_edges(void) {
    unsigned seen=0; TraceLiveSinkV1 sink={&seen,emit};
    size_t before=hooks;
    assert(graph.edges(graph.context,objects[0],TRACE_GRAPH_PRECISE,&sink)==0);
    assert(seen==(32|2|8) && hooks==before+1);
    seen=0;assert(graph.edges(graph.context,objects[0],0,&sink)==0);
    assert(seen==(32|2|4|8));
    seen=0;before=hooks;assert(graph.edges(graph.context,objects[0],TRACE_GRAPH_CORE_ONLY,&sink)==0);
    assert(seen==32 && hooks==before);
    seen=0;assert(graph.edges(graph.context,behavior,TRACE_GRAPH_CORE_ONLY,&sink)==0);
    assert((seen&(64|16))==(64|16));
    TraceLiveSinkV1 rejected={NULL,reject};
    assert(graph.edges(graph.context,objects[0],0,&rejected)==-77);
    assert(graph.edges(graph.context,objects[0],8,&sink)!=0);
    assert(graph.edges(graph.context,(void *)(uintptr_t)1,0,&sink)!=0);
}
static BirnamOutcome check(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args;assert(count==0);
    check_edges();
    check_inspection();
    assert(hooks>0 && providers>0);
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=1}};
}
static BirnamOutcome initialize(const BirnamProtocol *protocol,const BirnamOperand *args,size_t count) {
    assert(count==5);id=*protocol;
    graph=*(TraceGraphV2 *)args[0].data.word;roots=*(RootRegistryOwnedV1 *)args[1].data.word;
    lifetime=*(LifetimeOwnedV1 *)args[2].data.word;model=*(ObjectModelOwnedV1 *)args[3].data.word;
    assert(graph.version==2 && graph.byte_size==sizeof(graph));
    if(args[4].data.word) assert(graph.validate(graph.context)==0);
    BirnamOutcome proto=send(id.receiver,name("objectPrototype"),NULL,0);
    void *base=send(id.receiver,name("behaviorOf:"),&proto.value,1).value.data.object;
    parent=send(base,name("delegated"),NULL,0).value.data.object;
    behavior=send(parent,name("delegated"),NULL,0).value.data.object;
    objects[0]=allocate(behavior,2*sizeof(void *));
    for(size_t i=1;i<5;i++) objects[i]=allocate(base,0);
    ((void **)objects[0])[0]=objects[1];((void **)objects[0])[1]=objects[2];
    /* Reject missing descriptions even though collection itself is still off. */
    assert(graph.validate(graph.context)==3); /* INCOMPLETE */
    uint8_t fields[]={1,0};payload_layout_set_v1(lifetime.layouts,behavior,fields,2,0);
    trace_hooks_set_v1(lifetime.hooks,parent,hook); /* inherited through live parent */
    model.set_owner(model.context,behavior,objects[4]);
    if(args[4].data.word) assert(graph.validate(graph.context)==0);
    root_registry_register_v1(roots.api,provider);
    finalizers_set_v1(lifetime.finalizers,objects[1],finalizer);
    void *references=name("referencesOf:do:");
    BirnamOperand reference_arg=oop(references);
    BirnamOperand original=send(id.receiver,name("lookup"),&reference_arg,1).value;
    assert(original.kind==BIRNAM_NATIVE_ENTRY);
    BirnamOperand replacement[]={oop(references),{.kind=BIRNAM_NATIVE_ENTRY,.data.entry={forbidden_fact,NULL}}};
    send(id.receiver,name("addMethod"),replacement,2);
    check_edges(); /* Selected Id facts survive later method replacement. */
    check_inspection();
    unsigned reached=0;
    TraceLiveSourceV1 source={1,sizeof(source),&reached,source_roots,admit,source_edges};
    assert(graph.trace(&source)==0);
    assert(reached==(1|2|8|16|32|64));
    /* A changed description is visible on the next edge query. */
    uint8_t changed[]={0,1};payload_layout_set_v1(lifetime.layouts,behavior,changed,2,0);
    unsigned seen=0;TraceLiveSinkV1 sink={&seen,emit};
    assert(graph.edges(graph.context,objects[0],TRACE_GRAPH_PRECISE,&sink)==0 && seen==(32|4|8));
    payload_layout_set_v1(lifetime.layouts,behavior,fields,2,0);
    trace_hooks_set_v1(lifetime.hooks,behavior,replacement_hook);
    seen=0;assert(graph.edges(graph.context,objects[0],TRACE_GRAPH_PRECISE,&sink)==0 && seen==(32|2|4));
    lifetime.replace_hook(lifetime.context,behavior,(TraceHookSlotV1){0});
    check_edges();
    replacement[1]=original;send(id.receiver,name("addMethod"),replacement,2);
    keep_raw=1; /* Keep all fixture objects available for the post-adoption check. */
    BirnamOperand binding[]={oop(name("earlyTraceCheck")),{.kind=BIRNAM_NATIVE_ENTRY,.data.entry={check,NULL}}};
    send(id.receiver,name("addMethod"),binding,2);
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
__attribute__((visibility("default"))) const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
