#include "idapi_v15.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
static FixtureRootSlot watched_root_slot;
static RootRegistryV1 watched_roots=FIXTURE_ROOTS(watched_root_slot);
static FixtureRootSlot selected_root_slot;
static RootRegistryV1 selected_roots=FIXTURE_ROOTS(selected_root_slot);

static IdApi *api, selected, other;
static ModuleInitFn prelude_init;
static Method object_print, object_as, symbol_print;
static Oop subject, result, alias, custom;
static size_t names, makes, interns, sends, bindings, registrations, cases, lookups;
static int capture, first_reentry, reentered, name_mode, make_mode, send_mode, nested, collect;
static char name_buffer[32]="before";
static Oop (*original_make)(void *,const uint8_t *,size_t);
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(Oop type,const char *name) { return (Method)send1(type,"lookup",api->intern(name)); }
static void roots(MarkFn mark) { mark(subject); mark(result); mark(alias); mark(custom); }
static void registered(RootProvider provider) { registrations++; fixture_register_root(provider); }
static Oop allocate(Oop type,size_t size) { if(capture) printf("alloc %zu\n",size); return api->alloc(type,size); }
static Oop intern(const char *name) {
    interns++; if(capture) printf("intern %s\n",name);
    if(first_reentry && !strcmp(name,"printString")) {
        first_reentry=0; reentered=1; prelude_init(&other,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots);
        assert(lookup(api->object_vt,"printString")(NULL,NULL,1)==api->nil_obj);
    }
    if(collect) api->gc_collect();
    return api->intern(name);
}
static void method(Oop type,Oop selector,Method implementation) {
    const char *name=api->symbol_name(selector);
    if(capture) printf("method %s %s\n",type==api->object_vt?"Object":(type==api->symbol_vt?"Symbol":"other"),name);
    if(!strcmp(name,"printString") || !strcmp(name,"asString")) {
        bindings++; assert(implementation(NULL,NULL,1)==api->nil_obj);
    }
    api->method(type,selector,implementation);
}
static const char *symbol_name(Oop self) {
    names++; if(capture) puts("symbolName");
    if(collect) api->gc_collect();
    if(name_mode==1) return NULL;
    if(name_mode==2) return name_buffer;
    return api->symbol_name(self);
}
static const char *forbidden_name(Oop self) { (void)self; assert(!"registration table became execution table"); return NULL; }
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) {
    sends++; if(capture) printf("send %s %zu\n",api->symbol_name(selector),count);
    if(send_mode) { assert(count==0 && !args && selector==api->intern("printString")); return send_mode==1?NULL:alias; }
    if(collect) api->gc_collect();
    return api->send(self,selector,args,count);
}
static Oop make(void *context,const uint8_t *bytes,size_t length) {
    makes++; if(capture) { printf("make"); for(size_t i=0;i<length;i++) printf(" %02x",bytes[i]); putchar('\n'); }
    if(make_mode==1) return NULL;
    if(make_mode==2) return alias;
    if(make_mode==3) strcpy(name_buffer,"after");
    if(nested) { nested=0; assert(object_as(custom,NULL,0)==alias); }
    if(collect) api->gc_collect();
    return original_make(context,bytes,length);
}
static Oop answer(Oop self,const Oop *args,size_t count) { (void)self; assert(!args && count==0); if(collect) api->gc_collect(); return alias; }
static Oop answer_null(Oop self,const Oop *args,size_t count) { (void)self; assert(!args && count==0); return NULL; }
static Oop dynamic_lookup(Oop self,const Oop *args,size_t count) {
    (void)self; assert(args && count==1 && args[0]==api->intern("printString"));
    lookups++; if(collect) api->gc_collect(); return (Oop)(lookups%2?answer:answer_null);
}
static void expect(Oop value,const uint8_t *expected,size_t length) {
    size_t n=0; const uint8_t *bytes=fixture_string_bytes(value,&n);
    assert(bytes && n==length && !memcmp(bytes,expected,n));
}
static void expect_name(const char *name) {
    size_t n=strlen(name); uint8_t expected[64]; assert(n+1<=sizeof(expected));
    expected[0]='#'; memcpy(expected+1,name,n);
    result=symbol_print(subject,NULL,0); expect(result,expected,n+1); cases++;
}
static void begin(const char *name) { printf("case %s\n",name); capture=1; names=makes=interns=sends=0; }
static void end(void) { capture=0; printf("done name=%zu make=%zu intern=%zu send=%zu\n",names,makes,interns,sends); }
static void validate(void *image,int attached) {
    PrintingDependencyV1 attach=(PrintingDependencyV1)api->dlsym_fn(image,"birnam_printing_dependency_v1");
    if(!attach) return;
    PrintingApiV1 bad=*fixture_printing;
    assert(!attach(NULL)); bad.version++; assert(!attach(&bad)); bad=*fixture_printing;
    bad.byte_size--; assert(!attach(&bad)); bad=*fixture_printing; bad.install=NULL; assert(!attach(&bad));
    if(attached) { bad=*fixture_printing; assert(!attach(&bad) && attach(fixture_printing)==1); }
}
static void mount(const char *path,IdApi *table,int prelude,const RootRegistryV1 *root_registry) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    if(prelude && api->dlsym_fn(image,"birnam_printing_dependency_v1") && !fixture_printing) {
        IdApi watched=*api; watched.alloc=allocate; watched.method=method; watched_root_slot=registered;
        fixture_prepare_printing(&watched,&watched_roots); assert(!bindings && !registrations && !lookup(api->object_vt,"asString")); validate(image,0);
    }
    fixture_attach_integer(api,image,fixture_root_registry());
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    if(prelude) { validate(image,1); prelude_init=init; }
    init(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry);
}
int main(int argc,char **argv) {
    assert(argc==5); first_reentry=!strcmp(argv[4],"first-intern-reentry");
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor); api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1],fixture_root_registry()); fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor)); mount(argv[2],api,0,fixture_root_registry());
    selected=*api; selected.intern=intern; selected.method=method; selected.symbol_name=symbol_name; selected.send=send;
    selected.alloc=allocate; selected_root_slot=registered;
    other=selected; other.nil_obj=api->true_obj; other.symbol_name=forbidden_name;
    begin("initial registration"); mount(argv[3],&selected,1,&selected_roots); end(); assert(bindings==(reentered?6u:3u) && !registrations);
    object_print=lookup(api->object_vt,"printString"); object_as=lookup(api->object_vt,"asString"); symbol_print=lookup(api->symbol_vt,"printString");
    Method methods[]={object_print,object_as,symbol_print};
    for(size_t i=0;i<3;i++) { Dl_info owner; assert(methods[i] && dladdr((const void *)methods[i],&owner)); assert(strstr(owner.dli_fname,fixture_printing?"/libbirnam_printing.dylib":"_prelude.dylib")); }
    fixture_register_root(roots); original_make=fixture_string->make; fixture_string->make=make;
    alias=original_make(fixture_string->context,(const uint8_t *)"alias",5);
    subject=api->intern("alpha");
    begin("arity ignores carriers");
    for(size_t i=0;i<3;i++) for(size_t count=1;count<4;count++) assert(methods[i](NULL,(Oop *)(uintptr_t)1,count)==api->nil_obj);
    end(); assert(!names && !makes && !interns && !sends);
    begin("live nil identity"); selected.nil_obj=alias;
    for(size_t i=0;i<3;i++) assert(methods[i](NULL,NULL,1)==alias);
    selected.nil_obj=api->nil_obj; end();
    Oop receivers[]={NULL,api->nil_obj,api->true_obj,fixture_integer_make(7),subject,api->alloc(api->object_vt,0),api->alloc(api->proto(api->symbol_vt),0)};
    for(size_t i=0;i<7;i++) {
        begin("receiver independent Object printing"); result=object_print(receivers[i],(Oop *)(uintptr_t)1,0); end(); expect(result,(const uint8_t *)"anObject",8);
        Oop previous=result; result=object_print(receivers[i],NULL,0); assert(result!=previous); cases++;
        if(i!=4) { begin("non Symbol name is null"); assert(symbol_print(receivers[i],NULL,0)==api->nil_obj); end(); assert(names==1 && makes==0); }
    }
    char spelling[7]={0}; const uint8_t alphabet[]={'a','#',0xff}; size_t combinations=1;
    for(size_t n=0;n<=6;n++,combinations*=3) for(size_t code=0;code<combinations;code++) {
        size_t value=code; for(size_t i=0;i<n;i++) { spelling[i]=(char)alphabet[value%3]; value/=3; } spelling[n]=0;
        subject=api->intern(spelling); expect_name(spelling);
    }
    subject=api->intern("café"); begin("UTF8 Symbol"); expect_name("café"); end();
    name_mode=1; begin("live null name"); assert(symbol_print(subject,NULL,0)==api->nil_obj); end(); assert(names==1 && makes==0);
    name_mode=2; memcpy(name_buffer,"a\0hidden",9); begin("borrowed name ends at NUL"); expect_name("a"); end();
    strcpy(name_buffer,"before"); make_mode=3; begin("name copied before make mutates it"); expect_name("before"); end(); assert(!strcmp(name_buffer,"after")); make_mode=0;
    subject=NULL; strcpy(name_buffer,"foreign"); begin("callback defines names for arbitrary receivers"); expect_name("foreign"); end(); name_mode=0; subject=api->intern("alpha");
    for(make_mode=1;make_mode<=2;make_mode++) { begin("live make result"); assert(object_print(subject,NULL,0)==(make_mode==1?NULL:alias)); assert(symbol_print(subject,NULL,0)==(make_mode==1?NULL:alias)); end(); } make_mode=0;
    Oop type=api->proto(api->object_vt); custom=api->alloc(type,0); api->method(type,api->intern("printString"),answer);
    begin("fresh print dispatch"); assert(object_as(custom,(Oop *)(uintptr_t)1,0)==alias); api->method(type,api->intern("printString"),answer_null); assert(!object_as(custom,NULL,0)); end(); assert(interns==2 && sends==2);
    Oop private_type=api->proto(api->vtable_vt);
    Oop reassociate[]={type,private_type}; api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),reassociate,2);
    api->method(private_type,api->intern("lookup"),dynamic_lookup);
    begin("custom lookup executes every time"); assert(object_as(custom,NULL,0)==alias); assert(!object_as(custom,NULL,0)); end(); assert(lookups==2);
    for(send_mode=1;send_mode<=2;send_mode++) { begin("live send preserves null receiver and result"); assert(object_as(NULL,NULL,0)==(send_mode==1?NULL:alias)); end(); assert(interns==1 && sends==1); } send_mode=0;
    lookups=0; nested=collect=1; begin("nested printing and callback GC"); result=symbol_print(subject,NULL,0); end(); collect=0; expect(result,(const uint8_t *)"#alpha",6); assert(lookups==1);
    api->method(api->object_vt,api->intern("printString"),answer); api->method(api->object_vt,api->intern("asString"),answer); api->method(api->symbol_vt,api->intern("printString"),answer);
    begin("copied Methods survive replacement"); result=object_print(subject,NULL,0); expect(result,(const uint8_t *)"anObject",8); result=symbol_print(subject,NULL,0); expect(result,(const uint8_t *)"#alpha",6); end();
    begin("reinstallation retains execution table"); prelude_init(&other,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots); end();
    assert(lookup(api->object_vt,"printString")==object_print && lookup(api->object_vt,"asString")==object_as && lookup(api->symbol_vt,"printString")==symbol_print);
    if(fixture_printing) fixture_printing->install(&other,&other);
    begin("retained live execution callbacks"); result=symbol_print(subject,NULL,0); end(); expect(result,(const uint8_t *)"#alpha",6);
    other.object_vt=api->proto(api->object_vt); other.symbol_vt=api->proto(api->symbol_vt);
    begin("registration uses current target behaviors"); prelude_init(&other,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),&selected_roots); end();
    assert(lookup(other.object_vt,"printString")==object_print && lookup(other.object_vt,"asString")==object_as && lookup(other.symbol_vt,"printString")==symbol_print);
    assert(!registrations); api->gc_set_precise(1); api->gc_collect(); api->gc_set_precise(0); expect(result,(const uint8_t *)"#alpha",6);
    fixture_string->make=original_make;
    printf("printing: %zu byte/receiver cases, fresh dispatch, live callbacks, copied names, GC and table retention passed mode=%s\n",cases,argv[4]);
}
