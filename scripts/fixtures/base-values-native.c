/* Independently consume actual base entries before their successors exist. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/base-values-v1/base_values_v1.h"
#include "../../abi/fixtures/lifetime-v1/lifetime_v1.h"
#include <assert.h>
#include <string.h>
static BirnamProtocol id;
static BaseValuesV1 values;
static LifetimeOwnedV1 lifetime;
static void *receivers[8];
static const char *selectors[]={"==","not","&","|","isNil","notNil","isNil","notNil"};
static BirnamNativeEntry entries[8];
static BirnamOperand object(void *value) { return (BirnamOperand){.kind=BIRNAM_OBJECT,.data.object=value}; }
static BirnamOutcome send(void *receiver, void *selector, const BirnamOperand *args, size_t count) {
    BirnamOutcome result=id.message(id.context,receiver,selector,args,count);
    assert(result.status==BIRNAM_LOADED); return result;
}
static void *name(const char *text) {
    BirnamOperand argument={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    BirnamOutcome result=send(id.receiver,id.intern,&argument,1);
    assert(result.value.kind==BIRNAM_OBJECT); return result.value.data.object;
}
static void *behavior(void *value) {
    BirnamOperand argument=object(value);
    return send(id.receiver,name("behaviorOf:"),&argument,1).value.data.object;
}
static BirnamOperand lookup(void *receiver, const char *selector) {
    BirnamOperand argument=object(name(selector));
    return send(behavior(receiver),name("lookup"),&argument,1).value;
}
static void expect(void *receiver, const char *selector, const BirnamOperand *args, size_t count, void *expected) {
    BirnamOutcome result=send(receiver,name(selector),args,count);
    assert(result.value.kind==BIRNAM_OBJECT && result.value.data.object==expected);
}
static void operations(void) {
    expect(values.true_object,"not",NULL,0,values.false_object);
    expect(values.false_object,"not",NULL,0,values.true_object);
    BirnamOperand nil=object(values.nil), truth=object(values.true_object);
    expect(values.true_object,"&",&nil,1,values.nil);
    expect(values.false_object,"&",&nil,1,values.false_object);
    expect(values.true_object,"|",&nil,1,values.true_object);
    expect(values.false_object,"|",&nil,1,values.nil);
    expect(values.nil,"==",&nil,1,values.true_object);
    expect(values.nil,"==",&truth,1,values.false_object);
    expect(values.nil,"isNil",NULL,0,values.true_object);
    expect(values.nil,"notNil",NULL,0,values.false_object);
    expect(values.true_object,"isNil",NULL,0,values.false_object);
    expect(values.true_object,"notNil",NULL,0,values.true_object);
}
static BirnamOutcome check(void *context, void *receiver, const BirnamOperand *args, size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0);
    operations();
    assert(behavior(values.nil)==values.nil_behavior);
    assert(behavior(values.true_object)==values.boolean_behavior);
    for (size_t i=0;i<8;i++) {
        BirnamOperand found=lookup(receivers[i],selectors[i]);
        assert(found.kind==BIRNAM_NATIVE_ENTRY && found.data.entry.method==entries[i].method
            && found.data.entry.context==entries[i].context);
    }
    assert(lifetime.layout(lifetime.context,values.nil_behavior).count==1);
    assert(lookup(values.nil,"printString").kind==BIRNAM_NATIVE_ENTRY);
    assert(lookup(values.true_object,"ifTrue:").kind==BIRNAM_NATIVE_ENTRY);
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=1}};
}
static BirnamOutcome replacement(void *context, void *receiver, const BirnamOperand *args, size_t count) {
    (void)receiver;(void)args; assert(count==0 && context==&values);
    return (BirnamOutcome){.value=object(values.false_object)};
}
static BirnamOutcome initialize(const BirnamProtocol *protocol, const BirnamOperand *args, size_t count) {
    assert(count==2 && args[0].kind==BIRNAM_WORD && args[1].kind==BIRNAM_WORD);
    id=*protocol; values=*(BaseValuesV1 *)args[0].data.word; lifetime=*(LifetimeOwnedV1 *)args[1].data.word;
    assert(values.version==1 && values.byte_size==sizeof(values));
    assert(values.nil!=values.true_object && values.nil!=values.false_object && values.true_object!=values.false_object);
    void *objects[]={values.nil,values.true_object,values.false_object};
    for (size_t i=0;i<3;i++) {
        BirnamOperand argument=object(objects[i]);
        BirnamOperand size=send(id.receiver,name("allocationSizeOf:"),&argument,1).value;
        assert(size.kind==BIRNAM_WORD && size.data.word==0);
        assert(lookup(objects[i],"printString").kind==BIRNAM_UNIT);
    }
    assert(lookup(values.true_object,"ifTrue:").kind==BIRNAM_UNIT);
    assert(lookup(values.true_object,"and:").kind==BIRNAM_UNIT);
    operations();
    /* Nil predicates belong to its behavior, including other Nil instances. */
    BirnamOperand size={.kind=BIRNAM_WORD,.data.word=0};
    void *another_nil=send(values.nil_behavior,name("allocate"),&size,1).value.data.object;
    expect(another_nil,"isNil",NULL,0,values.true_object);
    expect(another_nil,"notNil",NULL,0,values.false_object);
    assert(id.message(id.context,values.nil,name("isNil"),&size,1).status==BIRNAM_INVALID_REQUEST);
    assert(id.message(id.context,values.true_object,name("&"),&size,1).status==BIRNAM_INVALID_REQUEST);
    /* A complete earlier declaration must survive upper construction. */
    uint8_t raw=0;
    payload_layout_set_v1(lifetime.layouts,values.nil_behavior,&raw,1,0);
    BirnamOperand replace[]={object(name("notNil")),{.kind=BIRNAM_NATIVE_ENTRY,.data.entry={replacement,&values}}};
    send(values.nil_behavior,name("addMethod"),replace,2);
    void *prototype=send(id.receiver,name("objectPrototype"),NULL,0).value.data.object;
    void *rs[]={prototype,values.true_object,values.true_object,values.true_object,prototype,prototype,values.nil,values.nil};
    for (size_t i=0;i<8;i++) {
        receivers[i]=rs[i]; BirnamOperand entry=lookup(rs[i],selectors[i]);
        assert(entry.kind==BIRNAM_NATIVE_ENTRY && entry.data.entry.method);
        entries[i]=entry.data.entry;
    }
    BirnamOperand install[]={object(name("checkBaseValues")),{.kind=BIRNAM_NATIVE_ENTRY,.data.entry={check,NULL}}};
    send(id.receiver,name("addMethod"),install,2);
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
