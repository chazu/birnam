#include "idapi_v12.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"

static IdApi *api,selected,other;
static ModuleInitFn collection_init;
static Method split;
static Oop subject,separator,result,alias;
static size_t registrations,bindings,other_bindings,allocations,vt_calls,byte_calls,make_calls,cases;
static int capture,first_reentry,reentered,byte_mode,make_mode,nested,collect;
static uint8_t input_buffer[4]={'a',',','b',0},separator_buffer[1]={','};
static Oop (*original_make)(void *,const uint8_t *,size_t);
static const uint8_t *(*original_bytes)(void *,Oop,size_t *);
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(Oop type,const char *name) { return (Method)send1(type,"lookup",api->intern(name)); }
static void roots(MarkFn mark) { mark(subject); mark(separator); mark(result); mark(alias); }
static void registered(RootProvider provider) { registrations++; api->gc_register_root_provider(provider); }
static Oop allocate(Oop type,size_t size) {
    if(capture) { allocations++; printf("alloc %zu\n",size); }
    if(collect) api->gc_collect();
    return api->alloc(type,size);
}
static Oop vtable(Oop value) {
    vt_calls++; if(capture) printf("vtable %s\n",value==separator?"separator":(value==subject?"subject":"other"));
    return api->vtable_of(value);
}
static Oop forbidden_vtable(Oop object) { (void)object; assert(!"registration table became execution table"); return NULL; }
static void method(Oop type,Oop selector,Method implementation) {
    if(type==fixture_string->behavior && !strcmp(api->symbol_name(selector),"split:")) {
        bindings++; assert(implementation(NULL,NULL,0)==api->nil_obj);
    }
    api->method(type,selector,implementation);
}
static void other_method(Oop type,Oop selector,Method implementation) {
    if(type==fixture_string->behavior && !strcmp(api->symbol_name(selector),"split:")) other_bindings++;
    method(type,selector,implementation);
}
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) {
    if(first_reentry && self==api->vtable_vt && !strcmp(api->symbol_name(selector),"parentOf:put:") && count==2 && args[0]==fixture_string->behavior) {
        first_reentry=0; reentered=1; collection_init(&other,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration());
        Method first=lookup(fixture_string->behavior,"split:"); assert(first && first(NULL,NULL,0)==api->nil_obj);
    }
    return api->send(self,selector,args,count);
}
static Oop make(void *context,const uint8_t *bytes,size_t length) {
    make_calls++; if(capture) { printf("make"); for(size_t i=0;i<length;i++) printf(" %02x",bytes[i]); putchar('\n'); }
    if(make_mode==1) return NULL;
    if(make_mode==2) return alias;
    if(make_mode==3) { memset(input_buffer,'x',3); separator_buffer[0]='x'; }
    if(collect) api->gc_collect();
    return original_make(context,bytes,length);
}
static const uint8_t *bytes(void *context,Oop object,size_t *length) {
    byte_calls++; if(capture) printf("bytes %s\n",object==subject?"subject":"separator");
    if(nested && object==subject) {
        nested=0; Oop nested_result=split(subject,(Oop[]){api->nil_obj},1);
        assert(fixture_vector->count(nested_result)==1 && fixture_vector->at(nested_result,0)==subject);
    }
    if(collect) api->gc_collect();
    if(byte_mode==1 && object==subject) { *length=1; return NULL; }
    if(byte_mode==2 && object==separator) { *length=1; return NULL; }
    if(byte_mode==3 && object==subject) { *length=0; return NULL; }
    if(byte_mode==4 && object==separator) { *length=0; return NULL; }
    if(byte_mode==5 || byte_mode==6) {
        if(object==subject) { *length=3; return input_buffer; }
        if(byte_mode==5) memset(input_buffer,'x',3);
        *length=1; return separator_buffer;
    }
    return original_bytes(context,object,length);
}
static void validate(void *image,int attached) {
    StringCollectionDependencyV1 attach=(StringCollectionDependencyV1)api->dlsym_fn(image,"birnam_string_collection_dependency_v1");
    if(!attach) return;
    StringCollectionApiV1 bad=*fixture_string_collection;
    assert(!attach(NULL)); bad.version++; assert(!attach(&bad)); bad=*fixture_string_collection;
    bad.byte_size--; assert(!attach(&bad)); bad=*fixture_string_collection; bad.install=NULL; assert(!attach(&bad));
    if(attached) { bad=*fixture_string_collection; assert(!attach(&bad) && attach(fixture_string_collection)==1); }
}
static void mount(const char *path,IdApi *table) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    if(strstr(path,"collection") && api->dlsym_fn(image,"birnam_string_collection_dependency_v1") && !fixture_string_collection) {
        IdApi watched=*api; watched.gc_register_root_provider=registered; watched.alloc=allocate; watched.method=method;
        size_t before=registrations; fixture_prepare_string_collection(&watched);
        assert(registrations==before && !lookup(fixture_string->behavior,"split:")); validate(image,0);
    }
    fixture_attach_integer(api,image);
    if(strstr(path,"collection")) { validate(image,1); collection_init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(collection_init); }
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init); init(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration());
}
static void expect_part(Oop item,const uint8_t *expected,size_t length) {
    assert(item && api->vtable_of(item)==fixture_string->behavior);
    size_t n=0; const uint8_t *actual=original_bytes(fixture_string->context,item,&n);
    assert(actual && n==length && (!length || !memcmp(actual,expected,length)));
}
static void expect(Oop vector,const uint8_t *input,size_t n,const uint8_t *delimiter,size_t m) {
    /* Independent cut positions: keep leftmost matches that do not overlap. */
    size_t cuts[16],count=0;
    if(m && m<=n) for(size_t i=0;i<=n-m;i++) {
        if((!count || i>=cuts[count-1]+m) && !memcmp(input+i,delimiter,m)) { assert(count<16); cuts[count++]=i; }
    }
    assert(fixture_vector->count(vector)==count+1);
    for(size_t i=0;i<=count;i++) {
        size_t start=i?cuts[i-1]+m:0,end=i<count?cuts[i]:n;
        Oop item=fixture_vector->at(vector,i); assert(item!=subject); expect_part(item,input+start,end-start);
    }
}
static void run(const uint8_t *input,size_t n,const uint8_t *delimiter,size_t m) {
    subject=original_make(fixture_string->context,input,n); separator=original_make(fixture_string->context,delimiter,m);
    result=split(subject,&separator,1); expect(result,input,n,delimiter,m); cases++;
}
static void begin(const char *name) { printf("case %s\n",name); capture=1; allocations=vt_calls=byte_calls=make_calls=0; }
static void end(void) { capture=0; printf("done alloc=%zu vtable=%zu bytes=%zu make=%zu\n",allocations,vt_calls,byte_calls,make_calls); }
static Oop forbidden(Oop self,const Oop *args,size_t count) { (void)self; (void)args; (void)count; assert(!"split used public String/Vector dispatch or lost a copied Method"); return NULL; }
int main(int argc,char **argv) {
    assert(argc==5); first_reentry=!strcmp(argv[4],"first-install-reentry");
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor); api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]); fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor)); mount(argv[2],api);
    original_make=fixture_string->make; original_bytes=fixture_string->bytes;
    selected=*api; selected.alloc=allocate; selected.vtable_of=vtable; selected.send=send; selected.method=method; selected.gc_register_root_provider=registered;
    other=selected; other.nil_obj=api->true_obj; other.vtable_of=forbidden_vtable; other.method=other_method;
    mount(argv[3],&selected); assert(bindings==(reentered?2u:1u) && other_bindings==(reentered?1u:0u));
    assert(registrations==4); api->gc_register_root_provider(roots);
    split=lookup(fixture_string->behavior,"split:"); assert(split); Dl_info owner; assert(dladdr((const void *)split,&owner));
    assert(strstr(owner.dli_fname,fixture_string_collection?"/libbirnam_string_collection.dylib":"/libbirnam_id_collection.dylib") || (!fixture_string_collection && strstr(owner.dli_fname,"/libbirnam_collection.dylib")));
    fixture_string->bytes=bytes; fixture_string->make=make;
    uint8_t input[5]={0},delimiter[4]={0}; const uint8_t alphabet[]={'a',0,'b'}; size_t inputs=1;
    for(size_t n=0;n<=4;n++,inputs*=3) for(size_t code=0;code<inputs;code++) {
        size_t value=code; for(size_t i=0;i<n;i++) { input[i]=alphabet[value%3]; value/=3; }
        size_t separators=1;
        for(size_t m=0;m<=3;m++,separators*=3) for(size_t code2=0;code2<separators;code2++) {
            size_t value2=code2; for(size_t i=0;i<m;i++) { delimiter[i]=alphabet[value2%3]; value2/=3; }
            run(input,n,delimiter,m);
        }
        api->gc_collect();
    }
    assert(cases==4840);
    const uint8_t utf8[]={0xc3,0xa9,',',0xc3,0xa9},continuation[]={0xa9};
    begin("UTF8 byte separator"); run(utf8,sizeof(utf8),continuation,sizeof(continuation)); end();
    begin("leading adjacent trailing"); run((const uint8_t *)",a,,b,",6,(const uint8_t *)",",1); end();
    begin("nonoverlap"); run((const uint8_t *)"aaaaa",5,(const uint8_t *)"aa",2); end();
    alias=original_make(fixture_string->context,(const uint8_t *)"alias",5);
    subject=original_make(fixture_string->context,(const uint8_t *)"a,b",3); separator=original_make(fixture_string->context,(const uint8_t *)",",1);
    begin("arity and carrier"); assert(split(subject,NULL,1)==api->nil_obj && split(subject,&separator,0)==api->nil_obj && split(subject,&separator,2)==api->nil_obj); end(); assert(!allocations && !vt_calls && !make_calls);
    Oop invalids[]={NULL,api->nil_obj,api->true_obj,fixture_integer_make(7),api->alloc(api->proto(fixture_string->behavior),16)};
    for(size_t i=0;i<5;i++) { begin("invalid separator preserves receiver"); result=split(subject,&invalids[i],1); end(); assert(fixture_vector->count(result)==1 && fixture_vector->at(result,0)==subject && !byte_calls && !make_calls); }
    Oop original_subject=subject;
    for(size_t i=0;i<5;i++) { subject=invalids[i]; begin("inexact receiver with exact separator"); assert(split(subject,&separator,1)==api->nil_obj); end(); }
    subject=NULL; begin("null receiver with invalid separator"); result=split(subject,(Oop[]){api->nil_obj},1); end(); assert(fixture_vector->count(result)==1 && fixture_vector->at(result,0)==NULL); subject=original_subject;
    for(byte_mode=1;byte_mode<=4;byte_mode++) {
        begin("null or empty byte extent"); result=split(subject,&separator,1); end();
        if(byte_mode<=2) assert(result==api->nil_obj && !allocations && !make_calls);
        else if(byte_mode==3) expect(result,(const uint8_t *)"",0,(const uint8_t *)",",1);
        else expect(result,(const uint8_t *)"a,b",3,(const uint8_t *)"",0);
    }
    byte_mode=5; memcpy(input_buffer,"a,b",3); begin("receiver snapshot before separator callback"); result=split(subject,&separator,1); end(); expect(result,(const uint8_t *)"a,b",3,(const uint8_t *)",",1);
    byte_mode=6; make_mode=3; memcpy(input_buffer,"a,b",3); separator_buffer[0]=',';
    begin("both byte snapshots before make callbacks"); result=split(subject,&separator,1); end(); expect(result,(const uint8_t *)"a,b",3,(const uint8_t *)",",1); byte_mode=make_mode=0;
    make_mode=1; begin("null make values retained"); result=split(subject,&separator,1); end(); assert(fixture_vector->count(result)==2 && !fixture_vector->at(result,0) && !fixture_vector->at(result,1));
    make_mode=2; begin("live make returns alias"); result=split(subject,&separator,1); end(); assert(fixture_vector->at(result,0)==alias && fixture_vector->at(result,1)==alias); make_mode=0;
    nested=collect=1; begin("nested split and callback GC"); result=split(subject,&separator,1); end(); collect=0; expect(result,(const uint8_t *)"a,b",3,(const uint8_t *)",",1);
    const char *vector_names[]={"new","add:","at:","size"}; Method prior[4];
    for(size_t i=0;i<4;i++) { prior[i]=lookup(fixture_vector->behavior(),vector_names[i]); api->method(fixture_vector->behavior(),api->intern(vector_names[i]),forbidden); }
    api->method(fixture_string->behavior,api->intern("split:"),forbidden);
    begin("copied Method and native Vector operations"); result=split(subject,&separator,1); end(); expect(result,(const uint8_t *)"a,b",3,(const uint8_t *)",",1);
    for(size_t i=0;i<4;i++) api->method(fixture_vector->behavior(),api->intern(vector_names[i]),prior[i]);
    collection_init(&other,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration()); assert(lookup(fixture_string->behavior,"split:")==split && split(subject,NULL,1)==api->nil_obj && registrations==4);
    if(fixture_string_collection) fixture_string_collection->install(&other,&other);
    begin("reload retains original execution table"); result=split(subject,&separator,1); end(); expect(result,(const uint8_t *)"a,b",3,(const uint8_t *)",",1);
    api->gc_set_precise(1); api->gc_collect(); api->gc_set_precise(0); expect(result,(const uint8_t *)"a,b",3,(const uint8_t *)",",1);
    fixture_string->make=original_make; fixture_string->bytes=original_bytes;
    printf("string-collection: %zu scalar cases, native dispatch, byte snapshots, live callbacks, GC and table retention passed mode=%s\n",cases,argv[4]);
}
