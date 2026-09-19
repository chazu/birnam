#include "idapi_v7.h"
#include "integer-support.h"
#include "trace-support.h"

static IdApi *api,selected,other;
static ModuleInitFn prelude_init;
static Method loops[2];
static const char *names[]={"whileTrue:","whileFalse:"};
static Oop block_type,block_object,alternate,expected_target,selectors[2][2],seen_selectors[2],seen_targets[2];
static Oop condition,body,nested_condition,bad_condition,alias,error_value,inherited_error,error_type,original_error_type,alternate_error_type;
static Oop factory,second_factory,factory_export,answer,second_answer,made,seen_factory,con[32],bod[32],candidate;
static Oop condition_type,body_type,value_selectors[2],changing_args[1];
static Oop (*original_make)(void *,const uint8_t *,size_t);
static void *string_context,*changed_context=(void *)(uintptr_t)0x1234;
static size_t bindings,registrations,allocations,interns,sends,exports,imports,vt_calls,makes,calls,lookups,cases,con_count,con_step,body_step;
static int capture,reentry_mode,reentered,map_selectors,map_generation,change_snapshot,collect,nested;
static int present,type_present,ctor_mode,change_factory,change_constructor,current_phase,recognition_phase,reported_match,mutate_type;
static int mutate_boolean,mutate_nil,mutate_methods,map_value,value_generation,mutate_args,null_receiver;
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(Oop type,Oop selector) { return (Method)send1(type,"lookup",selector); }
static void roots(MarkFn mark) {
    mark(block_type);mark(block_object);mark(alternate);mark(expected_target);mark(condition);mark(body);mark(nested_condition);mark(bad_condition);
    mark(alias);mark(error_value);mark(inherited_error);mark(error_type);mark(original_error_type);mark(alternate_error_type);
    mark(factory);mark(second_factory);mark(factory_export);mark(answer);mark(second_answer);mark(made);mark(seen_factory);mark(candidate);mark(condition_type);mark(body_type);
    mark(value_selectors[0]);mark(value_selectors[1]);mark(changing_args[0]);
    for(size_t i=0;i<32;i++) { mark(con[i]);mark(bod[i]); }
    for(size_t g=0;g<2;g++) for(size_t i=0;i<2;i++) mark(selectors[g][i]);
}
static void registered(RootProvider provider) { registrations++;api->gc_register_root_provider(provider); }
static Oop allocate(Oop type,size_t size) { allocations++;if(capture) printf("alloc %zu\n",size);return api->alloc(type,size); }
static void reenter(void) { reentry_mode=0;reentered=1;prelude_init(&other,NULL); }
static Oop changed_make(void *context,const uint8_t *bytes,size_t length);
static Oop intern(const char *name) {
    interns++;if(capture) printf("intern %s\n",name);if(collect) api->gc_collect();
    if(reentry_mode==1 && !strcmp(name,names[0])) reenter();
    if(map_selectors) for(size_t i=0;i<2;i++) if(!strcmp(name,names[i])) return selectors[map_generation][i];
    if(map_value && !strcmp(name,"value")) return value_selectors[value_generation];
    if(change_factory==2 && !strcmp(name,"kind:message:")) { change_factory=0;factory_export=second_factory; }
    if(change_constructor && !strcmp(name,"mustBeBoolean")) { change_constructor=0;fixture_string->make=changed_make;fixture_string->context=changed_context; }
    return api->intern(name);
}
static int exported(const char *name) {
    if(strcmp(name,"Error") && strcmp(name,"ErrorVt")) return api->is_exported(name);
    exports++;if(capture) printf("exported %s\n",name);if(collect) api->gc_collect();return !strcmp(name,"Error")?present:type_present;
}
static Oop imported(const char *name) {
    if(strcmp(name,"Error") && strcmp(name,"ErrorVt")) return api->import_fn(name);
    imports++;if(capture) printf("import %s\n",name);if(collect) api->gc_collect();return !strcmp(name,"Error")?factory_export:error_type;
}
static Oop vtable(Oop value) {
    vt_calls++;if(capture) puts("vtable");if(collect) api->gc_collect();
    if(mutate_boolean && current_phase==1) { mutate_boolean=0;selected.true_obj=alias;selected.false_obj=api->true_obj; }
    if(mutate_nil && current_phase==2) { mutate_nil=0;selected.nil_obj=alias; }
    if(recognition_phase && current_phase==recognition_phase && value==candidate) {
        Oop result=reported_match?error_type:alternate_error_type;
        if(mutate_type) { mutate_type=0;error_type=alternate_error_type; }
        return result;
    }
    return api->vtable_of(value);
}
static Oop forbidden_intern(const char *name) { (void)name;assert(!"registration table became execution table");return NULL; }
static Oop send(Oop self,Oop selector,const Oop *args,size_t count);
static Oop forbidden_send(Oop self,Oop selector,const Oop *args,size_t count) {
    /* Reentry before System exists legitimately assigns its vtable owner. */
    if(selector==api->intern("ownerOf:put:")) return send(self,selector,args,count);
    assert(!"registration table became execution table");return NULL;
}
static int forbidden_exported(const char *name) { if(strcmp(name,"Error") && strcmp(name,"ErrorVt")) return api->is_exported(name);assert(!"registration table became execution table");return 0; }
static Oop stop(Oop self,const Oop *args,size_t count) { (void)self;assert(!args && !count);return api->false_obj; }
static Oop bad(Oop self,const Oop *args,size_t count) { (void)self;assert(!args && !count);return api->nil_obj; }
static Oop failed_body(Oop self,const Oop *args,size_t count) { (void)self;assert(!args && !count);body_step++;return error_value; }
static Oop condition_value(Oop self,const Oop *args,size_t count) {
    assert((self==condition || (null_receiver && !self)) && !args && !count && con_step<con_count);
    current_phase=1;Oop result=con[con_step++];if(capture) puts("condition value");if(collect) api->gc_collect();
    if(mutate_args) { mutate_args=0;changing_args[0]=bad_condition; }
    if(mutate_methods==2) { mutate_methods=0;api->method(body_type,api->intern("value"),failed_body); }
    if(map_value) value_generation=1;
    if(nested) { nested=0;assert(loops[0](nested_condition,&body,1)==selected.nil_obj); }
    return result;
}
static Oop body_value(Oop self,const Oop *args,size_t count) {
    assert(self==body && !args && !count && body_step<32);current_phase=2;
    if(capture) puts("body value");if(collect) api->gc_collect();
    if(mutate_methods==1) { mutate_methods=0;api->method(condition_type,api->intern("value"),stop); }
    if(map_value) value_generation=0;
    return bod[body_step++];
}
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) {
    sends++;if(capture) printf("send %s %zu\n",api->symbol_name(selector),count);if(collect) api->gc_collect();
    if(null_receiver && !self) { assert(selector==api->intern("value"));return condition_value(self,args,count); }
    return api->send(self,selector,args,count);
}
static void setup(size_t op,size_t iterations,Oop terminal,Oop body_result) {
    assert(iterations+2<=32);con_step=body_step=0;con_count=iterations+2;
    for(size_t i=0;i<iterations;i++) con[i]=op?api->false_obj:api->true_obj;
    con[iterations]=terminal;con[iterations+1]=op?api->true_obj:api->false_obj;
    for(size_t i=0;i<32;i++) bod[i]=body_result;
}
static Oop make(void *context,const uint8_t *bytes,size_t length) {
    const char *message="while loop condition must answer true or false";
    assert(context==string_context && length==strlen(message) && !memcmp(bytes,message,length));makes++;if(capture) printf("make %zu\n",length);if(collect) api->gc_collect();
    if(nested) { nested=0;assert(loops[0](bad_condition,&body,1)==answer); }
    if(change_factory==1) { change_factory=0;present=1;factory_export=second_factory; }
    made=ctor_mode==1?NULL:ctor_mode==2?alias:original_make(context,bytes,length);return made;
}
static Oop changed_make(void *context,const uint8_t *bytes,size_t length) { assert(context==changed_context);if(capture) puts("changed constructor/context");return make(string_context,bytes,length); }
static Oop construct(Oop self,const Oop *args,size_t count) {
    assert(args && count==2 && args[0]==api->intern("mustBeBoolean") && args[1]==made);calls++;seen_factory=self;if(collect) api->gc_collect();return answer;
}
static Oop construct_second(Oop self,const Oop *args,size_t count) { (void)construct(self,args,count);return second_answer; }
static Oop custom_factory_lookup(Oop self,const Oop *args,size_t count) { (void)self;assert(count==1 && args[0]==api->intern("kind:message:"));lookups++;return (Oop)(lookups%2?construct:construct_second); }
static Oop custom_condition_lookup(Oop self,const Oop *args,size_t count) { (void)self;assert(count==1 && args[0]==api->intern("value"));lookups++;return (Oop)(lookups%2?condition_value:stop); }
static void method(Oop type,Oop selector,Method implementation) {
    if(capture) printf("method %s\n",api->symbol_name(selector));
    for(size_t i=0;i<2;i++) if(selector==api->intern(names[i]) || (selectors[0][i] && (selector==selectors[0][i] || selector==selectors[1][i]))) {
        bindings++;seen_selectors[i]=selector;seen_targets[i]=type;loops[i]=implementation;
        if(expected_target) assert(type==expected_target);
        assert(implementation(NULL,(Oop *)(uintptr_t)1,0)==selected.nil_obj);
        setup(i,0,i?api->true_obj:api->false_obj,api->nil_obj);assert(implementation(condition,&body,1)==selected.nil_obj && con_step==1 && !body_step);
        if(i==0) {
            if(reentry_mode==2) reenter();
            if(change_snapshot) { change_snapshot=0;map_generation=1;api->export_obj("BlockVt",alternate); }
        }
    }
    api->method(type,selector,implementation);
}
static void begin(const char *name) { printf("case %s\n",name);capture=1;interns=sends=exports=imports=vt_calls=makes=calls=0; }
static void end(void) { capture=0;printf("done intern=%zu send=%zu export=%zu import=%zu vtable=%zu make=%zu factory=%zu\n",interns,sends,exports,imports,vt_calls,makes,calls); }
static void validate(void *image,int attached) {
    BlockLoopDependencyV1 attach=(BlockLoopDependencyV1)api->dlsym_fn(image,"birnam_block_loop_dependency_v1");if(!attach) return;
    BlockLoopApiV1 bad_api=*fixture_block_loop;assert(!attach(NULL));bad_api.version++;assert(!attach(&bad_api));bad_api=*fixture_block_loop;
    bad_api.byte_size--;assert(!attach(&bad_api));bad_api=*fixture_block_loop;bad_api.install=NULL;assert(!attach(&bad_api));
    if(attached) { bad_api=*fixture_block_loop;assert(!attach(&bad_api) && attach(fixture_block_loop)==1); }
}
static void mount(const char *path,IdApi *table,int prelude) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    if(prelude && api->dlsym_fn(image,"birnam_block_loop_dependency_v1") && !fixture_block_loop) {
        IdApi watched=*api;watched.alloc=allocate;watched.method=method;watched.gc_register_root_provider=registered;
        size_t before=allocations;fixture_prepare_block_loop(&watched);
        assert(!bindings && !registrations && allocations==before && !api->is_exported("Block"));validate(image,0);
    }
    fixture_attach_integer(api,image);ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);
    if(prelude) { validate(image,1);prelude_init=init; }init(table,NULL);
}
static Oop failure(void) { return loops[0](bad_condition,&body,1); }
static Oop replacement(Oop self,const Oop *args,size_t count) { (void)self;(void)args;(void)count;return alias; }
int main(int argc,char **argv) {
    assert(argc==5);setvbuf(stdout,NULL,_IONBF,0);
    reentry_mode=!strcmp(argv[4],"first-while-intern-reentry")?1:(!strcmp(argv[4],"first-while-binding-reentry")?2:0);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]);fixture_mount_integer(api,argv[1]);uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));mount(argv[2],api,0);api->gc_register_root_provider(roots);
    condition_type=api->proto(api->object_vt);body_type=api->proto(api->object_vt);condition=api->alloc(condition_type,0);body=api->alloc(body_type,0);
    api->method(condition_type,api->intern("value"),condition_value);api->method(body_type,api->intern("value"),body_value);
    Oop type=api->proto(api->object_vt);nested_condition=api->alloc(type,0);api->method(type,api->intern("value"),stop);
    type=api->proto(api->object_vt);bad_condition=api->alloc(type,0);api->method(type,api->intern("value"),bad);
    alias=api->alloc(api->object_vt,0);original_make=fixture_string->make;string_context=fixture_string->context;fixture_string->make=make;
    selected=*api;selected.intern=intern;selected.method=method;selected.alloc=allocate;selected.gc_register_root_provider=registered;selected.send=send;selected.is_exported=exported;selected.import_fn=imported;selected.vtable_of=vtable;
    other=selected;other.nil_obj=api->true_obj;other.true_obj=other.false_obj=NULL;other.send=forbidden_send;other.is_exported=forbidden_exported;
    begin("initial registration");mount(argv[3],&selected,1);end();assert(bindings==(reentered?4u:2u) && !registrations);
    block_type=api->import_fn("BlockVt");block_object=api->import_fn("Block");
    for(size_t i=0;i<2;i++) { loops[i]=lookup(block_type,api->intern(names[i]));Dl_info owner;assert(loops[i] && dladdr((const void *)loops[i],&owner));assert(strstr(owner.dli_fname,fixture_block_loop?"/libbirnam_block_loop.dylib":"_prelude.dylib")); }
    begin("wrong arity and null carriers skip all callbacks");for(size_t op=0;op<2;op++) { for(size_t n=0;n<=3;n++) if(n!=1) assert(loops[op](NULL,(Oop *)(uintptr_t)1,n)==api->nil_obj);assert(loops[op](NULL,NULL,1)==api->nil_obj); }end();assert(!interns && !sends && !exports);
    begin("absent Error still constructs kind and message");assert(failure()==api->nil_obj);end();assert(makes==1 && !calls);
    type=api->proto(api->object_vt);factory=api->alloc(type,0);second_factory=api->alloc(type,0);api->method(type,api->intern("kind:message:"),construct);factory_export=factory;present=type_present=1;answer=alias;second_answer=api->alloc(api->object_vt,0);
    error_type=original_error_type=api->proto(api->object_vt);alternate_error_type=api->proto(error_type);error_value=api->alloc(error_type,0);inherited_error=api->alloc(alternate_error_type,0);
    Oop values[]={NULL,api->nil_obj,api->true_obj,api->false_obj,error_value,inherited_error,alias};
    for(size_t op=0;op<2;op++) for(size_t n=0;n<=8;n++) for(size_t c=0;c<7;c++) for(size_t b=0;b<6;b++) {
        setup(op,n,values[c],values[b]);Oop expected=api->nil_obj;size_t expected_conditions=0,expected_bodies=0;
        for(size_t step=0;step<con_count;step++) {
            Oop result=con[step];expected_conditions++;
            if(result==error_value) { expected=result;break; }
            if(result!=api->true_obj && result!=api->false_obj) { expected=answer;break; }
            if((result==api->true_obj)==(op==1)) break;
            expected_bodies++;if(values[b]==error_value) { expected=error_value;break; }
        }
        assert(loops[op](condition,&body,1)==expected && con_step==expected_conditions && body_step==expected_bodies);cases++;
    }assert(cases==756);
    for(size_t op=0;op<2;op++) for(recognition_phase=1;recognition_phase<=2;recognition_phase++) for(size_t v=0;v<7;v++) for(int flag=0;flag<4;flag++) for(reported_match=0;reported_match<2;reported_match++) {
        candidate=values[v];type_present=flag==0?0:flag==3?-1:1;error_type=flag==1?NULL:original_error_type;
        int recognized=candidate && type_present && error_type && reported_match;
        setup(op,recognition_phase==1?0:1,recognition_phase==1?candidate:(op?api->true_obj:api->false_obj),recognition_phase==2?candidate:api->nil_obj);
        Oop expected=recognized?candidate:recognition_phase==1 && candidate!=api->true_obj && candidate!=api->false_obj?answer:api->nil_obj;
        assert(loops[op](condition,&body,1)==expected);cases++;
    }assert(cases==980);recognition_phase=reported_match=0;type_present=1;error_type=original_error_type;
    begin("body argument is copied before condition callback");setup(0,1,api->false_obj,api->nil_obj);changing_args[0]=body;mutate_args=1;assert(loops[0](condition,changing_args,1)==api->nil_obj && body_step==1 && changing_args[0]==bad_condition);end();
    begin("null receiver uses ordinary selected send");setup(0,1,api->false_obj,api->nil_obj);null_receiver=1;assert(loops[0](NULL,&body,1)==api->nil_obj && body_step==1);null_receiver=0;end();
    begin("terminating condition never evaluates null body");Oop null_body=NULL;setup(0,0,api->false_obj,api->nil_obj);assert(loops[0](condition,&null_body,1)==api->nil_obj && !body_step);end();
    begin("Boolean selection stays live through Error recognition");setup(0,0,api->true_obj,api->nil_obj);mutate_boolean=1;assert(loops[0](condition,&body,1)==api->nil_obj && !body_step);selected.true_obj=api->true_obj;selected.false_obj=api->false_obj;end();
    begin("shared true and false identity keeps true comparison precedence");selected.true_obj=selected.false_obj=alias;setup(0,0,alias,error_value);assert(loops[0](condition,&body,1)==error_value && body_step==1);setup(1,0,alias,error_value);assert(loops[1](condition,&body,1)==api->nil_obj && !body_step);selected.true_obj=api->true_obj;selected.false_obj=api->false_obj;end();
    begin("nil selection stays live after body recognition");setup(0,1,api->false_obj,api->nil_obj);mutate_nil=1;assert(loops[0](condition,&body,1)==alias);selected.nil_obj=api->nil_obj;end();
    begin("recognition retains imported Error behavior snapshot");recognition_phase=1;candidate=error_value;reported_match=mutate_type=1;setup(0,0,candidate,api->nil_obj);assert(loops[0](condition,&body,1)==candidate && error_type==alternate_error_type);recognition_phase=reported_match=0;error_type=original_error_type;end();
    begin("fresh condition and body method lookup");setup(0,4,api->false_obj,api->nil_obj);mutate_methods=1;assert(loops[0](condition,&body,1)==api->nil_obj && body_step==1);api->method(condition_type,api->intern("value"),condition_value);
    setup(0,4,api->false_obj,api->nil_obj);mutate_methods=2;assert(loops[0](condition,&body,1)==error_value && body_step==1);api->method(body_type,api->intern("value"),body_value);end();
    Oop meta=api->proto(api->vtable_vt);api->method(meta,api->intern("lookup"),custom_condition_lookup);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){condition_type,meta},2);
    begin("custom condition lookup on every iteration");lookups=0;setup(0,4,api->false_obj,api->nil_obj);assert(loops[0](condition,&body,1)==api->nil_obj && body_step==1 && lookups==2);end();api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){condition_type,api->vtable_vt},2);
    value_selectors[0]=api->intern("loop-value-0");value_selectors[1]=api->intern("loop-value-1");api->method(condition_type,value_selectors[0],condition_value);api->method(body_type,value_selectors[1],body_value);
    begin("each value send interns its current selector");map_value=1;value_generation=0;setup(0,2,api->false_obj,api->nil_obj);assert(loops[0](condition,&body,1)==api->nil_obj && con_step==3 && body_step==2);map_value=0;end();
    begin("nested loop and callback GC");setup(0,2,api->false_obj,api->nil_obj);collect=nested=1;assert(loops[0](condition,&body,1)==api->nil_obj && body_step==2);collect=0;end();
    begin("nested error construction and callback GC");nested=collect=1;assert(failure()==answer);collect=0;end();assert(makes==2 && calls==2);
    begin("null factory and current nil");factory_export=NULL;selected.nil_obj=alias;assert(failure()==alias && loops[1](NULL,NULL,0)==alias);factory_export=factory;selected.nil_obj=api->nil_obj;end();
    begin("null and alias message and factory results");for(ctor_mode=0;ctor_mode<3;ctor_mode++) for(size_t i=0;i<3;i++) { answer=i==0?NULL:i==1?alias:second_answer;assert(failure()==answer); }ctor_mode=0;answer=alias;end();
    begin("message callback publishes factory");present=0;change_factory=1;assert(failure()==answer && seen_factory==second_factory);end();factory_export=factory;
    begin("selector callback preserves imported factory");change_factory=2;assert(failure()==answer && seen_factory==factory);end();factory_export=factory;
    begin("kind callback selects current String constructor and context");change_constructor=1;assert(failure()==answer);fixture_string->make=make;fixture_string->context=string_context;end();
    begin("fresh factory method and custom lookup");assert(failure()==answer);api->method(type,api->intern("kind:message:"),construct_second);assert(failure()==second_answer);api->method(type,api->intern("kind:message:"),construct);
    meta=api->proto(api->vtable_vt);api->method(meta,api->intern("lookup"),custom_factory_lookup);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){type,meta},2);lookups=0;assert(failure()==answer && failure()==second_answer && lookups==2);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){type,api->vtable_vt},2);end();
    for(size_t i=0;i<2;i++) api->method(block_type,api->intern(names[i]),replacement);
    begin("copied methods survive replacement");setup(0,1,api->false_obj,api->nil_obj);assert(loops[0](condition,&body,1)==api->nil_obj);end();
    begin("reload retains original execution table");prelude_init(&other,NULL);end();for(size_t i=0;i<2;i++) assert(lookup(block_type,api->intern(names[i]))==loops[i]);
    if(fixture_block_loop) { IdApi registration=other;registration.intern=forbidden_intern;fixture_block_loop->install(&registration,&other,block_type,api->intern(names[0]),api->intern(names[1])); }
    begin("original execution after direct installation");assert(failure()==answer);end();
    for(size_t g=0;g<2;g++) for(size_t i=0;i<2;i++) { char name[40];snprintf(name,sizeof(name),"block-loop-test-%zu-%zu",g,i);selectors[g][i]=api->intern(name); }
    alternate=api->proto(api->object_vt);expected_target=block_type;map_selectors=change_snapshot=1;map_generation=0;
    begin("first binding preserves second selector and behavior snapshot");prelude_init(&selected,NULL);end();for(size_t i=0;i<2;i++) assert(seen_selectors[i]==selectors[0][i] && seen_targets[i]==block_type && lookup(block_type,selectors[0][i])==loops[i]);
    expected_target=alternate;begin("next installation adopts current selection");prelude_init(&other,NULL);end();for(size_t i=0;i<2;i++) assert(seen_selectors[i]==selectors[1][i] && seen_targets[i]==alternate);
    api->export_obj("BlockVt",block_type);map_selectors=0;expected_target=NULL;assert(!registrations);
    api->gc_set_precise(1);api->gc_collect();api->gc_set_precise(0);setup(1,2,api->true_obj,api->nil_obj);assert(loops[1](condition,&body,1)==api->nil_obj && body_step==2 && failure()==answer);
    fixture_string->make=original_make;fixture_string->context=string_context;
    printf("block-loop: %zu evaluation/recognition cases, fresh dispatch, live identities, errors, nested loops, GC, registration reentry, snapshots and reload passed mode=%s\n",cases,argv[4]);
}
