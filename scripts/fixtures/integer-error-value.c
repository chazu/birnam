#include "idapi_v7.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <math.h>

typedef struct { const char *name,*kind,*message; } Spec;
static const Spec specs[]={
 {"+","typeMismatch","Integer arithmetic needs a number"},{"-","typeMismatch","Integer arithmetic needs a number"},
 {"*","typeMismatch","Integer arithmetic needs a number"},{"/","typeMismatch","Integer arithmetic needs a number"},
 {"mod:","typeMismatch","mod: needs an Integer"},{"<","typeMismatch","comparison needs a number"},
 {"<=","typeMismatch","comparison needs a number"},{">","typeMismatch","comparison needs a number"},
 {">=","typeMismatch","comparison needs a number"}
};
static IdApi *api,selected;
static ModuleInitFn integer_init;
static Method methods[9],iterate;
static IntegerProviderV1 integer_provider;
static Oop block,block_result;
static size_t iterations;
static int64_t visited[8];
static int stop_at,publish_type;
static Oop left,high,low,one,negative,zero,right,subject,candidates[5],error_type,other_type;
static Oop answer,second,factory,alternate,factory_export,made,kind_arg,message_arg,seen_factory,constructor_answer;
static size_t reads,interns,makes,exports,imports,vt_calls,sends,calls,lookups,bindings,cases;
static int capture,present,type_present,report_type,change_type,override_kind,override_message,swap_constructor,change_factory,collect,nested;
static const char *expected_kind,*expected_message;
static Oop (*original_make)(void *,const uint8_t *,size_t);
static int64_t (*original_read)(void *,Oop);
static void *string_context;
static void *changed_context=(void *)(uintptr_t)0x1234;
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(const char *name) { return (Method)send1(fixture_integer->behavior,"lookup",api->intern(name)); }
static void roots(MarkFn mark) {
    mark(block);mark(block_result);mark(low);mark(one);mark(left);mark(high);mark(negative);mark(zero);mark(right);mark(subject);mark(error_type);mark(other_type);
    for(size_t i=0;i<5;i++) mark(candidates[i]);
    mark(answer);mark(second);mark(factory);mark(alternate);mark(factory_export);mark(made);mark(kind_arg);mark(message_arg);mark(seen_factory);mark(constructor_answer);
}
static Oop changed_make(void *context,const uint8_t *bytes,size_t length);
static int exported(const char *name) {
    if(strcmp(name,"Error") && strcmp(name,"ErrorVt")) return api->is_exported(name);
    exports++;if(capture) printf("exported %s\n",name);if(collect) api->gc_collect();return !strcmp(name,"Error")?present:type_present;
}
static Oop imported(const char *name) {
    if(strcmp(name,"Error") && strcmp(name,"ErrorVt")) return api->import_fn(name);
    imports++;if(capture) printf("import %s\n",name);if(collect) api->gc_collect();return !strcmp(name,"Error")?factory_export:error_type;
}
static Oop vtable(Oop value) {
    vt_calls++;if(capture) puts("error vtable");if(collect) api->gc_collect();
    Oop type=value==right && report_type?(report_type==1?error_type:other_type):api->vtable_of(value);
    if(change_type && value==right) { change_type=0;error_type=other_type; }
    return type;
}
static Oop intern(const char *name) {
    interns++;if(capture) printf("intern %s\n",name);if(collect) api->gc_collect();
    if(!strcmp(name,"overflow")) {
        if(swap_constructor) { swap_constructor=0;fixture_string->make=changed_make;fixture_string->context=changed_context; }
        if(override_kind) return NULL;
    }
    if(!strcmp(name,"kind:message:") && change_factory==2) { change_factory=0;factory_export=alternate; }
    return api->intern(name);
}
static Oop make(void *context,const uint8_t *bytes,size_t length) {
    assert(context==string_context);makes++;if(capture) printf("make %zu\n",length);
    assert(expected_message && length==strlen(expected_message) && !memcmp(bytes,expected_message,length));
    if(collect) api->gc_collect();
    if(nested) { nested=0;assert(methods[0](high,&one,1)==answer); }
    if(change_factory==1) { change_factory=0;present=1;factory_export=alternate; }
    made=override_message?constructor_answer:original_make(context,bytes,length);return made;
}
static Oop changed_make(void *context,const uint8_t *bytes,size_t length) { assert(context==changed_context);if(capture) puts("changed String constructor/context");return make(string_context,bytes,length); }
static int64_t read_integer(void *context,Oop value) { reads++;if(capture) puts("read Integer");if(collect) api->gc_collect();return original_read(context,value); }
static Oop construct(Oop self,const Oop *args,size_t count) {
    assert(args && count==2);calls++;seen_factory=self;kind_arg=args[0];message_arg=args[1];
    assert(kind_arg==(override_kind?NULL:api->intern(expected_kind)));if(collect) api->gc_collect();return answer;
}
static Oop construct_second(Oop self,const Oop *args,size_t count) { (void)construct(self,args,count);return second; }
static Oop custom_lookup(Oop self,const Oop *args,size_t count) { (void)self;assert(count==1 && args[0]==api->intern("kind:message:"));lookups++;return (Oop)(lookups%2?construct:construct_second); }
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) { sends++;if(capture) printf("send %s %zu\n",api->symbol_name(selector),count);if(collect) api->gc_collect();return api->send(self,selector,args,count); }
static Oop invoke(size_t i) { expected_kind=specs[i].kind;expected_message=specs[i].message;return methods[i](left,&right,1); }
static Oop overflow(void) { expected_kind="overflow";expected_message="integer overflow";return methods[0](high,&one,1); }
static Oop block_call(Oop self,const Oop *args,size_t count) {
    assert(self==block && args && count==1 && iterations<8);
    visited[iterations++]=original_read(fixture_integer->context,args[0]);
    if(publish_type) { publish_type=0;type_present=1;error_type=api->vtable_of(block_result); }
    return stop_at && iterations<(size_t)stop_at?api->nil_obj:block_result;
}
static Oop run_loop(Oop start,Oop end) { iterations=0;Oop args[]={end,block};return iterate(start,args,2); }
static void prepare_values(void) {
    if(fixture_integer) return;
    fixture_integer=integer_provider();assert(fixture_integer && fixture_integer->behavior);
    original_read=fixture_integer->read;fixture_integer->read=read_integer;
    left=fixture_integer_make(2);high=fixture_integer_make(INT64_MAX);low=fixture_integer_make(INT64_MIN);
    one=fixture_integer_make(1);negative=fixture_integer_make(-1);zero=fixture_integer_make(0);right=api->true_obj;
}
static void method(Oop type,Oop selector,Method implementation) {
    if(capture) printf("method %s\n",api->symbol_name(selector));
    for(size_t i=0;i<9;i++) if(selector==api->intern(specs[i].name)) {
        prepare_values();bindings++;assert(type==fixture_integer->behavior);methods[i]=implementation;
        assert(implementation(left,NULL,2)==selected.nil_obj);
        assert(invoke(i)==selected.nil_obj);
    }
    if(selector==api->intern("to:do:")) {
        bindings++;iterate=implementation;block_result=api->nil_obj;
        assert(run_loop(zero,left)==zero && iterations==3);
    }
    api->method(type,selector,implementation);
}
static void begin(const char *name) { printf("case %s\n",name);capture=1;reads=interns=makes=exports=imports=vt_calls=sends=calls=0; }
static void end(void) { capture=0;printf("done read=%zu intern=%zu make=%zu export=%zu import=%zu vtable=%zu send=%zu calls=%zu\n",reads,interns,makes,exports,imports,vt_calls,sends,calls); }
static void mount(const char *path) { void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);fixture_attach_integer(api,image);ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);init(api,NULL); }
int main(int argc,char **argv) {
    assert(argc==4);setvbuf(stdout,NULL,_IONBF,0);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))();assert(api);api->gc_set_auto(0);fixture_mount_trace(api,argv[1]);
    uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));api->gc_register_root_provider(roots);
    selected=*api;selected.is_exported=exported;selected.import_fn=imported;selected.vtable_of=vtable;selected.intern=intern;selected.send=send;selected.method=method;
    fixture_prepare_string(api,argv[1]);fixture_prepare_word(api,argv[1]);fixture_prepare_float(api,argv[1]);
    original_make=fixture_string->make;string_context=fixture_string->context;fixture_string->make=make;
    Oop block_type=api->proto(api->object_vt);block=api->alloc(block_type,0);api->method(block_type,api->intern("value:"),block_call);
    void *image=api->dlopen_fn(argv[3],RTLD_NOW|RTLD_LOCAL);assert(image);
    integer_provider=(IntegerProviderV1)api->dlsym_fn(image,"birnam_integer_provider_v1");assert(integer_provider);
    integer_init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(integer_init);
    begin("all affected methods run during registration");fixture_mount_integer(&selected,argv[1]);end();assert(bindings==10);
    mount(argv[2]);
    answer=api->alloc(api->object_vt,0);second=api->alloc(api->object_vt,0);error_type=api->proto(api->object_vt);other_type=api->proto(error_type);
    Oop type=api->proto(api->object_vt);factory=api->alloc(type,0);alternate=api->alloc(type,0);factory_export=factory;api->method(type,api->intern("kind:message:"),construct);
    for(size_t i=0;i<9;i++) assert(lookup(specs[i].name)==methods[i]);
    begin("invalid arities skip policy");for(size_t i=0;i<9;i++) assert(methods[i](left,(Oop *)(uintptr_t)1,2)==api->nil_obj);end();assert(!reads && !makes && !exports);
    begin("null binary carriers retain earlier receiver read");for(size_t i=0;i<9;i++) assert(methods[i](left,NULL,1)==api->nil_obj);end();assert(reads==9 && !exports && !makes);
    begin("absent factory still constructs kind and message");assert(overflow()==api->nil_obj);end();assert(reads==2 && interns==1 && makes==1 && exports==2 && !imports);
    present=1;factory_export=NULL;selected.nil_obj=second;begin("null factory returns live nil");assert(overflow()==second);end();selected.nil_obj=api->nil_obj;factory_export=factory;
    present=-1;begin("all existing mismatch messages");for(size_t i=0;i<9;i++) { assert(invoke(i)==answer);cases++; }end();present=1;
    Oop overflow_left[]={high,low,high,low},overflow_right[]={one,one,left,negative};
    expected_kind="overflow";expected_message="integer overflow";
    for(size_t i=0;i<4;i++) { assert(methods[i](overflow_left[i],&overflow_right[i],1)==answer);cases++; }
    assert(fixture_integer_value(methods[4](low,&negative,1))==0);
    expected_kind="zeroDivide";expected_message="division by zero";
    for(size_t i=3;i<5;i++) { assert(methods[i](left,&zero,1)==answer);cases++; }
    for(int sign=0;sign<2;sign++) { subject=fixture_float_make(sign?-0.0:0.0);assert(methods[3](left,&subject,1)==answer);cases++; }
    for(int k=0;k<2;k++) for(int m=0;m<3;m++) for(int r=0;r<3;r++) {
        override_kind=k;override_message=m!=0;constructor_answer=m==1?NULL:left;Oop saved=answer;answer=r==0?NULL:r==1?second:saved;
        assert(overflow()==answer && message_arg==(m?constructor_answer:made));answer=saved;cases++;
    }
    override_kind=override_message=0;
    candidates[0]=NULL;candidates[1]=api->alloc(error_type,0);candidates[2]=api->alloc(other_type,0);candidates[3]=api->nil_obj;candidates[4]=api->true_obj;Oop saved_type=error_type;
    for(size_t i=0;i<9;i++) for(size_t v=0;v<5;v++) for(int flag=0;flag<4;flag++) for(report_type=1;report_type<=2;report_type++) {
        right=candidates[v];type_present=flag==0?0:flag==3?-1:1;error_type=flag==1?NULL:saved_type;
        int recognized=right && type_present && error_type && report_type==1;
        assert(invoke(i)==(recognized?right:answer));cases++;
    }
    assert(cases==395);type_present=1;error_type=saved_type;report_type=0;
    right=candidates[1];begin("equality does not recognize or propagate Error");assert(lookup("=")(left,&right,1)==api->false_obj);end();assert(!exports && !makes);
    right=candidates[2];begin("inherited Error does not propagate");for(size_t i=0;i<9;i++) assert(invoke(i)==answer);end();
    right=zero;error_type=fixture_integer->behavior;begin("numeric Integer Error alias wins before division and modulus");for(size_t i=0;i<9;i++) assert(invoke(i)==right);end();assert(!makes);
    right=fixture_float_make(0.0);error_type=fixture_float->behavior;begin("numeric Float Error alias wins before conversion and zero division");for(size_t i=0;i<9;i++) assert(invoke(i)==right);end();assert(!makes);
    right=candidates[1];error_type=saved_type;report_type=1;change_type=collect=1;
    begin("recognition keeps imported behavior snapshot across GC");assert(invoke(0)==right && error_type==other_type);end();collect=report_type=0;error_type=saved_type;right=api->true_obj;
    swap_constructor=1;begin("kind callback replaces String constructor and context");assert(overflow()==answer);end();fixture_string->make=make;fixture_string->context=string_context;
    present=0;change_factory=1;begin("message creation publishes factory");assert(overflow()==answer && seen_factory==alternate);end();factory_export=factory;
    change_factory=2;begin("selector callback keeps imported factory snapshot");assert(overflow()==answer && seen_factory==factory && factory_export==alternate);end();factory_export=factory;
    begin("fresh replacement");assert(overflow()==answer);api->method(type,api->intern("kind:message:"),construct_second);assert(overflow()==second);end();
    Oop meta=api->proto(api->vtable_vt);api->method(meta,api->intern("lookup"),custom_lookup);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){type,meta},2);
    begin("custom factory lookup");assert(overflow()==answer);assert(overflow()==second);end();assert(lookups==2);
    api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){type,api->vtable_vt},2);api->method(type,api->intern("kind:message:"),construct);
    collect=nested=1;begin("nested construction and callback GC");assert(overflow()==answer);end();collect=0;assert(calls==2 && makes==2);
    begin("iteration propagates exact Error and stops at first result");block_result=candidates[1];stop_at=2;collect=1;assert(run_loop(zero,left)==block_result && iterations==2 && visited[0]==0 && visited[1]==1);end();collect=stop_at=0;
    begin("iteration ignores inherited Error and missing Error export");block_result=candidates[2];assert(run_loop(zero,left)==zero && iterations==3);block_result=candidates[1];type_present=0;assert(run_loop(zero,left)==zero && iterations==3);end();
    begin("iteration rechecks export after block callback");publish_type=1;assert(run_loop(zero,left)==block_result && iterations==1);end();
    begin("iteration accepts numeric Error alias and retains max endpoint");block_result=one;error_type=fixture_integer->behavior;assert(run_loop(zero,left)==one && iterations==1);error_type=saved_type;block_result=NULL;assert(run_loop(high,high)==high && iterations==1 && visited[0]==INT64_MAX);assert(run_loop(left,zero)==left && !iterations);end();
    Oop behavior=fixture_integer->behavior,prototype=fixture_integer->prototype;api->method(behavior,api->intern("+"),construct_second);
    begin("copied method survives replacement and reload does not reinstall");assert(overflow()==answer);integer_init(&selected,NULL);end();assert(fixture_integer->behavior==behavior && fixture_integer->prototype==prototype && bindings==10 && lookup("+")==construct_second);
    api->gc_set_precise(1);api->gc_collect();api->gc_set_precise(0);assert(overflow()==answer);
    fixture_string->make=original_make;fixture_string->context=string_context;fixture_integer->read=original_read;
    printf("integer-error-value: %zu failure/recognition/construction cases, registration calls, iteration, precedence, live callbacks, lookup, reentry, GC and reload passed\n",cases);
}
