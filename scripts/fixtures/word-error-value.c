#include "idapi_v15.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"

static FixturePresenceSlot selected_presence_slot;
static NamespacePresenceV1 selected_presence=FIXTURE_PRESENCE(selected_presence_slot);
static FixtureImportSlot selected_import_slot;
static NamespaceImportV1 selected_imports=FIXTURE_IMPORT(selected_import_slot);

static IdApi *api,selected;
static ModuleInitFn word_init;
static Method convert;
static Oop high,low,subject,answer,second,factory,alternate,factory_export;
static Oop made,kind_arg,message_arg,seen_factory,constructor_answer;
static size_t reads,interns,makes,exports,imports,sends,calls,lookups,bindings,cases;
static int capture,present,override_kind,override_message,swap_constructor,change_factory,collect,nested;
static Oop (*original_make)(void *,const uint8_t *,size_t);
static uint64_t (*original_read)(void *,Oop);
static void *string_context;
static void *changed_context=(void *)(uintptr_t)0x1234;
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(void) { return (Method)send1(fixture_word->behavior,"lookup",api->intern("asInteger")); }
static void roots(MarkFn mark) { mark(high);mark(low);mark(subject);mark(answer);mark(second);mark(factory);mark(alternate);mark(factory_export);mark(made);mark(kind_arg);mark(message_arg);mark(seen_factory);mark(constructor_answer); }
static Oop changed_make(void *context,const uint8_t *bytes,size_t length);
static int exported(const char *name) {
    if(strcmp(name,"Error")) return fixture_namespace_contains(name);
    exports++;if(capture) puts("exported Error");if(collect) api->gc_collect();return present;
}
static Oop imported(const char *name) {
    if(strcmp(name,"Error")) return fixture_namespace_import(name);
    imports++;if(capture) puts("import Error");if(collect) api->gc_collect();return factory_export;
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
    assert(length==25 && !memcmp(bytes,"word out of Integer range",25));
    if(collect) api->gc_collect();
    if(nested) { nested=0;assert(convert(high,NULL,0)==answer); }
    if(change_factory==1) { change_factory=0;present=1;factory_export=alternate; }
    made=override_message?constructor_answer:original_make(context,bytes,length);return made;
}
static Oop changed_make(void *context,const uint8_t *bytes,size_t length) { assert(context==changed_context);if(capture) puts("changed String constructor/context");return make(string_context,bytes,length); }
static uint64_t read_word(void *context,Oop value) { reads++;if(capture) puts("read Word");if(collect) api->gc_collect();return original_read(context,value); }
static Oop construct(Oop self,const Oop *args,size_t count) { assert(args && count==2);calls++;seen_factory=self;kind_arg=args[0];message_arg=args[1];if(collect) api->gc_collect();return answer; }
static Oop construct_second(Oop self,const Oop *args,size_t count) { (void)construct(self,args,count);return second; }
static Oop custom_lookup(Oop self,const Oop *args,size_t count) { (void)self;assert(count==1 && args[0]==api->intern("kind:message:"));lookups++;return (Oop)(lookups%2?construct:construct_second); }
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) { sends++;if(capture) printf("send %s %zu\n",api->symbol_name(selector),count);if(collect) api->gc_collect();return api->send(self,selector,args,count); }
static void method(Oop type,Oop selector,Method implementation) {
    if(capture) printf("method %s\n",api->symbol_name(selector));
    if(selector==api->intern("asInteger")) {
        bindings++;assert(type==fixture_word->behavior);convert=implementation;
        assert(convert(high,NULL,1)==selected.nil_obj);
        assert(convert(high,NULL,0)==(present && factory_export?answer:selected.nil_obj));
    }
    api->method(type,selector,implementation);
}
static void begin(const char *name) { printf("case %s\n",name);capture=1;reads=interns=makes=exports=imports=sends=calls=0; }
static void end(void) { capture=0;printf("done read=%zu intern=%zu make=%zu export=%zu import=%zu send=%zu calls=%zu\n",reads,interns,makes,exports,imports,sends,calls); }
static void mount(const char *path) { void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);fixture_attach_integer(api,image,fixture_root_registry());ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry()); }
int main(int argc,char **argv) {
    assert(argc==4);setvbuf(stdout,NULL,_IONBF,0);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))();assert(api);api->gc_set_auto(0);fixture_mount_trace(api,argv[1],fixture_root_registry());
    uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));fixture_register_root(roots);
    selected=*api;selected_presence_slot=exported;selected_import_slot=imported;selected.intern=intern;selected.send=send;selected.method=method;
    fixture_prepare_word(&selected,argv[1], &selected_presence,&selected_imports,fixture_namespace_exports(),fixture_root_registry());fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());mount(argv[2]);
    original_make=fixture_string->make;string_context=fixture_string->context;fixture_string->make=make;
    original_read=fixture_word->read;fixture_word->read=read_word;
    high=fixture_word_make(UINT64_MAX);low=fixture_word_make(INT64_MAX);
    answer=api->alloc(api->object_vt,0);second=api->alloc(api->object_vt,0);
    Oop type=api->proto(api->object_vt);factory=api->alloc(type,0);alternate=api->alloc(type,0);factory_export=factory;api->method(type,api->intern("kind:message:"),construct);
    void *word=api->dlopen_fn(argv[3],RTLD_NOW|RTLD_LOCAL);assert(word);fixture_attach_integer(api,word,fixture_root_registry());word_init=(ModuleInitFn)api->dlsym_fn(word,"module_init");assert(word_init);
    begin("activation callback calls overflow before registration returns");word_init(&selected,NULL,&selected_presence,&selected_imports,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry());end();assert(bindings==1 && lookup()==convert);
    begin("invalid arity and receivers skip error policy");assert(convert(high,(Oop *)(uintptr_t)1,1)==api->nil_obj && convert(NULL,NULL,0)==api->nil_obj && convert(api->true_obj,NULL,0)==api->nil_obj);end();assert(!reads && !makes && !interns);
    const uint64_t values[]={0,1,INT64_MAX,(uint64_t)INT64_MAX+1,UINT64_C(0xfedcba9876543210),UINT64_MAX};
    for(size_t i=0;i<6;i++) { subject=fixture_word_make(values[i]);Oop result=convert(subject,(Oop *)(uintptr_t)1,0);if(values[i]<=INT64_MAX) assert(fixture_integer_value(result)==(int64_t)values[i]);else assert(result==api->nil_obj);cases++; }
    begin("absent Error still interns and makes");assert(convert(high,NULL,0)==api->nil_obj);end();assert(reads==1 && interns==1 && makes==1 && exports==1 && !imports && !sends);
    present=1;factory_export=NULL;selected.nil_obj=second;begin("null factory and live nil");assert(convert(high,NULL,0)==second);end();selected.nil_obj=api->nil_obj;factory_export=factory;
    present=-1;begin("negative presence flag and exact message");assert(convert(high,NULL,0)==answer && kind_arg==api->intern("overflow") && message_arg==made);end();present=1;
    for(int k=0;k<2;k++) for(int m=0;m<3;m++) for(int r=0;r<3;r++) {
        override_kind=k;override_message=m!=0;constructor_answer=m==1?NULL:low;Oop saved=answer;answer=r==0?NULL:r==1?second:saved;
        assert(convert(high,NULL,0)==answer && kind_arg==(k?NULL:api->intern("overflow")) && message_arg==(m?constructor_answer:made));answer=saved;cases++;
    }
    assert(cases==24);override_kind=override_message=0;
    swap_constructor=1;begin("kind callback replaces String constructor and context");assert(convert(high,NULL,0)==answer);end();fixture_string->make=make;fixture_string->context=string_context;
    present=0;change_factory=1;begin("message callback publishes factory");assert(convert(high,NULL,0)==answer && seen_factory==alternate);end();factory_export=factory;
    change_factory=2;begin("selector callback keeps imported factory snapshot");assert(convert(high,NULL,0)==answer && seen_factory==factory && factory_export==alternate);end();factory_export=factory;
    begin("fresh replacement");assert(convert(high,NULL,0)==answer);api->method(type,api->intern("kind:message:"),construct_second);assert(convert(high,NULL,0)==second);end();
    Oop meta=api->proto(api->vtable_vt);api->method(meta,api->intern("lookup"),custom_lookup);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){type,meta},2);
    begin("custom lookup");assert(convert(high,NULL,0)==answer);assert(convert(high,NULL,0)==second);end();assert(lookups==2);
    api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){type,api->vtable_vt},2);api->method(type,api->intern("kind:message:"),construct);
    collect=nested=1;begin("nested overflow and callback GC");assert(convert(high,NULL,0)==answer);end();collect=0;assert(calls==2 && makes==2);
    Oop behavior=fixture_word->behavior,prototype=fixture_word->prototype;
    api->method(behavior,api->intern("asInteger"),construct_second);
    begin("copied method retains live policy after public replacement");assert(convert(high,NULL,0)==answer);end();
    begin("reinitialization keeps original table and identities");word_init(&selected,NULL,&selected_presence,&selected_imports,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry());end();assert(lookup()==convert && fixture_word->behavior==behavior && fixture_word->prototype==prototype && bindings==2);
    api->gc_set_precise(1);api->gc_collect();api->gc_set_precise(0);assert(convert(high,NULL,0)==answer && fixture_word_value(high)==UINT64_MAX);
    fixture_string->make=original_make;fixture_string->context=string_context;fixture_word->read=original_read;
    printf("word-error-value: %zu boundary/construction cases, activation calls, live callbacks, lazy exports, lookup, reentry, GC and reload passed\n",cases);
}
