/* Namespace publication/root ownership with no Runtime, String or numbers. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/namespace-owner-v1/namespace_owner_v1.h"
#include <assert.h>
#include <string.h>
static BirnamProtocol id;
static NamespaceOwnedV1 namespace;
static void *object, *first, *second;
static size_t identity, first_index;
static BirnamOperand oop(void *value) { return (BirnamOperand){.kind=BIRNAM_OBJECT,.data.object=value}; }
static BirnamOutcome send(void *receiver, void *selector, const BirnamOperand *args, size_t count) {
    BirnamOutcome result=id.message(id.context,receiver,selector,args,count);
    assert(result.status==BIRNAM_LOADED); return result;
}
static void *name(const char *text) {
    BirnamOperand argument={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    return send(id.receiver,id.intern,&argument,1).value.data.object;
}
static BirnamOperand find(void *key) {
    BirnamOperand argument=oop(key);
    return send(namespace.receiver,name("bindingAt:"),&argument,1).value;
}
static void bind(void *key, void *value) {
    BirnamOperand arguments[]={oop(key),value ? oop(value) : (BirnamOperand){.kind=BIRNAM_UNIT}};
    BirnamOperand result=send(namespace.receiver,name("bindingAt:put:"),arguments,2).value;
    assert(result.kind==arguments[1].kind && (!value || result.data.object==value));
}
static BirnamOutcome check(void *context, void *receiver, const BirnamOperand *args, size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0);
    BirnamOperand argument=oop(object);
    BirnamOperand current=send(id.receiver,name("identityOf:"),&argument,1).value;
    if (namespace.api->find(namespace.storage,second)) {
        assert(current.kind==BIRNAM_WORD && current.data.word==identity);
        assert(find(second).data.object==object);
        assert(namespace.api->reverse(namespace.storage,object)==(namespace.api->find(namespace.storage,first) ? first : second));
    } else { assert(current.kind==BIRNAM_UNIT); }
    assert(namespace.api->name_at(namespace.storage,first_index)==first);
    assert(namespace.api->name_at(namespace.storage,first_index+1)==second);
    return (BirnamOutcome){.value=current};
}
static BirnamOutcome release(void *context, void *receiver, const BirnamOperand *args, size_t count) {
    (void)context;(void)receiver; assert(count==1 && args[0].kind==BIRNAM_WORD);
    assert(args[0].data.word<2); bind(args[0].data.word ? second : first,NULL);
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
static BirnamOutcome initialize(const BirnamProtocol *protocol, const BirnamOperand *args, size_t count) {
    assert(count==1 && args[0].kind==BIRNAM_WORD);
    id=*protocol; namespace=*(NamespaceOwnedV1 *)args[0].data.word;
    assert(namespace.version==1 && namespace.byte_size==sizeof(namespace));
    assert(namespace.api->version==8 && namespace.api->byte_size==sizeof(*namespace.api));
    assert(find(name("Namespace")).data.object==namespace.receiver);
    assert(find(name("Object")).kind==BIRNAM_OBJECT);
    for (size_t i=0;i<4;i++) {
        const char *absent[]={"String","Integer","Block","Error"};
        assert(find(name(absent[i])).kind==BIRNAM_UNIT);
    }
    BirnamOutcome prototype=send(id.receiver,name("objectPrototype"),NULL,0);
    void *behavior=send(id.receiver,name("behaviorOf:"),&prototype.value,1).value.data.object;
    BirnamOperand size={.kind=BIRNAM_WORD,.data.word=0};
    object=send(behavior,name("allocate"),&size,1).value.data.object;
    BirnamOperand argument=oop(object);
    identity=send(id.receiver,name("identityOf:"),&argument,1).value.data.word;
    first=name("earlyNamespaceFirst"); second=name("earlyNamespaceSecond");
    first_index=send(namespace.receiver,name("bindingCount"),NULL,0).value.data.word;
    bind(first,object); bind(second,object);
    assert(namespace.api->count(namespace.storage)==first_index+2);
    assert(namespace.api->find(namespace.storage,first)==object);
    BirnamOperand first_arg=oop(first);
    assert(send(namespace.receiver,name("nameOf:"),&argument,1).value.data.object==first);
    assert(id.message(id.context,namespace.receiver,name("bindingAt:"),&size,1).status==BIRNAM_INVALID_REQUEST);
    assert(id.message(id.context,namespace.receiver,name("bindingAt:"),&argument,1).status==BIRNAM_INVALID_REQUEST);
    assert(id.message(id.context,namespace.receiver,name("bindingNameAt:"),&first_arg,1).status==BIRNAM_INVALID_REQUEST);
    size.data.word=(size_t)-1;
    assert(send(namespace.receiver,name("bindingNameAt:"),&size,1).value.kind==BIRNAM_UNIT);
    const char *names[]={"earlyNamespaceCheck","releaseEarlyNamespaceAlias"};
    BirnamNativeMethod methods[]={check,release};
    for (size_t i=0;i<2;i++) {
        BirnamOperand install[]={oop(name(names[i])),{.kind=BIRNAM_NATIVE_ENTRY,.data.entry={methods[i],NULL}}};
        send(id.receiver,name("addMethod"),install,2);
    }
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=identity}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
