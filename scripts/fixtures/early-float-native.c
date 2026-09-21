/* Independent IEEE arithmetic and signed conversion checks on production Float. */
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
static void *kept, *small_result;
static size_t token, identity, calls;
static size_t (*collect)(void);
static BirnamNativeEntry original_add, add_override, original_print;
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
static void rebind(const char *text,void *value) {
    BirnamOperand args[]={oop(name(text)),oop(value)}; send(namespace.receiver,name("bindingAt:put:"),args,2);
}
static void *field(void *object,const char *text) { return send(object,name(text),NULL,0).value.data.object; }
static uint64_t bits(double value) { uint64_t result; memcpy(&result,&value,8); return result; }
static double decode(uint64_t value) { double result; memcpy(&result,&value,8); return result; }
static void *make_bits(uint64_t value) {
    BirnamOperand arg={.kind=BIRNAM_WORD,.data.word=(size_t)value};
    return send(floating->prototype,name("fromBits:"),&arg,1).value.data.object;
}
static void *make(double value) { return make_bits(bits(value)); }
static size_t retain(void *object) { size_t result=roots.retain(roots.context,object); assert(result); return result; }
static void release_root(size_t root) { assert(roots.release(roots.context,root)==1); }
static void same(void *object,double expected) {
    assert(floating->is_exact(floating->context,object));
    double actual=floating->read(floating->context,object);
    assert((isnan(expected)&&isnan(actual)) || bits(actual)==bits(expected));
}
static void error_is(void *error,const char *kind,const char *message) {
    assert(field(error,"isError")==values.true_object && field(error,"kind")==name(kind));
    size_t length=0; const uint8_t *bytes=string->bytes(string->context,field(error,"message"),&length);
    assert(length==strlen(message) && !memcmp(bytes,message,length));
}
static BirnamOutcome counted_add(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    assert(context==&calls); calls++;
    if(collect && calls%13==0) collect();
    return original_add.method(original_add.context,receiver,args,count);
}
static BirnamOutcome short_allocation(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==1);
    return (BirnamOutcome){.value=oop(small_result)};
}
static BirnamOutcome check(void *context,void *receiver,const BirnamOperand *args,size_t count) {
    (void)context;(void)receiver;(void)args; assert(count==0);
    assert(binding("Float")==floating->prototype && binding("FloatVt")==floating->behavior);
    BirnamNativeEntry add=lookup(floating->behavior,"+").data.entry, print=lookup(floating->behavior,"printString").data.entry;
    assert(add.method==add_override.method && add.context==add_override.context);
    assert(print.method==original_print.method && print.context==original_print.context);
    BirnamOperand arg=oop(kept), current=send(id.receiver,name("identityOf:"),&arg,1).value;
    if(token) {
        assert(current.kind==BIRNAM_WORD && current.data.word==identity);
        assert(send(kept,name("bits"),NULL,0).value.data.word==UINT64_C(0xfff8000000000042));
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
    integer=(IntegerApiV1 *)args[4].data.word; floating=(FloatApiV1 *)args[5].data.word;
    collect=(size_t (*)(void))args[7].data.word;
    assert(floating->version==1 && floating->byte_size==sizeof(*floating) && floating->storage->payload_size==8);
    if(!args[6].data.word) {
        assert(!binding("Word") && !binding("Block"));
        assert(lookup(integer->behavior,"asFloat").kind==BIRNAM_NATIVE_ENTRY && lookup(integer->behavior,"to:do:").kind==BIRNAM_UNIT);
        original_add=lookup(floating->behavior,"+").data.entry; add_override=(BirnamNativeEntry){counted_add,&calls};
        original_print=lookup(floating->behavior,"printString").data.entry; install(floating->behavior,"+",add_override);
    }
    const uint64_t payloads[]={0,UINT64_C(0x8000000000000000),1,UINT64_C(0x8000000000000001),UINT64_C(0x3ff0000000000000),UINT64_C(0xc001000000000000),UINT64_C(0x7fefffffffffffff),UINT64_C(0xffefffffffffffff),UINT64_C(0x7ff0000000000000),UINT64_C(0xfff0000000000000),UINT64_C(0x7ff8000000000011),UINT64_C(0xfff8000000000042)};
    const char *operators[]={"+","-","*","/"};
    const char *comparisons[]={"=","<","<=",">",">="};
    size_t checked=0;
    for(size_t i=0;i<sizeof(payloads)/sizeof(*payloads);i++) {
        double x=decode(payloads[i]); void *left=make_bits(payloads[i]); size_t left_root=retain(left);
        assert(send(left,name("bits"),NULL,0).value.data.word==payloads[i]);
        for(size_t j=0;j<sizeof(payloads)/sizeof(*payloads);j++) {
            double y=decode(payloads[j]); void *right=make_bits(payloads[j]); size_t right_root=retain(right); BirnamOperand arg=oop(right);
            for(size_t op=0;op<4;op++) {
                void *result=send(left,name(operators[op]),&arg,1).value.data.object; size_t root=retain(result);
                if(op==3 && y==0.0) error_is(result,"zeroDivide","division by zero");
                else { double expected=op==0?x+y:op==1?x-y:op==2?x*y:x/y; same(result,expected); assert(result!=left && result!=right); }
                release_root(root); checked++;
            }
            for(size_t op=0;op<5;op++) {
                int expected=op==0?x==y:op==1?x<y:op==2?x<=y:op==3?x>y:x>=y;
                assert(send(left,name(comparisons[op]),&arg,1).value.data.object==(expected?values.true_object:values.false_object));
            }
            release_root(right_root);
        }
        same(field(left,"negated"),-x); same(field(left,"abs"),fabs(x));
        void *square=field(left,"sqrt"); size_t square_root=retain(square);
        if(x<0.0) error_is(square,"domain","square root of a negative number"); else same(square,sqrt(x));
        release_root(square_root); release_root(left_root);
    }
    const int64_t ints[]={INT64_MIN,-17,-1,0,1,17,INT64_MAX};
    for(size_t i=0;i<sizeof(ints)/sizeof(*ints);i++) {
        void *left=integer->make(integer->context,ints[i]); size_t left_root=retain(left); double x=(double)ints[i];
        same(field(left,"asFloat"),x);
        for(size_t j=0;j<sizeof(payloads)/sizeof(*payloads);j++) {
            void *right=make_bits(payloads[j]); size_t right_root=retain(right); double y=decode(payloads[j]); BirnamOperand right_arg=oop(right), left_arg=oop(left);
            /* Integer + was replaced before Float construction; do not replace that override. */
            for(size_t op=1;op<4;op++) {
                void *result=send(left,name(operators[op]),&right_arg,1).value.data.object; size_t root=retain(result);
                if(op==3 && y==0.0) error_is(result,"zeroDivide","division by zero"); else same(result,op==1?x-y:op==2?x*y:x/y);
                release_root(root); checked++;
            }
            for(size_t op=0;op<4;op++) {
                void *result=send(right,name(operators[op]),&left_arg,1).value.data.object; size_t root=retain(result);
                if(op==3 && x==0.0) error_is(result,"zeroDivide","division by zero"); else same(result,op==0?y+x:op==1?y-x:op==2?y*x:y/x);
                release_root(root); checked++;
            }
            for(size_t op=0;op<5;op++) {
                int expected=op==0?0:op==1?x<y:op==2?x<=y:op==3?x>y:x>=y;
                assert(send(left,name(comparisons[op]),&right_arg,1).value.data.object==(expected?values.true_object:values.false_object));
            }
            release_root(right_root);
        }
        release_root(left_root);
    }
    /* Copied Integer arithmetic keeps its original mixed receiver behavior. */
    void *copied_left=make(2.5); size_t copied_left_root=retain(copied_left);
    void *copied_right=integer->make(integer->context,2); size_t copied_right_root=retain(copied_right);
    BirnamOperand copied_arg=oop(copied_right);
    for(size_t op=1;op<4;op++) {
        BirnamNativeEntry entry=lookup(integer->behavior,operators[op]).data.entry;
        BirnamOutcome outcome=entry.method(entry.context,copied_left,&copied_arg,1);
        assert(outcome.status==BIRNAM_LOADED); same(outcome.value.data.object,op==1?0.5:op==2?5.0:1.25);
    }
    release_root(copied_right_root); release_root(copied_left_root);
    const double conversions[]={-INFINITY,INFINITY,NAN,-0.0,0.49,0.5,-0.5,1.5,-1.5,-0x1p63,0x1p63,nextafter(0x1p63,0.0),nextafter(-0x1p63,-INFINITY)};
    const char *converters[]={"asInteger","floor","ceiling","rounded"};
    for(size_t i=0;i<sizeof(conversions)/sizeof(*conversions);i++) {
        double x=conversions[i]; void *value=make(x); size_t value_root=retain(value);
        for(size_t op=0;op<4;op++) {
            double expected=op==0?trunc(x):op==1?floor(x):op==2?ceil(x):round(x);
            void *result=field(value,converters[op]); size_t root=retain(result);
            if(!isfinite(expected) || expected < -0x1p63 || expected >= 0x1p63) error_is(result,"overflow","float out of Integer range");
            else { assert(integer->is_exact(integer->context,result)); assert(integer->read(integer->context,result)==(int64_t)expected); }
            release_root(root);
        }
        release_root(value_root);
    }
    const double prints[]={0.0,-0.0,1.5,2.0,NAN,INFINITY,-INFINITY};
    const char *texts[]={"0.0","-0.0","1.5","2.0","NaN.0","inf.0","-inf.0"};
    for(size_t i=0;i<sizeof(prints)/sizeof(*prints);i++) {
        void *value=make(prints[i]); size_t root=retain(value); void *text=field(value,"printString"); size_t text_root=retain(text), length=0;
        const uint8_t *bytes=string->bytes(string->context,text,&length); assert(length==strlen(texts[i]) && !memcmp(bytes,texts[i],length));
        release_root(text_root); release_root(root);
    }
    void *one=make(1.0); size_t one_root=retain(one); BirnamOperand bad=oop(values.true_object);
    void *error=send(one,name("+"),&bad,1).value.data.object; size_t error_root=retain(error);
    error_is(error,"typeMismatch","Float arithmetic needs a number"); BirnamOperand error_arg=oop(error);
    assert(send(one,name("*"),&error_arg,1).value.data.object==error);
    assert(send(one,name("="),&error_arg,1).value.data.object==values.false_object);
    void *old_error= binding("ErrorVt"); size_t old_root=retain(old_error); rebind("ErrorVt",floating->behavior);
    BirnamOperand one_arg=oop(one); assert(send(one,name("*"),&one_arg,1).value.data.object==one);
    rebind("ErrorVt",old_error); release_root(old_root); release_root(error_root);
    BirnamNativeEntry allocate=lookup(id.receiver,"allocate").data.entry;
    BirnamOperand zero={.kind=BIRNAM_WORD,.data.word=0}; small_result=send(floating->behavior,name("allocate"),&zero,1).value.data.object; size_t small_root=retain(small_result);
    install(id.receiver,"allocate",(BirnamNativeEntry){short_allocation,NULL});
    assert(id.message(id.context,floating->prototype,name("fromBits:"),&zero,1).status==BIRNAM_INVALID_REQUEST);
    install(id.receiver,"allocate",allocate); release_root(small_root);
    assert(id.message(id.context,floating->prototype,name("fromBits:"),&bad,1).status==BIRNAM_INVALID_REQUEST);
    assert(id.message(id.context,one,name("bits"),&zero,1).status==BIRNAM_INVALID_REQUEST);
    release_root(one_root);
    kept=make_bits(UINT64_C(0xfff8000000000042)); token=retain(kept); BirnamOperand kept_arg=oop(kept);
    identity=send(id.receiver,name("identityOf:"),&kept_arg,1).value.data.word;
    install(id.receiver,"earlyFloatCheck",(BirnamNativeEntry){check,NULL});
    install(id.receiver,"releaseEarlyFloat",(BirnamNativeEntry){release,NULL});
    check(NULL,NULL,NULL,0);
    printf("early Float: %zu arithmetic cases, comparisons, conversions, bits, errors and allocation checks pass (phase=%zu)\n",checked,args[6].data.word);
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=identity}};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
