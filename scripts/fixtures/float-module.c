#include "idapi_v4.h"
#include "integer-support.h"
#include "trace-support.h"
#include <math.h>
#include <string.h>

static IdApi *api;
static IdApi float_api;
static size_t boxes;
static Oop counted_box(double value) { boxes++; return api->mk_float(value); }
static void *mount(const char *path, IdApi *selected) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);
    if(!image) fprintf(stderr,"%s\n",dlerror());
    assert(image); fixture_attach_integer(api,image);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    init(selected,NULL); return image;
}
static Oop send(Oop object, const char *name, Oop argument) {
    return api->send(object,api->intern(name),&argument,1);
}
static Oop send0(Oop object, const char *name) {
    return api->send(object,api->intern(name),NULL,0);
}
static Method lookup(const char *name) {
    Oop selector=api->intern(name);
    return (Method)send(api->float_vt,"lookup",selector);
}
static void expect_float(Oop object, double expected) {
    assert(api->is_float(object));
    double actual=api->float_value(object);
    if(isnan(expected)) assert(isnan(actual));
    else assert(memcmp(&actual,&expected,sizeof(actual))==0);
}
static void expect_error(Oop object, const char *kind) {
    assert(api->vtable_of(object)==api->import_fn("ErrorVt"));
    assert(send0(object,"kind")==api->intern(kind));
}
static Oop identity(Oop self, const Oop *args, size_t count) {
    (void)self; assert(count==1); return args[0];
}
static void expect_text(double value, const char *text) {
    Oop string=send0(api->mk_float(value),"printString");
    size_t length=0; const uint8_t *bytes=api->string_bytes(string,&length);
    assert(length==strlen(text) && !memcmp(bytes,text,length));
}
int main(int argc, char **argv) {
    assert(argc==6);
    void *floor_handle=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor_handle);
    api=((IdInitFn)dlsym(floor_handle,"id_init"))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]); fixture_mount_integer(api,argv[1]);
    assert(!lookup("+"));
    float_api=*api;
    mount(argv[2],&float_api); /* Float methods work before Prelude, Error or Codegen. */
    Method add=lookup("+"); assert(add);
    Oop two=api->mk_float(2), zero=api->mk_float(0);
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
            Oop a=api->mk_float(x), b=api->mk_float(y);
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
        double x=inputs[i]; Oop a=api->mk_float(x);
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
    Oop integer=fixture_integer_make(2), half=api->mk_float(0.5);
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
    float_api.mk_float=counted_box;
    expect_float(send(two,"+",two),4); assert(boxes==1);
    float_api.mk_float=api->mk_float;
    api->method(api->float_vt,api->intern("+"),identity);
    assert(send(two,"+",half)==half);
    mount(argv[4],api); /* Later Prelude must preserve the currently selected Float method. */
    assert(lookup("+")==identity && send(two,"+",half)==half);
    api->method(api->float_vt,api->intern("+"),add);
    expect_float(send(two,"+",half),2.5);
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    Oop volatile kept=api->mk_float(-0.0);
    api->gc_set_precise(1); (void)api->gc_collect(); expect_float(kept,-0.0);
    expect_float(send(kept,"+",api->mk_float(2.5)),2.5);
    printf("float-module: %u IEEE arithmetic cases, ordering, conversions, errors, constructor and method replacement, GC passed\n",checked);
}
