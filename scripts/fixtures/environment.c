#include "idapi_v17.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <unistd.h>
#include <stdlib.h>
static FixtureRootSlot watched_root_slot;
static RootRegistryV1 watched_roots=FIXTURE_ROOTS(watched_root_slot);
static FixtureRootSlot selected_root_slot;
static RootRegistryV1 selected_roots=FIXTURE_ROOTS(selected_root_slot);

static FixturePresenceSlot selected_presence_slot;
static NamespacePresenceV1 selected_presence=FIXTURE_PRESENCE(selected_presence_slot);
static FixtureImportSlot selected_import_slot;
static NamespaceImportV1 selected_imports=FIXTURE_IMPORT(selected_import_slot);

static FixturePresenceSlot other_presence_slot;
static NamespacePresenceV1 other_presence=FIXTURE_PRESENCE(other_presence_slot);
static FixtureImportSlot other_import_slot;
static NamespaceImportV1 other_imports=FIXTURE_IMPORT(other_import_slot);

static IdApi *api,selected,other;
static ModuleInitFn prelude_init;
static Method env_method;
static Oop system_type,system_object,alias,alternate,expected_target,selectors[2],seen_selector,seen_target;
static Oop env_name,bad_name,input,factory,second_factory,factory_export,answer,second,made,nested_result,seen_factory;
static Oop (*original_make)(void *,const uint8_t *,size_t);
static const uint8_t *(*original_bytes)(void *,Oop,size_t *);
static void *string_context,*changed_context=(void *)(uintptr_t)0x1234;
static char key[80],lookup_buffer[256];
static size_t bindings,registrations,allocations,vt_calls,byte_calls,interns,makes,exports,imports,sends,calls,lookups,cases;
static int capture,reentry_mode,reentered,map_selectors,map_generation,change_snapshot,collect,nested,ctor_mode,present,change_factory;
static int byte_mode,spoof,swap_bytes,env_mutation,swap_constructor;
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(Oop type,Oop selector) { return (Method)send1(type,"lookup",selector); }
static void roots(MarkFn mark) {
    mark(system_type);mark(system_object);mark(alias);mark(alternate);mark(expected_target);mark(selectors[0]);mark(selectors[1]);
    mark(env_name);mark(bad_name);mark(input);mark(factory);mark(second_factory);mark(factory_export);mark(answer);mark(second);mark(made);mark(nested_result);mark(seen_factory);
}
static void registered(RootProvider provider) { registrations++;fixture_register_root(provider); }
static Oop allocate(Oop type,size_t size) { allocations++;if(capture) printf("alloc %zu\n",size);return api->alloc(type,size); }
static void reenter(void) { reentry_mode=0;reentered=1;prelude_init(&other,NULL,&other_presence,&other_imports,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,fixture_gc_settings(),fixture_payload_layout()); }
static Oop intern(const char *name) {
    interns++;if(capture) printf("intern %s\n",name);if(collect) api->gc_collect();
    if(reentry_mode==1 && !strcmp(name,"getEnv:")) reenter();
    if(map_selectors && !strcmp(name,"getEnv:")) return selectors[map_generation];
    if(change_factory==2 && !strcmp(name,"kind:message:")) { change_factory=0;factory_export=second_factory; }
    return api->intern(name);
}
static const uint8_t *alternate_bytes(void *context,Oop value,size_t *length) {
    assert(context==string_context && value==input);byte_calls++;if(capture) puts("alternate bytes");*length=strlen(lookup_buffer);return (const uint8_t *)lookup_buffer;
}
static Oop vtable(Oop value) {
    vt_calls++;if(capture) puts("name vtable");if(collect) api->gc_collect();
    if(swap_bytes) { swap_bytes=0;fixture_string->bytes=alternate_bytes; }
    return spoof && value==input?fixture_string->behavior:api->vtable_of(value);
}
static int exported(const char *name) {
    if(strcmp(name,"Error")) return fixture_namespace_contains(name);
    exports++;if(capture) puts("exported Error");if(collect) api->gc_collect();return present;
}
static Oop imported(const char *name) {
    if(strcmp(name,"Error")) return fixture_namespace_import(name);
    imports++;if(capture) puts("import Error");if(collect) api->gc_collect();return factory_export;
}
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) { sends++;if(capture) printf("send %s %zu\n",api->symbol_name(selector),count);if(collect) api->gc_collect();return api->send(self,selector,args,count); }
static Oop forbidden_intern(const char *name) { (void)name;assert(!"registration table became execution table");return NULL; }
static Oop forbidden_vtable(Oop value) { (void)value;assert(!"registration table became execution table");return NULL; }
static int forbidden_exported(const char *name) { if(strcmp(name,"Error")) return fixture_namespace_contains(name);assert(!"registration table became execution table");return 0; }
static Oop forbidden_send(Oop self,Oop selector,const Oop *args,size_t count) { (void)self;(void)selector;(void)args;(void)count;assert(!"registration table became execution table");return NULL; }
static void expect_bytes(Oop value,const uint8_t *wanted,size_t length) {
    assert(value);size_t actual=0;const uint8_t *bytes=original_bytes(string_context,value,&actual);
    assert(actual==length && (!length || (bytes && !memcmp(bytes,wanted,length))));
}
static Oop text(const uint8_t *bytes,size_t length) { return original_make(string_context,bytes,length); }
static Oop make(void *context,const uint8_t *bytes,size_t length) {
    assert(context==string_context);makes++;if(capture) { printf("make %zu ",length);for(size_t i=0;i<length;i++) printf("%02x",bytes[i]);puts(""); }
    if(collect) api->gc_collect();
    if(nested) {
        nested=0;uint8_t *copy=malloc(length?length:1);assert(copy);memcpy(copy,bytes,length);assert(!setenv(key,"nested",1));
        nested_result=env_method(NULL,&env_name,1);expect_bytes(nested_result,(const uint8_t *)"nested",6);
        assert(!memcmp(copy,bytes,length));free(copy);
    }
    if(change_factory==1) { change_factory=0;present=1;factory_export=second_factory; }
    made=ctor_mode==1?NULL:ctor_mode==2?alias:original_make(context,bytes,length);return made;
}
static Oop changed_make(void *context,const uint8_t *bytes,size_t length) { assert(context==changed_context);if(capture) puts("changed constructor/context");return make(string_context,bytes,length); }
static const uint8_t *bytes(void *context,Oop value,size_t *length) {
    assert(context==string_context);byte_calls++;if(capture) puts("name bytes");if(collect) api->gc_collect();
    const uint8_t *result;
    if(byte_mode==1) { *length=1;result=NULL; }
    else if(byte_mode==2) { *length=0;result=NULL; }
    else if(byte_mode==3) { *length=strlen(lookup_buffer);result=(const uint8_t *)lookup_buffer; }
    else result=original_bytes(context,value,length);
    if(env_mutation) { env_mutation=0;assert(!setenv(key,"from-bytes",1)); }
    if(swap_constructor) { swap_constructor=0;fixture_string->make=changed_make;fixture_string->context=changed_context; }
    return result;
}
static void method(Oop type,Oop selector,Method implementation) {
    if(capture) printf("method %s\n",api->symbol_name(selector));
    if(reentry_mode==2 && selector==api->intern("print:")) reenter();
    if(change_snapshot && selector==api->intern("writeFile:contents:")) { change_snapshot=0;map_generation=1;fixture_namespace_export("SystemVt",alternate); }
    if(selector==api->intern("getEnv:") || (selectors[0] && (selector==selectors[0] || selector==selectors[1]))) {
        bindings++;seen_selector=selector;seen_target=type;env_method=implementation;
        if(expected_target) assert(type==expected_target);
        assert(implementation(NULL,(Oop *)(uintptr_t)1,0)==selected.nil_obj);
        expect_bytes(implementation(NULL,&env_name,1),(const uint8_t *)"ready",5);
    }
    api->method(type,selector,implementation);
}
static void begin(const char *name) { printf("case %s\n",name);capture=1;vt_calls=byte_calls=interns=makes=exports=imports=sends=calls=0; }
static void end(void) { capture=0;printf("done vtable=%zu bytes=%zu intern=%zu make=%zu export=%zu import=%zu send=%zu calls=%zu\n",vt_calls,byte_calls,interns,makes,exports,imports,sends,calls); }
static void validate(void *image,int attached) {
    EnvironmentDependencyV3 attach=(EnvironmentDependencyV3)api->dlsym_fn(image,"birnam_environment_dependency_v3");if(!attach) return;
    EnvironmentApiV3 bad=*fixture_environment;
    assert(!attach(NULL));bad.version++;assert(!attach(&bad));bad=*fixture_environment;
    bad.byte_size--;assert(!attach(&bad));bad=*fixture_environment;bad.install=NULL;assert(!attach(&bad));
    if(attached) { bad=*fixture_environment;assert(!attach(&bad) && attach(fixture_environment)==1); }
}
static void mount(const char *path,IdApi *table,const NamespacePresenceV1 *presence,const NamespaceImportV1 *import_cap,int prelude,const RootRegistryV1 *root_registry) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    if(prelude && api->dlsym_fn(image,"birnam_environment_dependency_v3") && !fixture_environment) {
        IdApi watched=*api;watched.alloc=allocate;watched.method=method;watched_root_slot=registered;
        size_t before=allocations;fixture_prepare_environment(&watched,&watched_roots);
        assert(!bindings && !registrations && allocations==before && !fixture_namespace_contains("System"));validate(image,0);
    }
    fixture_attach_integer(api,image,fixture_root_registry());ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);
    if(prelude) { validate(image,1);prelude_init=init; }init(table,NULL,presence,import_cap,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout());
}
static Oop construct(Oop self,const Oop *args,size_t count) {
    assert(args && count==2 && args[0]==api->intern("ioError"));calls++;seen_factory=self;
    const char *message="could not read environment";expect_bytes(args[1],(const uint8_t *)message,strlen(message));if(collect) api->gc_collect();return answer;
}
static Oop construct_second(Oop self,const Oop *args,size_t count) { (void)construct(self,args,count);return second; }
static Oop custom_lookup(Oop self,const Oop *args,size_t count) { (void)self;assert(count==1 && args[0]==api->intern("kind:message:"));lookups++;return (Oop)(lookups%2?construct:construct_second); }
static Oop replacement(Oop self,const Oop *args,size_t count) { (void)self;(void)args;(void)count;return alias; }
static Oop failure(void) { return env_method(NULL,&bad_name,1); }
int main(int argc,char **argv) {
    assert(argc==5);setvbuf(stdout,NULL,_IONBF,0);
    reentry_mode=!strcmp(argv[4],"first-getenv-intern-reentry")?1:(!strcmp(argv[4],"first-print-binding-reentry")?2:0);
    snprintf(key,sizeof(key),"BIRNAM_81AR_%ld",(long)getpid());assert(!setenv(key,"ready",1));strcpy(lookup_buffer,key);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))();assert(api);fixture_set_auto(0);
    fixture_mount_trace(api,argv[1],fixture_root_registry());fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));mount(argv[2],api,fixture_namespace_presence(),fixture_namespace_imports(),0,fixture_root_registry());fixture_register_root(roots);
    original_make=fixture_string->make;original_bytes=fixture_string->bytes;string_context=fixture_string->context;fixture_string->make=make;fixture_string->bytes=bytes;
    env_name=text((const uint8_t *)key,strlen(key));bad_name=text((const uint8_t *)"\xff",1);
    selected=*api;selected.intern=intern;selected.method=method;selected.alloc=allocate;selected_root_slot=registered;selected.vtable_of=vtable;selected_presence_slot=exported;selected_import_slot=imported;selected.send=send;
    other=selected;other_import_slot=selected_import_slot;other.nil_obj=api->true_obj;other.vtable_of=forbidden_vtable;other_presence_slot=forbidden_exported;other.send=forbidden_send;
    begin("initial registration");mount(argv[3],&selected,&selected_presence,&selected_imports,1,&selected_roots);end();assert(bindings==(reentered?2u:1u) && !registrations);
    system_type=fixture_namespace_import("SystemVt");system_object=fixture_namespace_import("System");alias=api->alloc(api->object_vt,0);
    env_method=lookup(system_type,api->intern("getEnv:"));Dl_info owner;assert(env_method && dladdr((const void *)env_method,&owner));assert(strstr(owner.dli_fname,fixture_environment?"/libbirnam_environment.dylib":"_prelude.dylib"));
    begin("invalid arity and null carriers skip policy");for(size_t count=0;count<=3;count++) if(count!=1) assert(env_method(NULL,(Oop *)(uintptr_t)1,count)==api->nil_obj);assert(env_method(NULL,NULL,1)==api->nil_obj);end();assert(!vt_calls && !byte_calls && !makes && !exports && !interns);
    begin("null and inherited names are rejected without rendering");input=NULL;assert(env_method(NULL,&input,1)==api->nil_obj);input=api->alloc(api->proto(fixture_string->behavior),16);assert(env_method(NULL,&input,1)==api->nil_obj);end();assert(vt_calls==1 && !byte_calls && !sends);
    begin("empty and unavailable views preserve missing lookup");byte_mode=1;assert(env_method(NULL,&env_name,1)==api->nil_obj);byte_mode=2;assert(env_method(NULL,&env_name,1)==api->nil_obj);byte_mode=0;end();assert(!makes && !exports);
    begin("absent Error still constructs kind and message");assert(failure()==api->nil_obj);end();assert(interns==1 && makes==1 && exports==1 && !imports);
    Oop type=api->proto(api->object_vt);factory=api->alloc(type,0);second_factory=api->alloc(type,0);answer=alias;second=api->alloc(api->object_vt,0);api->method(type,api->intern("kind:message:"),construct);present=1;factory_export=factory;
    Oop receivers[]={NULL,api->nil_obj,alias,system_object};uint8_t data[5]={0};const uint8_t alphabet[]={'a','\n','\r',0xff};size_t combinations=1;
    for(size_t length=0;length<=3;length++,combinations*=4) for(size_t code=0;code<combinations;code++) {
        size_t value=code;int invalid=0;for(size_t i=0;i<length;i++) { data[i]=alphabet[value%4];value/=4;invalid|=data[i]==0xff; }data[length]=0;assert(!setenv(key,(const char *)data,1));
        for(size_t r=0;r<4;r++) { Oop got=env_method(receivers[r],&env_name,1);if(invalid) assert(got==answer);else expect_bytes(got,data,length);cases++; }
    }
    assert(!setenv(key,"ready",1));uint8_t name_bytes[128];size_t prefix=strlen(key);memcpy(name_bytes,key,prefix);const uint8_t name_alphabet[]={'x',0,'=',0xff};combinations=1;
    for(size_t length=0;length<=3;length++,combinations*=4) for(size_t code=0;code<combinations;code++) {
        size_t value=code;int invalid=0;for(size_t i=0;i<length;i++) { name_bytes[prefix+i]=name_alphabet[value%4];value/=4;invalid|=name_bytes[prefix+i]==0xff; }input=text(name_bytes,prefix+length);
        for(size_t r=0;r<4;r++) { Oop got=env_method(receivers[r],&input,1);if(invalid) assert(got==answer);else if(length && (name_bytes[prefix]!='=' || memchr(name_bytes+prefix,0,length))) assert(got==api->nil_obj);else expect_bytes(got,(const uint8_t *)"ready",5);cases++; }
    }
    /* The macOS baseline accepts KEY=... as KEY, after UTF-8 and interior-NUL checks. */
    assert(cases==680);
    char unicode_key[100];snprintf(unicode_key,sizeof(unicode_key),"%s_café",key);assert(!setenv(unicode_key,"κόσμος",1));input=text((const uint8_t *)unicode_key,strlen(unicode_key));expect_bytes(env_method(NULL,&input,1),(const uint8_t *)"κόσμος",strlen("κόσμος"));assert(!unsetenv(unicode_key));
    begin("live nil for missing lookup and errors");selected.nil_obj=alias;assert(!unsetenv(key));assert(env_method(NULL,&env_name,1)==alias);assert(env_method(NULL,NULL,0)==alias);factory_export=NULL;assert(failure()==alias);factory_export=factory;selected.nil_obj=api->nil_obj;assert(!setenv(key,"ready",1));end();
    begin("null and alias String results");for(ctor_mode=1;ctor_mode<=2;ctor_mode++) assert(env_method(NULL,&env_name,1)==(ctor_mode==1?NULL:alias));ctor_mode=0;end();
    begin("byte callback selects live environment value");env_mutation=1;expect_bytes(env_method(NULL,&env_name,1),(const uint8_t *)"from-bytes",10);end();assert(!setenv(key,"ready",1));
    begin("vtable callback selects current byte operation");input=env_name;swap_bytes=1;expect_bytes(env_method(NULL,&input,1),(const uint8_t *)"ready",5);fixture_string->bytes=bytes;end();
    begin("authoritative recognition and borrowed name extent");input=api->alloc(api->object_vt,0);spoof=1;byte_mode=3;expect_bytes(env_method(NULL,&input,1),(const uint8_t *)"ready",5);spoof=byte_mode=0;end();
    begin("name callback selects current constructor and context");swap_constructor=1;expect_bytes(env_method(NULL,&env_name,1),(const uint8_t *)"ready",5);fixture_string->make=make;fixture_string->context=string_context;end();
    assert(!setenv(key,"outer",1));collect=nested=1;begin("nested lookup and mutation keep owned outer value bytes");expect_bytes(env_method(NULL,&env_name,1),(const uint8_t *)"outer",5);end();collect=0;assert(makes==2);
    begin("nested error construction and callback GC");nested=collect=1;assert(failure()==answer);end();collect=0;
    begin("null and alias factory results");for(size_t i=0;i<3;i++) { answer=i==0?NULL:i==1?alias:system_object;assert(failure()==answer); }answer=alias;end();
    begin("message callback publishes factory");present=0;change_factory=1;assert(failure()==answer && seen_factory==second_factory);end();factory_export=factory;
    begin("selector callback keeps factory snapshot");change_factory=2;assert(failure()==answer && seen_factory==factory);end();factory_export=factory;
    begin("fresh factory replacement and custom lookup");assert(failure()==answer);api->method(type,api->intern("kind:message:"),construct_second);assert(failure()==second);
    Oop meta=api->proto(api->vtable_vt);api->method(meta,api->intern("lookup"),custom_lookup);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){type,meta},2);assert(failure()==answer && failure()==second && lookups==2);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){type,api->vtable_vt},2);api->method(type,api->intern("kind:message:"),construct);end();
    assert(!setenv(key,"ready",1));api->method(system_type,api->intern("getEnv:"),replacement);begin("copied method survives replacement");expect_bytes(env_method(NULL,&env_name,1),(const uint8_t *)"ready",5);end();
    begin("reload retains original execution table");prelude_init(&other,NULL,&other_presence,&other_imports,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,fixture_gc_settings(),fixture_payload_layout());end();assert(lookup(system_type,api->intern("getEnv:"))==env_method);
    if(fixture_environment) { IdApi registration=other;registration.intern=forbidden_intern;fixture_environment->install(&registration,&other,&other_presence,&other_imports,system_type,api->intern("getEnv:")); }
    begin("retained execution after reload");assert(failure()==answer);end();
    selectors[0]=api->intern("environment-test-0");selectors[1]=api->intern("environment-test-1");alternate=api->proto(api->object_vt);expected_target=system_type;map_selectors=change_snapshot=1;map_generation=0;
    begin("earlier registration keeps behavior and selector snapshot");prelude_init(&selected,NULL,&selected_presence,&selected_imports,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,fixture_gc_settings(),fixture_payload_layout());end();assert(seen_selector==selectors[0] && seen_target==system_type && lookup(system_type,selectors[0])==env_method);
    expected_target=alternate;begin("next installation adopts current behavior and selector");prelude_init(&other,NULL,&other_presence,&other_imports,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,fixture_gc_settings(),fixture_payload_layout());end();assert(seen_selector==selectors[1] && seen_target==alternate);
    fixture_namespace_export("SystemVt",system_type);map_selectors=0;expected_target=NULL;assert(!registrations);
    fixture_set_precise(1);api->gc_collect();fixture_set_precise(0);expect_bytes(env_method(alias,&env_name,1),(const uint8_t *)"ready",5);assert(failure()==answer);
    fixture_string->make=original_make;fixture_string->bytes=original_bytes;fixture_string->context=string_context;assert(!unsetenv(key));
    printf("environment: %zu name/value/receiver cases, Unicode, missing values, live callbacks, nested lookup, errors, registration reentry, snapshots and GC passed mode=%s\n",cases,argv[4]);
}
