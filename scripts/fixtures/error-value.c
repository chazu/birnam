#include "idapi_v15.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <stdlib.h>
#include <unistd.h>
#include <fcntl.h>
static FixtureRootSlot watched_root_slot;
static RootRegistryV1 watched_roots=FIXTURE_ROOTS(watched_root_slot);

static FixturePresenceSlot selected_presence_slot;
static NamespacePresenceV1 selected_presence=FIXTURE_PRESENCE(selected_presence_slot);
static FixtureImportSlot selected_import_slot;
static NamespaceImportV1 selected_imports=FIXTURE_IMPORT(selected_import_slot);

static IdApi *api,selected;
static ModuleInitFn prelude_init;
static Method read_file,write_file,get_env,load_source,load_module,read_line,while_true;
static Oop system_object,bad_path,nul_path,contents,condition,body,candidate,candidates[6];
static Oop factory,other_factory,error_factory,error_type,alternate_type,answer,second_answer;
static Oop made_value,kind_arg,message_arg,seen_factory,constructor_answer;
static size_t interns,makes,imports,exports,vt_calls,sends,factory_calls,lookups,cases,condition_step;
static int capture,error_present=1,type_present=1,override_kind,override_message,replace_make;
static int replace_presence_slot;
static int replace_import_slot;
static int collect,nested,mutate_factory,mutate_type,reported_match,recognition_phase,current_phase;
static const uint8_t *borrowed_message;
static size_t borrowed_length;
static Oop (*original_make)(void *,const uint8_t *,size_t);
static void *original_context;
static void *changed_context=(void *)(uintptr_t)0x1234;
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(Oop type,const char *name) { return (Method)send1(type,"lookup",api->intern(name)); }
static void roots(MarkFn mark) {
    mark(system_object);mark(bad_path);mark(nul_path);mark(contents);mark(condition);mark(body);mark(candidate);
    for(size_t i=0;i<6;i++) mark(candidates[i]);
    mark(factory);mark(other_factory);mark(error_factory);mark(error_type);mark(alternate_type);
    mark(answer);mark(second_answer);mark(made_value);mark(kind_arg);mark(message_arg);mark(seen_factory);mark(constructor_answer);
}
static Oop changed_make(void *context,const uint8_t *bytes,size_t length);
static Oop late_null(const char *name) { assert(!strcmp(name,"Error"));return NULL; }
static int exported(const char *name) {
    if(strcmp(name,"Error") && strcmp(name,"ErrorVt")) return fixture_namespace_contains(name);
    exports++;if(capture) printf("exported %s\n",name);if(collect) api->gc_collect();
    if(replace_import_slot==3) { replace_import_slot=0;selected_import_slot=late_null; }
    return !strcmp(name,"Error")?error_present:type_present;
}
static Oop imported(const char *name) {
    if(strcmp(name,"Error") && strcmp(name,"ErrorVt")) return fixture_namespace_import(name);
    imports++;if(capture) printf("import %s\n",name);if(collect) api->gc_collect();
    return !strcmp(name,"Error")?error_factory:error_type;
}
static int late_absent(const char *name) { assert(!strcmp(name,"Error"));return 0; }
static Oop intern(const char *name) {
    if(replace_import_slot==1) { replace_import_slot=0;selected_import_slot=late_null; }
    if(replace_presence_slot==1) { replace_presence_slot=0;selected_presence_slot=late_absent; }
    interns++;if(capture) printf("intern %s\n",name);if(collect) api->gc_collect();
    if(!strcmp(name,"kind:message:")) {
        if(mutate_factory==2) { mutate_factory=0;error_factory=other_factory; }
    } else if(strcmp(name,"value")) {
        if(replace_make) { replace_make=0;fixture_string->make=changed_make;fixture_string->context=changed_context; }
        if(override_kind) return NULL;
    }
    return api->intern(name);
}
static Oop make(void *context,const uint8_t *bytes,size_t length) {
    if(replace_import_slot==2) { replace_import_slot=0;selected_import_slot=late_null; }
    if(replace_presence_slot==2) { replace_presence_slot=0;selected_presence_slot=late_absent; }
    assert(context==original_context);makes++;if(capture) printf("make %zu\n",length);
    if(borrowed_message) assert(bytes==borrowed_message && length==borrowed_length);
    if(collect) api->gc_collect();
    if(nested) { nested=0;assert(get_env(system_object,&bad_path,1)==answer); }
    if(mutate_factory==1) { mutate_factory=0;error_factory=other_factory;error_present=1; }
    made_value=override_message?constructor_answer:original_make(context,bytes,length);
    return made_value;
}
static Oop changed_make(void *context,const uint8_t *bytes,size_t length) { assert(context==changed_context);if(capture) puts("changed constructor/context");return make(original_context,bytes,length); }
static Oop vtable(Oop value) {
    vt_calls++;if(capture) puts("vtable");if(collect) api->gc_collect();
    if(recognition_phase && current_phase==recognition_phase && value==candidate) {
        Oop selected_type=reported_match?error_type:alternate_type;
        if(mutate_type) { mutate_type=0;error_type=alternate_type; }
        return selected_type;
    }
    return api->vtable_of(value);
}
static Oop construct(Oop self,const Oop *args,size_t count) {
    assert(args && count==2);factory_calls++;seen_factory=self;kind_arg=args[0];message_arg=args[1];if(collect) api->gc_collect();return answer;
}
static Oop construct_second(Oop self,const Oop *args,size_t count) { (void)construct(self,args,count);return second_answer; }
static Oop custom_lookup(Oop self,const Oop *args,size_t count) { (void)self;assert(count==1 && args[0]==api->intern("kind:message:"));lookups++;return (Oop)(lookups%2?construct:construct_second); }
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) {
    sends++;if(capture) printf("send %s %zu\n",api->symbol_name(selector),count);if(collect) api->gc_collect();
    if(self==condition) { assert(selector==api->intern("value") && !args && !count);current_phase=1;condition_step++;return condition_step==1?(recognition_phase==1?candidate:api->true_obj):api->false_obj; }
    if(self==body) { assert(selector==api->intern("value") && !args && !count);current_phase=2;return recognition_phase==2?candidate:api->nil_obj; }
    return api->send(self,selector,args,count);
}
static Oop text(const uint8_t *bytes,size_t length) { return original_make(original_context,bytes,length); }
static void expect_message(const char *message) {
    size_t n=0;const uint8_t *bytes=fixture_string->bytes(fixture_string->context,message_arg,&n);assert(bytes && n==strlen(message) && !memcmp(bytes,message,n));
}
static void begin(const char *name) { printf("case %s\n",name);capture=1;interns=makes=imports=exports=vt_calls=sends=factory_calls=0;condition_step=0; }
static void end(void) { capture=0;printf("done intern=%zu make=%zu import=%zu export=%zu vtable=%zu send=%zu factory=%zu\n",interns,makes,imports,exports,vt_calls,sends,factory_calls); }
static void validate(void *image,int attached) {
    ErrorValueDependencyV3 attach=(ErrorValueDependencyV3)api->dlsym_fn(image,"birnam_error_value_dependency_v3");if(!attach) return;
    ErrorValueApiV3 bad=*fixture_error_value;assert(!attach(NULL));bad.version++;assert(!attach(&bad));bad=*fixture_error_value;
    bad.byte_size--;assert(!attach(&bad));bad=*fixture_error_value;bad.is_error=NULL;assert(!attach(&bad));bad=*fixture_error_value;bad.from_bytes=NULL;assert(!attach(&bad));
    if(attached) { bad=*fixture_error_value;assert(!attach(&bad) && attach(fixture_error_value)==1); }
}
static Oop forbidden_alloc(Oop type,size_t size) { (void)type;(void)size;assert(!"Error value allocated during initialization");return NULL; }
static void forbidden_method(Oop type,Oop selector,Method method) { (void)type;(void)selector;(void)method;assert(!"Error value installed methods during initialization"); }
static void forbidden_roots(RootProvider provider) { (void)provider;assert(!"Error value registered roots during initialization"); }
static void mount(const char *path,IdApi *table,const NamespacePresenceV1 *presence,const NamespaceImportV1 *import_cap,int prelude,const RootRegistryV1 *root_registry) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    if(prelude && api->dlsym_fn(image,"birnam_error_value_dependency_v3")) { IdApi watched=*api;watched.alloc=forbidden_alloc;watched.method=forbidden_method;watched_root_slot=forbidden_roots;fixture_prepare_error_value(&watched,&watched_roots);assert(!fixture_namespace_contains("Error") && !fixture_namespace_contains("System"));validate(image,0); }
    fixture_attach_integer(api,image,fixture_root_registry());ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);if(prelude) { prelude_init=init;validate(image,1); }init(table,NULL,presence,import_cap,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry);
}
static int provider_is_error(const IdApi *table,const NamespacePresenceV1 *presence,const NamespaceImportV1 *import_cap,Oop value) {
    return fixture_error_value->is_error(table,presence,import_cap,value);
}
static Oop provider_from_bytes(const IdApi *table,const NamespacePresenceV1 *presence,const NamespaceImportV1 *import_cap,const uint8_t *kind,size_t kind_length,const uint8_t *message,size_t message_length) {
    return fixture_error_value->from_bytes(table,presence,import_cap,kind,kind_length,message,message_length);
}
int main(int argc,char **argv) {
    assert(argc==4);setvbuf(stdout,NULL,_IONBF,0);void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))();assert(api);api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1],fixture_root_registry());fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));mount(argv[2],api,fixture_namespace_presence(),fixture_namespace_imports(),0,fixture_root_registry());fixture_register_root(roots);
    selected=*api;selected_presence_slot=exported;selected_import_slot=imported;selected.intern=intern;selected.send=send;selected.vtable_of=vtable;mount(argv[3],&selected,&selected_presence,&selected_imports,1,fixture_root_registry());
    system_object=fixture_namespace_import("System");Oop system_type=fixture_namespace_import("SystemVt");
    read_file=lookup(system_type,"readFile:");write_file=lookup(system_type,"writeFile:contents:");get_env=lookup(system_type,"getEnv:");load_source=lookup(system_type,"load:");load_module=lookup(system_type,"loadModule:");read_line=lookup(system_type,"readLine");while_true=lookup(fixture_namespace_import("BlockVt"),"whileTrue:");
    original_make=fixture_string->make;original_context=fixture_string->context;fixture_string->make=make;
    bad_path=text((const uint8_t *)"\xff",1);nul_path=text((const uint8_t *)"a\0b",3);contents=text((const uint8_t *)"contents",8);
    condition=api->alloc(api->object_vt,0);body=api->alloc(api->object_vt,0);answer=api->alloc(api->object_vt,0);second_answer=api->alloc(api->object_vt,0);
    Oop factory_type=api->proto(api->object_vt);factory=api->alloc(factory_type,0);other_factory=api->alloc(factory_type,0);api->method(factory_type,api->intern("kind:message:"),construct);
    error_factory=factory;error_type=api->proto(api->object_vt);alternate_type=api->proto(error_type);
    error_present=0;begin("absent factory still interns and makes message");assert(read_file(system_object,&bad_path,1)==api->nil_obj);end();assert(interns==1 && makes==1 && exports==1 && !imports && !sends);
    error_present=1;
    for(int stage=1;stage<=3;stage++) {
        replace_import_slot=stage;
        assert(read_file(system_object,&bad_path,1)==api->nil_obj);
        assert(!replace_import_slot);selected_import_slot=imported;
    }
    for(int stage=1;stage<=2;stage++) {
        replace_presence_slot=stage;
        assert(read_file(system_object,&bad_path,1)==api->nil_obj);
        assert(!replace_presence_slot);selected_presence_slot=exported;
    }
    error_present=1;error_factory=NULL;begin("null factory returns current nil");selected.nil_obj=second_answer;assert(read_file(system_object,&bad_path,1)==second_answer);end();assert(makes==1 && imports==1 && !sends);selected.nil_obj=api->nil_obj;error_factory=factory;
    begin("negative export flag remains present");error_present=-1;assert(get_env(system_object,&bad_path,1)==answer);end();assert(seen_factory==factory);expect_message("could not read environment");error_present=1;
    for(int kind=0;kind<2;kind++) for(int message=0;message<3;message++) for(int result=0;result<3;result++) {
        override_kind=kind;override_message=message!=0;constructor_answer=message==1?NULL:contents;
        Oop original_answer=answer;answer=result==0?NULL:result==1?second_answer:original_answer;
        assert(read_file(system_object,&bad_path,1)==answer && kind_arg==(kind?NULL:api->intern("ioError")));
        assert(message_arg==(message?constructor_answer:made_value));answer=original_answer;cases++;
    }
    override_kind=override_message=0;
    replace_make=1;begin("kind callback replaces String constructor and context");assert(read_file(system_object,&bad_path,1)==answer);end();fixture_string->make=make;fixture_string->context=original_context;
    error_present=0;mutate_factory=1;begin("String construction publishes replacement factory before lookup");assert(read_file(system_object,&bad_path,1)==answer && seen_factory==other_factory);end();error_factory=factory;
    mutate_factory=2;begin("selector callback changes export but keeps imported factory snapshot");assert(read_file(system_object,&bad_path,1)==answer && seen_factory==factory && error_factory==other_factory);end();error_factory=factory;
    begin("fresh factory method replacement");assert(read_file(system_object,&bad_path,1)==answer);api->method(factory_type,api->intern("kind:message:"),construct_second);assert(read_file(system_object,&bad_path,1)==second_answer);end();
    Oop meta=api->proto(api->vtable_vt);api->method(meta,api->intern("lookup"),custom_lookup);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){factory_type,meta},2);
    begin("custom factory lookup remains observable");assert(read_file(system_object,&bad_path,1)==answer);assert(read_file(system_object,&bad_path,1)==second_answer);end();assert(lookups==2);
    api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){factory_type,api->vtable_vt},2);api->method(factory_type,api->intern("kind:message:"),construct);
    collect=nested=1;begin("nested construction and callback GC");assert(read_file(system_object,&bad_path,1)==answer);end();collect=0;assert(makes==2 && factory_calls==2);
    candidates[0]=NULL;candidates[1]=api->nil_obj;candidates[2]=api->true_obj;candidates[3]=api->false_obj;candidates[4]=api->alloc(error_type,0);candidates[5]=api->alloc(alternate_type,0);
    Oop saved_type=error_type;
    for(recognition_phase=1;recognition_phase<=2;recognition_phase++) for(size_t v=0;v<6;v++) for(int flag=0;flag<4;flag++) for(reported_match=0;reported_match<2;reported_match++) {
        candidate=candidates[v];condition_step=0;type_present=flag==0?0:flag==3?-1:1;error_type=flag==1?NULL:saved_type;
        int recognized=candidate && type_present && error_type && reported_match;
        Oop expected=recognized?candidate:(recognition_phase==1 && candidate!=api->true_obj && candidate!=api->false_obj?answer:api->nil_obj);
        assert(while_true(condition,&body,1)==expected);cases++;
    }
    assert(cases==114);type_present=1;error_type=saved_type;recognition_phase=1;candidate=candidates[4];reported_match=mutate_type=1;
    collect=1;begin("recognition compares imported behavior snapshot");assert(while_true(condition,&body,1)==candidate && error_type==alternate_type);end();collect=0;error_type=saved_type;
    recognition_phase=2;candidate=candidates[5];reported_match=0;begin("inherited Error result is not exact");assert(while_true(condition,&body,1)==api->nil_obj);end();recognition_phase=0;
    begin("all System error-producing consumers");
    assert(write_file(system_object,(Oop[]){bad_path,contents},2)==answer);expect_message("could not write file");
    assert(get_env(system_object,&bad_path,1)==answer);expect_message("could not read environment");
    assert(load_source(system_object,&bad_path,1)==answer);expect_message("could not read source file");
    assert(load_module(system_object,&nul_path,1)==answer);expect_message("could not load module dylib");
    int saved=dup(STDIN_FILENO),unreadable=open("/",O_RDONLY);assert(saved>=0 && unreadable>=0 && dup2(unreadable,STDIN_FILENO)>=0);close(unreadable);
    assert(read_line(system_object,NULL,0)==answer);assert(dup2(saved,STDIN_FILENO)>=0);close(saved);expect_message("stdin read failed");end();
    if(fixture_error_value) {
        Dl_info owner;assert(dladdr((const void *)fixture_error_value->from_bytes,&owner) && strstr(owner.dli_fname,"/libbirnam_error_value.dylib"));
        IdApi alternate=selected;FixtureImportSlot alternate_import_slot=selected_import_slot;NamespaceImportV1 alternate_imports=FIXTURE_IMPORT(alternate_import_slot);assert(!provider_is_error(&alternate,fixture_namespace_presence(),&alternate_imports,candidates[4]));
        recognition_phase=current_phase=1;candidate=candidates[4];reported_match=1;assert(provider_is_error(&selected,&selected_presence,&selected_imports,candidate)==1);recognition_phase=0;
        const uint8_t bytes[]="bytes";borrowed_message=bytes;borrowed_length=5;
        assert(provider_from_bytes(&selected,&selected_presence,&selected_imports,(const uint8_t *)"ioError",8,bytes,5)==answer);expect_message("bytes");borrowed_message=NULL;
        assert(provider_from_bytes(&selected,&selected_presence,&selected_imports,(const uint8_t *)"ioError",8,(const uint8_t *)"",0)==answer);expect_message("");
        alternate.nil_obj=second_answer;assert(provider_from_bytes(&alternate,fixture_namespace_presence(),&alternate_imports,(const uint8_t *)"ioError",8,(const uint8_t *)"",0)==second_answer);
    }
    IdApi other=selected;FixtureImportSlot other_import_slot=selected_import_slot;NamespaceImportV1 other_imports=FIXTURE_IMPORT(other_import_slot);other.nil_obj=second_answer;prelude_init(&other,NULL,&selected_presence,&other_imports,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry());error_present=0;
    begin("reloaded Prelude preserves original execution selection");assert(read_file(system_object,&bad_path,1)==api->nil_obj);end();error_present=1;
    api->gc_set_precise(1);api->gc_collect();api->gc_set_precise(0);assert(read_file(system_object,&bad_path,1)==answer);
    fixture_string->make=original_make;fixture_string->context=original_context;
    printf("error-value: %zu recognition/construction cases, lazy exports, callback order, snapshots, live factories, reentry, GC and Prelude consumers passed\n",cases);
}
