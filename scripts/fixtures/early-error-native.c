/* The actual Error library constructs and propagates values before numbers. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/root-registry-owner-v1/root_registry_owner_v1.h"
#include "../../abi/fixtures/lifetime-v1/lifetime_v1.h"
#include "../../abi/fixtures/namespace-owner-v1/namespace_owner_v1.h"
#include "../../abi/fixtures/base-values-v1/base_values_v1.h"
typedef void (*Finalizer)(void *);
typedef struct IdApi IdApi;
#include "../../abi/fixtures/string-v1/string_v1.h"
#include <assert.h>
#include <string.h>
static BirnamProtocol id;
static RootRegistryOwnedV1 roots;
static LifetimeOwnedV1 lifetime;
static NamespaceOwnedV1 namespace;
static BaseValuesV1 values;
static StringApiV1 *string;
static void *factory, *behavior, *error, *kind, *message;
static size_t token, identity, calls;
static void *small_result;
static BirnamNativeEntry original_kind, kind_override, object_dnu;
static BirnamOperand oop(void *value) { return (BirnamOperand){.kind=BIRNAM_OBJECT,.data.object=value}; }
static BirnamOutcome send(void *receiver, void *selector, const BirnamOperand *args, size_t count) {
    BirnamOutcome result=id.message(id.context,receiver,selector,args,count);
    assert(result.status==BIRNAM_LOADED); return result;
}
static void *name(const char *text) {
    BirnamOperand arg={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    return send(id.receiver,id.intern,&arg,1).value.data.object;
}
static BirnamNativeEntry lookup(void *receiver,const char *text) {
    BirnamOperand arg=oop(name(text)), found=send(receiver,name("lookup"),&arg,1).value;
    assert(found.kind==BIRNAM_NATIVE_ENTRY); return found.data.entry;
}
static void install(void *target,const char *text,BirnamNativeEntry entry) {
    BirnamOperand args[]={oop(name(text)),{.kind=BIRNAM_NATIVE_ENTRY,.data.entry=entry}};
    send(target,name("addMethod"),args,2);
}
static void *binding(const char *text) {
    BirnamOperand arg=oop(name(text)), result=send(namespace.receiver,name("bindingAt:"),&arg,1).value;
    return result.kind==BIRNAM_OBJECT?result.data.object:NULL;
}
static void *field(void *object,const char *text) { return send(object,name(text),NULL,0).value.data.object; }
static BirnamOutcome counted_kind(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    assert(context==&calls); calls++;
    return original_kind.method(original_kind.context,receiver,args,count);
}
static BirnamOutcome small_allocation(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==1);
    return (BirnamOutcome){.value=oop(small_result)};
}
static BirnamOutcome check(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0);
    assert(binding("Error")==factory && binding("ErrorVt")==behavior);
    BirnamOperand arg=oop(error), current=send(id.receiver,name("identityOf:"),&arg,1).value;
    size_t length=0;
    if (token) {
        assert(current.kind==BIRNAM_WORD && current.data.word==identity);
        assert(field(error,"kind")==kind && field(error,"message")==message);
        const uint8_t *bytes=string->storage->borrow(string->context,message,&length);
        const uint8_t expected[]={'f','a','u','l','t',0,255};
        assert(bytes && length==sizeof(expected) && !memcmp(bytes,expected,length));
    } else {
        assert(current.kind==BIRNAM_UNIT);
        arg=oop(kind); assert(send(id.receiver,name("identityOf:"),&arg,1).value.kind==BIRNAM_UNIT);
        arg=oop(message); assert(send(id.receiver,name("identityOf:"),&arg,1).value.kind==BIRNAM_UNIT);
        assert(!string->storage->borrow(string->context,message,&length));
    }
    PayloadLayoutViewV1 layout=lifetime.layout(lifetime.context,behavior);
    assert(layout.is_set && layout.count==2 && layout.rest==0 && layout.kinds[0]==1 && layout.kinds[1]==1);
    BirnamNativeEntry selected=lookup(binding("ErrorFactoryVt"),"kind:");
    assert(selected.method==kind_override.method && selected.context==kind_override.context);
    void *object=send(id.receiver,name("objectPrototype"),NULL,0).value.data.object;
    BirnamOperand object_arg=oop(object);
    void *object_behavior=send(id.receiver,name("behaviorOf:"),&object_arg,1).value.data.object;
    selected=lookup(object_behavior,"doesNotUnderstand:");
    assert(selected.method==object_dnu.method && selected.context==object_dnu.context);
    return (BirnamOutcome){.value=current};
}
static BirnamOutcome release(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0 && token);
    assert(roots.release(roots.context,token)==1); token=0;
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
static BirnamOutcome initialize(const BirnamProtocol *protocol,const BirnamOperand *args,size_t count) {
    assert(count==7 && !token);
    for(size_t i=0;i<5;i++) assert(args[i].kind==BIRNAM_WORD);
    assert(args[5].kind==BIRNAM_OBJECT && args[6].kind==BIRNAM_WORD);
    id=*protocol; roots=*(RootRegistryOwnedV1 *)args[0].data.word;
    lifetime=*(LifetimeOwnedV1 *)args[1].data.word;
    namespace=*(NamespaceOwnedV1 *)args[2].data.word;
    values=*(BaseValuesV1 *)args[3].data.word;
    string=(StringApiV1 *)args[4].data.word; factory=args[5].data.object;
    behavior=binding("ErrorVt"); assert(behavior && binding("Error")==factory);
    void *factory_behavior=binding("ErrorFactoryVt"); assert(factory_behavior);
    void *object=send(id.receiver,name("objectPrototype"),NULL,0).value.data.object;
    BirnamOperand object_arg=oop(object);
    void *object_behavior=send(id.receiver,name("behaviorOf:"),&object_arg,1).value.data.object;
    if (!args[6].data.word) {
        assert(!binding("Integer") && !binding("Float") && !binding("Block"));
        original_kind=lookup(factory_behavior,"kind:");
        kind_override=(BirnamNativeEntry){counted_kind,&calls};
        install(factory_behavior,"kind:",kind_override);
        object_dnu=lookup(object_behavior,"doesNotUnderstand:");
        const uint8_t fields[]={1,1};
        lifetime.layouts->set(lifetime.layouts->context,behavior,fields,2,0);
    }
    kind=name(args[6].data.word?"earlyErrorKind1":"earlyErrorKind0");
    size_t kind_root=roots.retain(roots.context,kind); assert(kind_root);
    const uint8_t text[]={'f','a','u','l','t',0,255};
    BirnamOperand bytes={.kind=BIRNAM_BYTES,.data.bytes={text,sizeof(text)}};
    message=send(string->prototype,name("fromBytes:"),&bytes,1).value.data.object;
    size_t message_root=roots.retain(roots.context,message); assert(message_root);
    BirnamOperand fields[]={oop(kind),oop(message)};
    error=send(factory,name("kind:message:"),fields,2).value.data.object;
    token=roots.retain(roots.context,error); assert(token);
    assert(roots.release(roots.context,kind_root)==1 && roots.release(roots.context,message_root)==1);
    BirnamOperand arg=oop(error); identity=send(id.receiver,name("identityOf:"),&arg,1).value.data.word;
    assert(field(error,"isError")==values.true_object && field(object,"isError")==values.false_object);
    BirnamOperand arbitrary[]={ {.kind=BIRNAM_WORD,.data.word=7}, bytes };
    assert(send(error,name("earlyMissing:with:"),arbitrary,2).value.data.object==error);
    size_t before=calls;
    void *short_error=send(factory,name("kind:"),fields,1).value.data.object;
    size_t short_root=roots.retain(roots.context,short_error); assert(short_root && calls==before+1);
    assert(field(short_error,"kind")==kind && field(short_error,"message")==values.nil);
    assert(roots.release(roots.context,short_root)==1);
    void *wild=send(factory,name("message:"),fields+1,1).value.data.object;
    size_t wild_root=roots.retain(roots.context,wild); assert(wild_root);
    assert(field(wild,"kind")==name("error") && field(wild,"message")==message);
    assert(roots.release(roots.context,wild_root)==1);
    void *missing=send(object,name("earlyMissing:with:"),arbitrary,2).value.data.object;
    size_t missing_root=roots.retain(roots.context,missing); assert(missing_root);
    assert(field(missing,"kind")==name("doesNotUnderstand"));
    size_t n=0; const uint8_t *missing_text=string->bytes(string->context,field(missing,"message"),&n);
    const char *expected="does not understand #earlyMissing:with:";
    assert(n==strlen(expected) && !memcmp(missing_text,expected,n));
    assert(roots.release(roots.context,missing_root)==1);
    assert(id.message(id.context,factory,name("kind:"),arbitrary,1).status==BIRNAM_INVALID_REQUEST);
    assert(id.message(id.context,error,name("kind"),fields,1).status==BIRNAM_INVALID_REQUEST);
    BirnamNativeEntry read_kind=lookup(behavior,"kind");
    assert(read_kind.method(read_kind.context,string->prototype,NULL,0).status==BIRNAM_INVALID_REQUEST);
    BirnamNativeEntry allocation=lookup(id.receiver,"allocate");
    BirnamOperand zero={.kind=BIRNAM_WORD,.data.word=0};
    BirnamOutcome allocated=allocation.method(allocation.context,behavior,&zero,1);
    assert(allocated.status==BIRNAM_LOADED && allocated.value.kind==BIRNAM_OBJECT);
    small_result=allocated.value.data.object;
    size_t small_root=roots.retain(roots.context,small_result); assert(small_root);
    install(id.receiver,"allocate",(BirnamNativeEntry){small_allocation,NULL});
    assert(id.message(id.context,factory,name("kind:message:"),fields,2).status==BIRNAM_INVALID_REQUEST);
    install(id.receiver,"allocate",allocation);
    assert(roots.release(roots.context,small_root)==1);
    install(id.receiver,"earlyErrorCheck",(BirnamNativeEntry){check,NULL});
    install(id.receiver,"releaseEarlyError",(BirnamNativeEntry){release,NULL});
    check(NULL,NULL,NULL,0);
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=identity}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
