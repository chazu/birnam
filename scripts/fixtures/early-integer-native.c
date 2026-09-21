/* Signed-128 C oracle against the real Integer base, before Float is loaded. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/root-registry-owner-v1/root_registry_owner_v1.h"
#include "../../abi/fixtures/namespace-owner-v1/namespace_owner_v1.h"
#include "../../abi/fixtures/base-values-v1/base_values_v1.h"
#include "../../abi/fixtures/integer-v1/integer_v1.h"
typedef void (*Finalizer)(void *);
typedef struct IdApi IdApi;
#include "../../abi/fixtures/string-v1/string_v1.h"
#include <assert.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static BirnamProtocol id;
static RootRegistryOwnedV1 roots;
static NamespaceOwnedV1 namespace;
static BaseValuesV1 values;
static StringApiV1 *string;
static IntegerApiV1 *integer;
static void *kept, *small_result;
static size_t token, identity, calls;
static size_t (*collect)(void);
static BirnamNativeEntry original_add, add_override, original_mod, original_print;
static BirnamOperand oop(void *value) { return (BirnamOperand){.kind=BIRNAM_OBJECT,.data.object=value}; }
static BirnamOutcome send(void *receiver,void *selector,const BirnamOperand *args,size_t count) {
    BirnamOutcome result=id.message(id.context,receiver,selector,args,count);
    assert(result.status==BIRNAM_LOADED); return result;
}
static void *name(const char *text) {
    BirnamOperand arg={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    return send(id.receiver,id.intern,&arg,1).value.data.object;
}
static BirnamOperand lookup(const char *text) {
    BirnamOperand arg=oop(name(text)); return send(integer->behavior,name("lookup"),&arg,1).value;
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
static void *make(int64_t value) {
    BirnamOperand bits={.kind=BIRNAM_WORD,.data.word=(size_t)value};
    return send(integer->prototype,name("fromWord:"),&bits,1).value.data.object;
}
static int64_t read(void *value) {
    assert(integer->is_exact(integer->context,value));
    BirnamOperand result=send(value,name("word"),NULL,0).value;
    assert(result.kind==BIRNAM_WORD && (int64_t)result.data.word==integer->read(integer->context,value));
    return (int64_t)result.data.word;
}
static size_t retain(void *object) { size_t result=roots.retain(roots.context,object); assert(result); return result; }
static void release_root(size_t root) { assert(roots.release(roots.context,root)==1); }
static void error_is(void *error,const char *kind,const char *message) {
    assert(field(error,"isError")==values.true_object && field(error,"kind")==name(kind));
    size_t length=0; const uint8_t *bytes=string->bytes(string->context,field(error,"message"),&length);
    assert(length==strlen(message) && !memcmp(bytes,message,length));
}
static BirnamOutcome counted_add(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    assert(context==&calls); calls++;
    if (collect && calls%17==0) collect();
    return original_add.method(original_add.context,receiver,args,count);
}
static BirnamOutcome short_allocation(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==1);
    return (BirnamOutcome){.value=oop(small_result)};
}
static BirnamOutcome check(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0);
    assert(binding("Integer")==integer->prototype && binding("IntegerVt")==integer->behavior);
    BirnamNativeEntry add=lookup("+").data.entry, mod=lookup("mod:").data.entry, print=lookup("printString").data.entry;
    assert(add.method==add_override.method && add.context==add_override.context);
    assert(mod.method==original_mod.method && mod.context==original_mod.context);
    assert(print.method==original_print.method && print.context==original_print.context);
    BirnamOperand arg=oop(kept), current=send(id.receiver,name("identityOf:"),&arg,1).value;
    if(token) { assert(current.kind==BIRNAM_WORD && current.data.word==identity && read(kept)==INT64_MIN); }
    else assert(current.kind==BIRNAM_UNIT);
    return (BirnamOutcome){.value=current};
}
static BirnamOutcome release(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0 && token);
    release_root(token); token=0;
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
static BirnamOutcome initialize(const BirnamProtocol *protocol,const BirnamOperand *args,size_t count) {
    assert(count==7 && !token);
    for(size_t i=0;i<count;i++) assert(args[i].kind==BIRNAM_WORD);
    id=*protocol; roots=*(RootRegistryOwnedV1 *)args[0].data.word;
    namespace=*(NamespaceOwnedV1 *)args[1].data.word; values=*(BaseValuesV1 *)args[2].data.word;
    string=(StringApiV1 *)args[3].data.word; integer=(IntegerApiV1 *)args[4].data.word;
    collect=(size_t (*)(void))args[6].data.word;
    assert(integer->version==1 && integer->byte_size==sizeof(*integer));
    assert(integer->storage->payload_size==8 && integer->allocation_context(integer->context)==0);
    if(!args[5].data.word) {
        assert(!binding("Float") && !binding("Word") && !binding("Block"));
        assert(lookup("asFloat").kind==BIRNAM_UNIT && lookup("to:do:").kind==BIRNAM_UNIT);
        original_add=lookup("+").data.entry; add_override=(BirnamNativeEntry){counted_add,&calls};
        original_mod=lookup("mod:").data.entry; original_print=lookup("printString").data.entry;
        install(integer->behavior,"+",add_override);
    } else {
        assert(binding("Float") && lookup("asFloat").kind==BIRNAM_NATIVE_ENTRY && lookup("to:do:").kind==BIRNAM_NATIVE_ENTRY);
    }
    const int64_t inputs[]={INT64_MIN,INT64_MIN+1,-17,-1,0,1,17,INT64_C(9007199254740992),INT64_C(9007199254740993),INT64_MAX-1,INT64_MAX};
    const char *operators[]={"+","-","*","/","mod:"};
    const char *comparisons[]={"=","<","<=",">",">="};
    size_t checked=0;
    for(size_t i=0;i<sizeof(inputs)/sizeof(*inputs);i++) {
        void *left=make(inputs[i]); size_t left_root=retain(left);
        void *printed=field(left,"printString"); size_t print_root=retain(printed);
        char expected_text[32]; int text_length=snprintf(expected_text,sizeof(expected_text),"%lld",(long long)inputs[i]);
        size_t length=0; const uint8_t *text=string->bytes(string->context,printed,&length);
        assert(text_length>=0 && length==(size_t)text_length && !memcmp(text,expected_text,length)); release_root(print_root);
        for(size_t j=0;j<sizeof(inputs)/sizeof(*inputs);j++) {
            void *right=make(inputs[j]); size_t right_root=retain(right);
            BirnamOperand argument=oop(right);
            for(size_t op=0;op<5;op++) {
                __int128 x=inputs[i],y=inputs[j],expected=0;
                int zero=op>=3 && y==0;
                if(!zero) switch(op) { case 0:expected=x+y;break;case 1:expected=x-y;break;case 2:expected=x*y;break;case 3:expected=x/y;break;default:expected=x%y; }
                void *result=send(left,name(operators[op]),&argument,1).value.data.object; size_t result_root=retain(result);
                if(zero || expected<INT64_MIN || expected>INT64_MAX) {
                    error_is(result,zero?"zeroDivide":"overflow",zero?"division by zero":"integer overflow");
                    BirnamOperand incoming=oop(result); assert(send(left,name(operators[op]),&incoming,1).value.data.object==result);
                } else { assert(read(result)==(int64_t)expected && result!=left && result!=right); }
                assert(read(left)==inputs[i] && read(right)==inputs[j]); release_root(result_root); checked++;
            }
            int comparisons_expected[]={inputs[i]==inputs[j],(double)inputs[i]<(double)inputs[j],(double)inputs[i]<=(double)inputs[j],(double)inputs[i]>(double)inputs[j],(double)inputs[i]>=(double)inputs[j]};
            for(size_t op=0;op<5;op++) assert(send(left,name(comparisons[op]),&argument,1).value.data.object==(comparisons_expected[op]?values.true_object:values.false_object));
            release_root(right_root);
        }
        release_root(left_root);
    }
    kept=make(INT64_MIN); token=retain(kept);
    BirnamOperand kept_arg=oop(kept); identity=send(id.receiver,name("identityOf:"),&kept_arg,1).value.data.word;
    BirnamOperand wrong=oop(values.true_object);
    for(size_t op=0;op<5;op++) {
        void *result=send(kept,name(operators[op]),&wrong,1).value.data.object; size_t root=retain(result);
        error_is(result,"typeMismatch",op==4?"mod: needs an Integer":"Integer arithmetic needs a number"); release_root(root);
    }
    for(size_t op=1;op<5;op++) {
        void *result=send(kept,name(comparisons[op]),&wrong,1).value.data.object; size_t root=retain(result);
        error_is(result,"typeMismatch","comparison needs a number"); release_root(root);
    }
    assert(send(kept,name("="),&wrong,1).value.data.object==values.false_object);
    assert(id.message(id.context,integer->prototype,name("fromWord:"),&wrong,1).status==BIRNAM_INVALID_REQUEST);
    BirnamOperand wrong_kind={.kind=BIRNAM_WORD,.data.word=3};
    /* + remains our native base binding even after mixed extensions mount. */
    assert(id.message(id.context,kept,name("+"),&wrong_kind,1).status==BIRNAM_INVALID_REQUEST);
    BirnamOperand lookup_arg=oop(name("allocate"));
    BirnamNativeEntry allocator=send(id.receiver,name("lookup"),&lookup_arg,1).value.data.entry;
    BirnamOperand zero={.kind=BIRNAM_WORD,.data.word=0};
    BirnamOutcome tiny=allocator.method(allocator.context,integer->behavior,&zero,1); assert(tiny.status==BIRNAM_LOADED);
    small_result=tiny.value.data.object; size_t small_root=retain(small_result);
    install(id.receiver,"allocate",(BirnamNativeEntry){short_allocation,NULL});
    assert(id.message(id.context,integer->prototype,name("fromWord:"),&zero,1).status==BIRNAM_INVALID_REQUEST);
    install(id.receiver,"allocate",allocator); release_root(small_root);
    install(id.receiver,"earlyIntegerCheck",(BirnamNativeEntry){check,NULL});
    install(id.receiver,"releaseEarlyInteger",(BirnamNativeEntry){release,NULL});
    if(collect) {
        /* A cached selector is still a native root when its method is removed. */
        void *selector=name("fromBytes:"); BirnamOperand arg=oop(selector);
        size_t selector_identity=send(id.receiver,name("identityOf:"),&arg,1).value.data.word;
        BirnamOperand removed=send(string->behavior,name("removeMethod:"),&arg,1).value;
        assert(removed.kind==BIRNAM_NATIVE_ENTRY);
        collect();
        BirnamOperand current=send(id.receiver,name("identityOf:"),&arg,1).value;
        assert(current.kind==BIRNAM_WORD && current.data.word==selector_identity);
        BirnamOperand restore[]={arg,removed}; send(string->behavior,name("addMethod"),restore,2);
    }
    check(NULL,NULL,NULL,0);
    printf("early Integer: %zu signed arithmetic cases, comparisons, printing, errors and allocation checks pass (phase=%zu)\n",checked,args[5].data.word);
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=identity}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
