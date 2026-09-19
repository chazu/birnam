#include "idapi_v7.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <unistd.h>
#include <stdlib.h>
#include <sys/stat.h>

static IdApi *api,selected;
static FILE *trace;
static ModuleInitFn prelude_init;
static Method print_method,println_method,read_file,write_file,get_env,load_source,load_module;
static Oop system_object,input,rendered,second,path_object,contents,env_name,error_object,last_error;
static size_t byte_calls,vt_calls,interns,sends,source_reads,cases,lookups;
static int capture,byte_mode,send_mode,spoof,swap_bytes,collect,nested,path_view,path_mutation,eval_mode,module_mode,init_calls;
static char path_buffer[1024],first_path[1024],second_path[1024];
static uint8_t replacement_bytes[]={0,'x',0xff};
static Oop (*original_make)(void *,const uint8_t *,size_t);
static const uint8_t *(*original_bytes)(void *,Oop,size_t *);
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(Oop type,const char *name) { return (Method)send1(type,"lookup",api->intern(name)); }
static const char *role(Oop v) { return !v?"null":v==input?"input":v==rendered?"rendered":v==second?"second":v==path_object?"path":v==contents?"contents":v==env_name?"env":"other"; }
static void roots(MarkFn mark) { mark(input);mark(rendered);mark(second);mark(path_object);mark(contents);mark(env_name);mark(error_object);mark(last_error);mark(system_object); }
static const uint8_t *alternate_bytes(void *context,Oop value,size_t *length) {
    (void)context;(void)value;byte_calls++;if(capture) fputs("alternate bytes\n",trace);*length=sizeof(replacement_bytes);return replacement_bytes;
}
static Oop vtable(Oop value) {
    vt_calls++;if(capture) fprintf(trace,"vtable %s\n",role(value));
    if(swap_bytes) { swap_bytes=0;fixture_string->bytes=alternate_bytes; }
    if(collect) api->gc_collect();
    return spoof && value==input?fixture_string->behavior:api->vtable_of(value);
}
static Oop intern(const char *name) { interns++;if(capture) fprintf(trace,"intern %s\n",name);if(collect) api->gc_collect();return api->intern(name); }
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) {
    sends++;if(capture) fprintf(trace,"send %s %s %zu\n",role(self),api->symbol_name(selector),count);
    if(collect) api->gc_collect();
    if(send_mode) { assert(selector==api->intern("printString") && !args && !count);return send_mode==1?input:send_mode==2?rendered:send_mode==3?NULL:api->true_obj; }
    return api->send(self,selector,args,count);
}
static const uint8_t *bytes(void *context,Oop value,size_t *length) {
    byte_calls++;if(capture) fprintf(trace,"bytes %s\n",role(value));
    if(nested) { nested=0;Oop r=get_env(system_object,&env_name,1);size_t n=0;const uint8_t *b=original_bytes(context,r,&n);assert(b && n==6 && !memcmp(b,"nested",6)); }
    if(collect) api->gc_collect();
    if(value==input) {
        source_reads++;
        if(byte_mode==1 || (byte_mode==3 && source_reads==1)) { *length=1;return NULL; }
        if(byte_mode==2) { *length=0;return NULL; }
        if(byte_mode==4) { *length=sizeof(replacement_bytes);return replacement_bytes; }
    }
    if(path_view && value==path_object) { *length=strlen(path_buffer);return (const uint8_t *)path_buffer; }
    if(path_mutation && value==contents) { path_mutation=0;strcpy(path_buffer,second_path); }
    return original_bytes(context,value,length);
}
static Oop make(void *context,const uint8_t *value,size_t length) { if(capture) fprintf(trace,"make %zu\n",length);if(collect) api->gc_collect();return original_make(context,value,length); }
static Oop text(const char *value) { return original_make(fixture_string->context,(const uint8_t *)value,strlen(value)); }
static void expect_string(Oop value,const uint8_t *wanted,size_t count) {
    size_t n=0;const uint8_t *b=original_bytes(fixture_string->context,value,&n);assert(b && n==count && !memcmp(b,wanted,n));
}
static Oop capture_call(int descriptor,Method method,Oop receiver,const Oop *args,size_t count,uint8_t *out,size_t *length) {
    fflush(NULL);int saved=dup(descriptor);FILE *file=tmpfile();assert(saved>=0 && file && dup2(fileno(file),descriptor)>=0);
    Oop answer=method(receiver,args,count);fflush(NULL);assert(dup2(saved,descriptor)>=0);close(saved);
    assert(!fseek(file,0,SEEK_SET));*length=fread(out,1,4096,file);assert(!ferror(file) && *length<4096);fclose(file);return answer;
}
static void expect_output(Method method,Oop value,const uint8_t *wanted,size_t count,int newline,Oop answer) {
    uint8_t output[4096];size_t n=0;Oop got=capture_call(STDOUT_FILENO,method,system_object,&value,1,output,&n);
    assert(got==answer && n==count+(size_t)newline && !memcmp(output,wanted,count));if(newline) assert(output[count]=='\n');
}
static Oop first_answer(Oop self,const Oop *args,size_t count) { (void)self;assert(!args && !count);if(collect) api->gc_collect();return rendered; }
static Oop second_answer(Oop self,const Oop *args,size_t count) { (void)self;assert(!args && !count);return second; }
static Oop dynamic_lookup(Oop self,const Oop *args,size_t count) { (void)self;assert(count==1 && args[0]==api->intern("printString"));lookups++;return (Oop)(lookups%2?first_answer:second_answer); }
static Oop revision(Oop self,const Oop *args,size_t count) { (void)self;assert(args && count==1);return api->nil_obj; }
static Oop evaluate(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==2 && args[1]==path_object);expect_string(args[0],(const uint8_t *)"source",6);
    strcpy(path_buffer,second_path);if(collect) api->gc_collect();return eval_mode?api->nil_obj:error_object;
}
static Oop last_error_method(Oop self,const Oop *args,size_t count) { (void)self;assert(!args && !count);return last_error; }
static void *fake_init(IdApi *passed,void *state) { assert(passed==&selected && !state);init_calls++;return NULL; }
static void *open_module(const char *path,int flags) { assert(!strcmp(path,first_path) && flags==1);if(capture) fputs("dlopen\n",trace);return module_mode==1?NULL:(void *)(uintptr_t)0x1234; }
static void *find_module(void *handle,const char *name) { assert(handle==(void *)(uintptr_t)0x1234 && !strcmp(name,"module_init"));if(capture) fputs("dlsym\n",trace);return module_mode==2?NULL:(void *)fake_init; }
static void begin(const char *name) { fprintf(trace,"case %s\n",name);capture=1;byte_calls=vt_calls=interns=sends=source_reads=0; }
static void end(void) { capture=0;fprintf(trace,"done bytes=%zu vtable=%zu intern=%zu send=%zu\n",byte_calls,vt_calls,interns,sends); }
static void validate(void *image,int attached) {
    TextViewDependencyV1 attach=(TextViewDependencyV1)api->dlsym_fn(image,"birnam_text_view_dependency_v1");if(!attach) return;
    TextViewApiV1 bad=*fixture_text_view;assert(!attach(NULL));bad.version++;assert(!attach(&bad));bad=*fixture_text_view;
    bad.byte_size--;assert(!attach(&bad));bad=*fixture_text_view;bad.exact=NULL;assert(!attach(&bad));bad=*fixture_text_view;bad.printable=NULL;assert(!attach(&bad));
    if(attached) { bad=*fixture_text_view;assert(!attach(&bad) && attach(fixture_text_view)==1); }
}
static void mount(const char *path,IdApi *table,int prelude) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    if(prelude && api->dlsym_fn(image,"birnam_text_view_dependency_v1")) { fixture_prepare_text_view(api);assert(!api->is_exported("System"));validate(image,0); }
    fixture_attach_integer(api,image);ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);if(prelude) { prelude_init=init;validate(image,1); }init(table,NULL);
}
int main(int argc,char **argv) {
    assert(argc==4);trace=fdopen(dup(STDOUT_FILENO),"w");assert(trace);setvbuf(trace,NULL,_IONBF,0);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))();assert(api);api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]);fixture_mount_integer(api,argv[1]);uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));mount(argv[2],api,0);
    selected=*api;selected.vtable_of=vtable;selected.intern=intern;selected.send=send;mount(argv[3],&selected,1);api->gc_register_root_provider(roots);
    Oop system_type=api->import_fn("SystemVt");system_object=api->import_fn("System");
    print_method=lookup(system_type,"print:");println_method=lookup(system_type,"println:");read_file=lookup(system_type,"readFile:");write_file=lookup(system_type,"writeFile:contents:");get_env=lookup(system_type,"getEnv:");load_source=lookup(system_type,"load:");load_module=lookup(system_type,"loadModule:");
    original_make=fixture_string->make;original_bytes=fixture_string->bytes;fixture_string->make=make;fixture_string->bytes=bytes;
    rendered=text("rendered");second=text("second");env_name=text("BIRNAM_TEXT_VIEW_NESTED");assert(!setenv("BIRNAM_TEXT_VIEW_NESTED","nested",1));
    uint8_t data[5]={0};const uint8_t alphabet[]={'a',0,0xff,0xc3};size_t combinations=1;
    for(size_t length=0;length<=4;length++,combinations*=4) for(size_t code=0;code<combinations;code++) {
        size_t value=code;for(size_t i=0;i<length;i++) { data[i]=alphabet[value%4];value/=4; }
        input=original_make(fixture_string->context,data,length);
        expect_output(print_method,input,data,length,0,system_object);expect_output(println_method,input,data,length,1,system_object);cases+=2;
    }
    assert(cases==682);input=text("input");
    begin("null input skips all callbacks");expect_output(print_method,NULL,(const uint8_t *)"",0,0,api->nil_obj);end();assert(!byte_calls && !vt_calls && !interns && !sends);
    begin("arity and null carrier");assert(print_method(system_object,NULL,1)==api->nil_obj && println_method(system_object,(Oop *)(uintptr_t)1,0)==api->nil_obj);end();assert(!vt_calls);
    byte_mode=2;begin("null empty extent is present");expect_output(println_method,input,(const uint8_t *)"",0,1,system_object);end();assert(byte_calls==1 && !sends);
    byte_mode=3;send_mode=1;begin("failed exact String bytes retries through printing");expect_output(print_method,input,(const uint8_t *)"input",5,0,system_object);end();assert(byte_calls==2 && sends==1 && interns==1);
    byte_mode=1;send_mode=2;begin("failed String bytes falls back to another String");expect_output(print_method,input,(const uint8_t *)"rendered",8,0,system_object);end();
    send_mode=3;begin("null fallback result");expect_output(print_method,input,(const uint8_t *)"",0,0,api->nil_obj);end();assert(byte_calls==1 && vt_calls==1);
    send_mode=4;begin("non String fallback result");expect_output(print_method,input,(const uint8_t *)"",0,0,api->nil_obj);end();assert(byte_calls==1 && vt_calls==2);byte_mode=send_mode=0;
    input=api->alloc(api->object_vt,0);spoof=1;byte_mode=4;begin("selected recognition and byte callback are authoritative");expect_output(print_method,input,replacement_bytes,sizeof(replacement_bytes),0,system_object);end();spoof=byte_mode=0;
    input=text("input");swap_bytes=1;begin("byte operation selected after vtable callback");expect_output(print_method,input,replacement_bytes,sizeof(replacement_bytes),0,system_object);end();fixture_string->bytes=bytes;
    Oop type=api->proto(api->object_vt);input=api->alloc(type,0);api->method(type,api->intern("printString"),first_answer);
    begin("fresh replacement");expect_output(print_method,input,(const uint8_t *)"rendered",8,0,system_object);api->method(type,api->intern("printString"),second_answer);expect_output(print_method,input,(const uint8_t *)"second",6,0,system_object);end();
    Oop meta=api->proto(api->vtable_vt);api->method(meta,api->intern("lookup"),dynamic_lookup);Oop reassociate[]={type,meta};api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),reassociate,2);
    begin("custom lookup remains observable");expect_output(print_method,input,(const uint8_t *)"rendered",8,0,system_object);expect_output(print_method,input,(const uint8_t *)"second",6,0,system_object);end();assert(lookups==2);
    input=text("input");nested=collect=1;begin("nested exact reads and callback GC");expect_output(print_method,input,(const uint8_t *)"input",5,0,system_object);end();collect=0;
    if(fixture_text_view) {
        Dl_info owner;assert(dladdr((const void *)fixture_text_view->exact,&owner) && strstr(owner.dli_fname,"/libbirnam_text_view.dylib"));
        size_t n=0;const uint8_t *original=original_bytes(fixture_string->context,input,&n);TextBytesV1 view=fixture_text_view->exact(&selected,input);assert(view.data==original && view.length==n);
        byte_mode=2;view=fixture_text_view->exact(&selected,input);assert(view.data && !view.length);byte_mode=0;
        view=fixture_text_view->printable(&selected,NULL);assert(!view.data && !view.length);
        IdApi current=selected;current.vtable_of=api->vtable_of;input=api->alloc(api->object_vt,0);spoof=1;byte_mode=4;
        view=fixture_text_view->exact(&current,input);assert(!view.data);view=fixture_text_view->exact(&selected,input);assert(view.data==replacement_bytes);spoof=byte_mode=0;
    }
    /* Every actual Prelude consumer uses the same dependency, preserving its
       own OS/error policy and its original borrowed-byte read points. */
    char directory[]="/tmp/birnam-text-view-XXXXXX";assert(mkdtemp(directory));snprintf(first_path,sizeof(first_path),"%s/a",directory);snprintf(second_path,sizeof(second_path),"%s/b",directory);
    path_object=text(first_path);contents=original_make(fixture_string->context,replacement_bytes,sizeof(replacement_bytes));Oop pair[]={path_object,contents};
    begin("binary file round trip");assert(write_file(system_object,pair,2)==system_object);Oop loaded=read_file(system_object,&path_object,1);expect_string(loaded,replacement_bytes,sizeof(replacement_bytes));end();
    assert(!unlink(first_path));strcpy(path_buffer,first_path);path_view=path_mutation=1;
    begin("path remains borrowed across contents callback");assert(write_file(system_object,pair,2)==system_object);end();assert(access(first_path,F_OK)!=0 && access(second_path,F_OK)==0);assert(!unlink(second_path));path_view=0;
    Oop inexact=api->alloc(api->proto(fixture_string->behavior),16);begin("inherited String arguments are rejected without rendering");assert(read_file(system_object,&inexact,1)==api->nil_obj && get_env(system_object,&inexact,1)==api->nil_obj);end();assert(!byte_calls && !sends);
    begin("environment exact bytes");expect_string(get_env(system_object,&env_name,1),(const uint8_t *)"nested",6);end();
    FILE *source=fopen(first_path,"wb");assert(source && fwrite("source",1,6,source)==6 && !fclose(source));
    Oop codegen_type=api->proto(api->object_vt),codegen=api->alloc(codegen_type,0);api->method(codegen_type,api->intern("sourceRevisionOf:"),revision);api->method(codegen_type,api->intern("evalPersisting:from:"),evaluate);api->method(codegen_type,api->intern("lastError"),last_error_method);api->export_obj("Codegen",codegen);
    Oop error_type=api->proto(api->object_vt);error_object=api->alloc(error_type,0);api->method(error_type,api->intern("printString"),first_answer);api->export_obj("ErrorVt",error_type);last_error=text("syntax");
    path_view=1;uint8_t diagnostic[4096];size_t diagnostic_length=0;char expected[1200];
    for(eval_mode=0;eval_mode<=1;eval_mode++) {
        strcpy(path_buffer,first_path);begin(eval_mode?"lastError exact diagnostic after evaluation":"printable diagnostic after evaluation");
        Oop r=capture_call(STDERR_FILENO,load_source,system_object,&path_object,1,diagnostic,&diagnostic_length);end();assert(r==(eval_mode?api->nil_obj:error_object));
        int n=snprintf(expected,sizeof(expected),eval_mode?"birnam: load %s failed:\nsyntax\n":"birnam: load %s: rendered\n",second_path);assert(n>0 && diagnostic_length==(size_t)n && !memcmp(diagnostic,expected,diagnostic_length));
    }
    path_view=0;selected.dlopen_fn=open_module;selected.dlsym_fn=find_module;
    for(module_mode=0;module_mode<3;module_mode++) { begin("module path conversion and loader result");assert(load_module(system_object,&path_object,1)==(module_mode?api->nil_obj:system_object));end(); }assert(init_calls==1);
    selected.dlopen_fn=api->dlopen_fn;selected.dlsym_fn=api->dlsym_fn;
    IdApi other=selected;other.nil_obj=api->true_obj;prelude_init(&other,NULL);input=text("reload");
    begin("reinstalled consumer keeps original execution callbacks");expect_output(print_method,input,(const uint8_t *)"reload",6,0,system_object);end();
    api->gc_set_precise(1);api->gc_collect();api->gc_set_precise(0);expect_string(input,(const uint8_t *)"reload",6);
    fixture_string->bytes=original_bytes;fixture_string->make=original_make;assert(!unlink(first_path) && !rmdir(directory));
    fprintf(trace,"text-view: %zu binary output cases, borrowed extents, live callbacks, fallback, lookup, GC and all Prelude consumers passed\n",cases);fclose(trace);
}
