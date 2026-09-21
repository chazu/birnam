/* Real installed collection implementation with no Runtime and explicit roots. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/root-registry-owner-v1/root_registry_owner_v1.h"
#include "../../abi/fixtures/lifetime-v1/lifetime_v1.h"
#include "../../abi/fixtures/collection-state-v2/collection_state_v2.h"
#include <assert.h>
#include <string.h>
static BirnamProtocol id;
static CollectionStateV2 state;
static LifetimeOwnedV1 lifetime;
static void *kept,*dead,*weak,*behavior,*size_selector;
static size_t finalized;
static BirnamOutcome send(void *receiver,void *selector,const BirnamOperand *args,size_t count) {
    BirnamOutcome answer=id.message(id.context,receiver,selector,args,count);assert(answer.status==BIRNAM_LOADED);return answer;
}
static void *name(const char *text) {
    BirnamOperand arg={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    return send(id.receiver,id.intern,&arg,1).value.data.object;
}
static int allocated(void *object) {
    BirnamOperand argument={.kind=BIRNAM_OBJECT,.data.object=object};
    return send(id.receiver,size_selector,&argument,1).value.kind==BIRNAM_WORD;
}
static void provider(void (*mark)(void *)) { if(kept)mark(kept); }
static void finalize(void *object) {
    assert(object==kept || object==dead);
    assert(state.phase_factory->read(state.phase_context)==2);
    assert(state.records->get(state.records->context,(uintptr_t)object)==UINT32_MAX);
    assert(!allocated(object));
    assert(!lifetime.finalizer(lifetime.context,object).callback);
    CollectorInputV1 input={0};size_t untouched=99;
    assert(state.collector->collect(state.collector->context,&input,&untouched)==2 && untouched==99);
    finalized++;
}
static int reject(void *context,const TraceLiveSinkV1 *sink) { (void)context;(void)sink;return -77; }
static BirnamOutcome initialize(const BirnamProtocol *protocol,const BirnamOperand *args,size_t count) {
    assert(count==4);id=*protocol;state=*(CollectionStateV2 *)args[0].data.word;
    RootRegistryOwnedV1 roots=*(RootRegistryOwnedV1 *)args[1].data.word;
    lifetime=*(LifetimeOwnedV1 *)args[2].data.word;
    assert(state.version==2 && state.byte_size==sizeof(state));
    assert(state.collector->version==1 && state.collector->byte_size==sizeof(CollectorV1));
    size_selector=name("allocationSizeOf:");
    BirnamOutcome proto=send(id.receiver,name("objectPrototype"),NULL,0);
    void *base=send(id.receiver,name("behaviorOf:"),&proto.value,1).value.data.object;
    BirnamOperand zero={.kind=BIRNAM_WORD,.data.word=0};
    kept=send(base,name("allocate"),&zero,1).value.data.object;
    dead=send(base,name("allocate"),&zero,1).value.data.object;
    weak=name("earlyCollectorUnreferencedSymbol");
    behavior=send(base,name("delegated"),NULL,0).value.data.object;
    roots.api->register_root(roots.api->context,provider);
    finalizers_set_v1(lifetime.finalizers,kept,finalize);
    finalizers_set_v1(lifetime.finalizers,dead,finalize);
    assert(state.records->get(state.records->context,(uintptr_t)dead)==UINT32_MAX);
    state.settings->set_precise(state.settings->context,1);
    CollectorInputV1 input={0};size_t collected=99;
    assert(state.collector->collect(state.collector->context,NULL,&collected)==1 && collected==99);
    if(args[3].data.word) {
        input.roots=reject;
        assert(state.collector->collect(state.collector->context,&input,&collected)==-77);
        assert(collected==99 && !finalized && allocated(kept) && allocated(dead) && allocated(weak) && allocated(behavior));
        return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
    }
    assert(state.collector->collect(state.collector->context,&input,&collected)==0 && collected>=3);
    assert(finalized==1 && allocated(kept) && !allocated(dead) && !allocated(weak) && !allocated(behavior));
    assert(state.records->get(state.records->context,(uintptr_t)kept)==0);
    assert(state.phase_factory->read(state.phase_context)==0 && state.budget_factory->read(state.budget_context)==0);
    dead=kept;kept=NULL;collected=99;
    assert(state.collector->collect(state.collector->context,&input,&collected)==0 && collected>=1);
    assert(finalized==2 && !allocated(dead));
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
