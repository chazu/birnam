#include "idapi_v7.h"
#include "integer-support.h"
#include "trace-support.h"
#include <unistd.h>
#include <fcntl.h>
#include <stdlib.h>
#include <poll.h>
#include <signal.h>
#include <sys/wait.h>

static IdApi *api,selected,other;
static ModuleInitFn prelude_init;
static Method input_method;
static Oop system_type,system_object,alias,alternate,expected_target,selectors[2],seen_selector,seen_target;
static Oop factory,second_factory,factory_export,answer,second,made,nested_result,kind_arg,message_arg,seen_factory;
static Oop (*original_make)(void *,const uint8_t *,size_t);
static const uint8_t *(*original_bytes)(void *,Oop,size_t *);
static void *string_context,*changed_context=(void *)(uintptr_t)0x1234;
static size_t bindings,registrations,allocations,interns,makes,exports,imports,sends,calls,lookups,cases;
static int capture,reentry_mode,reentered,map_selectors,map_generation,change_snapshot,collect,nested,ctor_mode,present,change_factory;
static int nested_notice=-1;
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(Oop type,Oop selector) { return (Method)send1(type,"lookup",selector); }
static void roots(MarkFn mark) {
    mark(system_type);mark(system_object);mark(alias);mark(alternate);mark(expected_target);mark(selectors[0]);mark(selectors[1]);
    mark(factory);mark(second_factory);mark(factory_export);mark(answer);mark(second);mark(made);mark(nested_result);mark(kind_arg);mark(message_arg);mark(seen_factory);
}
static void registered(RootProvider provider) { registrations++;api->gc_register_root_provider(provider); }
static Oop allocate(Oop type,size_t size) { allocations++;if(capture) printf("alloc %zu\n",size);return api->alloc(type,size); }
static void reenter(void) { reentry_mode=0;reentered=1;prelude_init(&other,NULL); }
static Oop intern(const char *name) {
    interns++;if(capture) printf("intern %s\n",name);if(collect) api->gc_collect();
    if(reentry_mode==1 && !strcmp(name,"readLine")) reenter();
    if(map_selectors && !strcmp(name,"readLine")) return selectors[map_generation];
    if(change_factory==2 && !strcmp(name,"kind:message:")) { change_factory=0;factory_export=second_factory; }
    return api->intern(name);
}
static int exported(const char *name) {
    if(strcmp(name,"Error")) return api->is_exported(name);
    exports++;if(capture) puts("exported Error");if(collect) api->gc_collect();return present;
}
static Oop imported(const char *name) {
    if(strcmp(name,"Error")) return api->import_fn(name);
    imports++;if(capture) puts("import Error");if(collect) api->gc_collect();return factory_export;
}
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) { sends++;if(capture) printf("send %s %zu\n",api->symbol_name(selector),count);if(collect) api->gc_collect();return api->send(self,selector,args,count); }
static Oop forbidden_intern(const char *name) { (void)name;assert(!"registration table became execution table");return NULL; }
static int forbidden_exported(const char *name) { if(strcmp(name,"Error")) return api->is_exported(name);assert(!"registration table became execution table");return 0; }
static Oop forbidden_send(Oop self,Oop selector,const Oop *args,size_t count) { (void)self;(void)selector;(void)args;(void)count;assert(!"registration table became execution table");return NULL; }
static void expect_bytes(Oop value,const uint8_t *wanted,size_t length) {
    assert(value);size_t actual=0;const uint8_t *bytes=original_bytes(string_context,value,&actual);
    assert(actual==length && (!length || (bytes && !memcmp(bytes,wanted,length))));
}
static Oop make(void *context,const uint8_t *bytes,size_t length) {
    assert(context==string_context);makes++;if(capture) { printf("make %zu ",length);for(size_t i=0;i<length;i++) printf("%02x",bytes[i]);puts(""); }
    if(collect) api->gc_collect();
    if(nested) {
        nested=0;uint8_t *copy=malloc(length?length:1);assert(copy);memcpy(copy,bytes,length);
        assert(write(nested_notice,"E",1)==1);
        nested_result=input_method(NULL,NULL,0);expect_bytes(nested_result,(const uint8_t *)"inner",5);
        assert(!memcmp(copy,bytes,length));free(copy);
    }
    if(change_factory==1) { change_factory=0;present=1;factory_export=second_factory; }
    made=ctor_mode==1?NULL:ctor_mode==2?alias:original_make(context,bytes,length);return made;
}
static Oop changed_make(void *context,const uint8_t *bytes,size_t length) { assert(context==changed_context);if(capture) puts("changed constructor/context");return make(string_context,bytes,length); }
static FILE *feed(const uint8_t *bytes,size_t length,int *saved) {
    FILE *file=tmpfile();assert(file);assert(fwrite(bytes,1,length,file)==length && !fflush(file) && !fseek(file,0,SEEK_SET));
    *saved=dup(STDIN_FILENO);assert(*saved>=0 && dup2(fileno(file),STDIN_FILENO)>=0);return file;
}
static void restore(FILE *file,int saved) { assert(dup2(saved,STDIN_FILENO)>=0);close(saved);fclose(file); }
static void line_case(Method method,Oop receiver,const uint8_t *bytes,size_t length,const uint8_t *expected,size_t extent) {
    int saved;FILE *file=feed(bytes,length,&saved);Oop result=method(receiver,(Oop *)(uintptr_t)1,0);
    if(length) expect_bytes(result,expected,extent);else assert(result==selected.nil_obj);
    assert(method(NULL,NULL,0)==selected.nil_obj);restore(file,saved);
}
static void method(Oop type,Oop selector,Method implementation) {
    if(capture) printf("method %s\n",api->symbol_name(selector));
    if(reentry_mode==2 && selector==api->intern("print:")) reenter();
    if(change_snapshot && selector==api->intern("loadModule:")) { change_snapshot=0;map_generation=1;api->export_obj("SystemVt",alternate); }
    if(selector==api->intern("readLine") || (selectors[0] && (selector==selectors[0] || selector==selectors[1]))) {
        bindings++;seen_selector=selector;seen_target=type;input_method=implementation;
        if(expected_target) assert(type==expected_target);
        assert(implementation(NULL,(Oop *)(uintptr_t)1,1)==selected.nil_obj);
        line_case(implementation,NULL,(const uint8_t *)"ready\n",6,(const uint8_t *)"ready",5);
    }
    api->method(type,selector,implementation);
}
static void begin(const char *name) { printf("case %s\n",name);capture=1;interns=makes=exports=imports=sends=calls=0; }
static void end(void) { capture=0;printf("done intern=%zu make=%zu export=%zu import=%zu send=%zu calls=%zu\n",interns,makes,exports,imports,sends,calls); }
static void validate(void *image,int attached) {
    SystemInputDependencyV1 attach=(SystemInputDependencyV1)api->dlsym_fn(image,"birnam_system_input_dependency_v1");if(!attach) return;
    SystemInputApiV1 bad=*fixture_system_input;
    assert(!attach(NULL));bad.version++;assert(!attach(&bad));bad=*fixture_system_input;
    bad.byte_size--;assert(!attach(&bad));bad=*fixture_system_input;bad.install=NULL;assert(!attach(&bad));
    if(attached) { bad=*fixture_system_input;assert(!attach(&bad) && attach(fixture_system_input)==1); }
}
static void mount(const char *path,IdApi *table,int prelude) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    if(prelude && api->dlsym_fn(image,"birnam_system_input_dependency_v1") && !fixture_system_input) {
        IdApi watched=*api;watched.alloc=allocate;watched.method=method;watched.gc_register_root_provider=registered;
        size_t before=allocations;fixture_prepare_system_input(&watched);
        assert(!bindings && !registrations && allocations==before && !api->is_exported("System"));validate(image,0);
    }
    fixture_attach_integer(api,image);ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);
    if(prelude) { validate(image,1);prelude_init=init; }init(table,NULL);
}
static Oop construct(Oop self,const Oop *args,size_t count) {
    assert(args && count==2 && args[0]==api->intern("ioError"));calls++;seen_factory=self;kind_arg=args[0];message_arg=args[1];
    expect_bytes(message_arg,(const uint8_t *)"stdin read failed",17);if(collect) api->gc_collect();return answer;
}
static Oop construct_second(Oop self,const Oop *args,size_t count) { (void)construct(self,args,count);return second; }
static Oop custom_lookup(Oop self,const Oop *args,size_t count) { (void)self;assert(count==1 && args[0]==api->intern("kind:message:"));lookups++;return (Oop)(lookups%2?construct:construct_second); }
static Oop replacement(Oop self,const Oop *args,size_t count) { (void)self;(void)args;(void)count;return alias; }
static Oop read_error(void) {
    /* A directory fd gives a real read error; a bad stdin fd is normalized to
       EOF by Rust's stdin wrapper on the baseline platform. */
    int saved=dup(STDIN_FILENO),directory=open("/",O_RDONLY);assert(saved>=0 && directory>=0 && dup2(directory,STDIN_FILENO)>=0);close(directory);
    Oop result=input_method(NULL,NULL,0);assert(dup2(saved,STDIN_FILENO)>=0);close(saved);return result;
}
static void nested_lock_probe(int failure) {
    int notices[2];assert(!pipe(notices));pid_t child=fork();assert(child>=0);
    if(!child) {
        close(notices[0]);nested_notice=notices[1];capture=0;nested=1;alarm(3);
        if(failure) (void)read_error();
        else { int saved;FILE *file=feed((const uint8_t *)"outer\ninner\n",12,&saved);(void)input_method(NULL,NULL,0);restore(file,saved); }
        assert(write(nested_notice,"R",1)==1);_exit(0);
    }
    close(notices[1]);struct pollfd notification={notices[0],POLLIN,0};assert(poll(&notification,1,1000)==1);
    char entered=0;assert(read(notices[0],&entered,1)==1 && entered=='E');
    assert(poll(&notification,1,150)==0);int status=0;assert(waitpid(child,&status,WNOHANG)==0);
    assert(!kill(child,SIGTERM));assert(waitpid(child,&status,0)==child && WIFSIGNALED(status) && WTERMSIG(status)==SIGTERM);close(notices[0]);
    printf("nested %s read remains blocked while the outer stdin lock is held\n",failure?"error":"success");
}
int main(int argc,char **argv) {
    assert(argc==5);setvbuf(stdout,NULL,_IONBF,0);
    reentry_mode=!strcmp(argv[4],"first-readline-intern-reentry")?1:(!strcmp(argv[4],"first-print-binding-reentry")?2:0);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]);fixture_mount_integer(api,argv[1]);uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));mount(argv[2],api,0);api->gc_register_root_provider(roots);
    original_make=fixture_string->make;original_bytes=fixture_string->bytes;string_context=fixture_string->context;fixture_string->make=make;
    selected=*api;selected.intern=intern;selected.method=method;selected.alloc=allocate;selected.gc_register_root_provider=registered;selected.is_exported=exported;selected.import_fn=imported;selected.send=send;
    other=selected;other.nil_obj=api->true_obj;other.is_exported=forbidden_exported;other.send=forbidden_send;
    /* Registration still interns selectors, so only failure dispatch must use
       the original table. A later direct provider install can forbid intern too. */
    begin("initial registration");mount(argv[3],&selected,1);end();assert(bindings==(reentered?2u:1u) && !registrations);
    system_type=api->import_fn("SystemVt");system_object=api->import_fn("System");alias=api->alloc(api->object_vt,0);
    input_method=lookup(system_type,api->intern("readLine"));Dl_info owner;assert(input_method && dladdr((const void *)input_method,&owner));
    assert(strstr(owner.dli_fname,fixture_system_input?"/libbirnam_system_input.dylib":"_prelude.dylib"));
    begin("invalid arity skips input and all policy");for(size_t count=1;count<=3;count++) assert(input_method(NULL,(Oop *)(uintptr_t)1,count)==api->nil_obj);end();assert(!makes && !exports && !interns);
    Oop receivers[]={NULL,api->nil_obj,alias,system_object};uint8_t data[5]={0};const uint8_t alphabet[]={'a',0,0xff,'\r'};size_t combinations=1;
    for(size_t length=0;length<=3;length++,combinations*=4) for(size_t code=0;code<combinations;code++) {
        size_t value=code;for(size_t i=0;i<length;i++) { data[i]=alphabet[value%4];value/=4; }
        for(size_t r=0;r<4;r++) for(int terminated=0;terminated<2;terminated++) {
            data[length]='\n';line_case(input_method,receivers[r],data,length+(size_t)terminated,data,length);cases++;
        }
    }
    assert(cases==680);
    begin("live nil, EOF and null or alias String results");selected.nil_obj=alias;line_case(input_method,NULL,(const uint8_t *)"",0,NULL,0);assert(input_method(NULL,NULL,1)==alias);selected.nil_obj=api->nil_obj;
    for(ctor_mode=1;ctor_mode<=2;ctor_mode++) { int saved;FILE *file=feed((const uint8_t *)"x\n",2,&saved);assert(input_method(NULL,NULL,0)==(ctor_mode==1?NULL:alias));assert(input_method(NULL,NULL,0)==api->nil_obj);restore(file,saved); }ctor_mode=0;end();
    begin("live constructor and context");fixture_string->make=changed_make;fixture_string->context=changed_context;line_case(input_method,alias,(const uint8_t *)"fresh\n",6,(const uint8_t *)"fresh",5);fixture_string->make=make;fixture_string->context=string_context;end();
    begin("buffered lines retain binary bytes through callback GC");int saved;FILE *file=feed((const uint8_t *)"outer\ninner\nlast",16,&saved);collect=1;expect_bytes(input_method(NULL,NULL,0),(const uint8_t *)"outer",5);expect_bytes(input_method(NULL,NULL,0),(const uint8_t *)"inner",5);collect=0;expect_bytes(input_method(NULL,NULL,0),(const uint8_t *)"last",4);assert(input_method(NULL,NULL,0)==api->nil_obj);restore(file,saved);end();
    nested_lock_probe(0);nested_lock_probe(1);
    uint8_t *long_line=malloc(20001);assert(long_line);memset(long_line,'z',20000);long_line[20000]='\n';line_case(input_method,alias,long_line,20001,long_line,20000);free(long_line);
    begin("read error without a factory still constructs its message");assert(read_error()==api->nil_obj);end();assert(interns==1 && makes==1 && exports==1 && !imports);
    Oop type=api->proto(api->object_vt);factory=api->alloc(type,0);second_factory=api->alloc(type,0);answer=alias;second=api->alloc(api->object_vt,0);api->method(type,api->intern("kind:message:"),construct);present=1;factory_export=NULL;
    begin("null factory returns current nil");selected.nil_obj=second;assert(read_error()==second);selected.nil_obj=api->nil_obj;end();factory_export=factory;
    begin("error callbacks, null and alias factory results");collect=1;for(size_t i=0;i<3;i++) { answer=i==0?NULL:i==1?alias:system_object;assert(read_error()==answer); }collect=0;answer=alias;end();
    begin("message callback publishes factory");present=0;change_factory=1;assert(read_error()==answer && seen_factory==second_factory);end();factory_export=factory;
    begin("selector callback keeps factory snapshot");change_factory=2;assert(read_error()==answer && seen_factory==factory);end();factory_export=factory;
    begin("fresh factory replacement and custom lookup");assert(read_error()==answer);api->method(type,api->intern("kind:message:"),construct_second);assert(read_error()==second);
    Oop meta=api->proto(api->vtable_vt);api->method(meta,api->intern("lookup"),custom_lookup);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){type,meta},2);assert(read_error()==answer && read_error()==second && lookups==2);api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),(Oop[]){type,api->vtable_vt},2);api->method(type,api->intern("kind:message:"),construct);end();
    api->method(system_type,api->intern("readLine"),replacement);begin("copied method survives public replacement");line_case(input_method,NULL,(const uint8_t *)"copy\n",5,(const uint8_t *)"copy",4);end();
    begin("reload retains original execution table");prelude_init(&other,NULL);end();assert(lookup(system_type,api->intern("readLine"))==input_method);
    if(fixture_system_input) { IdApi registration=other;registration.intern=forbidden_intern;fixture_system_input->install(&registration,&other,system_type,api->intern("readLine")); }
    begin("retained execution after reload");assert(read_error()==answer);end();
    selectors[0]=api->intern("input-test-0");selectors[1]=api->intern("input-test-1");alternate=api->proto(api->object_vt);expected_target=system_type;map_selectors=change_snapshot=1;map_generation=0;
    begin("earlier registration keeps behavior and selector snapshot");prelude_init(&selected,NULL);end();assert(seen_selector==selectors[0] && seen_target==system_type && lookup(system_type,selectors[0])==input_method);
    expected_target=alternate;begin("next installation adopts current behavior and selector");prelude_init(&other,NULL);end();assert(seen_selector==selectors[1] && seen_target==alternate);
    api->export_obj("SystemVt",system_type);map_selectors=0;expected_target=NULL;assert(!registrations);
    api->gc_set_precise(1);api->gc_collect();api->gc_set_precise(0);line_case(input_method,alias,(const uint8_t *)"gc\n",3,(const uint8_t *)"gc",2);assert(read_error()==answer);
    fixture_string->make=original_make;fixture_string->context=string_context;
    printf("system-input: %zu byte/receiver cases, registration reentry, buffered and nested reads, EOF, errors, live callbacks, snapshots, replacement and GC passed mode=%s\n",cases,argv[4]);
}
