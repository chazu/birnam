/* Independent unsigned payload, formatting and conversion checks on production Word. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/root-registry-owner-v1/root_registry_owner_v1.h"
#include "../../abi/fixtures/namespace-owner-v1/namespace_owner_v1.h"
#include "../../abi/fixtures/base-values-v1/base_values_v1.h"
#include "../../abi/fixtures/integer-v1/integer_v1.h"
typedef void (*Finalizer)(void *);
typedef struct IdApi IdApi;
#include "../../abi/fixtures/string-v1/string_v1.h"
#include "../../abi/fixtures/word-v1/word_v1.h"
#include <assert.h>
#include <limits.h>
#include <math.h>
#include <stdio.h>
#include <string.h>
static BirnamProtocol id;
static RootRegistryOwnedV1 roots;
static NamespaceOwnedV1 namespace;
static BaseValuesV1 values;
static StringApiV1 *string;
static IntegerApiV1 *integer;
static WordApiV1 *word;
static void *kept, *small_result;
static size_t token, identity, calls;
static size_t (*collect)(void);
static BirnamNativeEntry original_print, print_override;
static BirnamOperand oop(void *value) { return (BirnamOperand){.kind=BIRNAM_OBJECT,.data.object=value}; }
static BirnamOutcome send(void *receiver,void *selector,const BirnamOperand *args,size_t count) {
    BirnamOutcome result=id.message(id.context,receiver,selector,args,count);
    assert(result.status==BIRNAM_LOADED); return result;
}
static void *name(const char *text) {
    BirnamOperand arg={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    return send(id.receiver,id.intern,&arg,1).value.data.object;
}
static BirnamOperand lookup(void *behavior,const char *text) {
    BirnamOperand arg=oop(name(text)); return send(behavior,name("lookup"),&arg,1).value;
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
static void *make_bits(uint64_t value) {
    BirnamOperand arg={.kind=BIRNAM_WORD,.data.word=(size_t)value};
    return send(word->prototype,name("fromBits:"),&arg,1).value.data.object;
}
static size_t retain(void *object) { size_t result=roots.retain(roots.context,object); assert(result); return result; }
static void release_root(size_t root) { assert(roots.release(roots.context,root)==1); }
static void error_is(void *error,const char *kind,const char *message) {
    assert(field(error,"isError")==values.true_object && field(error,"kind")==name(kind));
    size_t length=0; const uint8_t *bytes=string->bytes(string->context,field(error,"message"),&length);
    assert(length==strlen(message) && !memcmp(bytes,message,length));
}
static BirnamOutcome counted_print(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    assert(context==&calls); calls++;
    if(collect && calls%17==0) collect();
    return original_print.method(original_print.context,receiver,args,count);
}
static BirnamOutcome short_allocation(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==1);
    return (BirnamOutcome){.value=oop(small_result)};
}
static BirnamOutcome check(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0);
    assert(binding("Word")==word->prototype && binding("WordVt")==word->behavior);
    BirnamNativeEntry print=lookup(word->behavior,"printString").data.entry;
    assert(print.method==print_override.method && print.context==print_override.context);
    BirnamOperand arg=oop(kept), current=send(id.receiver,name("identityOf:"),&arg,1).value;
    if(token) {
        assert(current.kind==BIRNAM_WORD && current.data.word==identity);
        assert(send(kept,name("bits"),NULL,0).value.data.word==UINT64_MAX);
    } else assert(current.kind==BIRNAM_UNIT);
    return (BirnamOutcome){.value=current};
}
static BirnamOutcome release(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0 && token);
    release_root(token); token=0;
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
static BirnamOutcome initialize(const BirnamProtocol *protocol,const BirnamOperand *args,size_t count) {
    assert(count==8 && !token);
    for(size_t i=0;i<count;i++) assert(args[i].kind==BIRNAM_WORD);
    id=*protocol; roots=*(RootRegistryOwnedV1 *)args[0].data.word; namespace=*(NamespaceOwnedV1 *)args[1].data.word;
    values=*(BaseValuesV1 *)args[2].data.word; string=(StringApiV1 *)args[3].data.word;
    integer=(IntegerApiV1 *)args[4].data.word; word=(WordApiV1 *)args[5].data.word;
    collect=(size_t (*)(void))args[7].data.word;
    assert(word->version==1 && word->byte_size==sizeof(*word) && word->storage->payload_size==8);
    if(!args[6].data.word) {
        assert(!binding("Block"));
        original_print=lookup(word->behavior,"printString").data.entry;
        print_override=(BirnamNativeEntry){counted_print,&calls}; install(word->behavior,"printString",print_override);
    }
    const uint64_t edges[]={0,1,UINT64_C(0x7fffffffffffffff),UINT64_C(0x8000000000000000),UINT64_MAX,UINT64_C(0x0123456789abcdef)};
    size_t checked=0; uint64_t state=UINT64_C(0x9e3779b97f4a7c15);
    for(size_t i=0;i<262;i++) {
        state=state*UINT64_C(6364136223846793005)+1;
        uint64_t value=i<6?edges[i]:state;
        void *object=make_bits(value); size_t root=retain(object);
        BirnamOperand returned=send(object,name("bits"),NULL,0).value;
        assert(returned.kind==BIRNAM_WORD && returned.data.word==value && word->read(word->context,object)==value);
        void *copy=word->make(word->context,value); size_t copy_root=retain(copy); assert(copy!=object);
        BirnamOperand arg=oop(copy); assert(send(object,name("="),&arg,1).value.data.object==values.true_object);
        release_root(copy_root);
        void *different=make_bits(value^1); size_t different_root=retain(different); arg=oop(different);
        assert(send(object,name("="),&arg,1).value.data.object==values.false_object); release_root(different_root);
        arg=oop(values.true_object); assert(send(object,name("="),&arg,1).value.data.object==values.false_object);
        void *printed=field(object,"printString"); size_t print_root=retain(printed), length=0;
        char expected[24]; int size=snprintf(expected,sizeof(expected),"0x%llx",(unsigned long long)value);
        const uint8_t *bytes=string->bytes(string->context,printed,&length);
        assert(size>0 && length==(size_t)size && !memcmp(bytes,expected,length)); release_root(print_root);
        void *result=field(object,"asInteger"); size_t result_root=retain(result);
        if(value>(uint64_t)INT64_MAX) error_is(result,"overflow","word out of Integer range");
        else { assert(integer->is_exact(integer->context,result)); assert(integer->read(integer->context,result)==(int64_t)value); }
        release_root(result_root); release_root(root); checked++;
    }
    void *one=make_bits(1); size_t one_root=retain(one); BirnamOperand bad=oop(values.true_object);
    BirnamNativeEntry allocate=lookup(id.receiver,"allocate").data.entry;
    BirnamOperand zero={.kind=BIRNAM_WORD,.data.word=0}; small_result=send(word->behavior,name("allocate"),&zero,1).value.data.object; size_t small_root=retain(small_result);
    install(id.receiver,"allocate",(BirnamNativeEntry){short_allocation,NULL});
    assert(id.message(id.context,word->prototype,name("fromBits:"),&zero,1).status==BIRNAM_INVALID_REQUEST);
    install(id.receiver,"allocate",allocate); release_root(small_root);
    assert(id.message(id.context,word->prototype,name("fromBits:"),&bad,1).status==BIRNAM_INVALID_REQUEST);
    assert(id.message(id.context,one,name("bits"),&zero,1).status==BIRNAM_INVALID_REQUEST);
    release_root(one_root);
    kept=make_bits(UINT64_MAX); token=retain(kept); BirnamOperand kept_arg=oop(kept);
    identity=send(id.receiver,name("identityOf:"),&kept_arg,1).value.data.word;
    install(id.receiver,"earlyWordCheck",(BirnamNativeEntry){check,NULL});
    install(id.receiver,"releaseEarlyWord",(BirnamNativeEntry){release,NULL});
    check(NULL,NULL,NULL,0);
    printf("early Word: %zu unsigned payload, equality, printing, conversion and allocation cases pass (phase=%zu)\n",checked,args[6].data.word);
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=identity}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
