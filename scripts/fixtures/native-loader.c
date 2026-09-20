#include "idapi_v10.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <unistd.h>
#include <stdlib.h>

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
static Method loader;
static Oop system_type,system_object,alias,alternate,expected_target,selectors[2],seen_selector,seen_target;
static Oop path_name,bad_name,input,factory,second_factory,factory_export,answer,second,made,nested_result,seen_factory;
static Oop (*original_make)(void *,const uint8_t *,size_t);
static const uint8_t *(*original_bytes)(void *,Oop,size_t *);
static void *string_context,*changed_context=(void *)(uintptr_t)0x1234;
static uint8_t expected_bytes[32],lookup_buffer[32];
static size_t expected_length,borrowed_length,depth,opens,symbols,inits;
static const char *owned_paths[8];static char path_copies[8][32];
static int failure_stage,loader_nested,mutate_path,swap_symbol,swap_open,alternate_init;
static const char *error_kind="ioError",*error_message="could not load module dylib";
static Oop invoke(Method method,Oop receiver,const Oop *args,size_t count);
static void *mock_open(const char *path,int mode);
static void *changed_open(const char *path,int mode);
static void *mock_symbol(void *handle,const char *name);
static void *changed_symbol(void *handle,const char *name);
static size_t bindings,registrations,allocations,vt_calls,byte_calls,interns,makes,exports,imports,sends,calls,lookups,cases;
static int capture,reentry_mode,reentered,map_selectors,map_generation,change_snapshot,collect,nested,ctor_mode,present,change_factory;
static int byte_mode,spoof,swap_bytes,swap_constructor;
static void expected(const uint8_t *bytes,size_t length) {
    assert(length<sizeof(expected_bytes));memcpy(expected_bytes,bytes,length);expected_bytes[length]=0;expected_length=length;
    memcpy(lookup_buffer,expected_bytes,length+1);borrowed_length=length;
}
static void owned(void) {
    assert(depth && owned_paths[depth-1] && !strcmp(owned_paths[depth-1],path_copies[depth-1]));
}
static Oop invoke(Method method,Oop receiver,const Oop *args,size_t count) {
    assert(depth<8);owned_paths[depth++]=NULL;Oop result=method(receiver,args,count);depth--;return result;
}
static void nested_load(int stage) {
    if(loader_nested==stage) {
        loader_nested=0;assert(invoke(loader,alias,&path_name,1)==alias);owned();
    }
}
static void *initialize(IdApi *table,void *state,const NamespacePresenceV1 *presence,const NamespaceImportV1 *import_cap) {
    assert(presence==&selected_presence && import_cap==&selected_imports);
    assert(table==&selected && !state);inits++;if(capture) puts("initialize original API and null state");owned();
    if(collect) api->gc_collect();nested_load(3);owned();return (void *)(uintptr_t)0x1234;
}
static void *initialize_second(IdApi *table,void *state,const NamespacePresenceV1 *presence,const NamespaceImportV1 *import_cap) { (void)initialize(table,state,presence,import_cap);if(capture) puts("alternate initializer");return NULL; }
static void *mock_open(const char *path,int mode) {
    assert(depth && mode==1 && strlen(path)==expected_length && !memcmp(path,expected_bytes,expected_length));
    assert((const void *)path!=(const void *)lookup_buffer);owned_paths[depth-1]=path;memcpy(path_copies[depth-1],path,expected_length+1);
    opens++;if(capture) printf("open %zu\n",expected_length);if(collect) api->gc_collect();
    if(mutate_path) { mutate_path=0;memset(lookup_buffer,'x',borrowed_length); }
    if(swap_symbol) { swap_symbol=0;selected.dlsym_fn=changed_symbol; }
    nested_load(1);owned();return failure_stage==1?NULL:(void *)(uintptr_t)(0x1100+depth);
}
static void *changed_open(const char *path,int mode) { if(capture) puts("changed open");return mock_open(path,mode); }
static void *mock_symbol(void *handle,const char *name) {
    assert(handle==(void *)(uintptr_t)(0x1100+depth) && !strcmp(name,"module_init"));symbols++;if(capture) puts("symbol module_init");owned();
    if(collect) api->gc_collect();nested_load(2);owned();
    return failure_stage==2?NULL:(void *)(alternate_init?initialize_second:initialize);
}
static void *changed_symbol(void *handle,const char *name) { if(capture) puts("changed symbol");return mock_symbol(handle,name); }
static void *forbidden_open(const char *path,int mode) { (void)path;(void)mode;assert(!"registration table became execution table");return NULL; }
static void *forbidden_symbol(void *handle,const char *name) { (void)handle;(void)name;assert(!"registration table became execution table");return NULL; }
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(Oop type,Oop selector) { return (Method)send1(type,"lookup",selector); }
static void roots(MarkFn mark) {
    mark(system_type);mark(system_object);mark(alias);mark(alternate);mark(expected_target);mark(selectors[0]);mark(selectors[1]);
    mark(path_name);mark(bad_name);mark(input);mark(factory);mark(second_factory);mark(factory_export);mark(answer);mark(second);mark(made);mark(nested_result);mark(seen_factory);
}
static void registered(RootProvider provider) { registrations++;api->gc_register_root_provider(provider); }
static Oop allocate(Oop type,size_t size) { allocations++;if(capture) printf("alloc %zu\n",size);return api->alloc(type,size); }
static void reenter(void) { reentry_mode=0;reentered=1;prelude_init(&other,NULL,&other_presence,&other_imports); }
static Oop intern(const char *name) {
    interns++;if(capture) printf("intern %s\n",name);if(collect) api->gc_collect();
    if(reentry_mode==1 && !strcmp(name,"loadModule:")) reenter();
    if(map_selectors && !strcmp(name,"loadModule:")) return selectors[map_generation];
    if(change_factory==2 && !strcmp(name,"kind:message:")) { change_factory=0;factory_export=second_factory; }
    return api->intern(name);
}
static const uint8_t *alternate_bytes(void *context,Oop value,size_t *length) {
    assert(context==string_context && value==input);byte_calls++;if(capture) puts("alternate bytes");*length=borrowed_length;return lookup_buffer;
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
        nested=0;uint8_t *copy=malloc(length?length:1);assert(copy);memcpy(copy,bytes,length);nested_result=invoke(loader,alias,&bad_name,1);assert(nested_result==answer);
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
    else if(byte_mode==3) { *length=borrowed_length;result=lookup_buffer; }
    else result=original_bytes(context,value,length);
    if(swap_open) { swap_open=0;selected.dlopen_fn=changed_open; }
    if(swap_constructor) { swap_constructor=0;fixture_string->make=changed_make;fixture_string->context=changed_context; }
    return result;
}
static void method(Oop type,Oop selector,Method implementation) {
    if(capture) printf("method %s\n",api->symbol_name(selector));
    if(reentry_mode==2 && selector==api->intern("print:")) reenter();
    if(change_snapshot && selector==api->intern("load:")) { change_snapshot=0;map_generation=1;api->export_obj("SystemVt",alternate); }
    if(selector==api->intern("loadModule:") || (selectors[0] && (selector==selectors[0] || selector==selectors[1]))) {
        bindings++;seen_selector=selector;seen_target=type;loader=implementation;
        if(expected_target) assert(type==expected_target);
        assert(implementation(NULL,(Oop *)(uintptr_t)1,0)==selected.nil_obj);
        assert(invoke(implementation,NULL,&path_name,1)==NULL);
    }
    api->method(type,selector,implementation);
}
static void begin(const char *name) { printf("case %s\n",name);capture=1;vt_calls=byte_calls=interns=makes=exports=imports=sends=calls=opens=symbols=inits=0; }
static void end(void) { capture=0;printf("done vtable=%zu bytes=%zu intern=%zu make=%zu export=%zu import=%zu send=%zu calls=%zu\n",vt_calls,byte_calls,interns,makes,exports,imports,sends,calls);printf("loader open=%zu symbol=%zu init=%zu\n",opens,symbols,inits); }
static void validate(void *image,int attached) {
    NativeLoaderDependencyV3 attach=(NativeLoaderDependencyV3)api->dlsym_fn(image,"birnam_native_loader_dependency_v3");if(!attach) return;
    NativeLoaderApiV3 bad=*fixture_native_loader;
    assert(!attach(NULL));bad.version++;assert(!attach(&bad));bad=*fixture_native_loader;
    bad.byte_size--;assert(!attach(&bad));bad=*fixture_native_loader;bad.install=NULL;assert(!attach(&bad));
    if(attached) { bad=*fixture_native_loader;assert(!attach(&bad) && attach(fixture_native_loader)==1); }
}
static void mount(const char *path,IdApi *table,const NamespacePresenceV1 *presence,const NamespaceImportV1 *import_cap,int prelude) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    if(prelude && api->dlsym_fn(image,"birnam_native_loader_dependency_v3") && !fixture_native_loader) {
        IdApi watched=*api;watched.alloc=allocate;watched.method=method;watched.gc_register_root_provider=registered;
        size_t before=allocations;fixture_prepare_native_loader(&watched);
        assert(!bindings && !registrations && allocations==before && !fixture_namespace_contains("System"));validate(image,0);
    }
    fixture_attach_integer(api,image);ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);
    if(prelude) { validate(image,1);prelude_init=init; }init(table,NULL,presence,import_cap);
}
static Oop construct(Oop self,const Oop *args,size_t count) {
    assert(args && count==2 && args[0]==api->intern(error_kind));calls++;seen_factory=self;
    if(ctor_mode) assert(args[1]==made);else expect_bytes(args[1],(const uint8_t *)error_message,strlen(error_message));if(collect) api->gc_collect();return answer;
}
static Oop construct_second(Oop self,const Oop *args,size_t count) { (void)construct(self,args,count);return second; }
static Oop custom_lookup(Oop self,const Oop *args,size_t count) { (void)self;assert(count==1 && args[0]==api->intern("kind:message:"));lookups++;return (Oop)(lookups%2?construct:construct_second); }
static Oop replacement(Oop self,const Oop *args,size_t count) { (void)self;(void)args;(void)count;return alias; }
static Oop failure(void) { return invoke(loader,NULL,&bad_name,1); }
int main(int argc,char **argv) {
    assert(argc==5);setvbuf(stdout,NULL,_IONBF,0);
    reentry_mode=!strcmp(argv[4],"first-loadmodule-intern-reentry")?1:(!strcmp(argv[4],"first-print-binding-reentry")?2:0);
    expected((const uint8_t *)"ready",5);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))();assert(api);api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]);fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports());uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));mount(argv[2],api,fixture_namespace_presence(),fixture_namespace_imports(),0);api->gc_register_root_provider(roots);
    original_make=fixture_string->make;original_bytes=fixture_string->bytes;string_context=fixture_string->context;fixture_string->make=make;fixture_string->bytes=bytes;
    path_name=text(expected_bytes,expected_length);bad_name=text((const uint8_t *)"a\0b",3);
    selected=*api;selected.intern=intern;selected.method=method;selected.alloc=allocate;selected.gc_register_root_provider=registered;selected.vtable_of=vtable;selected_presence_slot=exported;selected_import_slot=imported;selected.send=send;selected.dlopen_fn=mock_open;selected.dlsym_fn=mock_symbol;
    other=selected;other_import_slot=selected_import_slot;other.nil_obj=api->true_obj;other.vtable_of=forbidden_vtable;other_presence_slot=forbidden_exported;other.send=forbidden_send;other.dlopen_fn=forbidden_open;other.dlsym_fn=forbidden_symbol;
    begin("initial registration");mount(argv[3],&selected,&selected_presence,&selected_imports,1);end();assert(bindings==(reentered?2u:1u) && !registrations);
    system_type=fixture_namespace_import("SystemVt");system_object=fixture_namespace_import("System");alias=api->alloc(api->object_vt,0);
    loader=lookup(system_type,api->intern("loadModule:"));Dl_info owner;assert(loader && dladdr((const void *)loader,&owner));assert(strstr(owner.dli_fname,fixture_native_loader?"/libbirnam_native_loader.dylib":"_prelude.dylib"));
    begin("invalid arity and null carriers skip policy");for(size_t n=0;n<=3;n++) if(n!=1) assert(invoke(loader,NULL,(Oop *)(uintptr_t)1,n)==api->nil_obj);assert(invoke(loader,NULL,NULL,1)==api->nil_obj);end();assert(!vt_calls && !byte_calls && !makes && !opens);
    begin("null and inherited paths have no printing fallback");input=NULL;assert(invoke(loader,NULL,&input,1)==api->nil_obj);input=api->alloc(api->proto(fixture_string->behavior),16);assert(invoke(loader,NULL,&input,1)==api->nil_obj);end();assert(!byte_calls && !sends && !opens);
    begin("absent view skips loading while zero extent loads empty path");byte_mode=1;assert(invoke(loader,NULL,&path_name,1)==api->nil_obj);expected((const uint8_t *)"",0);byte_mode=2;assert(invoke(loader,alias,&path_name,1)==alias);byte_mode=0;expected((const uint8_t *)"ready",5);end();assert(opens==1 && symbols==1 && inits==1);
    begin("absent Error still constructs kind and message");assert(failure()==api->nil_obj);end();assert(interns==1 && makes==1 && exports==1 && !imports && !opens);
    Oop type=api->proto(api->object_vt);factory=api->alloc(type,0);second_factory=api->alloc(type,0);answer=alias;second=api->alloc(api->object_vt,0);api->method(type,api->intern("kind:message:"),construct);present=1;factory_export=factory;
    Oop receivers[]={NULL,api->nil_obj,alias,system_object};uint8_t data[5]={0};const uint8_t alphabet[]={'a',0,0xff,0xc3};size_t combinations=1;
    for(size_t length=0;length<=3;length++,combinations*=4) for(size_t code=0;code<combinations;code++) {
        size_t value=code;int interior_nul=0;for(size_t i=0;i<length;i++) { data[i]=alphabet[value%4];value/=4;interior_nul|=!data[i]; }expected(data,length);input=text(data,length);
        for(failure_stage=0;failure_stage<3;failure_stage++) for(size_t r=0;r<4;r++) {
            error_kind=interior_nul || failure_stage==1?"ioError":"notAModule";
            error_message=interior_nul || failure_stage==1?"could not load module dylib":"dylib has no module_init";
            opens=symbols=inits=0;Oop got=invoke(loader,receivers[r],&input,1);
            assert(got==(interior_nul || failure_stage?answer:receivers[r]));
            assert(opens==(size_t)!interior_nul && symbols==(size_t)(!interior_nul && failure_stage!=1) && inits==(size_t)(!interior_nul && !failure_stage));cases++;
        }
    }assert(cases==1020);failure_stage=0;error_kind="ioError";error_message="could not load module dylib";expected((const uint8_t *)"ready",5);
    begin("live nil and null Error factory");selected.nil_obj=alias;factory_export=NULL;assert(failure()==alias && invoke(loader,NULL,NULL,0)==alias);factory_export=factory;selected.nil_obj=api->nil_obj;end();
    begin("initializer return is ignored");alternate_init=1;assert(invoke(loader,alias,&path_name,1)==alias);alternate_init=0;assert(invoke(loader,NULL,&path_name,1)==NULL);end();assert(inits==2);
    begin("vtable callback selects current byte operation");input=path_name;swap_bytes=1;assert(invoke(loader,alias,&input,1)==alias);fixture_string->bytes=bytes;end();
    begin("authoritative recognition and borrowed path extent");input=api->alloc(api->object_vt,0);spoof=1;byte_mode=3;assert(invoke(loader,alias,&input,1)==alias);spoof=byte_mode=0;end();
    begin("byte callback selects current loader");swap_open=1;assert(invoke(loader,alias,&path_name,1)==alias);selected.dlopen_fn=mock_open;end();
    begin("open callback selects current symbol operation");swap_symbol=1;assert(invoke(loader,alias,&path_name,1)==alias);selected.dlsym_fn=mock_symbol;end();
    begin("owned CString survives mutation of original borrowed bytes");byte_mode=3;mutate_path=1;assert(invoke(loader,alias,&path_name,1)==alias && memcmp(lookup_buffer,expected_bytes,borrowed_length));byte_mode=0;expected((const uint8_t *)"ready",5);end();
    for(int stage=1;stage<=3;stage++) { begin(stage==1?"nested load during open":stage==2?"nested load during symbol lookup":"nested load during initialization");collect=1;loader_nested=stage;assert(invoke(loader,alias,&path_name,1)==alias);collect=0;end();assert(opens==2 && symbols==2 && inits==2); }
    begin("byte callback selects current error constructor and context");swap_constructor=1;failure_stage=1;assert(invoke(loader,NULL,&path_name,1)==answer);failure_stage=0;fixture_string->make=make;fixture_string->context=string_context;end();
    begin("nested error construction with callback GC");collect=nested=1;assert(failure()==answer);collect=0;end();assert(makes==2 && calls==2);
    begin("null and alias messages and factory results");for(ctor_mode=0;ctor_mode<3;ctor_mode++) for(size_t i=0;i<3;i++) { answer=i==0?NULL:i==1?alias:system_object;assert(failure()==answer); }ctor_mode=0;answer=alias;end();
    begin("message callback publishes factory");present=0;change_factory=1;assert(failure()==answer && seen_factory==second_factory);end();factory_export=factory;
    begin("selector callback preserves imported factory");change_factory=2;assert(failure()==answer && seen_factory==factory);end();factory_export=factory;
    begin("fresh factory replacement and custom lookup");assert(failure()==answer);api->method(type,api->intern("kind:message:"),construct_second);assert(failure()==second);
    Oop meta=api->proto(api->vtable_vt);api->method(meta,api->intern("lookup"),custom_lookup);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){type,meta},2);assert(failure()==answer && failure()==second && lookups==2);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){type,api->vtable_vt},2);api->method(type,api->intern("kind:message:"),construct);end();
    api->method(system_type,api->intern("loadModule:"),replacement);begin("copied method survives replacement");assert(invoke(loader,alias,&path_name,1)==alias);end();
    begin("reload retains original execution table");prelude_init(&other,NULL,&other_presence,&other_imports);end();assert(lookup(system_type,api->intern("loadModule:"))==loader);
    if(fixture_native_loader) { IdApi registration=other;registration.intern=forbidden_intern;fixture_native_loader->install(&registration,&other,&other_presence,&other_imports,system_type,api->intern("loadModule:")); }
    begin("original execution after direct installation");assert(invoke(loader,NULL,&path_name,1)==NULL && failure()==answer);end();
    selectors[0]=api->intern("native-loader-test-0");selectors[1]=api->intern("native-loader-test-1");alternate=api->proto(api->object_vt);expected_target=system_type;map_selectors=change_snapshot=1;map_generation=0;
    begin("earlier binding preserves behavior and selector snapshot");prelude_init(&selected,NULL,&selected_presence,&selected_imports);end();assert(seen_selector==selectors[0] && seen_target==system_type && lookup(system_type,selectors[0])==loader);
    expected_target=alternate;begin("next installation adopts current selection");prelude_init(&other,NULL,&other_presence,&other_imports);end();assert(seen_selector==selectors[1] && seen_target==alternate);
    api->export_obj("SystemVt",system_type);map_selectors=0;expected_target=NULL;assert(!registrations);
    api->gc_set_precise(1);api->gc_collect();api->gc_set_precise(0);assert(invoke(loader,alias,&path_name,1)==alias && failure()==answer);
    fixture_string->make=original_make;fixture_string->bytes=original_bytes;fixture_string->context=string_context;
    printf("native-loader: %zu path/failure/receiver cases, original API/null state, live callbacks, owned paths, nested loads, errors, GC, reentry and reload passed mode=%s\n",cases,argv[4]);
}
