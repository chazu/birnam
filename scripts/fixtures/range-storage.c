#include "idapi_v10.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <inttypes.h>

/* Compare foreign construction traces against the previous installed build. */
typedef struct { int64_t start,end; } Payload;
static FixturePresenceSlot selected_presence_slot;
static NamespacePresenceV1 selected_presence=FIXTURE_PRESENCE(selected_presence_slot);
static FixtureImportSlot selected_import_slot;
static NamespaceImportV1 selected_imports=FIXTURE_IMPORT(selected_import_slot);

static IdApi *api, selected;
static Oop behavior, prototype, range_candidate, identities[256];
static size_t identity_count;
static RootProvider owner_roots;
static unsigned phase, proto_calls, range_allocations, finalizers, family_marks, selector_marks, initial_partial_checks;
static int modern, adopting, missing_prototype, rebound_prototype, capture, constructor_probe, expected_steps;
static Oop first_integer, last_integer;
static int64_t (*original_read)(void *,Oop);
static int (*original_exact)(void *,Oop);
static int id(Oop object) {
    if(!object) return 0;
    for(size_t i=0;i<identity_count;i++) if(identities[i]==object) return (int)i+1;
    assert(identity_count<256); identities[identity_count++]=object; return (int)identity_count;
}
static Oop send0(Oop receiver,const char *name) { return api->send(receiver,api->intern(name),NULL,0); }
static Oop send1(Oop receiver,const char *name,Oop value) { return api->send(receiver,api->intern(name),&value,1); }
static Method lookup(Oop type,const char *name) { return (Method)send1(type,"lookup",api->intern(name)); }
static void mark(Oop object) {
    if(object==behavior || object==prototype) family_marks++;
    if(object==api->intern("value:") || object==api->intern("value:value:") || object==api->intern("printString")) selector_marks++;
}
static void partial(unsigned expected) {
    if(modern) { assert(owner_roots); family_marks=selector_marks=0; owner_roots(mark); assert(family_marks==expected); initial_partial_checks++; }
}
static void roots(RootProvider provider) {
    Dl_info owner; assert(dladdr((const void *)provider,&owner));
    if(strstr(owner.dli_fname,"/libbirnam_range.dylib")) owner_roots=provider;
    api->gc_register_root_provider(provider);
}
static Oop proto(Oop parent) {
    if(proto_calls==4) partial(1);
    if(capture) printf("proto parent=%d\n",id(parent));
    Oop result=api->proto(parent); proto_calls++;
    if(proto_calls==4) behavior=result;
    if(capture) printf("proto result=%d\n",id(result));
    return result;
}
static Oop import(const char *name) {
    if(!strcmp(name,"Range")) partial(1);
    if(capture) printf("import %s\n",name);
    Oop result=fixture_namespace_import(name);
    if(!strcmp(name,"RangeVt")) behavior=result;
    if(!strcmp(name,"Range")) prototype=result;
    if(capture) printf("import result=%d\n",id(result));
    return result;
}
static int exported(const char *name) {
    int result=fixture_namespace_contains(name); if(capture) printf("isExported %s=%d\n",name,result); return result;
}
static Oop intern(const char *name) {
    if(capture) printf("intern %s\n",name); return api->intern(name);
}
static void layout(Oop type,const uint8_t *kinds,size_t count,uint8_t rest) {
    if(capture) { printf("layout %d rest=%u",id(type),rest); for(size_t i=0;i<count;i++) printf(" %u",kinds[i]); putchar('\n'); }
    api->set_layout(type,kinds,count,rest);
}
static void method(Oop type,Oop selector,Method implementation) {
    if(capture) printf("method %d %s\n",id(type),api->symbol_name(selector));
    api->method(type,selector,implementation);
}
static Oop send(Oop receiver,Oop selector,const Oop *args,size_t count) {
    if(capture) { printf("send %d %s",id(receiver),api->symbol_name(selector)); for(size_t i=0;i<count;i++) printf(" %d",id(args[i])); putchar('\n'); }
    return api->send(receiver,selector,args,count);
}
static void finalize(Oop object,Finalizer callback) { finalizers++; api->set_finalizer(object,callback); }
static Oop allocate(Oop type,size_t size) {
    if(phase==0 && type!=behavior && range_candidate) { prototype=range_candidate; partial(2); range_candidate=NULL; }
    if(capture) printf("alloc %d %zu\n",id(type),size);
    if(type==behavior) {
        assert(size==sizeof(Payload)); range_allocations++;
        if(phase==0) partial(prototype?2:1);
        if(constructor_probe) {
            assert(expected_steps==4); expected_steps++;
            api->gc_collect();
        }
    }
    Oop result=api->alloc(type,size);
    if(type==behavior && phase==0 && !prototype) range_candidate=result;
    if(type==behavior && constructor_probe) *(Payload *)result=(Payload){987,654};
    if(capture) printf("alloc result=%d\n",id(result));
    return result;
}
static Oop publish(const char *name,Oop value) {
    if(capture) printf("export %s=%d\n",name,id(value));
    if(!strcmp(name,"Range")) {
        prototype=value; assert((value==api->nil_obj || api->vtable_of(value)==behavior) && lookup(behavior,"size"));
        Method to=lookup(fixture_integer->behavior,"to:"); assert(to);
        Oop end=fixture_integer_make(9),r=to(fixture_integer_make(7),&end,1);
        assert(api->vtable_of(r)==behavior && ((Payload *)r)->start==7 && ((Payload *)r)->end==9);
        api->gc_collect(); assert(fixture_integer_value(send0(r,"size"))==3);
    }
    return api->export_obj(name,value);
}
static int exact(void *context,Oop object) {
    if(constructor_probe) {
        assert(expected_steps<2 && object==(expected_steps==0?first_integer:last_integer)); expected_steps++;
    }
    return original_exact(context,object);
}
static int64_t read_integer(void *context,Oop object) {
    if(constructor_probe) {
        assert(expected_steps==2 || expected_steps==3);
        assert(object==(expected_steps==2?first_integer:last_integer)); expected_steps++;
        return object==first_integer?-17:23;
    }
    return original_read(context,object);
}
static void mount(const char *path,IdApi *table,const NamespacePresenceV1 *presence,const NamespaceImportV1 *import_cap) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init); init(table,NULL,presence,import_cap);
}
static void seed(void) {
    const char *names[]={"CollectionVt","VectorNode","VectorVt","RangeVt","MapVt","MapNode"}; Oop types[6];
    for(size_t i=0;i<6;i++) { types[i]=api->proto(api->object_vt); api->export_obj(names[i],types[i]); }
    api->export_obj("Collection",types[0]);
    Oop vector=api->alloc(types[2],40),range=api->alloc(types[3],16),map=api->alloc(types[4],16);
    memset(vector,0,40); *(Payload *)range=(Payload){8,-3}; memset(map,0,16);
    behavior=types[3]; prototype=missing_prototype?NULL:(rebound_prototype?api->nil_obj:range);
    api->export_obj("Vector",vector); api->export_obj("Range",prototype); api->export_obj("Map",map);
}
int main(int argc,char **argv) {
    assert(argc==5 || (argc==6 && !strcmp(argv[5],"baseline"))); modern=argc==5;
    int ignored_exports=!strcmp(argv[4],"range-only-exports");
    adopting=strcmp(argv[4],"normal")!=0 && !ignored_exports;
    missing_prototype=!strcmp(argv[4],"missing-prototype"); rebound_prototype=!strcmp(argv[4],"rebound-prototype");
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]); fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports());
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor)); mount(argv[2],api,fixture_namespace_presence(),fixture_namespace_imports());
    if(adopting || ignored_exports) seed();
    if(ignored_exports) { api->export_obj("Vector",NULL); behavior=prototype=NULL; }
    Oop adopted_behavior=behavior,adopted_prototype=prototype;
    selected=*api; selected.proto=proto; selected_import_slot=import; selected_presence_slot=exported;
    selected.intern=intern; selected.alloc=allocate; selected.method=method; selected.send=send;
    selected.export_obj=publish; selected.set_layout=layout; selected.set_finalizer=finalize; selected.gc_register_root_provider=roots;
    capture=1; mount(argv[3],&selected,&selected_presence,&selected_imports); capture=0; phase=1;
    if(range_candidate) { prototype=range_candidate; range_candidate=NULL; }
    if(adopting) assert(behavior==adopted_behavior && (!adopted_prototype || prototype==adopted_prototype));
    else assert(fixture_namespace_import("RangeVt")==behavior && fixture_namespace_import("Range")==prototype && vtable_parent(api,behavior)==fixture_namespace_import("CollectionVt"));
    assert(prototype && api->object_payload_size(prototype)==(size_t)(rebound_prototype?0:16));
    if(!adopting || missing_prototype) assert(((Payload *)prototype)->start==0 && ((Payload *)prototype)->end==0);
    else if(rebound_prototype) assert(prototype==api->nil_obj);
    else assert(((Payload *)prototype)->start==8 && ((Payload *)prototype)->end==-3 && vtable_parent(api,behavior)==api->object_vt);
    if(modern) { family_marks=selector_marks=0; owner_roots(mark); assert(family_marks==2 && selector_marks==3 && initial_partial_checks>0); }
    assert(!finalizers);
    Method to=lookup(fixture_integer->behavior,"to:"); Dl_info owner; assert(dladdr((const void *)to,&owner));
    assert(strstr(owner.dli_fname,modern?"/libbirnam_range.dylib":"_collection.dylib"));
    Oop end=fixture_integer_make(1); unsigned before=range_allocations;
    assert(to(api->true_obj,&end,1)==api->nil_obj && to(end,NULL,1)==api->nil_obj);
    assert(to(end,NULL,0)==api->nil_obj && to(end,&end,2)==api->nil_obj);
    assert(to(end,&api->true_obj,1)==api->nil_obj && range_allocations==before);
    first_integer=fixture_integer_make(1); last_integer=fixture_integer_make(2);
    api->export_obj("RangeFirstInteger",first_integer); api->export_obj("RangeLastInteger",last_integer);
    original_exact=fixture_integer->is_exact; original_read=fixture_integer->read;
    fixture_integer->is_exact=exact; fixture_integer->read=read_integer;
    constructor_probe=1; expected_steps=0; Oop created=to(first_integer,&last_integer,1); constructor_probe=0;
    fixture_integer->is_exact=original_exact; fixture_integer->read=original_read;
    assert(expected_steps==5 && range_allocations==before+1 && !finalizers);
    assert(api->vtable_of(created)==behavior && ((Payload *)created)->start==-17 && ((Payload *)created)->end==23);
    api->export_obj("RangeStorageRoot",created);
    Oop clone=send0(created,"clone"); assert(clone!=created && api->vtable_of(clone)==behavior);
    assert(((Payload *)clone)->start==-17 && ((Payload *)clone)->end==23);
    if(fixture_range) { RangeBoundsV1 bounds=fixture_range->bounds(created); assert(bounds.start==-17 && bounds.end==23); }
    api->export_obj("RangeVt",api->nil_obj); api->export_obj("Range",api->nil_obj);
    /* Selection comes from the first table, including after a different reload table. */
    IdApi other=selected;FixtureImportSlot other_import_slot=selected_import_slot;NamespaceImportV1 other_imports=FIXTURE_IMPORT(other_import_slot); other.nil_obj=created;
    capture=1; mount(argv[3],&other,&selected_presence,&other_imports); capture=0;
    assert(to(end,NULL,0)==api->nil_obj && lookup(fixture_integer->behavior,"to:")==to);
    assert(fixture_namespace_import("Range")==api->nil_obj && fixture_namespace_import("RangeVt")==api->nil_obj);
    api->gc_set_precise(1); api->gc_collect();
    Oop rebuilt=to(first_integer,&last_integer,1); assert(api->vtable_of(rebuilt)==behavior);
    if(modern) { family_marks=selector_marks=0; owner_roots(mark); assert(family_marks==2 && selector_marks==3); }
    api->export_obj("Vector",NULL); capture=1; mount(argv[3],&selected,&selected_presence,&selected_imports); capture=0;
    assert(fixture_namespace_import("RangeVt")==behavior && fixture_namespace_import("Range")==prototype && !finalizers);
    printf("range-storage: construction, adoption, callbacks, roots, clone and reload passed mode=%s\n",argv[4]);
}
