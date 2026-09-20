#include "idapi_v20.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <unistd.h>
#include <stdlib.h>
#include <fcntl.h>
#include <sys/stat.h>
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
static Method operations[2];
static const char *names[]={"readFile:","writeFile:contents:"};
static Oop system_type,system_object,alias,alternate,expected_target,selectors[2][2],seen_selectors[2],seen_targets[2];
static Oop path_name,bad_name,contents,input,factory,second_factory,factory_export,answer,second,made,nested_result,seen_factory;
static Oop (*original_make)(void *,const uint8_t *,size_t);
static const uint8_t *(*original_bytes)(void *,Oop,size_t *);
static void *string_context,*changed_context=(void *)(uintptr_t)0x1234;
static char directory[]="/tmp/birnam-file-io-XXXXXX",first_path[256],second_path[256],lookup_buffer[256];
static size_t path_extent;
static int large_read;
static const char *error_message="could not read file";
static size_t bindings,registrations,allocations,vt_calls,byte_calls,interns,makes,exports,imports,sends,calls,lookups,cases;
static int capture,reentry_mode,reentered,map_selectors,map_generation,change_snapshot,collect,nested,ctor_mode,present,change_factory;
static int byte_mode,spoof,swap_bytes,path_mutation,swap_constructor;
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(Oop type,Oop selector) { return (Method)send1(type,"lookup",selector); }
static void roots(MarkFn mark) {
    mark(system_type);mark(system_object);mark(alias);mark(alternate);mark(expected_target);for(size_t g=0;g<2;g++) for(size_t i=0;i<2;i++) mark(selectors[g][i]);
    mark(path_name);mark(contents);mark(bad_name);mark(input);mark(factory);mark(second_factory);mark(factory_export);mark(answer);mark(second);mark(made);mark(nested_result);mark(seen_factory);
}
static void registered(RootProvider provider) { registrations++;fixture_register_root(provider); }
static Oop allocate(Oop type,size_t size) { allocations++;if(capture) printf("alloc %zu\n",size);return api->alloc(type,size); }
static void reenter(void) { reentry_mode=0;reentered=1;prelude_init(&other,NULL,&other_presence,&other_imports,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); }
static Oop intern(const char *name) {
    interns++;if(capture) printf("intern %s\n",name);if(collect) api->gc_collect();
    if(reentry_mode==1 && !strcmp(name,"readFile:")) reenter();
    if(map_selectors) for(size_t i=0;i<2;i++) if(!strcmp(name,names[i])) return selectors[map_generation][i];
    if(change_factory==2 && !strcmp(name,"kind:message:")) { change_factory=0;factory_export=second_factory; }
    return api->intern(name);
}
static const uint8_t *alternate_bytes(void *context,Oop value,size_t *length) {
    assert(context==string_context && value==input);byte_calls++;if(capture) puts("alternate bytes");*length=path_extent;return (const uint8_t *)lookup_buffer;
}
static Oop vtable(Oop value) {
    vt_calls++;if(capture) puts("value vtable");if(collect) api->gc_collect();
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
static void write_bytes(const char *path,const uint8_t *data,size_t count) {
    FILE *file=fopen(path,"wb");assert(file);assert(fwrite(data,1,count,file)==count && !fclose(file));
}
static void expect_file(const char *path,const uint8_t *data,size_t count) {
    FILE *file=fopen(path,"rb");assert(file);for(size_t i=0;i<count;i++) assert(fgetc(file)==data[i]);assert(fgetc(file)==EOF && !ferror(file));assert(!fclose(file));
}
static Oop write_file(Oop receiver,Oop path,Oop data) { return operations[1](receiver,(Oop[]){path,data},2); }
static Oop make(void *context,const uint8_t *bytes,size_t length) {
    assert(context==string_context);
    if(large_read && length>1024) {
        assert(length==((size_t)1<<30) && bytes[0]==0 && bytes[length/2]==0 && bytes[length-1]==0);
        makes++;return alias;
    }
    makes++;if(capture) { printf("make %zu ",length);for(size_t i=0;i<length;i++) printf("%02x",bytes[i]);puts(""); }
    if(collect) api->gc_collect();
    if(nested) {
        nested=0;uint8_t *copy=malloc(length?length:1);assert(copy);memcpy(copy,bytes,length);assert(write_file(alias,path_name,contents)==alias);
        nested_result=operations[0](NULL,&path_name,1);expect_bytes(nested_result,(const uint8_t *)"nested",6);
        assert(!memcmp(copy,bytes,length));free(copy);
    }
    if(change_factory==1) { change_factory=0;present=1;factory_export=second_factory; }
    made=ctor_mode==1?NULL:ctor_mode==2?alias:original_make(context,bytes,length);return made;
}
static Oop changed_make(void *context,const uint8_t *bytes,size_t length) { assert(context==changed_context);if(capture) puts("changed constructor/context");return make(string_context,bytes,length); }
static const uint8_t *bytes(void *context,Oop value,size_t *length) {
    assert(context==string_context);byte_calls++;if(capture) puts("value bytes");if(collect) api->gc_collect();
    const uint8_t *result;
    if(byte_mode==1) { *length=1;result=NULL; }
    else if(byte_mode==2) { *length=0;result=NULL; }
    else if(byte_mode==3 && value!=contents) { *length=path_extent;result=(const uint8_t *)lookup_buffer; }
    else result=original_bytes(context,value,length);
    if(path_mutation && value==contents) {
        int mode=path_mutation;path_mutation=0;
        if(mode==1) memcpy(lookup_buffer,second_path,path_extent);
        else if(mode==2) lookup_buffer[0]=(char)0xff;
        else memcpy(lookup_buffer,first_path,path_extent);
    }
    if(swap_constructor) { swap_constructor=0;fixture_string->make=changed_make;fixture_string->context=changed_context; }
    return result;
}
static int operation_index(Oop selector) {
    for(size_t i=0;i<2;i++) if(selector==api->intern(names[i]) || (selectors[0][i] && (selector==selectors[0][i] || selector==selectors[1][i]))) return (int)i;
    return -1;
}
static void method(Oop type,Oop selector,Method implementation) {
    if(capture) printf("method %s\n",api->symbol_name(selector));
    if(reentry_mode==2 && selector==api->intern("print:")) reenter();
    int index=operation_index(selector);
    if(index>=0) {
        bindings++;seen_selectors[index]=selector;seen_targets[index]=type;operations[index]=implementation;
        if(expected_target) assert(type==expected_target);
        assert(implementation(NULL,(Oop *)(uintptr_t)1,0)==selected.nil_obj);
        if(index==0) {
            expect_bytes(implementation(NULL,&path_name,1),(const uint8_t *)"ready",5);
            if(change_snapshot) { change_snapshot=0;map_generation=1;fixture_namespace_export("SystemVt",alternate); }
        } else {
            assert(implementation(NULL,(Oop[]){path_name,contents},2)==NULL);
            expect_file(first_path,(const uint8_t *)"ready",5);
        }
    }
    api->method(type,selector,implementation);
}
static void begin(const char *name) { printf("case %s\n",name);capture=1;vt_calls=byte_calls=interns=makes=exports=imports=sends=calls=0; }
static void end(void) { capture=0;printf("done vtable=%zu bytes=%zu intern=%zu make=%zu export=%zu import=%zu send=%zu calls=%zu\n",vt_calls,byte_calls,interns,makes,exports,imports,sends,calls); }
static void validate(void *image,int attached) {
    FileIoDependencyV3 attach=(FileIoDependencyV3)api->dlsym_fn(image,"birnam_file_io_dependency_v3");if(!attach) return;
    FileIoApiV3 bad=*fixture_file_io;
    assert(!attach(NULL));bad.version++;assert(!attach(&bad));bad=*fixture_file_io;
    bad.byte_size--;assert(!attach(&bad));bad=*fixture_file_io;bad.install=NULL;assert(!attach(&bad));
    if(attached) { bad=*fixture_file_io;assert(!attach(&bad) && attach(fixture_file_io)==1); }
}
static void mount(const char *path,IdApi *table,const NamespacePresenceV1 *presence,const NamespaceImportV1 *import_cap,int prelude,const RootRegistryV1 *root_registry) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    if(prelude && api->dlsym_fn(image,"birnam_file_io_dependency_v3") && !fixture_file_io) {
        IdApi watched=*api;watched.alloc=allocate;watched.method=method;watched_root_slot=registered;
        size_t before=allocations;fixture_prepare_file_io(&watched,&watched_roots);
        assert(!bindings && !registrations && allocations==before && !fixture_namespace_contains("System"));validate(image,0);
    }
    fixture_attach_integer(api,image,fixture_root_registry());ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);
    if(prelude) { validate(image,1);prelude_init=init; }init(table,NULL,presence,import_cap,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers());
}
static Oop construct(Oop self,const Oop *args,size_t count) {
    assert(args && count==2 && args[0]==api->intern("ioError"));calls++;seen_factory=self;
    expect_bytes(args[1],(const uint8_t *)error_message,strlen(error_message));if(collect) api->gc_collect();return answer;
}
static Oop construct_second(Oop self,const Oop *args,size_t count) { (void)construct(self,args,count);return second; }
static Oop custom_lookup(Oop self,const Oop *args,size_t count) { (void)self;assert(count==1 && args[0]==api->intern("kind:message:"));lookups++;return (Oop)(lookups%2?construct:construct_second); }
static Oop replacement(Oop self,const Oop *args,size_t count) { (void)self;(void)args;(void)count;return alias; }
static Oop failure(void) { return operations[0](NULL,&bad_name,1); }
int main(int argc,char **argv) {
    assert(argc==5);setvbuf(stdout,NULL,_IONBF,0);
    reentry_mode=!strcmp(argv[4],"first-readfile-intern-reentry")?1:(!strcmp(argv[4],"first-print-binding-reentry")?2:0);
    assert(mkdtemp(directory));snprintf(first_path,sizeof(first_path),"%s/first",directory);snprintf(second_path,sizeof(second_path),"%s/other",directory);
    strcpy(lookup_buffer,first_path);path_extent=strlen(first_path);assert(strlen(second_path)==path_extent);write_bytes(first_path,(const uint8_t *)"ready",5);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))();assert(api);fixture_set_auto(0);
    fixture_mount_trace(api,argv[1],fixture_root_registry());fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());uintptr_t anchor=0;fixture_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));mount(argv[2],api,fixture_namespace_presence(),fixture_namespace_imports(),0,fixture_root_registry());fixture_register_root(roots);
    original_make=fixture_string->make;original_bytes=fixture_string->bytes;string_context=fixture_string->context;fixture_string->make=make;fixture_string->bytes=bytes;
    path_name=text((const uint8_t *)first_path,path_extent);bad_name=text((const uint8_t *)"\xff",1);contents=text((const uint8_t *)"ready",5);
    selected=*api;selected.intern=intern;selected.method=method;selected.alloc=allocate;selected_root_slot=registered;selected.vtable_of=vtable;selected_presence_slot=exported;selected_import_slot=imported;selected.send=send;
    other=selected;other_import_slot=selected_import_slot;other.nil_obj=api->true_obj;other.vtable_of=forbidden_vtable;other_presence_slot=forbidden_exported;other.send=forbidden_send;
    begin("initial registration");mount(argv[3],&selected,&selected_presence,&selected_imports,1,&selected_roots);end();assert(bindings==(reentered?4u:2u) && !registrations);
    system_type=fixture_namespace_import("SystemVt");system_object=fixture_namespace_import("System");alias=api->alloc(api->object_vt,0);
    for(size_t i=0;i<2;i++) {
        operations[i]=lookup(system_type,api->intern(names[i]));Dl_info owner;assert(operations[i] && dladdr((const void *)operations[i],&owner));
        assert(strstr(owner.dli_fname,fixture_file_io?"/libbirnam_file_io.dylib":"_prelude.dylib"));
    }
    begin("invalid arity and null carriers skip policy");for(size_t op=0;op<2;op++) {
        for(size_t count=0;count<=3;count++) if(count!=op+1) assert(operations[op](NULL,(Oop *)(uintptr_t)1,count)==api->nil_obj);
        assert(operations[op](NULL,NULL,op+1)==api->nil_obj);
    }end();assert(!vt_calls && !byte_calls && !makes && !exports && !interns);
    begin("null and inherited arguments are rejected without rendering");input=NULL;
    assert(operations[0](NULL,&input,1)==api->nil_obj);assert(write_file(NULL,input,contents)==api->nil_obj);assert(write_file(NULL,path_name,input)==api->nil_obj);
    input=api->alloc(api->proto(fixture_string->behavior),16);
    assert(operations[0](NULL,&input,1)==api->nil_obj);assert(write_file(NULL,input,contents)==api->nil_obj);assert(write_file(NULL,path_name,input)==api->nil_obj);end();assert(!sends && !makes);
    begin("absent view skips IO and zero extent preserves empty path errors");for(byte_mode=1;byte_mode<=2;byte_mode++) {
        assert(operations[0](NULL,&path_name,1)==api->nil_obj);assert(write_file(NULL,path_name,contents)==api->nil_obj);
    }byte_mode=0;end();assert(makes==2 && exports==2);
    begin("absent Error still constructs kind and message");assert(failure()==api->nil_obj);end();assert(interns==1 && makes==1 && exports==1 && !imports);
    Oop type=api->proto(api->object_vt);factory=api->alloc(type,0);second_factory=api->alloc(type,0);answer=alias;second=api->alloc(api->object_vt,0);api->method(type,api->intern("kind:message:"),construct);present=1;factory_export=factory;
    Oop receivers[]={NULL,api->nil_obj,alias,system_object};uint8_t data[5]={0};const uint8_t alphabet[]={'a',0,'\n',0xff};size_t combinations=1;
    for(size_t length=0;length<=3;length++,combinations*=4) for(size_t code=0;code<combinations;code++) {
        size_t value=code;for(size_t i=0;i<length;i++) { data[i]=alphabet[value%4];value/=4; }contents=text(data,length);
        for(size_t r=0;r<4;r++) {
            write_bytes(first_path,(const uint8_t *)"longer old contents",19);
            assert(write_file(receivers[r],path_name,contents)==receivers[r]);expect_file(first_path,data,length);cases++;
            expect_bytes(operations[0](receivers[r],&path_name,1),data,length);cases++;
        }
    }assert(cases==680);
    begin("invalid UTF-8, NUL, empty, directory and missing paths");Oop paths[]={bad_name,text((const uint8_t *)"a\0b",3),text((const uint8_t *)"",0),text((const uint8_t *)directory,strlen(directory)),text((const uint8_t *)second_path,path_extent)};
    for(size_t i=0;i<5;i++) {
        input=paths[i];error_message="could not read file";assert(operations[0](NULL,&input,1)==answer);
        if(i<4) { error_message="could not write file";assert(write_file(NULL,input,contents)==answer); }
    }error_message="could not read file";end();
    if(!strcmp(argv[4],"normal")) {
        begin("one GiB accepted and one GiB plus one rejected");
        int fd=open(second_path,O_WRONLY|O_CREAT|O_TRUNC,0600);assert(fd>=0 && !ftruncate(fd,(off_t)1<<30));
        input=text((const uint8_t *)second_path,path_extent);large_read=1;
        assert(operations[0](NULL,&input,1)==alias);
        assert(!ftruncate(fd,((off_t)1<<30)+1) && !close(fd));
        assert(operations[0](NULL,&input,1)==answer);large_read=0;assert(makes==2 && calls==1);
        assert(!unlink(second_path));end();
    }
    begin("contents rejection precedes invalid path conversion");assert(write_file(NULL,bad_name,alias)==api->nil_obj);end();assert(!makes && !exports);
    write_bytes(first_path,(const uint8_t *)"ready",5);contents=text((const uint8_t *)"ready",5);
    begin("live nil for arity and errors");selected.nil_obj=alias;assert(operations[0](NULL,NULL,0)==alias);assert(operations[1](NULL,NULL,0)==alias);factory_export=NULL;assert(failure()==alias);factory_export=factory;selected.nil_obj=api->nil_obj;end();
    begin("null and alias String results");for(ctor_mode=1;ctor_mode<=2;ctor_mode++) assert(operations[0](NULL,&path_name,1)==(ctor_mode==1?NULL:alias));ctor_mode=0;end();
    begin("vtable callback selects current byte operation");input=path_name;swap_bytes=1;expect_bytes(operations[0](NULL,&input,1),(const uint8_t *)"ready",5);fixture_string->bytes=bytes;end();
    begin("authoritative recognition and borrowed path extent");input=api->alloc(api->object_vt,0);spoof=1;byte_mode=3;expect_bytes(operations[0](NULL,&input,1),(const uint8_t *)"ready",5);spoof=byte_mode=0;end();
    begin("path callback selects current constructor and context");swap_constructor=1;expect_bytes(operations[0](NULL,&path_name,1),(const uint8_t *)"ready",5);fixture_string->make=make;fixture_string->context=string_context;end();
    begin("contents callback mutates borrowed path before conversion");byte_mode=3;path_mutation=1;
    assert(write_file(alias,path_name,contents)==alias);expect_file(second_path,(const uint8_t *)"ready",5);
    path_mutation=2;error_message="could not write file";assert(write_file(NULL,path_name,contents)==answer);
    path_mutation=3;assert(write_file(NULL,path_name,contents)==NULL);expect_file(first_path,(const uint8_t *)"ready",5);byte_mode=0;error_message="could not read file";end();assert(!unlink(second_path));
    write_bytes(first_path,(const uint8_t *)"outer",5);contents=text((const uint8_t *)"nested",6);collect=nested=1;
    begin("nested write and read preserve owned outer bytes with GC");expect_bytes(operations[0](NULL,&path_name,1),(const uint8_t *)"outer",5);end();collect=0;assert(makes==2);
    begin("nested error construction and callback GC");nested=collect=1;assert(failure()==answer);end();collect=0;
    begin("null and alias factory results");for(size_t i=0;i<3;i++) { answer=i==0?NULL:i==1?alias:system_object;assert(failure()==answer); }answer=alias;end();
    begin("message callback publishes factory");present=0;change_factory=1;assert(failure()==answer && seen_factory==second_factory);end();factory_export=factory;
    begin("selector callback keeps factory snapshot");change_factory=2;assert(failure()==answer && seen_factory==factory);end();factory_export=factory;
    begin("fresh factory replacement and custom lookup");assert(failure()==answer);api->method(type,api->intern("kind:message:"),construct_second);assert(failure()==second);
    Oop meta=api->proto(api->vtable_vt);api->method(meta,api->intern("lookup"),custom_lookup);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){type,meta},2);assert(failure()==answer && failure()==second && lookups==2);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){type,api->vtable_vt},2);api->method(type,api->intern("kind:message:"),construct);end();
    write_bytes(first_path,(const uint8_t *)"ready",5);contents=text((const uint8_t *)"ready",5);
    for(size_t i=0;i<2;i++) api->method(system_type,api->intern(names[i]),replacement);
    begin("copied methods survive public replacement");expect_bytes(operations[0](NULL,&path_name,1),(const uint8_t *)"ready",5);assert(write_file(alias,path_name,contents)==alias);end();
    begin("reload retains original execution table");prelude_init(&other,NULL,&other_presence,&other_imports,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers());end();for(size_t i=0;i<2;i++) assert(lookup(system_type,api->intern(names[i]))==operations[i]);
    if(fixture_file_io) { IdApi registration=other;registration.intern=forbidden_intern;fixture_file_io->install(&registration,&other,&other_presence,&other_imports,system_type,api->intern(names[0]),api->intern(names[1])); }
    begin("retained execution after reload");assert(failure()==answer);end();
    for(size_t g=0;g<2;g++) for(size_t i=0;i<2;i++) { char name[40];snprintf(name,sizeof(name),"file-io-test-%zu-%zu",g,i);selectors[g][i]=api->intern(name); }
    alternate=api->proto(api->object_vt);expected_target=system_type;map_selectors=change_snapshot=1;map_generation=0;
    begin("first binding preserves second selector and behavior snapshot");prelude_init(&selected,NULL,&selected_presence,&selected_imports,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers());end();
    for(size_t i=0;i<2;i++) assert(seen_selectors[i]==selectors[0][i] && seen_targets[i]==system_type && lookup(system_type,selectors[0][i])==operations[i]);
    expected_target=alternate;begin("next installation adopts current behavior and selectors");prelude_init(&other,NULL,&other_presence,&other_imports,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers());end();for(size_t i=0;i<2;i++) assert(seen_selectors[i]==selectors[1][i] && seen_targets[i]==alternate);
    fixture_namespace_export("SystemVt",system_type);map_selectors=0;expected_target=NULL;assert(!registrations);
    fixture_set_precise(1);api->gc_collect();fixture_set_precise(0);expect_bytes(operations[0](alias,&path_name,1),(const uint8_t *)"ready",5);assert(write_file(alias,path_name,contents)==alias && failure()==answer);
    fixture_string->make=original_make;fixture_string->bytes=original_bytes;fixture_string->context=string_context;
    assert(!unlink(first_path) && !rmdir(directory));
    printf("file-io: %zu binary IO/receiver cases, errors, borrowed path mutation, live callbacks, nested IO, reentry, snapshots and GC passed mode=%s\n",cases,argv[4]);
}
