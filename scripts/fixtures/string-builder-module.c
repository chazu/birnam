#include "idapi_v7.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "collect-support.h"
#include "../../abi/fixtures/trace-live-v1/trace_live_v1.h"
#include <stdlib.h>

/* Independent byte/growth oracle. No Rust implementation constants imported. */
typedef struct { uint8_t *bytes; size_t length, capacity; } Builder;
_Static_assert(sizeof(Builder)==24, "existing builder payload extent");
static IdApi *api, builder_api;
static RootProvider roots;
static Oop behavior, prototype, print_selector, victim, reentrant_builder;
static Finalizer cleanup;
static unsigned registrations, allocations, finalizers, marked, collections, publications, finalized;
static unsigned string_calls, byte_calls, integer_calls, conversion_calls;
static int baseline, initializing, force_preparation_gc;
static Oop (*original_make)(void *,const uint8_t *,size_t);
static const uint8_t *(*original_bytes)(void *,Oop,size_t *);
static Oop (*original_integer)(void *,int64_t);
static TraceLiveV1 actual_trace;
static TraceLiveSourceV1 original_source;
static int admit(void *context, uintptr_t address) {
    if(victim && address==(uintptr_t)victim) return 0;
    return original_source.admit(context,address);
}
static int traced(const TraceLiveSourceV1 *source) {
    original_source=*source; TraceLiveSourceV1 watched=*source; watched.admit=admit;
    return actual_trace(&watched);
}
static void mount_trace(const char *floor_path, void *floor) {
    const char *slash=strrchr(floor_path,'/'); assert(slash); char path[PATH_MAX];
    snprintf(path,sizeof(path),"%.*s/libbirnam_trace.dylib",(int)(slash-floor_path),floor_path);
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    ((ModuleInitFn)api->dlsym_fn(image,"module_init"))(api,NULL);
    const TraceLiveApiV1 *provided=((TraceLiveProviderV1)api->dlsym_fn(image,"birnam_trace_provider_v1"))();
    actual_trace=provided->trace;
    TraceLiveApiV1 watched={1,sizeof(watched),traced};
    assert(((TraceLiveDependencyV1)api->dlsym_fn(floor,"birnam_trace_dependency_v1"))(&watched)==1);
    fixture_mount_collect(api,floor_path);
}
static void mount(const char *path, IdApi *selected) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    fixture_attach_integer(api,image);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init); init(selected,NULL);
}
static Oop send0(Oop receiver,const char *name) { return api->send(receiver,api->intern(name),NULL,0); }
static Oop send(Oop receiver,const char *name,Oop argument) { return api->send(receiver,api->intern(name),&argument,1); }
static Method lookup(const char *name) { return (Method)send(behavior,"lookup",api->intern(name)); }
static Oop text(const char *s) { return fixture_string_make((const uint8_t *)s,strlen(s)); }
static void expect_bytes(Oop value,const uint8_t *bytes,size_t length) {
    assert(api->vtable_of(value)==fixture_string->behavior);
    size_t count=0; const uint8_t *actual=fixture_string_bytes(value,&count);
    assert(count==length && (!length || !memcmp(actual,bytes,length)));
}
static void expect_size(Oop value,size_t length) {
    Oop result=send0(value,"size"); assert(fixture_is_integer(result) && fixture_integer_value(result)==(int64_t)length);
}
static void register_roots(RootProvider provider) {
    registrations++; roots=provider; api->gc_register_root_provider(provider);
}
static void record_mark(Oop object) { if(object==behavior || object==prototype || object==print_selector) marked++; }
static Oop allocate(Oop type,size_t length) {
    allocations++;
    if(initializing && force_preparation_gc) { collections++; api->gc_collect(); }
    return api->alloc(type,length);
}
static void finalizer(Oop object,Finalizer method) {
    finalizers++;
    assert(api->object_payload_size(object)==sizeof(Builder));
    Builder *b=object; assert(!b->bytes && !b->length && !b->capacity);
    cleanup=method;
    if(initializing && !prototype) prototype=object;
    api->set_finalizer(object,method);
}
static Oop intern(const char *name) {
    if(initializing && force_preparation_gc) { collections++; api->gc_collect(); }
    return api->intern(name);
}
static void layout(Oop type,const uint8_t *kinds,size_t count,uint8_t rest) {
    if(count==3 && kinds[0]==0 && kinds[1]==0 && kinds[2]==0 && rest==0) behavior=type;
    api->set_layout(type,kinds,count,rest);
}
static Oop publish(const char *name,Oop value) {
    if(!strcmp(name,"StringBuilder")) {
        publications++; assert(value==prototype);
        Oop copy=send0(value,"new"); assert(copy!=value && api->vtable_of(copy)==behavior);
        api->gc_collect(); expect_size(copy,0);
    }
    return api->export_obj(name,value);
}
static Oop counted_string(void *context,const uint8_t *bytes,size_t length) {
    string_calls++; api->gc_collect(); return original_make(context,bytes,length);
}
static const uint8_t *counted_bytes(void *context,Oop object,size_t *length) {
    byte_calls++; return original_bytes(context,object,length);
}
static Oop counted_integer(void *context,int64_t value) { integer_calls++; return original_integer(context,value); }
static Oop print_first(Oop object,const Oop *args,size_t count) {
    (void)object; (void)args; assert(!count); conversion_calls++; return text("one");
}
static Oop print_reentrant(Oop object,const Oop *args,size_t count) {
    (void)object; (void)args; assert(!count); conversion_calls++;
    send(reentrant_builder,"addByte:",fixture_integer_make('!'));
    api->gc_collect(); return text("two");
}
static Oop print_invalid(Oop object,const Oop *args,size_t count) {
    (void)object; (void)args; assert(!count); return api->true_obj;
}
static Oop identity(Oop self,const Oop *args,size_t count) { (void)args; (void)count; return self; }
static void observed_cleanup(Oop object) {
    assert(object==victim && ((Builder *)object)->length==4 && ((Builder *)object)->capacity==64);
    cleanup(object);
    assert(!((Builder *)object)->bytes && !((Builder *)object)->length && !((Builder *)object)->capacity);
    finalized++;
}
static void seed_collections(void) {
    const char *names[]={"CollectionVt","VectorNode","VectorVt","RangeVt","MapVt","MapNode"};
    Oop types[6];
    for(size_t i=0;i<6;i++) { types[i]=api->proto(api->object_vt); api->export_obj(names[i],types[i]); }
    api->export_obj("Collection",types[0]);
    Oop vector=api->alloc(types[2],40), range=api->alloc(types[3],16), map=api->alloc(types[4],16);
    memset(vector,0,40); memset(range,0,16); memset(map,0,16);
    api->export_obj("Vector",vector); api->export_obj("Range",range); api->export_obj("Map",map);
}
int main(int argc,char **argv) {
    assert(argc==7 || argc==8); baseline=argc==8;
    int adopted=strstr(argv[6],"adopted")!=NULL, present=strstr(argv[6],"present")!=NULL;
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); api->gc_set_auto(0);
    mount_trace(argv[1],floor); fixture_mount_integer(api,argv[1]); mount(argv[2],api); mount(argv[3],api);
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    if(present) seed_collections();
    Oop adopted_type=NULL, adopted_prototype=NULL;
    if(adopted) {
        adopted_type=api->proto(api->object_vt); adopted_prototype=api->alloc(adopted_type,sizeof(Builder));
        memset(adopted_prototype,0,sizeof(Builder)); prototype=adopted_prototype;
        api->export_obj("StringBuilderVt",adopted_type); api->export_obj("StringBuilder",adopted_prototype);
    }
    builder_api=*api; builder_api.alloc=allocate; builder_api.set_finalizer=finalizer;
    builder_api.intern=intern; builder_api.set_layout=layout; builder_api.export_obj=publish;
    builder_api.gc_register_root_provider=register_roots;
    initializing=1; force_preparation_gc=!baseline; mount(argv[4],&builder_api); initializing=0;
    assert(registrations==1 && roots && behavior && prototype && publications==(unsigned)!present);
    if(!baseline) assert(collections>0);
    if(adopted) assert(behavior==adopted_type && prototype==adopted_prototype);
    else assert(api->object_payload_size(prototype)==24);
    if(present && !adopted) assert(!api->is_exported("StringBuilder") && !api->is_exported("StringBuilderVt"));
    else assert(api->import_fn("StringBuilder")==prototype && api->import_fn("StringBuilderVt")==behavior);
    assert(vtable_parent(api,behavior)==api->object_vt);
    if(!present) assert(vtable_owner(api,behavior)==prototype);
    for(size_t i=0;i<4;i++) assert(api->slot_kind(behavior,i)==IDAPI_V7_SLOT_RAW);
    print_selector=api->intern("printString"); roots(record_mark); assert(marked==3);
    Method create=lookup("new"), clone=lookup("clone"), append=lookup("add:"), add_byte=lookup("addByte:");
    assert(create==clone && create && append && add_byte && lookup("asString")==lookup("printString"));
    allocations=finalizers=0;
    Oop builder=send0(prototype,"new"); api->export_obj("BuilderTestRoot",builder);
    assert(allocations==1 && finalizers==1 && builder!=prototype && api->vtable_of(builder)==behavior);
    assert(create(api->true_obj,NULL,1)==api->nil_obj && clone(builder,NULL,2)==api->nil_obj);
    Oop unrelated=create(api->true_obj,NULL,0); assert(api->vtable_of(unrelated)==behavior);
    uint8_t expected[4096]; size_t length=0, checked=0;
    static const size_t spans[]={0,1,62,1,1,64,128,257,511,1024};
    for(size_t pass=0;pass<sizeof(spans)/sizeof(*spans);pass++) {
        uint8_t bytes[1024]; size_t n=spans[pass];
        for(size_t i=0;i<n;i++) bytes[i]=(uint8_t)(i*37+pass*71);
        Oop value=fixture_string_make(bytes,n); assert(send(builder,"add:",value)==builder);
        memcpy(expected+length,bytes,n); length+=n;
        Builder *raw=builder; size_t capacity=length?64:0; while(capacity<length) capacity*=2;
        assert(raw->length==length && raw->capacity==capacity && (!length || !memcmp(raw->bytes,expected,length)));
        Oop first=send0(builder,"asString"), second=send0(builder,"printString");
        assert(first!=second); expect_bytes(first,expected,length); expect_bytes(second,expected,length); expect_size(builder,length);
        checked++;
    }
    const int64_t values[]={INT64_MIN,-257,-256,-1,0,1,127,255,256,257,INT64_MAX};
    for(size_t i=0;i<sizeof(values)/sizeof(*values);i++) {
        assert(send(builder,"addByte:",fixture_integer_make(values[i]))==builder);
        expected[length++]=(uint8_t)values[i]; checked++;
    }
    expect_bytes(send0(builder,"asString"),expected,length);
    assert(append(builder,NULL,1)==builder && add_byte(builder,NULL,1)==builder);
    assert(append(builder,NULL,2)==api->nil_obj && add_byte(builder,NULL,0)==api->nil_obj);
    assert(send(builder,"addByte:",api->true_obj)==builder); expect_size(builder,length);
    assert(lookup("size")(builder,NULL,1)==api->nil_obj && lookup("asString")(builder,NULL,1)==api->nil_obj);
    Oop empty=send0(builder,"clone"); assert(empty!=builder); expect_size(empty,0); expect_bytes(send0(empty,"asString"),NULL,0);
    Oop inherited=send0(prototype,"new"), child=api->proto(behavior), reassign[]={inherited,child};
    api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),reassign,2);
    send(inherited,"addByte:",fixture_integer_make('i')); expect_bytes(send0(inherited,"asString"),(const uint8_t *)"i",1);
    assert(api->vtable_of(send0(inherited,"clone"))==behavior);
    Oop printer_type=api->proto(api->object_vt), printer=api->alloc(printer_type,0);
    api->export_obj("BuilderTestPrinter",printer);
    reentrant_builder=send0(prototype,"new"); api->export_obj("ReentrantBuilder",reentrant_builder);
    api->method(printer_type,print_selector,print_first); send(reentrant_builder,"add:",printer);
    api->method(printer_type,print_selector,print_reentrant); send(reentrant_builder,"add:",printer);
    assert(conversion_calls==2); expect_bytes(send0(reentrant_builder,"asString"),(const uint8_t *)"one!two",7);
    api->method(printer_type,print_selector,print_invalid); send(reentrant_builder,"add:",printer); expect_size(reentrant_builder,7);
    Oop inherited_string=text("ignored"), string_child=api->proto(fixture_string->behavior), string_reassign[]={inherited_string,string_child};
    api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),string_reassign,2);
    send(reentrant_builder,"add:",inherited_string); expect_size(reentrant_builder,7);
    original_bytes=fixture_string->bytes; fixture_string->bytes=counted_bytes; send(reentrant_builder,"add:",text("!"));
    assert(byte_calls==1); fixture_string->bytes=original_bytes;
    original_make=fixture_string->make; fixture_string->make=counted_string;
    Oop copied=send0(reentrant_builder,"asString"); assert(string_calls==1); fixture_string->make=original_make;
    expect_bytes(copied,(const uint8_t *)"one!two!",8);
    original_integer=fixture_integer->make; fixture_integer->make=counted_integer; expect_size(reentrant_builder,8);
    assert(integer_calls==1); fixture_integer->make=original_integer;
    send(reentrant_builder,"addByte:",fixture_integer_make('?')); expect_bytes(copied,(const uint8_t *)"one!two!",8);
    victim=send0(prototype,"new"); send(victim,"add:",text("dead")); api->set_finalizer(victim,observed_cleanup);
    api->gc_set_precise(1); api->gc_collect(); assert(finalized==1); victim=NULL; api->gc_collect(); assert(finalized==1);
    /* Match the ordinary next rung and preserve a replacement through it. */
    Method size=lookup("size"); api->method(behavior,api->intern("size"),identity);
    if(!baseline) { mount(argv[5],api); assert(lookup("size")==identity); }
    else api->method(behavior,api->intern("size"),size);
    /* Vector present: own reinitialization reinstalls methods but not exports. */
    api->export_obj("Vector",api->true_obj); api->export_obj("StringBuilder",api->nil_obj); api->export_obj("StringBuilderVt",api->nil_obj);
    unsigned before=publications; mount(argv[4],&builder_api); assert(lookup("size")==size && publications==before);
    assert(api->import_fn("StringBuilder")==api->nil_obj && api->import_fn("StringBuilderVt")==api->nil_obj);
    api->gc_collect(); expect_size(builder,length); marked=0; roots(record_mark); assert(marked==3 && registrations==1);
    /* Vector absent: cached canonical identities are republished, as before. */
    api->export_obj("Vector",NULL); mount(argv[4],&builder_api);
    assert(publications==before+1 && api->import_fn("StringBuilder")==prototype && api->import_fn("StringBuilderVt")==behavior);
    printf("string-builder: %zu byte/growth cases, empty clone/new, inherited receivers, live callbacks, conversion reentry, cleanup, roots and Vector-guarded reinitialization passed mode=%s%s\n",checked,argv[6],baseline?" baseline":"");
}
