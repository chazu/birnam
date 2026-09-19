#include "idapi_v6.h"
#include "integer-support.h"
#include "trace-support.h"
#include <math.h>

static IdApi *api;
static IdApi string_api;
static RootProvider provider;
static unsigned registrations, marked_selector, strings, integers, floats, reads, sends;
static Oop print_selector;
static Oop (*original_integer)(void *, int64_t);
static Oop (*original_float)(void *, double);
static void register_roots(RootProvider roots) {
    ++registrations; provider=roots; api->gc_register_root_provider(roots);
}
static void record_mark(Oop object) { if(object==print_selector) ++marked_selector; }
static Oop counted_string(const uint8_t *bytes, size_t length) { ++strings; return api->mk_string(bytes,length); }
static Oop counted_integer(void *context, int64_t value) { ++integers; return original_integer(context,value); }
static Oop counted_float(void *context, double value) { ++floats; return original_float(context,value); }
static const uint8_t *shim_bytes(Oop object, size_t *length) {
    (void)object; ++reads; *length=4; return (const uint8_t *)"shim";
}
static Oop forwarded(Oop receiver, Oop selector, const Oop *args, size_t count) {
    if(selector==print_selector) ++sends;
    return api->send(receiver,selector,args,count);
}
static void mount(const char *path, IdApi *selected) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    fixture_attach_integer(api,image);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init); init(selected,NULL);
}
static Oop raw(const uint8_t *bytes, size_t length) { return api->mk_string(bytes,length); }
static Oop text(const char *value) { return raw((const uint8_t *)value,strlen(value)); }
static Oop send0(Oop self, const char *name) { return api->send(self,api->intern(name),NULL,0); }
static Oop send(Oop self, const char *name, Oop arg) { return api->send(self,api->intern(name),&arg,1); }
static Oop send2(Oop self, const char *name, Oop a, Oop b) {
    Oop args[]={a,b}; return api->send(self,api->intern(name),args,2);
}
static Method lookup(const char *name) { return (Method)send(api->string_vt,"lookup",api->intern(name)); }
static void expect_bytes(Oop value, const uint8_t *expected, size_t length) {
    assert(api->vtable_of(value)==api->string_vt);
    size_t count=0; const uint8_t *bytes=api->string_bytes(value,&count);
    assert(count==length && (!length || !memcmp(bytes,expected,length)));
}
static void expect_text(Oop value, const char *expected) { expect_bytes(value,(const uint8_t *)expected,strlen(expected)); }
static void expect_integer(Oop value, int64_t expected) {
    assert(fixture_is_integer(value) && fixture_integer_value(value)==expected);
}
static void expect_float(Oop value, double expected) {
    assert(fixture_is_float(value)); double actual=fixture_float_value(value);
    assert(isnan(expected)?isnan(actual):!memcmp(&actual,&expected,sizeof(actual)));
}
static Oop print_one(Oop self, const Oop *args, size_t count) {
    (void)self; (void)args; assert(!count); return text("one");
}
static Oop print_two(Oop self, const Oop *args, size_t count) {
    (void)self; (void)args; assert(!count); (void)api->gc_collect(); return text("two");
}
static Oop identity(Oop self, const Oop *args, size_t count) { (void)args; (void)count; return self; }

