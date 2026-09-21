/* Text and base-value printing on the real prefix, without Block or Runtime. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include "../../abi/fixtures/root-registry-owner-v1/root_registry_owner_v1.h"
#include "../../abi/fixtures/namespace-owner-v1/namespace_owner_v1.h"
#include "../../abi/fixtures/base-values-v1/base_values_v1.h"
#include "../../abi/fixtures/integer-v1/integer_v1.h"
typedef void (*Finalizer)(void *);
typedef struct IdApi IdApi;
#include "../../abi/fixtures/string-v1/string_v1.h"
#include "../../abi/fixtures/float-v1/float_v1.h"
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
static FloatApiV1 *floating;
static size_t (*collect)(void);
static void *kept;
static size_t token, identity, calls, bytes_calls;
static BirnamNativeEntry original_size, size_override, original_nil_print, nil_override, boolean_print;
static const uint8_t *(*original_bytes)(void *,void *,size_t *);
static BirnamOperand oop(void *value) { return (BirnamOperand){.kind=BIRNAM_OBJECT,.data.object=value}; }
static BirnamOutcome send(void *receiver,void *selector,const BirnamOperand *args,size_t count) {
    BirnamOutcome result=id.message(id.context,receiver,selector,args,count);
    assert(result.status==BIRNAM_LOADED); return result;
}
static void *name(const char *text) {
    BirnamOperand arg={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)text,strlen(text)}};
    return send(id.receiver,id.intern,&arg,1).value.data.object;
}
static BirnamNativeEntry lookup(void *behavior,const char *text) {
    BirnamOperand arg=oop(name(text)), result=send(behavior,name("lookup"),&arg,1).value;
    assert(result.kind==BIRNAM_NATIVE_ENTRY); return result.data.entry;
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
static void *make(const void *bytes,size_t length) {
    BirnamOperand arg={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)bytes,length}};
    return send(string->prototype,name("fromBytes:"),&arg,1).value.data.object;
}
static size_t retain(void *object) { size_t result=roots.retain(roots.context,object); assert(result); return result; }
static void release_root(size_t root) { assert(roots.release(roots.context,root)==1); }
static void bytes_are(void *object,const void *expected,size_t length) {
    size_t actual_length=0; const uint8_t *actual=string->bytes(string->context,object,&actual_length);
    assert(actual && actual_length==length && !memcmp(actual,expected,length));
}
static int64_t number(void *object) { assert(integer->is_exact(integer->context,object)); return integer->read(integer->context,object); }
static BirnamOutcome counted_size(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    assert(context==&calls); calls++; if(collect && calls%17==0) collect();
    return original_size.method(original_size.context,receiver,args,count);
}
static BirnamOutcome counted_nil(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    assert(context==&calls); calls++; if(collect) collect();
    return original_nil_print.method(original_nil_print.context,receiver,args,count);
}
static const uint8_t *collecting_bytes(void *context,void *object,size_t *length) {
    bytes_calls++; if(collect) collect(); return original_bytes(context,object,length);
}
static BirnamOutcome check(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0);
    BirnamNativeEntry size=lookup(string->behavior,"size"), nil=lookup(values.nil_behavior,"printString"), boolean=lookup(values.boolean_behavior,"printString");
    assert(size.method==size_override.method && size.context==size_override.context);
    assert(nil.method==nil_override.method && nil.context==nil_override.context);
    assert(boolean.method==boolean_print.method && boolean.context==boolean_print.context);
    BirnamOperand arg=oop(kept), current=send(id.receiver,name("identityOf:"),&arg,1).value;
    if(token) { assert(current.kind==BIRNAM_WORD && current.data.word==identity); bytes_are(kept,"alpha\0beta",10); }
    else { assert(current.kind==BIRNAM_UNIT); size_t length=99; assert(!string->storage->borrow(string->context,kept,&length));  }
    return (BirnamOutcome){.value=current};
}
static BirnamOutcome release(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0 && token); release_root(token); token=0;
    return (BirnamOutcome){.value={.kind=BIRNAM_UNIT}};
}
static BirnamOutcome initialize(const BirnamProtocol *protocol,const BirnamOperand *args,size_t count) {
    assert(count==8 && !token); for(size_t i=0;i<count;i++) assert(args[i].kind==BIRNAM_WORD);
    id=*protocol; roots=*(RootRegistryOwnedV1 *)args[0].data.word; namespace=*(NamespaceOwnedV1 *)args[1].data.word;
    values=*(BaseValuesV1 *)args[2].data.word; string=(StringApiV1 *)args[3].data.word;
    integer=(IntegerApiV1 *)args[4].data.word; floating=(FloatApiV1 *)args[5].data.word; collect=(size_t (*)(void))args[7].data.word;
    if(!args[6].data.word) {
        assert(!binding("Block")); original_size=lookup(string->behavior,"size"); size_override=(BirnamNativeEntry){counted_size,&calls};
        install(string->behavior,"size",size_override);
        original_nil_print=lookup(values.nil_behavior,"printString"); nil_override=(BirnamNativeEntry){counted_nil,&calls};
        install(values.nil_behavior,"printString",nil_override); boolean_print=lookup(values.boolean_behavior,"printString");
    }
    void *singletons[]={values.nil,values.true_object,values.false_object}; const char *printed[]={"nil","true","false"};
    for(size_t i=0;i<3;i++) { void *text=field(singletons[i],"printString"); size_t root=retain(text); bytes_are(text,printed[i],strlen(printed[i])); release_root(root); }
    uint8_t payload[256]; for(size_t i=0;i<256;i++) payload[i]=(uint8_t)i;
    void *text=make(payload,256); size_t text_root=retain(text); assert(field(text,"printString")==text);
    for(size_t i=0;i<256;i++) {
        assert(number(field(text,"size"))==256);
        void *index=integer->make(integer->context,(int64_t)i); size_t index_root=retain(index); BirnamOperand arg=oop(index);
        void *byte=send(text,name("at:"),&arg,1).value.data.object; size_t byte_root=retain(byte); bytes_are(byte,payload+i,1);
        assert(number(send(text,name("byteAt:"),&arg,1).value.data.object)==(int64_t)i);
        BirnamOperand byte_arg=oop(byte); assert(number(send(text,name("indexOf:"),&byte_arg,1).value.data.object)==(int64_t)i);
        void *end=integer->make(integer->context,(int64_t)i+1); size_t end_root=retain(end); BirnamOperand range[]={arg,oop(end)};
        void *copy=send(text,name("copyFrom:to:"),range,2).value.data.object; size_t copy_root=retain(copy); bytes_are(copy,payload+i,1);
        BirnamOperand copied=oop(copy); assert(send(byte,name("="),&copied,1).value.data.object==values.true_object);
        release_root(copy_root); release_root(end_root); release_root(byte_root); release_root(index_root);
    }
    void *bad_index=integer->make(integer->context,-1); size_t bad_root=retain(bad_index); BirnamOperand bad=oop(bad_index);
    assert(send(text,name("at:"),&bad,1).value.data.object==values.nil); release_root(bad_root); release_root(text_root);
    const char spaced[]="\v\f \t\r\nhi\v\f"; text=make(spaced,sizeof(spaced)-1); text_root=retain(text);
    void *trim=field(text,"trimmed"); size_t trim_root=retain(trim); bytes_are(trim,"hi",2); release_root(trim_root); release_root(text_root);
    const char brackets[]="[ { \"}]\" // ]}\n]"; text=make(brackets,sizeof(brackets)-1); text_root=retain(text); assert(number(field(text,"bracketDepth"))==1); release_root(text_root);
    text=make("ababa",5); text_root=retain(text); void *needle=make("aba",3); size_t needle_root=retain(needle); void *replacement=make("x",1); size_t replacement_root=retain(replacement);
    BirnamOperand replacements[]={oop(needle),oop(replacement)}; void *replaced=send(text,name("replace:with:"),replacements,2).value.data.object;
    size_t replaced_root=retain(replaced); bytes_are(replaced,"xba",3); release_root(replaced_root); release_root(replacement_root); release_root(needle_root); release_root(text_root);
    text=make("a\0b",3); text_root=retain(text); assert(field(text,"asSymbol")==name("a")); release_root(text_root);
    const char *numbers[]={" 42\n","-9223372036854775808","9223372036854775808"," 1.25 ","NaN","bad","\v1"};
    for(size_t i=0;i<7;i++) {
        text=make(numbers[i],strlen(numbers[i])); text_root=retain(text);
        void *result=field(text,"asInteger"); size_t root=retain(result);
        if(i<2) assert(number(result)==(i==0?42:INT64_MIN)); else assert(result==values.nil); release_root(root);
        result=field(text,"asNumber"); root=retain(result);
        if(i<2) assert(number(result)==(i==0?42:INT64_MIN));
        else if(i<5) { assert(floating->is_exact(floating->context,result)); double value=floating->read(floating->context,result); if(i==2) assert(value==0x1p63); else if(i==3) assert(value==1.25); else assert(isnan(value)); }
        else assert(result==values.nil); release_root(root); release_root(text_root);
    }
    /* A temporary printable result must remain rooted across later byte callbacks. */
    text=make("prefix:",7); text_root=retain(text); original_bytes=string->bytes; string->bytes=collecting_bytes;
    BirnamOperand nil_arg=oop(values.nil); void *joined=send(text,name(","),&nil_arg,1).value.data.object; size_t joined_root=retain(joined);
    bytes_are(joined,"prefix:nil",10); assert(bytes_calls); string->bytes=original_bytes; release_root(joined_root); release_root(text_root);
    BirnamOperand malformed={.kind=BIRNAM_WORD,.data.word=0};
    assert(id.message(id.context,string->prototype,name("at:"),&malformed,1).status==BIRNAM_INVALID_REQUEST);
    assert(id.message(id.context,values.nil,name("printString"),&malformed,1).status==BIRNAM_INVALID_REQUEST);
    kept=make("alpha\0beta",10); token=retain(kept); BirnamOperand kept_arg=oop(kept); identity=send(id.receiver,name("identityOf:"),&kept_arg,1).value.data.word;
    install(id.receiver,"earlyTextCheck",(BirnamNativeEntry){check,NULL}); install(id.receiver,"releaseEarlyText",(BirnamNativeEntry){release,NULL}); check(NULL,NULL,NULL,0);
    printf("early text: 256 binary indices, transforms, numeric parsing, printing and callback GC pass (phase=%zu)\n",args[6].data.word);
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=identity}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
