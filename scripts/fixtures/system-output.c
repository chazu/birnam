#include "idapi_v19.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <unistd.h>
#include <fcntl.h>
#include <signal.h>
#include <sys/wait.h>
static FixtureRootSlot watched_root_slot;
static RootRegistryV1 watched_roots=FIXTURE_ROOTS(watched_root_slot);
static FixtureRootSlot selected_root_slot;
static RootRegistryV1 selected_roots=FIXTURE_ROOTS(selected_root_slot);

static IdApi *api,selected,other;
static FILE *trace;
static ModuleInitFn prelude_init;
static Method outputs[2];
static const char *names[2]={"print:","println:"};
static Oop input,rendered,alias,system_type,system_object,alternate,expected_target;
static Oop selectors[2][2],seen_selectors[2],seen_targets[2];
static size_t bindings,registrations,allocations,vt_calls,sends,byte_calls,cases;
static int capture,reentry_mode,reentered,map_selectors,map_generation,change_snapshot,collect,nested;
static const uint8_t *(*original_bytes)(void *,Oop,size_t *);
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(Oop type,const char *name) { return (Method)send1(type,"lookup",api->intern(name)); }
static void roots(MarkFn mark) {
    mark(input);mark(rendered);mark(alias);mark(system_type);mark(system_object);mark(alternate);mark(expected_target);
    for(size_t g=0;g<2;g++) for(size_t i=0;i<2;i++) mark(selectors[g][i]);
}
static void registered(RootProvider provider) { registrations++;fixture_register_root(provider); }
static Oop allocate(Oop type,size_t size) { allocations++;if(capture) fprintf(trace,"alloc %zu\n",size);return api->alloc(type,size); }
static int output_index(Oop selector) {
    for(size_t i=0;i<2;i++) if(selector==api->intern(names[i]) || (selectors[0][i] && (selector==selectors[0][i] || selector==selectors[1][i]))) return (int)i;
    return -1;
}
static void reenter(void) { reentry_mode=0;reentered=1;prelude_init(&other,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); }
static Oop intern(const char *name) {
    if(capture) fprintf(trace,"intern %s\n",name);
    if(reentry_mode==1 && !strcmp(name,"print:")) reenter();
    if(map_selectors) for(size_t i=0;i<2;i++) if(!strcmp(name,names[i])) return selectors[map_generation][i];
    return api->intern(name);
}
static Oop vtable(Oop value) { vt_calls++;if(capture) fputs("vtable\n",trace);if(collect) api->gc_collect();return api->vtable_of(value); }
static Oop forbidden_vtable(Oop value) { (void)value;assert(!"registration table became execution table");return NULL; }
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) {
    sends++;if(capture) fprintf(trace,"send %s %zu\n",api->symbol_name(selector),count);if(collect) api->gc_collect();return api->send(self,selector,args,count);
}
static Oop forbidden_send(Oop self,Oop selector,const Oop *args,size_t count) { (void)self;(void)selector;(void)args;(void)count;assert(!"registration table became execution table");return NULL; }
static Oop capture_call(Method method,Oop receiver,const Oop *args,size_t count,uint8_t *out,size_t *length) {
    fflush(NULL);int saved=dup(STDOUT_FILENO);FILE *file=tmpfile();assert(saved>=0 && file && dup2(fileno(file),STDOUT_FILENO)>=0);
    Oop answer=method(receiver,args,count);assert(dup2(saved,STDOUT_FILENO)>=0);close(saved);
    assert(!fseek(file,0,SEEK_SET));*length=fread(out,1,4096,file);assert(!ferror(file) && *length<4096);fclose(file);return answer;
}
static void expect_output(Method method,Oop receiver,Oop value,const uint8_t *wanted,size_t count,int newline,Oop answer) {
    uint8_t output[4096];size_t n=0;Oop got=capture_call(method,receiver,&value,1,output,&n);
    assert(got==answer && n==count+(size_t)newline && !memcmp(output,wanted,count));if(newline) assert(output[count]=='\n');
}
static void method(Oop type,Oop selector,Method implementation) {
    if(capture) fprintf(trace,"method %s\n",api->symbol_name(selector));
    int index=output_index(selector);
    if(index>=0) {
        bindings++;seen_selectors[index]=selector;seen_targets[index]=type;
        assert(implementation(NULL,NULL,2)==api->nil_obj);
        expect_output(implementation,NULL,rendered,(const uint8_t *)"ready",5,index,NULL);
        if(expected_target) assert(type==expected_target);
        if(index==0) {
            if(reentry_mode==2) reenter();
            if(change_snapshot) { change_snapshot=0;map_generation=1;fixture_namespace_export("SystemVt",alternate); }
        }
    }
    api->method(type,selector,implementation);
}
static const uint8_t *bytes(void *context,Oop value,size_t *length) {
    byte_calls++;if(capture) fputs("bytes\n",trace);if(collect) api->gc_collect();
    if(nested) { nested=0;expect_output(outputs[1],alias,rendered,(const uint8_t *)"ready",5,1,alias); }
    return original_bytes(context,value,length);
}
static Oop text(const uint8_t *data,size_t length) { return fixture_string->make(fixture_string->context,data,length); }
static Oop render(Oop self,const Oop *args,size_t count) { (void)self;assert(!args && !count);if(collect) api->gc_collect();return rendered; }
static Oop replacement(Oop self,const Oop *args,size_t count) { (void)self;(void)args;(void)count;return alias; }
static void begin(const char *name) { fprintf(trace,"case %s\n",name);capture=1;vt_calls=sends=byte_calls=0; }
static void end(void) { capture=0;fprintf(trace,"done vtable=%zu send=%zu bytes=%zu\n",vt_calls,sends,byte_calls); }
static void validate(void *image,int attached) {
    SystemOutputDependencyV1 attach=(SystemOutputDependencyV1)api->dlsym_fn(image,"birnam_system_output_dependency_v1");if(!attach) return;
    SystemOutputApiV1 bad=*fixture_system_output;
    assert(!attach(NULL));bad.version++;assert(!attach(&bad));bad=*fixture_system_output;
    bad.byte_size--;assert(!attach(&bad));bad=*fixture_system_output;bad.install=NULL;assert(!attach(&bad));
    if(attached) { bad=*fixture_system_output;assert(!attach(&bad) && attach(fixture_system_output)==1); }
}
static void mount(const char *path,IdApi *table,int prelude,const RootRegistryV1 *root_registry) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    if(prelude && api->dlsym_fn(image,"birnam_system_output_dependency_v1") && !fixture_system_output) {
        IdApi watched=*api;watched.alloc=allocate;watched.method=method;watched_root_slot=registered;
        size_t before=allocations;fixture_prepare_system_output(&watched,&watched_roots);
        assert(!bindings && !registrations && allocations==before && !fixture_namespace_contains("System"));validate(image,0);
    }
    fixture_attach_integer(api,image,fixture_root_registry());ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);
    if(prelude) { validate(image,1);prelude_init=init; }init(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers());
}
static void output_errors(void) {
    /* Isolate a broken stdout and any library buffering in a child. Neither
       failed writes nor failed flushes may change the method's receiver result. */
    pid_t child=fork();assert(child>=0);
    if(!child) {
        signal(SIGPIPE,SIG_IGN);int pipefd[2];assert(!pipe(pipefd));close(pipefd[0]);assert(dup2(pipefd[1],STDOUT_FILENO)>=0);close(pipefd[1]);
        for(size_t op=0;op<2;op++) { assert(outputs[op](alias,&rendered,1)==alias);assert(outputs[op](NULL,&rendered,1)==NULL); }
        _exit(0);
    }
    int status=0;assert(waitpid(child,&status,0)==child && WIFEXITED(status) && WEXITSTATUS(status)==0);
}
int main(int argc,char **argv) {
    assert(argc==5);trace=fdopen(dup(STDOUT_FILENO),"w");assert(trace);setvbuf(trace,NULL,_IONBF,0);
    reentry_mode=!strcmp(argv[4],"first-print-intern-reentry")?1:(!strcmp(argv[4],"first-print-binding-reentry")?2:0);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))();assert(api);fixture_set_auto(0);
    fixture_mount_trace(api,argv[1],fixture_root_registry());fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));mount(argv[2],api,0,fixture_root_registry());fixture_register_root(roots);
    rendered=text((const uint8_t *)"ready",5);original_bytes=fixture_string->bytes;fixture_string->bytes=bytes;
    selected=*api;selected.intern=intern;selected.method=method;selected.alloc=allocate;selected_root_slot=registered;selected.vtable_of=vtable;selected.send=send;
    other=selected;other.nil_obj=api->true_obj;other.vtable_of=forbidden_vtable;other.send=forbidden_send;
    begin("initial registration");mount(argv[3],&selected,1,&selected_roots);end();assert(bindings==(reentered?4u:2u) && !registrations);
    system_type=fixture_namespace_import("SystemVt");system_object=fixture_namespace_import("System");alias=api->alloc(api->object_vt,0);
    for(size_t i=0;i<2;i++) {
        outputs[i]=lookup(system_type,names[i]);Dl_info owner;assert(outputs[i] && dladdr((const void *)outputs[i],&owner));
        assert(strstr(owner.dli_fname,fixture_system_output?"/libbirnam_system_output.dylib":"_prelude.dylib"));
    }
    begin("invalid arity and carriers");for(size_t i=0;i<2;i++) {
        assert(outputs[i](alias,NULL,1)==api->nil_obj);
        for(size_t count=0;count<=3;count++) if(count!=1) assert(outputs[i](alias,(Oop *)(uintptr_t)1,count)==api->nil_obj);
    }end();assert(!vt_calls && !sends && !byte_calls);
    begin("live nil and unavailable conversion");selected.nil_obj=alias;
    for(size_t i=0;i<2;i++) { assert(outputs[i](NULL,NULL,2)==alias);expect_output(outputs[i],NULL,NULL,(const uint8_t *)"",0,0,alias); }
    selected.nil_obj=api->nil_obj;end();
    Oop receivers[]={NULL,api->nil_obj,alias,system_object};uint8_t data[5]={0};const uint8_t alphabet[]={'a',0,0xff,0xc3};size_t combinations=1;
    for(size_t length=0;length<=3;length++,combinations*=4) for(size_t code=0;code<combinations;code++) {
        size_t value=code;for(size_t i=0;i<length;i++) { data[i]=alphabet[value%4];value/=4; }input=text(data,length);
        for(size_t r=0;r<4;r++) for(size_t op=0;op<2;op++) { expect_output(outputs[op],receivers[r],input,data,length,(int)op,receivers[r]);cases++; }
    }
    assert(cases==680);Oop type=api->proto(api->object_vt);input=api->alloc(type,0);api->method(type,api->intern("printString"),render);
    collect=nested=1;begin("fresh printing with nested output and callback GC");expect_output(outputs[0],alias,input,(const uint8_t *)"ready",5,0,alias);end();collect=0;
    for(size_t i=0;i<2;i++) api->method(system_type,api->intern(names[i]),replacement);
    begin("copied methods survive public replacement");for(size_t i=0;i<2;i++) expect_output(outputs[i],NULL,rendered,(const uint8_t *)"ready",5,(int)i,NULL);end();
    begin("reload retains original execution table");prelude_init(&other,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers());end();for(size_t i=0;i<2;i++) assert(lookup(system_type,names[i])==outputs[i]);
    if(fixture_system_output) fixture_system_output->install(&other,&other,system_type,api->intern(names[0]),api->intern(names[1]));
    begin("retained execution after reload");expect_output(outputs[0],alias,input,(const uint8_t *)"ready",5,0,alias);end();
    for(size_t g=0;g<2;g++) for(size_t i=0;i<2;i++) { char name[40];snprintf(name,sizeof(name),"output-test-%zu-%zu",g,i);selectors[g][i]=api->intern(name); }
    alternate=api->proto(api->object_vt);expected_target=system_type;map_selectors=change_snapshot=1;map_generation=0;
    begin("first binding preserves second selector and behavior snapshot");prelude_init(&selected,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers());end();
    for(size_t i=0;i<2;i++) { assert(seen_selectors[i]==selectors[0][i] && seen_targets[i]==system_type);assert((Method)send1(system_type,"lookup",selectors[0][i])==outputs[i]); }
    expected_target=alternate;begin("next installation adopts current behavior and selectors");prelude_init(&other,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers());end();
    for(size_t i=0;i<2;i++) assert(seen_selectors[i]==selectors[1][i] && seen_targets[i]==alternate);
    fixture_namespace_export("SystemVt",system_type);map_selectors=0;expected_target=NULL;assert(!registrations);
    fixture_set_precise(1);api->gc_collect();fixture_set_precise(0);expect_output(outputs[1],alias,input,(const uint8_t *)"ready",5,1,alias);
    output_errors();fixture_string->bytes=original_bytes;
    fprintf(trace,"system-output: %zu byte/receiver cases, registration reentry, live callbacks, flush, IO errors, snapshots, replacement and GC passed mode=%s\n",cases,argv[4]);fclose(trace);
}
