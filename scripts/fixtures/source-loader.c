#include "idapi_v10.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <unistd.h>
#include <fcntl.h>
#include <stdlib.h>

static FixturePresenceSlot selected_presence_slot;
static NamespacePresenceV1 selected_presence=FIXTURE_PRESENCE(selected_presence_slot);

static FixturePresenceSlot other_presence_slot;
static NamespacePresenceV1 other_presence=FIXTURE_PRESENCE(other_presence_slot);

static IdApi *api,selected,other;
static ModuleInitFn prelude_init;
static Method loader;
static Oop system_type,system_object,alternate,expected_target,selectors[2],seen_selector,seen_target;
static Oop expected_path,path_name,bad_path,input,alias,error_value,inherited_error,error_type;
static Oop codegen,second_codegen,codegen_export,expected_codegen,revision,name_value,recorded,result,last_error,module,module_export;
static Oop revision_object,name_symbol,diagnostic,rendered,source_value,index_value,factory,second_factory,factory_export,answer,second,made,seen_factory;
static Oop (*original_make)(void *,const uint8_t *,size_t);
static Oop (*original_integer_make)(void *,int64_t);
static const uint8_t *(*original_bytes)(void *,Oop,size_t *);
static void *string_context,*integer_context,*changed_context=(void *)(uintptr_t)0x1234;
static char directory[]="/tmp/birnam-source-loader-XXXXXX",first_path[256],second_path[256],path_buffer[256];
static uint8_t source_bytes[16];static size_t source_length,path_length;
static const char *error_kind="ioError",*error_message="could not read source file";
static size_t bindings,registrations,allocations,interns,sends,exports,imports,vt_calls,makes,source_makes,integer_makes,factory_calls,cases,rev_calls,at_calls,record_calls,eval_calls,last_calls,mark_calls,print_calls;
static int capture,reentry_mode,reentered,map_selectors,map_generation,change_snapshot,collect;
static int codegen_present=1,module_present=1,error_present=1,type_present=1,source_ctor,error_ctor,integer_ctor,path_view,byte_mode,path_mutation;
static int mutate_codegen,mutate_factory,mutate_integer,mutate_nil,nested_make,nested_send,swap_bytes,spoof,swap_constructor;
static char diagnostic_bytes[2048];static size_t diagnostic_length;
static Oop invoke(Method implementation,Oop receiver,const Oop *args,size_t count);
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(Oop type,Oop selector) { return (Method)send1(type,"lookup",selector); }
static void roots(MarkFn mark) {
    mark(system_type);mark(system_object);mark(alternate);mark(expected_target);mark(selectors[0]);mark(selectors[1]);
    mark(expected_path);mark(path_name);mark(bad_path);mark(input);mark(alias);mark(error_value);mark(inherited_error);mark(error_type);mark(codegen);mark(second_codegen);mark(codegen_export);mark(expected_codegen);
    mark(revision);mark(name_value);mark(recorded);mark(result);mark(last_error);mark(module);mark(module_export);mark(revision_object);mark(name_symbol);mark(diagnostic);mark(rendered);mark(source_value);mark(index_value);
    mark(factory);mark(second_factory);mark(factory_export);mark(answer);mark(second);mark(made);mark(seen_factory);
}
static void registered(RootProvider provider) { registrations++;api->gc_register_root_provider(provider); }
static Oop allocate(Oop type,size_t size) { allocations++;if(capture) printf("alloc %zu\n",size);return api->alloc(type,size); }
static void reenter(void) { reentry_mode=0;reentered=1;prelude_init(&other,NULL,&other_presence); }
static Oop intern(const char *name) {
    interns++;if(capture) printf("intern %s\n",name);if(collect) api->gc_collect();
    if(reentry_mode==1 && !strcmp(name,"load:")) reenter();
    if(map_selectors && !strcmp(name,"load:")) return selectors[map_generation];
    if(mutate_factory==2 && !strcmp(name,"kind:message:")) { mutate_factory=0;factory_export=second_factory; }
    return api->intern(name);
}
static int exported(const char *name) {
    if(strcmp(name,"Error") && strcmp(name,"ErrorVt") && strcmp(name,"Codegen") && strcmp(name,"Module")) return fixture_namespace_contains(name);
    exports++;if(capture) printf("exported %s\n",name);if(collect) api->gc_collect();
    if(!strcmp(name,"Error")) return error_present;if(!strcmp(name,"ErrorVt")) return type_present;if(!strcmp(name,"Codegen")) return codegen_present;return module_present;
}
static Oop imported(const char *name) {
    if(strcmp(name,"Error") && strcmp(name,"ErrorVt") && strcmp(name,"Codegen") && strcmp(name,"Module")) return api->import_fn(name);
    imports++;if(capture) printf("import %s\n",name);if(collect) api->gc_collect();
    if(!strcmp(name,"Error")) return factory_export;if(!strcmp(name,"ErrorVt")) return error_type;if(!strcmp(name,"Codegen")) return codegen_export;return module_export;
}
static const uint8_t *alternate_bytes(void *context,Oop value,size_t *length) { assert(context==string_context && value==input);*length=path_length;return (const uint8_t *)path_buffer; }
static Oop vtable(Oop value) {
    vt_calls++;if(capture) puts("vtable");if(collect) api->gc_collect();
    if(swap_bytes) { swap_bytes=0;fixture_string->bytes=alternate_bytes; }
    return spoof && value==input?fixture_string->behavior:api->vtable_of(value);
}
static Oop forbidden_intern(const char *name) { (void)name;assert(!"registration table became execution table");return NULL; }
static Oop forbidden_vtable(Oop value) { (void)value;assert(!"registration table became execution table");return NULL; }
static int forbidden_exported(const char *name) {
    if(strcmp(name,"Error") && strcmp(name,"ErrorVt") && strcmp(name,"Codegen") && strcmp(name,"Module")) return fixture_namespace_contains(name);
    assert(!"registration table became execution table");return 0;
}
static Oop forbidden_send(Oop self,Oop selector,const Oop *args,size_t count) { (void)self;(void)selector;(void)args;(void)count;assert(!"registration table became execution table");return NULL; }
static Oop text(const uint8_t *bytes,size_t length) { return original_make(string_context,bytes,length); }
static void write_source(const uint8_t *bytes,size_t length) {
    assert(length<=sizeof(source_bytes));memcpy(source_bytes,bytes,length);source_length=length;
    FILE *file=fopen(first_path,"wb");assert(file && fwrite(bytes,1,length,file)==length && !fclose(file));
}
static Oop make(void *context,const uint8_t *bytes,size_t length) {
    assert(context==string_context);makes++;if(capture) printf("make %zu\n",length);if(collect) api->gc_collect();
    if(length==source_length && !memcmp(bytes,source_bytes,length)) {
        source_makes++;
        if(nested_make) { nested_make=0;uint8_t copy[16];memcpy(copy,bytes,length);Oop saved_source=source_value;
            Oop inner=loader(NULL,&path_name,1);assert(inner==result && !memcmp(copy,bytes,length));source_value=saved_source;
        }
        if(mutate_codegen) { mutate_codegen=0;codegen_export=second_codegen; }
        source_value=source_ctor==1?NULL:source_ctor==2?alias:original_make(context,bytes,length);return source_value;
    }
    assert(length==strlen(error_message) && !memcmp(bytes,error_message,length));
    if(mutate_factory==1) { mutate_factory=0;error_present=1;factory_export=second_factory; }
    made=error_ctor==1?NULL:error_ctor==2?alias:original_make(context,bytes,length);return made;
}
static Oop changed_make(void *context,const uint8_t *bytes,size_t length) { assert(context==changed_context);if(capture) puts("changed String constructor/context");return make(string_context,bytes,length); }
static const uint8_t *bytes(void *context,Oop value,size_t *length) {
    assert(context==string_context);if(capture) puts("bytes");if(collect) api->gc_collect();
    if(path_mutation==2 && value==diagnostic) { path_mutation=0;memcpy(path_buffer,second_path,path_length+1); }
    const uint8_t *result;
    if(value==path_name || value==input) {
        if(byte_mode==1) { *length=1;return NULL; }
        if(byte_mode==2) { *length=0;return NULL; }
        if(path_view) { *length=path_length;return (const uint8_t *)path_buffer; }
    }
    result=original_bytes(context,value,length);
    if(swap_constructor) { swap_constructor=0;fixture_string->make=changed_make;fixture_string->context=changed_context; }
    return result;
}
static Oop make_integer(void *context,int64_t value) { assert(context==integer_context && value==1);integer_makes++;if(capture) puts("Integer make 1");index_value=integer_ctor==1?NULL:integer_ctor==2?alias:original_integer_make(context,value);return index_value; }
static Oop changed_integer(void *context,int64_t value) { assert(context==changed_context);if(capture) puts("changed Integer constructor/context");return make_integer(integer_context,value); }
static Oop revision_of(Oop self,const Oop *args,size_t count) {
    assert(self==expected_codegen && args && count==1 && args[0]==source_value);rev_calls++;if(collect) api->gc_collect();
    if(mutate_integer) { mutate_integer=0;fixture_integer->make=changed_integer;fixture_integer->context=changed_context; }
    if(nested_send) { nested_send=0;Oop saved_source=source_value;assert(loader(NULL,&path_name,1)==result);source_value=saved_source; }
    return revision;
}
static Oop revision_at(Oop self,const Oop *args,size_t count) { assert(self==revision && args && count==1 && args[0]==index_value);at_calls++;return name_value; }
static Oop record_source(Oop self,const Oop *args,size_t count) { assert(self==expected_codegen && args && count==2 && args[0]==source_value && args[1]==name_value);record_calls++;if(collect) api->gc_collect();return recorded; }
static Oop evaluate(Oop self,const Oop *args,size_t count) {
    assert(self==expected_codegen && args && count==2 && args[0]==source_value && args[1]==expected_path);eval_calls++;if(collect) api->gc_collect();
    if(path_mutation==1) { path_mutation=0;memcpy(path_buffer,second_path,path_length+1); }
    if(mutate_nil) { mutate_nil=0;selected.nil_obj=result; }
    return result;
}
static Oop evaluate_second(Oop self,const Oop *args,size_t count) { (void)evaluate(self,args,count);return second; }
static Oop error_after_eval(Oop self,const Oop *args,size_t count) { assert(self==expected_codegen && !args && !count);last_calls++;return last_error; }
static Oop mark_loaded(Oop self,const Oop *args,size_t count) { assert(self==module && args && count==1 && args[0]==name_value);mark_calls++;return error_value; }
static Oop print_error(Oop self,const Oop *args,size_t count) { assert(self==error_value && !args && !count);print_calls++;return rendered; }
static Oop construct(Oop self,const Oop *args,size_t count) { assert(args && count==2 && args[0]==api->intern(error_kind) && args[1]==made);factory_calls++;seen_factory=self;if(collect) api->gc_collect();return answer; }
static Oop construct_second(Oop self,const Oop *args,size_t count) { (void)construct(self,args,count);return second; }
static size_t lookups;
static Oop factory_lookup(Oop self,const Oop *args,size_t count) { (void)self;assert(args && count==1 && args[0]==api->intern("kind:message:"));return (Oop)(++lookups%2?construct:construct_second); }
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) {
    sends++;if(capture) printf("send %s %zu\n",api->symbol_name(selector),count);if(collect) api->gc_collect();
    if(!self && selector==api->intern("at:")) return revision_at(self,args,count);
    return api->send(self,selector,args,count);
}
static Oop invoke(Method implementation,Oop receiver,const Oop *args,size_t count) {
    if(count==1 && args) expected_path=args[0];
    fflush(NULL);int saved=dup(STDERR_FILENO);FILE *file=tmpfile();assert(saved>=0 && file && dup2(fileno(file),STDERR_FILENO)>=0);
    Oop got=implementation(receiver,args,count);assert(dup2(saved,STDERR_FILENO)>=0);close(saved);
    assert(!fseek(file,0,SEEK_SET));diagnostic_length=fread(diagnostic_bytes,1,sizeof(diagnostic_bytes),file);assert(diagnostic_length<sizeof(diagnostic_bytes) && !ferror(file));fclose(file);return got;
}
static void diagnostic_is(int mode,const char *path) {
    char wanted[1024];int length=mode?snprintf(wanted,sizeof(wanted),mode==1?"birnam: load %s failed:\nA\xef\xbf\xbd\n":"birnam: load %s: A\xef\xbf\xbd\n",path):0;
    assert(length>=0 && diagnostic_length==(size_t)length && !memcmp(diagnostic_bytes,wanted,diagnostic_length));
}
static void defaults(void) {
    revision=api->nil_obj;name_value=name_symbol;recorded=api->nil_obj;result=alias;last_error=api->nil_obj;
    codegen_export=expected_codegen=codegen;codegen_present=module_present=error_present=type_present=1;module_export=module;rendered=diagnostic;
    error_kind="ioError";error_message="could not read source file";
}
static void counts(void) { interns=sends=exports=imports=vt_calls=makes=source_makes=integer_makes=factory_calls=rev_calls=at_calls=record_calls=eval_calls=last_calls=mark_calls=print_calls=0; }
static void begin(const char *name) { printf("case %s\n",name);capture=1;counts(); }
static void end(void) { capture=0;printf("done intern=%zu send=%zu export=%zu import=%zu vtable=%zu make=%zu source=%zu integer=%zu factory=%zu revision=%zu at=%zu record=%zu eval=%zu last=%zu mark=%zu print=%zu\n",interns,sends,exports,imports,vt_calls,makes,source_makes,integer_makes,factory_calls,rev_calls,at_calls,record_calls,eval_calls,last_calls,mark_calls,print_calls); }
static void method(Oop type,Oop selector,Method implementation) {
    if(capture) printf("method %s\n",api->symbol_name(selector));
    if(reentry_mode==2 && selector==api->intern("print:")) reenter();
    if(change_snapshot && selector==api->intern("getEnv:")) { change_snapshot=0;map_generation=1;api->export_obj("SystemVt",alternate); }
    if(selector==api->intern("load:") || (selectors[0] && (selector==selectors[0] || selector==selectors[1]))) {
        bindings++;seen_selector=selector;seen_target=type;loader=implementation;if(expected_target) assert(type==expected_target);
        assert(implementation(NULL,(Oop *)(uintptr_t)1,0)==selected.nil_obj);assert(invoke(implementation,NULL,&path_name,1)==alias);diagnostic_is(0,first_path);
    }
    api->method(type,selector,implementation);
}
static void validate(void *image,int attached) {
    SourceLoaderDependencyV2 attach=(SourceLoaderDependencyV2)api->dlsym_fn(image,"birnam_source_loader_dependency_v2");if(!attach) return;
    SourceLoaderApiV2 bad=*fixture_source_loader;assert(!attach(NULL));bad.version++;assert(!attach(&bad));bad=*fixture_source_loader;bad.byte_size--;assert(!attach(&bad));bad=*fixture_source_loader;bad.install=NULL;assert(!attach(&bad));
    if(attached) { bad=*fixture_source_loader;assert(!attach(&bad) && attach(fixture_source_loader)==1); }
}
static void mount(const char *path,IdApi *table,const NamespacePresenceV1 *presence,int prelude) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    if(prelude && api->dlsym_fn(image,"birnam_source_loader_dependency_v2") && !fixture_source_loader) {
        IdApi watched=*api;watched.alloc=allocate;watched.method=method;watched.gc_register_root_provider=registered;
        size_t before=allocations;fixture_prepare_source_loader(&watched);assert(!bindings && !registrations && allocations==before && !fixture_namespace_contains("System"));validate(image,0);
    }
    fixture_attach_integer(api,image);ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);if(prelude) { validate(image,1);prelude_init=init; }init(table,NULL,presence);
}
static Oop failure(void) { return invoke(loader,NULL,&bad_path,1); }
static Oop replacement(Oop self,const Oop *args,size_t count) { (void)self;(void)args;(void)count;return alias; }
int main(int argc,char **argv) {
    assert(argc==5);setvbuf(stdout,NULL,_IONBF,0);reentry_mode=!strcmp(argv[4],"first-load-intern-reentry")?1:(!strcmp(argv[4],"first-print-binding-reentry")?2:0);
    assert(mkdtemp(directory));snprintf(first_path,sizeof(first_path),"%s/a-src",directory);snprintf(second_path,sizeof(second_path),"%s/b-src",directory);path_length=strlen(first_path);strcpy(path_buffer,first_path);assert(strlen(second_path)==path_length);
    write_source((const uint8_t *)"source",6);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))();assert(api);api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]);fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports());uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));mount(argv[2],api,fixture_namespace_presence(),0);api->gc_register_root_provider(roots);
    original_make=fixture_string->make;original_bytes=fixture_string->bytes;string_context=fixture_string->context;original_integer_make=fixture_integer->make;integer_context=fixture_integer->context;
    fixture_string->make=make;fixture_string->bytes=bytes;fixture_integer->make=make_integer;
    path_name=text((const uint8_t *)first_path,path_length);bad_path=text((const uint8_t *)"\xff",1);diagnostic=text((const uint8_t *)"A\xff",2);alias=api->alloc(api->object_vt,0);second=api->alloc(api->object_vt,0);
    Oop cg_type=api->proto(api->object_vt);codegen=api->alloc(cg_type,0);second_codegen=api->alloc(cg_type,0);api->method(cg_type,api->intern("sourceRevisionOf:"),revision_of);api->method(cg_type,api->intern("recordModuleSource:named:"),record_source);api->method(cg_type,api->intern("evalPersisting:from:"),evaluate);api->method(cg_type,api->intern("lastError"),error_after_eval);
    Oop rev_type=api->proto(api->object_vt);revision_object=api->alloc(rev_type,0);api->method(rev_type,api->intern("at:"),revision_at);name_symbol=api->intern("source-probe");
    Oop module_type=api->proto(api->object_vt);module=api->alloc(module_type,0);api->method(module_type,api->intern("markLoaded:"),mark_loaded);
    Oop factory_type=api->proto(api->object_vt);factory=api->alloc(factory_type,0);second_factory=api->alloc(factory_type,0);factory_export=factory;answer=alias;api->method(factory_type,api->intern("kind:message:"),construct);
    error_type=api->proto(api->object_vt);error_value=api->alloc(error_type,0);inherited_error=api->alloc(api->proto(error_type),0);api->method(error_type,api->intern("printString"),print_error);defaults();
    selected=*api;selected.intern=intern;selected.method=method;selected.alloc=allocate;selected.gc_register_root_provider=registered;selected.vtable_of=vtable;selected_presence_slot=exported;selected.import_fn=imported;selected.send=send;
    other=selected;other.nil_obj=api->true_obj;other.vtable_of=forbidden_vtable;other_presence_slot=forbidden_exported;other.send=forbidden_send;
    begin("initial registration");mount(argv[3],&selected,&selected_presence,1);end();assert(bindings==(reentered?2u:1u) && !registrations);
    system_type=api->import_fn("SystemVt");system_object=api->import_fn("System");loader=lookup(system_type,api->intern("load:"));Dl_info owner;assert(loader && dladdr((const void *)loader,&owner));assert(strstr(owner.dli_fname,fixture_source_loader?"/libbirnam_source_loader.dylib":"_prelude.dylib"));
    begin("wrong arity and null carriers skip all callbacks");for(size_t n=0;n<=3;n++) if(n!=1) assert(invoke(loader,NULL,(Oop *)(uintptr_t)1,n)==api->nil_obj);assert(invoke(loader,NULL,NULL,1)==api->nil_obj);end();assert(!interns && !vt_calls && !makes && !sends);
    begin("null and inherited paths have no printing fallback");input=NULL;assert(invoke(loader,NULL,&input,1)==api->nil_obj);input=api->alloc(api->proto(fixture_string->behavior),16);assert(invoke(loader,NULL,&input,1)==api->nil_obj);end();assert(!sends && !makes);
    begin("absent view versus empty path");byte_mode=1;assert(invoke(loader,NULL,&path_name,1)==api->nil_obj);byte_mode=2;assert(invoke(loader,NULL,&path_name,1)==answer);byte_mode=0;end();assert(makes==1 && !source_makes && !rev_calls);
    begin("invalid UTF-8, interior NUL, missing and directory paths");Oop paths[]={bad_path,text((const uint8_t *)"a\0b",3),text((const uint8_t *)second_path,path_length),text((const uint8_t *)directory,strlen(directory))};for(size_t i=0;i<4;i++) assert(invoke(loader,NULL,&paths[i],1)==answer);end();assert(!source_makes && !rev_calls);
    begin("absent and null Codegen after file reading");error_kind="noCodegen";error_message="Codegen module is not loaded";codegen_present=0;assert(invoke(loader,NULL,&path_name,1)==answer);codegen_present=1;codegen_export=NULL;assert(invoke(loader,NULL,&path_name,1)==answer);end();assert(!source_makes && !rev_calls);defaults();
    if(!strcmp(argv[4],"normal")) {
        begin("one GiB accepted before Codegen check and excess rejected");int fd=open(first_path,O_WRONLY|O_TRUNC);assert(fd>=0 && !ftruncate(fd,(off_t)1<<30));codegen_present=0;error_kind="noCodegen";error_message="Codegen module is not loaded";
        assert(invoke(loader,NULL,&path_name,1)==answer);error_kind="ioError";error_message="could not read source file";assert(!ftruncate(fd,((off_t)1<<30)+1) && !close(fd));assert(invoke(loader,NULL,&path_name,1)==answer);end();assert(!source_makes);write_source((const uint8_t *)"source",6);defaults();
    }
    Oop receivers[]={NULL,api->nil_obj,alias,system_object};uint8_t data[4]={0};const uint8_t alphabet[]={'a',0,'\n',0xff};size_t combinations=1;
    for(size_t length=0;length<=3;length++,combinations*=4) for(size_t code=0;code<combinations;code++) {
        size_t value=code;for(size_t i=0;i<length;i++) { data[i]=alphabet[value%4];value/=4; }write_source(data,length);
        for(size_t r=0;r<4;r++) { counts();assert(invoke(loader,receivers[r],&path_name,1)==alias && source_makes==1 && rev_calls==1 && eval_calls==1 && !record_calls);size_t n=0;const uint8_t *actual=original_bytes(string_context,source_value,&n);assert(n==length && !memcmp(actual,data,length));diagnostic_is(0,first_path);cases++; }
    }assert(cases==340);write_source((const uint8_t *)"source",6);
    Oop revisions[]={api->nil_obj,NULL,error_value,revision_object},names[]={api->nil_obj,NULL,name_symbol,error_value},records[]={api->nil_obj,error_value,inherited_error,NULL};
    for(size_t r=0;r<4;r++) for(size_t n=0;n<4;n++) for(size_t v=0;v<4;v++) {
        defaults();revision=revisions[r];name_value=names[n];recorded=records[v];counts();int staged=revision!=api->nil_obj && revision!=error_value,early=staged && recorded==error_value;
        assert(invoke(loader,NULL,&path_name,1)==(early?error_value:alias));assert(rev_calls==1 && at_calls==(size_t)staged && integer_makes==(size_t)staged && record_calls==(size_t)staged && eval_calls==(size_t)!early && mark_calls==(size_t)(staged && !early && name_value!=api->nil_obj));diagnostic_is(0,first_path);cases++;
    }
    Oop results[]={api->nil_obj,NULL,error_value,inherited_error,alias},lasts[]={api->nil_obj,NULL,diagnostic,alias};
    for(size_t e=0;e<5;e++) for(size_t l=0;l<4;l++) for(int staged=0;staged<2;staged++) for(int m=0;m<3;m++) {
        defaults();revision=staged?revision_object:api->nil_obj;result=results[e];last_error=lasts[l];module_present=m!=0;module_export=m==1?NULL:module;error_kind="syntaxError";error_message="source file failed to compile";counts();
        int syntax=result==api->nil_obj && last_error!=api->nil_obj;Oop expected=syntax?answer:result;
        assert(invoke(loader,NULL,&path_name,1)==expected && eval_calls==1 && last_calls==(size_t)(result==api->nil_obj) && mark_calls==(size_t)(staged && result!=api->nil_obj && result!=error_value && m==2));
        diagnostic_is(result==error_value?2:syntax && last_error==diagnostic?1:0,first_path);cases++;
    }assert(cases==524);defaults();
    begin("null and alias source and Integer constructor outputs");revision=revision_object;for(source_ctor=0;source_ctor<3;source_ctor++) for(integer_ctor=0;integer_ctor<3;integer_ctor++) assert(invoke(loader,NULL,&path_name,1)==alias);source_ctor=integer_ctor=0;end();defaults();
    begin("revision callback selects current Integer constructor and context");revision=revision_object;mutate_integer=1;assert(invoke(loader,NULL,&path_name,1)==alias);fixture_integer->make=make_integer;fixture_integer->context=integer_context;end();defaults();
    begin("String callback preserves imported Codegen snapshot");mutate_codegen=1;assert(invoke(loader,NULL,&path_name,1)==alias && codegen_export==second_codegen);end();defaults();
    begin("path callback selects current String constructor and context");swap_constructor=1;assert(invoke(loader,NULL,&path_name,1)==alias);fixture_string->make=make;fixture_string->context=string_context;end();
    begin("vtable callback selects byte operation");input=path_name;swap_bytes=1;assert(invoke(loader,NULL,&input,1)==alias);fixture_string->bytes=bytes;end();
    begin("authoritative recognition and external borrowed path extent");input=api->alloc(api->object_vt,0);spoof=path_view=1;assert(invoke(loader,NULL,&input,1)==alias);spoof=path_view=0;end();
    begin("nested source construction preserves owned bytes with GC");collect=nested_make=1;assert(invoke(loader,NULL,&path_name,1)==alias);collect=0;end();assert(source_makes==2 && eval_calls==2);
    begin("nested evaluation protocol with callback GC");collect=nested_send=1;assert(invoke(loader,NULL,&path_name,1)==alias);collect=0;end();assert(source_makes==2 && eval_calls==2);
    begin("fresh evaluator method replacement");api->method(cg_type,api->intern("evalPersisting:from:"),evaluate_second);assert(invoke(loader,NULL,&path_name,1)==second);api->method(cg_type,api->intern("evalPersisting:from:"),evaluate);end();
    begin("current nil is consulted after evaluation");result=last_error=alias;mutate_nil=1;assert(invoke(loader,NULL,&path_name,1)==alias && last_calls==1);selected.nil_obj=api->nil_obj;end();defaults();
    for(int mode=1;mode<=2;mode++) {
        begin(mode==1?"borrowed path changes during evaluation before diagnostic":"borrowed path changes during diagnostic bytes callback");path_view=1;strcpy(path_buffer,first_path);result=mode==1?error_value:api->nil_obj;last_error=diagnostic;error_kind="syntaxError";error_message="source file failed to compile";path_mutation=mode;
        assert(invoke(loader,NULL,&path_name,1)==(mode==1?error_value:answer));diagnostic_is(mode==1?2:1,second_path);path_view=0;strcpy(path_buffer,first_path);end();defaults();
    }
    begin("unavailable printable error leaves result intact");result=error_value;rendered=alias;assert(invoke(loader,NULL,&path_name,1)==error_value);diagnostic_is(0,first_path);end();defaults();
    begin("absent factory still constructs kind and message");error_present=0;assert(failure()==api->nil_obj);end();assert(makes==1 && !factory_calls);defaults();
    begin("null factory and current nil");factory_export=NULL;selected.nil_obj=alias;assert(failure()==alias);selected.nil_obj=api->nil_obj;factory_export=factory;end();
    begin("null and alias error messages and results");for(error_ctor=0;error_ctor<3;error_ctor++) for(size_t i=0;i<3;i++) { answer=i==0?NULL:i==1?alias:second;assert(failure()==answer); }error_ctor=0;answer=alias;end();
    begin("message construction publishes Error factory");error_present=0;mutate_factory=1;assert(failure()==answer && seen_factory==second_factory);factory_export=factory;end();
    begin("selector callback preserves imported Error factory");mutate_factory=2;assert(failure()==answer && seen_factory==factory);factory_export=factory;end();
    begin("fresh Error factory method and custom lookup");api->method(factory_type,api->intern("kind:message:"),construct_second);assert(failure()==second);api->method(factory_type,api->intern("kind:message:"),construct);
    Oop meta=api->proto(api->vtable_vt);api->method(meta,api->intern("lookup"),factory_lookup);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){factory_type,meta},2);assert(failure()==answer && failure()==second && lookups==2);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){factory_type,api->vtable_vt},2);end();
    api->method(system_type,api->intern("load:"),replacement);begin("copied Method survives replacement");assert(invoke(loader,NULL,&path_name,1)==alias);end();
    begin("reload retains original execution table");prelude_init(&other,NULL,&other_presence);end();assert(lookup(system_type,api->intern("load:"))==loader);
    if(fixture_source_loader) { IdApi registration=other;registration.intern=forbidden_intern;fixture_source_loader->install(&registration,&other,&other_presence,system_type,api->intern("load:")); }
    begin("original execution after direct installation");assert(invoke(loader,NULL,&path_name,1)==alias && failure()==answer);end();
    selectors[0]=api->intern("source-loader-test-0");selectors[1]=api->intern("source-loader-test-1");alternate=api->proto(api->object_vt);expected_target=system_type;map_selectors=change_snapshot=1;map_generation=0;
    begin("earlier binding preserves behavior and selector snapshot");prelude_init(&selected,NULL,&selected_presence);end();assert(seen_selector==selectors[0] && seen_target==system_type && lookup(system_type,selectors[0])==loader);
    expected_target=alternate;begin("next installation adopts current selection");prelude_init(&other,NULL,&other_presence);end();assert(seen_selector==selectors[1] && seen_target==alternate);
    api->export_obj("SystemVt",system_type);map_selectors=0;expected_target=NULL;assert(!registrations);
    api->gc_set_precise(1);api->gc_collect();api->gc_set_precise(0);assert(invoke(loader,NULL,&path_name,1)==alias && failure()==answer);
    fixture_string->make=original_make;fixture_string->bytes=original_bytes;fixture_string->context=string_context;fixture_integer->make=original_integer_make;fixture_integer->context=integer_context;
    assert(!unlink(first_path) && !rmdir(directory));
    printf("source-loader: %zu binary/staging/evaluation cases, diagnostics, live callbacks, nested loads, GC, reentry, snapshots and reload passed mode=%s\n",cases,argv[4]);
}
