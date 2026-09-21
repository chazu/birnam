/* Retaining only a private behavior must retain its owner through collection. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/root-registry-owner-v1/root_registry_owner_v1.h"
#include "../../abi/fixtures/object-model-owner-v1/object_model_owner_v1.h"
#include <assert.h>
#include <string.h>
static BirnamProtocol id;
static RootRegistryOwnedV1 roots;
static ObjectModelOwnedV1 model;
static void *object, *behavior, *selector;
static size_t token, identity;
static BirnamNativeEntry ensure_entry;
static BirnamOperand oop(void *value) { return (BirnamOperand){.kind=BIRNAM_OBJECT,.data.object=value}; }
static BirnamOutcome send(void *receiver, void *selector, const BirnamOperand *args, size_t count) {
    BirnamOutcome result=id.message(id.context,receiver,selector,args,count);
    assert(result.status==BIRNAM_LOADED); return result;
}
static void *name(const char *text) {
    BirnamOperand argument={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    return send(id.receiver,id.intern,&argument,1).value.data.object;
}
static BirnamNativeEntry lookup(void *receiver, void *selector) {
    BirnamOperand argument=oop(selector);
    BirnamOperand entry=send(receiver,name("lookup"),&argument,1).value;
    assert(entry.kind==BIRNAM_NATIVE_ENTRY); return entry.data.entry;
}
static BirnamOutcome echo(void *context, void *receiver, const BirnamOperand *args, size_t count) {
    (void)context;(void)args; assert(count==0);
    return (BirnamOutcome){.value=oop(receiver)};
}
static BirnamOutcome check(void *context, void *receiver, const BirnamOperand *args, size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0);
    BirnamOperand argument=oop(object);
    BirnamOperand current=send(id.receiver,name("identityOf:"),&argument,1).value;
    int found=0;
    for (size_t i=0;i<model.enumeration->count(model.enumeration->context);i++)
        if (model.enumeration->at(model.enumeration->context,i)==behavior) found=1;
    if (token) {
        assert(current.kind==BIRNAM_WORD && current.data.word==identity && found);
        assert(model.owner(model.context,behavior)==object);
        assert(send(object,selector,NULL,0).value.data.object==object);
        assert(!model.enumeration->selector_at(model.enumeration->context,behavior,0));
        assert(send(id.receiver,name("ensurePrivateVtableFor:"),&argument,1).value.data.object==behavior);
    } else {
        assert(current.kind==BIRNAM_UNIT && !found);
        assert(!model.owner(model.context,behavior));
    }
    BirnamNativeEntry current_ensure=lookup(id.receiver,name("ensurePrivateVtableFor:"));
    assert(current_ensure.method==ensure_entry.method && current_ensure.context==ensure_entry.context);
    return (BirnamOutcome){.value=current};
}
static BirnamOutcome release(void *context, void *receiver, const BirnamOperand *args, size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0 && token);
    assert(roots.release(roots.context,token)==1); token=0;
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
static BirnamOutcome initialize(const BirnamProtocol *protocol,const BirnamOperand *args,size_t count) {
    assert(count==3 && args[0].kind==BIRNAM_WORD && args[1].kind==BIRNAM_WORD && args[2].kind==BIRNAM_WORD);
    assert(!token);
    id=*protocol; roots=*(RootRegistryOwnedV1 *)args[0].data.word; model=*(ObjectModelOwnedV1 *)args[1].data.word;
    assert(model.version==1 && model.byte_size==sizeof(model));
    BirnamOutcome prototype=send(id.receiver,name("objectPrototype"),NULL,0);
    void *parent=send(id.receiver,name("behaviorOf:"),&prototype.value,1).value.data.object;
    BirnamOperand size={.kind=BIRNAM_WORD,.data.word=0};
    object=send(parent,name("allocate"),&size,1).value.data.object;
    /* Protect construction input on the second invocation with GC already active. */
    size_t input_root=roots.retain(roots.context,object); assert(input_root);
    BirnamOperand argument=oop(object);
    size_t before=model.enumeration->count(model.enumeration->context);
    behavior=send(id.receiver,name("ensurePrivateVtableFor:"),&argument,1).value.data.object;
    assert(behavior!=parent && model.owner(model.context,behavior)==object);
    assert(model.enumeration->count(model.enumeration->context)==before+1);
    assert(model.enumeration->at(model.enumeration->context,before)==behavior);
    assert(!model.enumeration->at(model.enumeration->context,(size_t)-1));
    assert(send(id.receiver,name("ensurePrivateVtableFor:"),&argument,1).value.data.object==behavior);
    assert(model.enumeration->count(model.enumeration->context)==before+1);
    selector=name("earlyPrivateEcho");
    BirnamOperand install[]={oop(selector),{.kind=BIRNAM_NATIVE_ENTRY,.data.entry={echo,NULL}}};
    /* Store backfill intentionally retains behaviors with direct methods.
       Put the probe on the shared parent so this private behavior has only
       the one explicit root and its owner edge under test. */
    send(parent,name("addMethod"),install,2);
    token=roots.retain(roots.context,behavior); assert(token);
    assert(roots.release(roots.context,input_root)==1);
    identity=send(id.receiver,name("identityOf:"),&argument,1).value.data.word;
    ensure_entry=lookup(id.receiver,name("ensurePrivateVtableFor:"));
    if (!args[2].data.word) {
        assert(id.message(id.context,id.receiver,name("ensurePrivateVtableFor:"),&size,1).status==BIRNAM_INVALID_REQUEST);
        BirnamOperand wrong[]={oop(object),oop(object)};
        assert(id.message(id.context,id.receiver,name("ownerOf:put:"),wrong,2).status==BIRNAM_INVALID_REQUEST);
        BirnamOperand query[]={size,oop(object)};
        query[0].data.word=(size_t)-1;
        assert(id.message(id.context,id.receiver,name("selectorAt:in:"),query,2).status==BIRNAM_INVALID_REQUEST);
        BirnamOperand change[]={oop(behavior),{.kind=BIRNAM_UNIT}};
        send(id.receiver,name("parentOf:put:"),change,2);
        assert(send(id.receiver,name("parentOf:"),change,1).value.kind==BIRNAM_UNIT);
        change[1]=oop(parent); send(id.receiver,name("parentOf:put:"),change,2);
    }
    const char *names[]={"earlyObjectModelCheck","releaseEarlyPrivateBehavior"};
    BirnamNativeMethod methods[]={check,release};
    for (size_t i=0;i<2;i++) {
        BirnamOperand method[]={oop(name(names[i])),{.kind=BIRNAM_NATIVE_ENTRY,.data.entry={methods[i],NULL}}};
        send(id.receiver,name("addMethod"),method,2);
    }
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=identity}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