int main(int argc, char **argv) {
    assert(argc==5 || (argc==6 && !strcmp(argv[5],"baseline")));
    int baseline=argc==6;
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    api=((IdInitFn)dlsym(floor,"id_init"))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]); fixture_mount_integer(api,argv[1]); mount(argv[2],api);
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    assert(!lookup("size")); string_api=*api; string_api.gc_register_root_provider=register_roots;
    mount(argv[3],&string_api); assert(registrations==1 && provider);
    print_selector=api->intern("printString"); provider(record_mark); assert(marked_selector==1);
    if(!baseline) assert(!api->is_exported("Array") && !lookup("split:"));
    static const uint8_t binary[]={0,'a',0xff}, utf8[]={0xc3,0xa9,0xff};
    struct Bytes { const uint8_t *p; size_t n; } samples[]={
        {(const uint8_t *)"",0},{(const uint8_t *)"a",1},{binary,sizeof(binary)},
        {utf8,sizeof(utf8)},{(const uint8_t *)"ababa",5},{(const uint8_t *)"\t A\n",4},
    };
    unsigned checked=0;
    for(size_t i=0;i<sizeof(samples)/sizeof(*samples);i++) {
        struct Bytes a=samples[i]; Oop left=raw(a.p,a.n);
        assert(send0(left,"printString")==left); expect_integer(send0(left,"size"),(int64_t)a.n);
        for(int64_t index=-1;index<=(int64_t)a.n;index++) {
            Oop at=send(left,"at:",fixture_integer_make(index));
            Oop byte=send(left,"byteAt:",fixture_integer_make(index));
            if(index<0 || index==(int64_t)a.n) assert(at==api->nil_obj && byte==api->nil_obj);
            else { expect_bytes(at,a.p+index,1); expect_integer(byte,a.p[index]); }
            checked+=2;
        }
        for(int64_t from=-1;from<=(int64_t)a.n+1;from++) {
            for(int64_t to=-1;to<=(int64_t)a.n+1;to++) {
                Oop result=send2(left,"copyFrom:to:",fixture_integer_make(from),fixture_integer_make(to));
                if(from<0 || to<from || to>(int64_t)a.n) assert(result==api->nil_obj);
                else { expect_bytes(result,a.p+from,(size_t)(to-from)); assert(result!=left); }
                checked++;
            }
        }
        for(size_t j=0;j<sizeof(samples)/sizeof(*samples);j++) {
            struct Bytes b=samples[j]; Oop right=raw(b.p,b.n);
            assert(send(left,"=",right)==((a.n==b.n && !memcmp(a.p,b.p,a.n))?api->true_obj:api->false_obj));
            uint8_t joined[32]; memcpy(joined,a.p,a.n); memcpy(joined+a.n,b.p,b.n);
            Oop concat=send(left,",",right); expect_bytes(concat,joined,a.n+b.n); assert(concat!=left && concat!=right);
            int64_t index=-1;
            for(size_t k=0;k+b.n<=a.n;k++) if(!memcmp(a.p+k,b.p,b.n)) { index=(int64_t)k; break; }
            Oop found=send(left,"indexOf:",right);
            if(index<0) assert(found==api->nil_obj); else expect_integer(found,index);
            uint8_t replaced[32]; size_t used=0;
            for(size_t k=0;k<a.n;) {
                if(b.n && k+b.n<=a.n && !memcmp(a.p+k,b.p,b.n)) { replaced[used++]='X'; k+=b.n; }
                else replaced[used++]=a.p[k++];
            }
            Oop result=send2(left,"replace:with:",right,text("X")); expect_bytes(result,replaced,used); assert(result!=left);
            checked+=4;
        }
    }
    expect_text(send0(text(" \t\v\fhello\r\n"),"trimmed"),"hello");
    expect_text(send0(text(" \v\f"),"trimmed"),"");
    assert(send0(raw((const uint8_t *)"ab\0cd",5),"asSymbol")==api->intern("ab"));
    struct Depth { const char *text; int64_t depth; } depths[]={
        {"",0},{"[]{}",0},{"]}",-2},{"[{{",3},{"\"[\"",0},{"//[\n{",1},
        {"\"a\\\"[\" {",1},{"\"unterminated[",0},{"/*[",1},
    };
    for(size_t i=0;i<sizeof(depths)/sizeof(*depths);i++) expect_integer(send0(text(depths[i].text),"bracketDepth"),depths[i].depth);
    struct Number { const char *text; int integer; int64_t i; int number_kind; double f; } numbers[]={
        {"123",1,123,1,0},{"  -9223372036854775808 \t",1,INT64_MIN,1,0},
        {"9223372036854775807",1,INT64_MAX,1,0},{"9223372036854775808",0,0,2,0x1p63},
        {"-0",1,0,1,0},{"+42",1,42,1,0},{"-0.0",0,0,2,-0.0},
        {"1.25",0,0,2,1.25},{"1e2",0,0,2,100},{"inf",0,0,2,INFINITY},
        {"-inf",0,0,2,-INFINITY},{"NaN",0,0,2,NAN},{"3.",0,0,2,3},{".5",0,0,2,0.5},
        {" \v42 \f",0,0,0,0},{"",0,0,0,0},{"0x10",0,0,0,0},{"1_000",0,0,0,0},
    };
    for(size_t i=0;i<sizeof(numbers)/sizeof(*numbers);i++) {
        Oop value=text(numbers[i].text), integer=send0(value,"asInteger"), number=send0(value,"asNumber");
        if(numbers[i].integer) expect_integer(integer,numbers[i].i); else assert(integer==api->nil_obj);
        if(numbers[i].number_kind==1) expect_integer(number,numbers[i].i);
        else if(numbers[i].number_kind==2) expect_float(number,numbers[i].f);
        else assert(number==api->nil_obj);
    }
    assert(send0(raw(binary,sizeof(binary)),"asInteger")==api->nil_obj);
    assert(send0(raw(utf8,sizeof(utf8)),"asNumber")==api->nil_obj);
    const char *names[]={"printString",",","=","size","at:","byteAt:","copyFrom:to:","trimmed","indexOf:","bracketDepth","replace:with:","asSymbol","asInteger","asNumber"};
    const size_t arities[]={0,1,1,0,1,1,2,0,1,0,2,0,0,0};
    Oop kept=text("kept"); api->export_obj("StringTestKept",kept);
    for(unsigned i=0;i<14;i++) {
        Method method=lookup(names[i]); assert(method);
        assert(method(kept,NULL,arities[i]+1)==api->nil_obj);
        if(arities[i]) assert(method(kept,NULL,arities[i])==api->nil_obj);
    }
    assert(lookup("printString")(api->true_obj,NULL,0)==api->true_obj);
    assert(send(kept,"=",api->true_obj)==api->false_obj);
    assert(send(kept,"at:",api->true_obj)==api->nil_obj);
    Oop inherited=text("kept"), child=api->proto(api->string_vt), reassociate[]={inherited,child};
    api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),reassociate,2);
    assert(send0(inherited,"printString")==inherited && send0(inherited,"size")==api->nil_obj);
    assert(send(inherited,"=",kept)==api->false_obj);
    string_api.string_bytes=shim_bytes; expect_integer(send0(kept,"size"),4); assert(reads==1); string_api.string_bytes=api->string_bytes;
    string_api.mk_string=counted_string; expect_text(send0(kept,"trimmed"),"kept"); assert(strings==1); string_api.mk_string=api->mk_string;
    original_integer=fixture_integer->make; fixture_integer->make=counted_integer;
    expect_integer(send0(text("42"),"asInteger"),42); assert(integers==1); fixture_integer->make=original_integer;
    original_float=fixture_float->make; fixture_float->make=counted_float;
    expect_float(send0(text("1.5"),"asNumber"),1.5); assert(floats==1); fixture_float->make=original_float;
    Oop printer_vt=api->proto(api->object_vt), printer=api->alloc(printer_vt,0);
    api->export_obj("StringTestPrinter",printer); api->method(printer_vt,print_selector,print_one);
    string_api.send=forwarded; expect_text(send(kept,",",printer),"keptone"); assert(sends==1);
    api->method(printer_vt,print_selector,print_two); expect_text(send(kept,",",printer),"kepttwo"); assert(sends==2);
    api->method(printer_vt,print_selector,identity); Oop fallback=send(kept,",",printer); expect_text(fallback,"kept"); assert(fallback!=kept);
    string_api.send=api->send;
    Method size=lookup("size"); api->method(api->string_vt,api->intern("size"),identity);
    Oop prototype=api->import_fn("String"); api->export_obj("String",api->nil_obj);
    mount(argv[3],&string_api); assert(registrations==1 && lookup("size")==size);
    assert(api->import_fn("String")==api->nil_obj); api->export_obj("String",prototype);
    api->method(api->string_vt,api->intern("size"),identity);
    if(!baseline) { mount(argv[4],api); assert(lookup("size")==identity && send0(kept,"size")==kept); }
    api->method(api->string_vt,api->intern("size"),size);
    Oop array=send(text(",a,"),"split:",text(",")); expect_integer(send0(array,"size"),3);
    expect_text(send(array,"at:",fixture_integer_make(0)),"");
    expect_text(send(array,"at:",fixture_integer_make(1)),"a");
    expect_text(send(array,"at:",fixture_integer_make(2)),"");
    api->gc_set_precise(1); (void)api->gc_collect(); expect_integer(send0(kept,"size"),4);
    marked_selector=0; provider(record_mark); assert(marked_selector==1);
    printf("string-module: %u binary/index/slice cases, 18 numeric boundaries, trimming, search, replacement, symbols, bracket depth, exact recognition, live operations, roots, reinitialization and GC passed%s\n",checked,baseline?" (Collection baseline)":"");
}
