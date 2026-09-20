#include "idapi_v14.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"

static IdApi *api;
static Oop (*original_string)(void *, const uint8_t *, size_t);
static IdApi boolean_api;
static unsigned evaluations, forwarded_values, strings;
static int collect_in_branch;

static void mount(const char *path, IdApi *selected) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    fixture_attach_integer(api,image);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    init(selected,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation());
}
static Oop send0(Oop receiver, const char *name) {
    return api->send(receiver,api->intern(name),NULL,0);
}
static Oop send(Oop receiver, const char *name, Oop argument) {
    return api->send(receiver,api->intern(name),&argument,1);
}
static Method lookup(Oop behavior, const char *name) {
    return (Method)send(behavior,"lookup",api->intern(name));
}
static Oop value(Oop self, const Oop *args, size_t count) {
    (void)args; assert(count==0); ++evaluations;
    if(collect_in_branch) (void)api->gc_collect();
    return self;
}
static Oop replacement_value(Oop self, const Oop *args, size_t count) {
    (void)self; (void)args; assert(count==0); ++evaluations;
    return api->true_obj;
}
static Oop identity(Oop self, const Oop *args, size_t count) {
    (void)args; assert(count==0); return self;
}
static Oop forwarded(Oop receiver, Oop selector, const Oop *args, size_t count) {
    if(selector==api->intern("value")) ++forwarded_values;
    return api->send(receiver,selector,args,count);
}
static Oop counted_string(void *context, const uint8_t *bytes, size_t count) {
    ++strings; return original_string(context,bytes,count);
}
static void expect_text(Oop object, const char *text) {
    Oop result=send0(object,"printString"); size_t length=0;
    const uint8_t *bytes=fixture_string_bytes(result,&length);
    assert(length==strlen(text) && !memcmp(bytes,text,length));
}

int main(int argc, char **argv) {
    assert(argc==5 || (argc==6 && !strcmp(argv[5],"baseline")));
    int baseline=argc==6;
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]); fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    mount(argv[2],api); /* object-model, before either Boolean or Prelude */
    Oop true_vt=api->vtable_of(api->true_obj);
    assert(api->vtable_of(api->false_obj)==true_vt && !lookup(true_vt,"not"));
    boolean_api=*api; mount(argv[3],&boolean_api);
    Oop false_vt=api->vtable_of(api->false_obj);
    assert(false_vt!=true_vt && vtable_parent(api,false_vt)==true_vt);
    assert(vtable_owner(api,false_vt)==api->false_obj);
    Oop branch_vt=api->proto(api->object_vt);
    api->method(branch_vt,api->intern("value"),value);
    Oop left=api->alloc(branch_vt,0), right=api->alloc(branch_vt,0);
    fixture_namespace_export("BooleanTestLeft",left); fixture_namespace_export("BooleanTestRight",right);
    Oop branches[]={left,right}, receivers[]={api->false_obj,api->true_obj};
    unsigned checked=0;
    for(unsigned truth=0;truth<2;truth++) {
        Oop receiver=receivers[truth];
        unsigned before=evaluations;
        assert(api->send(receiver,api->intern("ifTrue:ifFalse:"),branches,2)==branches[truth?0:1]);
        assert(evaluations==before+1); checked++;
        const char *selectors[]={"ifTrue:","ifFalse:","and:","or:"};
        for(unsigned op=0;op<4;op++) {
            before=evaluations;
            int executes=(op==0 || op==2)?truth:!truth;
            Oop skipped=op<2?api->nil_obj:(truth?api->true_obj:api->false_obj);
            assert(send(receiver,selectors[op],left)==(executes?left:skipped));
            assert(evaluations==before+(unsigned)executes); checked++;
        }
        Oop operands[]={api->true_obj,api->false_obj,api->nil_obj,left,NULL};
        for(unsigned rhs=0;rhs<5;rhs++) {
            before=evaluations;
            assert(send(receiver,"&",operands[rhs])==(truth?operands[rhs]:api->false_obj));
            assert(send(receiver,"|",operands[rhs])==(truth?api->true_obj:operands[rhs]));
            assert(evaluations==before); checked+=2;
        }
        assert(send0(receiver,"not")==receivers[!truth]); checked++;
        expect_text(receiver,truth?"true":"false"); checked++;
        const char *names[]={"ifTrue:ifFalse:","ifTrue:","ifFalse:","&","|","and:","or:","not","printString"};
        for(unsigned i=0;i<9;i++) {
            Method method=lookup(api->vtable_of(receiver),names[i]); assert(method);
            size_t arity=i==0?2:(i<7?1:0);
            before=evaluations;
            assert(method(receiver,NULL,arity+1)==api->nil_obj);
            if(arity) assert(method(receiver,NULL,arity)==api->nil_obj);
            assert(evaluations==before);
        }
    }
    /* Conditional methods intentionally accept inherited receivers. Other
       operators and printing require the exact singleton identities. */
    Oop inherited=api->alloc(true_vt,0);
    fixture_namespace_export("BooleanTestInherited",inherited);
    assert(send(inherited,"ifTrue:",left)==left);
    assert(send(inherited,"ifFalse:",left)==api->nil_obj);
    assert(lookup(false_vt,"ifFalse:")(inherited,&right,1)==right);
    const char *exact[]={"&","|","and:","or:","not","printString"};
    for(unsigned i=0;i<6;i++) {
        Method method=lookup(true_vt,exact[i]); size_t count=i<4?1:0;
        assert(method(inherited,&left,count)==api->nil_obj);
        assert(method(api->nil_obj,&left,count)==api->nil_obj);
        assert(method(NULL,&left,count)==api->nil_obj);
    }
    boolean_api.send=forwarded;
    assert(send(api->true_obj,"and:",left)==left && forwarded_values==1);
    api->method(branch_vt,api->intern("value"),replacement_value);
    assert(send(api->true_obj,"and:",left)==api->true_obj && forwarded_values==2);
    boolean_api.send=api->send; api->method(branch_vt,api->intern("value"),value);
    original_string=fixture_string->make; fixture_string->make=counted_string;
    expect_text(api->true_obj,"true"); expect_text(api->false_obj,"false"); assert(strings==2);
    fixture_string->make=original_string;
    Method not=lookup(true_vt,"not"); api->method(true_vt,api->intern("not"),identity);
    assert(send0(api->false_obj,"not")==api->false_obj);
    fixture_namespace_export("Boolean",api->nil_obj);
    mount(argv[3],&boolean_api);
    assert(lookup(true_vt,"not")==not && api->vtable_of(api->false_obj)==false_vt);
    assert(fixture_namespace_import("Boolean")==api->nil_obj); fixture_namespace_export("Boolean",api->true_obj);
    api->method(true_vt,api->intern("not"),identity);
    if(!baseline) {
        mount(argv[4],api); /* Later Prelude must preserve Boolean replacements. */
        assert(lookup(true_vt,"not")==identity && send0(api->false_obj,"not")==api->false_obj);
    }
    api->method(true_vt,api->intern("not"),not);
    collect_in_branch=1; api->gc_set_precise(1);
    assert(send(api->false_obj,"or:",left)==left);
    assert(send0(api->false_obj,"not")==api->true_obj);
    assert(api->vtable_of(api->false_obj)==false_vt);
    printf("boolean-module: %u truth/branch cases, evaluation counts, arbitrary operands, inherited behavior, arity, live callbacks, reinitialization, replacement and GC passed%s\n",checked,baseline?" (Prelude baseline)":"");
}
