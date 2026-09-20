#include "idapi_v11.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <math.h>
#include <string.h>

static IdApi *api;
static IdApi float_api;
static size_t boxes;
static Oop (*original_box)(void *, double);
static Oop counted_box(void *context, double value) { boxes++; return original_box(context, value); }
static unsigned preparation_collections, publication_reentries, allocations;
static Oop collecting_alloc(Oop behavior, size_t size) {
    ++preparation_collections;
    (void)api->gc_collect();
    return api->alloc(behavior, size);
}
static Oop counted_alloc(Oop behavior, size_t size) {
    ++allocations;
    return api->alloc(behavior, size);
}
static Oop reenter_publication(Oop receiver, Oop selector, const Oop *args, size_t count) {
    if (receiver==api->vtable_vt && selector==api->intern("ownerOf:put:")) {
        ++publication_reentries;
        Oop integer=fixture_integer_make(7);
        Oop converted=api->send(integer,api->intern("asFloat"),NULL,0);
        assert(fixture_is_float(converted) && fixture_float_value(converted)==7);
        (void)api->gc_collect();
        assert(api->vtable_of(fixture_float->prototype)==fixture_float->behavior);
    }
    return api->send(receiver,selector,args,count);
}
static void *mount(const char *path, IdApi *selected) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);
    if(!image) fprintf(stderr,"%s\n",dlerror());
    assert(image); fixture_attach_integer(api,image);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    init(selected,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports()); return image;
}
static Oop send(Oop object, const char *name, Oop argument) {
    return api->send(object,api->intern(name),&argument,1);
}
static Oop send0(Oop object, const char *name) {
    return api->send(object,api->intern(name),NULL,0);
}
static Method lookup(const char *name) {
    Oop selector=api->intern(name);
    return (Method)send(fixture_float->behavior,"lookup",selector);
}
static void expect_float(Oop object, double expected) {
    assert(fixture_is_float(object));
    double actual=fixture_float_value(object);
    if(isnan(expected)) assert(isnan(actual));
    else assert(memcmp(&actual,&expected,sizeof(actual))==0);
}
static void expect_error(Oop object, const char *kind) {
    assert(api->vtable_of(object)==fixture_namespace_import("ErrorVt"));
    assert(send0(object,"kind")==api->intern(kind));
}
static Oop identity(Oop self, const Oop *args, size_t count) {
    (void)self; assert(count==1); return args[0];
}
static void expect_text(double value, const char *text) {
    Oop string=send0(fixture_float_make(value),"printString");
    size_t length=0; const uint8_t *bytes=fixture_string_bytes(string,&length);
    assert(length==strlen(text) && !memcmp(bytes,text,length));
}
int main(int argc, char **argv) {
    assert(argc==6);
    void *floor_handle=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor_handle);
    api=((IdInitFn)(fixture_prepare_namespace(floor_handle,argv[1]),dlsym(floor_handle,"id_init")))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]);
    assert(!fixture_namespace_contains("Float") && !fixture_namespace_contains("FloatVt"));
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    float_api=*api; float_api.alloc=collecting_alloc;
    fixture_prepare_float(&float_api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
    assert(preparation_collections==1);
    float_api.alloc=api->alloc;
    fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
    assert(!lookup("+"));
    float_api.send=reenter_publication;
    mount(argv[2],&float_api); /* Float methods work before Prelude, Error or Codegen. */
    assert(publication_reentries==1); float_api.send=api->send;
    FloatProviderV1 provide=(FloatProviderV1)api->dlsym_fn(fixture_float_image,"birnam_float_provider_v1");
    FloatPrepareV4 prepare=(FloatPrepareV4)api->dlsym_fn(fixture_float_image,"birnam_float_prepare_v4");
    assert(provide && prepare && provide()==fixture_float && prepare(&float_api,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports())==fixture_float);
    FloatDependencyV1 attach=(FloatDependencyV1)api->dlsym_fn(floor_handle,"birnam_float_dependency_v1");
    assert(attach && !attach(NULL) && attach(fixture_float)==1);
    FloatApiV1 copy=*fixture_float; copy.version=999; assert(!attach(&copy));
    copy=*fixture_float; assert(!attach(&copy));
    const uint64_t payloads[]={0,1,UINT64_C(0x8000000000000000),UINT64_C(0x7ff0000000000000),
        UINT64_C(0xfff0000000000000),UINT64_C(0x7ff8deadbeef1234),UINT64_C(0x7ff0000000000001)};
    for(size_t i=0;i<sizeof(payloads)/sizeof(*payloads);i++) {
        double value; memcpy(&value,&payloads[i],8);
        Oop boxed=fixture_float_make(value);
        assert(api->object_payload_size(boxed)==8 && api->slot_kind(fixture_float->behavior,0)==IDAPI_V11_SLOT_RAW);
        assert(!memcmp(boxed,&payloads[i],8));
        double read=fixture_float_value(boxed); assert(!memcmp(&read,&payloads[i],8));
        uint8_t raw[10]; memset(raw,0xab,sizeof(raw));
        fixture_float->storage->initialize_bits(raw+1,payloads[i]);
        assert(fixture_float->storage->read_bits(raw+1)==payloads[i] && raw[0]==0xab && raw[9]==0xab);
    }
    assert(!fixture_is_float(NULL) && !fixture_is_float((Oop)(uintptr_t)1));
    Oop inherited=api->alloc(api->proto(fixture_float->behavior),8);
    assert(!fixture_is_float(inherited));
    float_api.alloc=counted_alloc; (void)fixture_float_make(1.25); assert(allocations==1); float_api.alloc=api->alloc;
    Method add=lookup("+"); assert(add);
    Oop two=fixture_float_make(2), zero=fixture_float_make(0);
    expect_float(send(two,"+",fixture_integer_make(3)),5);
    assert(send(two,"/",zero)==api->nil_obj); /* Error is still absent. */
    mount(argv[3],api); /* object model */
    mount(argv[5],api); /* Error */
    const double inputs[]={-INFINITY,-0x1p63,-17.5,-1,-0.0,0.0,0x1p-1074,0.5,1,17.5,0x1p63,INFINITY,NAN};
    const char *ops[]={"+","-","*","/"};
    unsigned checked=0;
    for(size_t i=0;i<sizeof(inputs)/sizeof(*inputs);i++) {
        for(size_t j=0;j<sizeof(inputs)/sizeof(*inputs);j++) {
            double x=inputs[i], y=inputs[j];
            Oop a=fixture_float_make(x), b=fixture_float_make(y);
            for(unsigned op=0;op<4;op++) {
                Oop result=send(a,ops[op],b);
                if(op==3 && y==0) expect_error(result,"zeroDivide");
                else {
                    double expected;
                    switch(op) { case 0: expected=x+y; break; case 1: expected=x-y; break;
                                 case 2: expected=x*y; break; default: expected=x/y; }
                    expect_float(result,expected); assert(result!=a && result!=b);
                }
                checked++;
            }
            assert(send(a,"=",b)==(x==y?api->true_obj:api->false_obj));
            assert(send(a,"<",b)==(x<y?api->true_obj:api->false_obj));
            assert(send(a,"<=",b)==(x<=y?api->true_obj:api->false_obj));
            assert(send(a,">",b)==(x>y?api->true_obj:api->false_obj));
            assert(send(a,">=",b)==(x>=y?api->true_obj:api->false_obj));
        }
        double x=inputs[i]; Oop a=fixture_float_make(x);
        expect_float(send0(a,"negated"),-x);
        expect_float(send0(a,"abs"),fabs(x));
        if(x<0) expect_error(send0(a,"sqrt"),"domain");
        else expect_float(send0(a,"sqrt"),sqrt(x));
        const char *conversions[]={"asInteger","floor","ceiling","rounded"};
        const double expected[]={trunc(x),floor(x),ceil(x),round(x)};
        for(unsigned j=0;j<4;j++) {
            Oop result=send0(a,conversions[j]);
            if(!isfinite(expected[j]) || expected[j]<-0x1p63 || expected[j]>=0x1p63)
                expect_error(result,"overflow");
            else { assert(fixture_is_integer(result)); assert(fixture_integer_value(result)==(int64_t)expected[j]); }
        }
    }
    Oop integer=fixture_integer_make(2), half=fixture_float_make(0.5);
    assert(send(two,"=",integer)==api->false_obj);
    assert(send(two,">=",integer)==api->true_obj);
    expect_float(send(half,"+",integer),2.5);
    Oop error=send(two,"/",zero);
    for(size_t i=0;i<4;i++) assert(send(two,ops[i],error)==error);
    assert(send(two,"<",error)==error && send(two,"=",error)==api->false_obj);
    expect_error(send(two,"+",api->true_obj),"typeMismatch");
    expect_error(send(two,"<",api->true_obj),"typeMismatch");
    assert(add(two,NULL,0)==api->nil_obj && add(two,NULL,1)==api->nil_obj);
    assert(add(api->true_obj,&two,1)==api->nil_obj);
    expect_text(-0.0,"-0.0"); expect_text(2,"2.0"); expect_text(0.5,"0.5");
    expect_text(INFINITY,"inf.0"); expect_text(-INFINITY,"-inf.0"); expect_text(NAN,"NaN.0");
    original_box=fixture_float->make; fixture_float->make=counted_box;
    expect_float(send(two,"+",two),4); assert(boxes==1);
    fixture_float->make=original_box;
    api->method(fixture_float->behavior,api->intern("+"),identity);
    assert(send(two,"+",half)==half);
    Oop prototype=fixture_float->prototype, behavior=fixture_float->behavior;
    fixture_namespace_export("Float",api->nil_obj);
    mount(argv[2],&float_api); /* Reinitialization reinstalls methods, as before extraction. */
    assert(lookup("+")==add && fixture_float->prototype==prototype && fixture_float->behavior==behavior);
    expect_float(send(two,"+",half),2.5);
    assert(fixture_namespace_import("Float")==api->nil_obj); fixture_namespace_export("Float",prototype);
    api->method(behavior,api->intern("+"),identity);
    mount(argv[4],api); /* Later Prelude must preserve the currently selected Float method. */
    assert(lookup("+")==identity && send(two,"+",half)==half);
    api->method(fixture_float->behavior,api->intern("+"),add);
    expect_float(send(two,"+",half),2.5);
    Oop volatile kept=fixture_float_make(-0.0);
    api->gc_set_precise(1); (void)api->gc_collect(); expect_float(kept,-0.0);
    expect_float(send(kept,"+",fixture_float_make(2.5)),2.5);
    printf("float-module: %u IEEE arithmetic cases, ordering, conversions, errors, bits, preparation GC, publication reentry, live constructor, reinitialization and method replacement, GC passed\n",checked);
}
