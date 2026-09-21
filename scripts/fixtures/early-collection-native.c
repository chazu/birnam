/* Native collection records/control ownership before Runtime and after adoption. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/root-registry-owner-v1/root_registry_owner_v1.h"
#include "../../abi/fixtures/lifetime-v2/lifetime_v2.h"
#include "../../abi/fixtures/collection-state-v2/collection_state_v2.h"
#include <assert.h>
#include <string.h>
static BirnamProtocol id;
static RootRegistryOwnedV1 roots;
static LifetimeOwnedV2 lifetime;
static CollectionStateV2 owner;
static CollectionRecordsV1 records;
static void *kept,*late;
static size_t token,finalized;
static BirnamOperand oop(void *object) { return (BirnamOperand){.kind=BIRNAM_OBJECT,.data.object=object}; }
static BirnamOutcome send(void *receiver,void *selector,const BirnamOperand *args,size_t count) {
    BirnamOutcome answer=id.message(id.context,receiver,selector,args,count);assert(answer.status==BIRNAM_LOADED);return answer;
}
static void *name(const char *text) {
    BirnamOperand arg={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    return send(id.receiver,id.intern,&arg,1).value.data.object;
}
static BirnamOutcome allocation(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)receiver;assert(count==3 && args[0].kind==BIRNAM_OBJECT && args[1].kind==BIRNAM_WORD);
    assert(records.get(records.context,(uintptr_t)args[0].data.object)==0);
    *(size_t *)context+=(sizeof(void *)+args[1].data.word+7)&~(size_t)7;
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
static void finalize(void *object) {
    assert(object==kept || object==late);
    assert(owner.phase_factory->read(owner.phase_context)==2);
    assert(records.get(records.context,(uintptr_t)object)==UINT32_MAX);
    finalized|=object==kept ? 1u : 2u;
}
static BirnamOutcome check(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args;assert(count==0);
    assert(owner.phase_factory->read(owner.phase_context)==0);
    assert(owner.stack_factory->get(owner.stack)!=0);
    assert(owner.settings_factory->precise(owner.settings)==1);
    assert(records.get(records.context,(uintptr_t)late)==UINT32_MAX);
    if(token) { assert(finalized==2);assert(records.get(records.context,(uintptr_t)kept)==0); }
    else { assert(finalized==3);assert(records.get(records.context,(uintptr_t)kept)==UINT32_MAX); }
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=finalized}};
}
static BirnamOutcome adopted(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args;assert(count==0);
    assert(records.get(records.context,(uintptr_t)kept)==1);
    assert(records.get(records.context,(uintptr_t)late)==0);
    assert(owner.settings_factory->precise(owner.settings)==1);
    assert(!owner.settings_factory->automatic_due(owner.settings,1,SIZE_MAX));
    BirnamOutcome proto=send(id.receiver,name("objectPrototype"),NULL,0);
    void *vt=send(id.receiver,name("behaviorOf:"),&proto.value,1).value.data.object;
    size_t before=owner.budget_factory->read(owner.budget_context);
    BirnamOperand size={.kind=BIRNAM_WORD,.data.word=0};
    void *fresh=send(vt,name("allocate"),&size,1).value.data.object;
    assert(records.get(records.context,(uintptr_t)fresh)==0);
    assert(owner.budget_factory->read(owner.budget_context)==before+sizeof(void *));
    owner.settings->set_auto(owner.settings->context,1);
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=1}};
}
static BirnamOutcome release(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args;assert(count==0 && token);
    assert(roots.release(roots.context,token)==1);token=0;
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
static void captured_before_runtime(void *context,uintptr_t start,uintptr_t bottom) {
    size_t local=0;assert((uintptr_t)&local<start && start<bottom);
    assert(bottom==*(uintptr_t *)context);
    *(uintptr_t *)context=1;
}
static BirnamOutcome initialize(const BirnamProtocol *protocol,const BirnamOperand *args,size_t count) {
    assert(count==3);id=*protocol;owner=*(CollectionStateV2 *)args[0].data.word;
    roots=*(RootRegistryOwnedV1 *)args[1].data.word;lifetime=*(LifetimeOwnedV2 *)args[2].data.word;
    assert(owner.version==2 && owner.byte_size==sizeof(owner));records=*owner.records;
    size_t native_anchor=0;uintptr_t captured=(uintptr_t)&native_anchor;
    assert(owner.stack_factory->capture((uintptr_t)&native_anchor,&captured,captured_before_runtime)==1);
    assert(captured==1);
    size_t rejected_count=99;
    assert(owner.collect(NULL,&rejected_count)==1 && rejected_count==99);
    size_t expected=0;
    BirnamOperand visitor={.kind=BIRNAM_NATIVE_ENTRY,.data.entry={allocation,&expected}};
    send(id.receiver,name("allocationsDo:"),&visitor,1);
    assert(owner.budget_factory->read(owner.budget_context)==expected && expected>0);
    CollectionRecordsViewV1 view=records.snapshot(records.context);assert(view.count>0);
    for(size_t i=0;i<view.count;i++) assert(view.entries[i].object && view.entries[i].flags==0 && view.entries[i].reserved==0);
    assert(records.get(records.context,1)==UINT32_MAX && !records.mark(records.context,1));
    assert(!records.remove_dead(records.context,1));
    assert(owner.phase_factory->read(owner.phase_context)==0);
    assert(owner.phase_factory->set(owner.phase_context,1)==1 && owner.phase_factory->read(owner.phase_context)==1);
    assert(owner.phase_factory->set(owner.phase_context,3)==0);
    assert(owner.phase_factory->set(owner.phase_context,0)==1);
    size_t anchor=0;owner.stack->set(owner.stack->context,(uintptr_t)&anchor);
    assert(owner.stack_factory->get(owner.stack)==(uintptr_t)&anchor);owner.stack->set(owner.stack->context,0);
    owner.settings->set_precise(owner.settings->context,1);
    owner.settings->set_auto(owner.settings->context,0);
    assert(owner.settings_factory->precise(owner.settings)==1);
    assert(!owner.settings_factory->automatic_due(owner.settings,1,SIZE_MAX));
    BirnamOutcome proto=send(id.receiver,name("objectPrototype"),NULL,0);
    void *vt=send(id.receiver,name("behaviorOf:"),&proto.value,1).value.data.object;
    assert(records.retain(records.context,(uintptr_t)proto.value.data.object)==1);
    assert(records.get(records.context,(uintptr_t)proto.value.data.object)==2);
    assert(!records.mark(records.context,(uintptr_t)proto.value.data.object));
    assert(!records.remove_dead(records.context,(uintptr_t)proto.value.data.object));
    BirnamOperand size={.kind=BIRNAM_WORD,.data.word=0};
    kept=send(vt,name("allocate"),&size,1).value.data.object;
    assert(records.insert(records.context,kept,0)==1);
    assert(records.insert(records.context,kept,0)==0);
    assert(owner.budget_factory->charge(owner.budget_context,sizeof(void *))==1);
    assert(records.mark(records.context,(uintptr_t)kept)==1);
    assert(records.mark(records.context,(uintptr_t)kept)==0);
    assert(!records.remove_dead(records.context,(uintptr_t)kept));
    assert(records.get(records.context,(uintptr_t)kept)==1);
    records.clear(records.context);
    assert(records.get(records.context,(uintptr_t)kept)==0);
    assert(records.get(records.context,(uintptr_t)proto.value.data.object)==2);
    assert(records.mark(records.context,(uintptr_t)kept)==1);
    token=roots.retain(roots.context,kept);
    late=send(vt,name("allocate"),&size,1).value.data.object;
    assert(records.get(records.context,(uintptr_t)late)==UINT32_MAX); /* Adoption must add this. */
    finalizers_set_v1(lifetime.finalizers,kept,finalize);finalizers_set_v1(lifetime.finalizers,late,finalize);
    const char *names[]={"earlyCollectionCheck","releaseEarlyCollection","earlyCollectionAdopted"};
    BirnamNativeMethod methods[]={check,release,adopted};
    for(size_t i=0;i<3;i++) {
        BirnamOperand binding[]={oop(name(names[i])),{.kind=BIRNAM_NATIVE_ENTRY,.data.entry={methods[i],NULL}}};
        send(id.receiver,name("addMethod"),binding,2);
    }
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=1}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
