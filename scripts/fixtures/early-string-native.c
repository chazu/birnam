/* Exercise production byte ownership before values/Runtime and after adoption. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/root-registry-owner-v1/root_registry_owner_v1.h"
#include "../../abi/fixtures/lifetime-v1/lifetime_v1.h"
typedef void (*Finalizer)(void *);
typedef struct IdApi IdApi;
#include "../../abi/fixtures/string-v1/string_v1.h"
#include <assert.h>
#include <string.h>
static BirnamProtocol id;
static RootRegistryOwnedV1 roots;
static LifetimeOwnedV1 lifetime;
static StringApiV1 *ops;
static void *value, *copy, *allocation_override;
static const uint8_t expected[]={ 'a',0,255,'z' };
static const uint8_t *buffer;
static size_t tokens[2], identity, allocation_calls;
static BirnamNativeEntry clone_entry, allocation_entry;
static BirnamOperand oop(void *value) { return (BirnamOperand){.kind=BIRNAM_OBJECT,.data.object=value}; }
static BirnamOutcome send(void *receiver, void *selector, const BirnamOperand *args, size_t count) {
    BirnamOutcome result=id.message(id.context,receiver,selector,args,count);
    assert(result.status==BIRNAM_LOADED); return result;
}
static void *name(const char *text) {
    BirnamOperand arg={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    return send(id.receiver,id.intern,&arg,1).value.data.object;
}
static BirnamOperand lookup(void *receiver,const char *text) {
    BirnamOperand arg=oop(name(text)); return send(receiver,name("lookup"),&arg,1).value;
}
static void install(void *behavior,const char *text,BirnamNativeMethod method) {
    BirnamOperand args[]={oop(name(text)),{.kind=BIRNAM_NATIVE_ENTRY,.data.entry={method,NULL}}};
    send(behavior,name("addMethod"),args,2);
}
static BirnamOutcome allocate(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context; allocation_calls++;
    if (allocation_override) return (BirnamOutcome){.value=oop(allocation_override)};
    /* A constructor replacement can send messages before delegating. */
    (void)lookup(id.receiver,"behaviorOf:");
    return allocation_entry.method(allocation_entry.context,receiver,args,count);
}
static BirnamOutcome check(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0);
    assert(ops->version==1 && ops->byte_size==sizeof(*ops));
    BirnamOperand arg=oop(value), current=send(id.receiver,name("identityOf:"),&arg,1).value;
    size_t length=999;
    if (tokens[0]) {
        assert(current.kind==BIRNAM_WORD && current.data.word==identity);
        assert(ops->storage->borrow(ops->context,value,&length)==buffer && length==sizeof(expected));
        assert(!memcmp(buffer,expected,length));
        const uint8_t *cloned=ops->storage->borrow(ops->context,copy,&length);
        assert(cloned && cloned!=buffer && length==sizeof(expected) && !memcmp(cloned,expected,length));
        assert(lifetime.finalizer(lifetime.context,value).callback==ops->storage->cleanup);
        assert(lifetime.finalizer(lifetime.context,copy).callback==ops->storage->cleanup);
    } else {
        assert(current.kind==BIRNAM_UNIT);
        assert(!ops->storage->borrow(ops->context,value,&length));
        assert(!ops->storage->borrow(ops->context,copy,&length));
        assert(!lifetime.finalizer(lifetime.context,value).callback);
        assert(!lifetime.finalizer(lifetime.context,copy).callback);
    }
    BirnamOperand selected=lookup(ops->behavior,"clone");
    assert(selected.kind==BIRNAM_NATIVE_ENTRY);
    assert(selected.data.entry.method==clone_entry.method && selected.data.entry.context==clone_entry.context);
    const uint8_t *empty=ops->storage->borrow(ops->context,ops->prototype,&length);
    assert(empty && length==0);
    return (BirnamOutcome){.value=current};
}
static BirnamOutcome release(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0);
    for(size_t i=0;i<2;i++) { assert(tokens[i] && roots.release(roots.context,tokens[i])==1); tokens[i]=0; }
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
static BirnamOutcome initialize(const BirnamProtocol *protocol,const BirnamOperand *args,size_t count) {
    assert(count==4 && args[0].kind==BIRNAM_WORD && args[1].kind==BIRNAM_WORD && args[2].kind==BIRNAM_WORD && args[3].kind==BIRNAM_WORD);
    assert(!tokens[0] && !tokens[1]);
    id=*protocol; roots=*(RootRegistryOwnedV1 *)args[0].data.word;
    ops=(StringApiV1 *)args[1].data.word; lifetime=*(LifetimeOwnedV1 *)args[2].data.word;
    assert(ops->version==1 && ops->byte_size==sizeof(*ops));
    assert(ops->storage->version==1 && ops->storage->byte_size==sizeof(*ops->storage));
    assert(ops->storage->cleanup_owner(ops->storage->cleanup)==ops->context);
    if (!args[3].data.word) {
        const char *absent[]={"size","at:","asInteger","asNumber","split:","printString"};
        for(size_t i=0;i<sizeof(absent)/sizeof(*absent);i++) assert(lookup(ops->behavior,absent[i]).kind==BIRNAM_UNIT);
    }
    allocation_entry=lookup(id.receiver,"allocate").data.entry;
    install(id.receiver,"allocate",allocate);
    size_t calls=allocation_calls;
    uint8_t input[sizeof(expected)]; memcpy(input,expected,sizeof(input));
    BirnamOperand bytes={.kind=BIRNAM_BYTES,.data.bytes={input,sizeof(input)}};
    value=send(ops->prototype,name("fromBytes:"),&bytes,1).value.data.object;
    tokens[0]=roots.retain(roots.context,value); assert(tokens[0]);
    memset(input,0,sizeof(input));
    BirnamOperand size=send(value,name("length"),NULL,0).value;
    assert(size.kind==BIRNAM_WORD && size.data.word==sizeof(expected));
    BirnamOperand read=send(value,name("bytes"),NULL,0).value;
    assert(read.kind==BIRNAM_BYTES && read.data.bytes.length==sizeof(expected));
    buffer=read.data.bytes.pointer; assert(!memcmp(buffer,expected,sizeof(expected)));
    /* Native ownership, not writable payload pointers, determines byte access. */
    ((size_t *)value)[0]=0; ((size_t *)value)[1]=(size_t)-1;
    copy=send(value,name("clone"),NULL,0).value.data.object;
    tokens[1]=roots.retain(roots.context,copy); assert(tokens[1] && copy!=value);
    assert(allocation_calls==calls+2);
    BirnamOperand arg=oop(value);
    identity=send(id.receiver,name("identityOf:"),&arg,1).value.data.word;
    clone_entry=lookup(ops->behavior,"clone").data.entry;
    BirnamOperand malformed={.kind=BIRNAM_WORD,.data.word=3};
    assert(id.message(id.context,ops->prototype,name("fromBytes:"),&malformed,1).status==BIRNAM_INVALID_REQUEST);
    assert(id.message(id.context,value,name("bytes"),&malformed,1).status==BIRNAM_INVALID_REQUEST);
    /* Replacing construction cannot redirect payload writes to a small or
       already owned object. This is a safe-language boundary, not pointer vetting. */
    BirnamNativeEntry raw_allocator=allocation_entry;
    BirnamOperand zero={.kind=BIRNAM_WORD,.data.word=0};
    BirnamOutcome tiny=raw_allocator.method(raw_allocator.context,ops->behavior,&zero,1);
    assert(tiny.status==BIRNAM_LOADED && tiny.value.kind==BIRNAM_OBJECT);
    allocation_override=tiny.value.data.object;
    size_t tiny_root=roots.retain(roots.context,allocation_override); assert(tiny_root);
    assert(id.message(id.context,ops->prototype,name("fromBytes:"),&bytes,1).status==BIRNAM_INVALID_REQUEST);
    assert(roots.release(roots.context,tiny_root)==1);
    allocation_override=value;
    assert(id.message(id.context,ops->prototype,name("fromBytes:"),&bytes,1).status==BIRNAM_INVALID_REQUEST);
    allocation_override=NULL;
    BirnamOperand restore[]={oop(name("allocate")),{.kind=BIRNAM_NATIVE_ENTRY,.data.entry=allocation_entry}};
    send(id.receiver,name("addMethod"),restore,2);
    install(id.receiver,"earlyStringCheck",check); install(id.receiver,"releaseEarlyStrings",release);
    check(NULL,NULL,NULL,0);
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=identity}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
