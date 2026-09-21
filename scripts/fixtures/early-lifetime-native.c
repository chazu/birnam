/* Real native records before Runtime; tracing/finalization after adoption. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/root-registry-owner-v1/root_registry_owner_v1.h"
#include "../../abi/fixtures/lifetime-v2/lifetime_v2.h"
#include <assert.h>
#include <string.h>
static BirnamProtocol id;
static RootRegistryOwnedV1 roots;
static LifetimeOwnedV2 lifetime;
static void *objects[5], *behaviors[3];
static size_t identities[5], token, finalized, traced;
static BirnamObject identity;
static BirnamOutcome send(void *receiver, void *selector, const BirnamOperand *args, size_t count) {
    BirnamOutcome answer=id.message(id.context,receiver,selector,args,count);
    assert(answer.status==BIRNAM_LOADED); return answer;
}
static void *name(const char *text) {
    BirnamOperand arg={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    BirnamOutcome answer=send(id.receiver,id.intern,&arg,1);
    assert(answer.value.kind==BIRNAM_OBJECT); return answer.value.data.object;
}
static void finalize_value(void *object) {
    size_t flag=*(size_t *)object;
    assert(flag==1 || flag==2);
    assert(!(finalized&flag)); finalized|=flag;
}
static void finalize_behavior(void *object) {
    assert(object==behaviors[0]);
    assert(!(finalized&4)); finalized|=4;
}
static void trace(void *object, void (*mark)(void *)) {
    assert(object==objects[2]); traced++;
    mark(*(void **)object);
}
static void *allocate(void *behavior, size_t bytes) {
    BirnamOperand size={.kind=BIRNAM_WORD,.data.word=bytes};
    BirnamOutcome answer=send(behavior,name("allocate"),&size,1);
    assert(answer.value.kind==BIRNAM_OBJECT); return answer.value.data.object;
}
static BirnamOutcome check(void *context, void *receiver, const BirnamOperand *args, size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0);
    for (size_t i=0;i<5;i++) {
        BirnamOperand object={.kind=BIRNAM_OBJECT,.data.object=objects[i]};
        BirnamOutcome answer=send(id.receiver,identity,&object,1);
        if (finalized==0) { assert(answer.value.kind==BIRNAM_WORD && answer.value.data.word==identities[i]); }
        else { assert(answer.value.kind==BIRNAM_UNIT); }
    }
    if (finalized) {
        assert(finalized==7);
        for (size_t i=0;i<3;i++) {
            assert(!lifetime.layout(lifetime.context,behaviors[i]).is_set);
            assert(!lifetime.hook(lifetime.context,behaviors[i]).hook);
        }
        assert(!lifetime.finalizer(lifetime.context,objects[1]).callback);
        assert(!lifetime.finalizer(lifetime.context,objects[3]).callback);
        assert(!lifetime.finalizer(lifetime.context,behaviors[0]).callback);
    } else { assert(traced>0); }
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=finalized}};
}
static BirnamOutcome release_root(void *context, void *receiver, const BirnamOperand *args, size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0);
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=(size_t)roots.release(roots.context,token)}};
}
static BirnamOutcome initialize(const BirnamProtocol *protocol,const BirnamOperand *args,size_t count) {
    assert(count==2 && args[0].kind==BIRNAM_WORD && args[1].kind==BIRNAM_WORD);
    id=*protocol; roots=*(RootRegistryOwnedV1 *)args[0].data.word;
    lifetime=*(LifetimeOwnedV2 *)args[1].data.word;
    assert(lifetime.version==2 && lifetime.byte_size==sizeof(lifetime));
    identity=name("identityOf:");
    BirnamOutcome prototype=send(id.receiver,name("objectPrototype"),NULL,0);
    void *parent=send(id.receiver,name("behaviorOf:"),&prototype.value,1).value.data.object;
    for (size_t i=0;i<3;i++) behaviors[i]=send(parent,name("delegated"),NULL,0).value.data.object;
    uint8_t fields[]={1,1,1};
    payload_layout_set_v1(lifetime.layouts,behaviors[0],fields,3,0);
    /* Replacement can borrow the old description's bytes. */
    PayloadLayoutViewV1 old=lifetime.layout(lifetime.context,behaviors[0]);
    payload_layout_set_v1(lifetime.layouts,behaviors[0],old.kinds,old.count,0);
    assert(payload_layout_slot_v1(lifetime.layouts,behaviors[0],2)==1);
    payload_layout_set_v1(lifetime.layouts,behaviors[1],NULL,0,0);
    trace_hooks_set_v1(lifetime.hooks,behaviors[2],trace);
    finalizers_set_v1(lifetime.finalizers,behaviors[0],finalize_behavior);
    /* The only strong native reference is registered before state publication. */
    void *root=allocate(behaviors[0],3*sizeof(void *));
    token=roots.retain(roots.context,root); assert(token); objects[0]=root;
    objects[1]=allocate(behaviors[1],sizeof(size_t)); *(size_t *)objects[1]=1;
    objects[2]=allocate(behaviors[2],sizeof(void *));
    objects[3]=allocate(behaviors[1],sizeof(size_t)); *(size_t *)objects[3]=2;
    objects[4]=allocate(behaviors[1],sizeof(size_t)); *(size_t *)objects[4]=83;
    ((void **)root)[0]=objects[1]; ((void **)root)[1]=objects[2]; ((void **)root)[2]=objects[4];
    *(void **)objects[2]=objects[3];
    finalizers_set_v1(lifetime.finalizers,objects[1],finalize_value);
    finalizers_set_v1(lifetime.finalizers,objects[3],finalize_value);
    finalizers_set_v1(lifetime.finalizers,NULL,finalize_value); /* Missing object is ignored. */
    for (size_t i=0;i<5;i++) {
        BirnamOperand object={.kind=BIRNAM_OBJECT,.data.object=objects[i]};
        identities[i]=send(id.receiver,identity,&object,1).value.data.word;
    }
    assert(lifetime.finalizer(lifetime.context,objects[1]).callback==finalize_value);
    const char *names[]={"earlyLifetimeCheck","releaseEarlyLifetime"};
    BirnamNativeMethod methods[]={check,release_root};
    for (size_t i=0;i<2;i++) {
        BirnamOperand install[]={ {.kind=BIRNAM_OBJECT,.data.object=name(names[i])},
            {.kind=BIRNAM_NATIVE_ENTRY,.data.entry={methods[i],NULL}} };
        send(id.receiver,name("addMethod"),install,2);
    }
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=token}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
