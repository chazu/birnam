#include "idapi_v15.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
static FixtureRootSlot selected_root_slot;
static RootRegistryV1 selected_roots=FIXTURE_ROOTS(selected_root_slot);

/* Independent C representation and constructor/allocation trace oracle. */
typedef struct { Oop root; size_t count; } Payload;
_Static_assert(sizeof(Payload)==16 && offsetof(Payload,count)==8,"Map payload");
static FixturePresenceSlot selected_presence_slot;
static NamespacePresenceV1 selected_presence=FIXTURE_PRESENCE(selected_presence_slot);
static FixtureImportSlot selected_import_slot;
static NamespaceImportV1 selected_imports=FIXTURE_IMPORT(selected_import_slot);

static IdApi *api, selected;
static Oop node_type, behavior, prototype, candidate, identities[256];
static size_t identity_count, marks, partial_checks, allocation_count;
static unsigned proto_calls, finalizers;
static RootProvider owner_roots;
static Method new_method;
static int capture, phase, adopting, missing, rebound, operation_probe, modern;

static int id(Oop value) {
    if(!value) return 0;
    for(size_t i=0;i<identity_count;i++) if(identities[i]==value) return (int)i+1;
    assert(identity_count<256); identities[identity_count++]=value; return (int)identity_count;
}
static Oop send0(Oop receiver,const char *name) { return api->send(receiver,api->intern(name),NULL,0); }
static Oop send1(Oop receiver,const char *name,Oop arg) { return api->send(receiver,api->intern(name),&arg,1); }
static Method lookup(Oop type,const char *name) { return (Method)send1(type,"lookup",api->intern(name)); }
static void mark(Oop value) {
    if(value==node_type || value==behavior || value==prototype) marks++;
    else assert(fixture_map_v2 && (value==api->intern("value") || value==api->intern("value:value:") || value==api->intern("printString")));
}
static void partial(size_t expected) {
    if(!modern) return;
    assert(owner_roots); marks=0; owner_roots(mark); assert(marks==expected); partial_checks++;
}
static void roots(RootProvider provider) {
    Dl_info owner; assert(dladdr((const void *)provider,&owner));
    if(strstr(owner.dli_fname,"/libbirnam_map.dylib")) owner_roots=provider;
    fixture_register_root(provider);
}
static Oop proto(Oop parent) {
    if(proto_calls==5) partial(1);
    if(capture) printf("proto parent=%d\n",id(parent));
    Oop result=api->proto(parent); proto_calls++;
    if(proto_calls==6) node_type=result;
    if(proto_calls==5) behavior=result;
    if(capture) printf("proto result=%d\n",id(result));
    return result;
}
static Oop import(const char *name) {
    if(!strcmp(name,"Map")) partial(1);
    if(!strcmp(name,"MapNode")) partial(missing?1:2);
    if(capture) printf("import %s\n",name);
    Oop result=fixture_namespace_import(name);
    if(!strcmp(name,"MapNode")) node_type=result;
    if(!strcmp(name,"MapVt")) behavior=result;
    if(!strcmp(name,"Map")) prototype=result;
    if(capture) printf("import result=%d\n",id(result));
    return result;
}
static int exported(const char *name) { int result=fixture_namespace_contains(name); if(capture) printf("isExported %s=%d\n",name,result); return result; }
static Oop intern(const char *name) { if(phase==0 && candidate) { prototype=candidate; candidate=NULL; partial(3); } if(capture) printf("intern %s\n",name); return api->intern(name); }
static void layout(Oop type,const uint8_t *kinds,size_t count,uint8_t rest) {
    if(capture) { printf("layout %d rest=%u",id(type),rest); for(size_t i=0;i<count;i++) printf(" %u",kinds[i]); putchar('\n'); }
    if(type==node_type) assert(count==2 && kinds[0]==IDAPI_V15_SLOT_RAW && kinds[1]==IDAPI_V15_SLOT_RAW && rest==IDAPI_V15_SLOT_OOP);
    if(type==behavior) { const uint8_t expected[]={IDAPI_V15_SLOT_OOP,IDAPI_V15_SLOT_RAW}; assert(count==2 && rest==IDAPI_V15_SLOT_RAW && !memcmp(kinds,expected,2)); }
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
    if(phase==0 && type==behavior) partial(prototype?3:2);
    if(capture) printf("alloc %d %zu\n",id(type),size);
    if(operation_probe) {
        assert(type==node_type || type==behavior);
        assert(type==behavior && size==16); allocation_count++;
        Dl_info owner; assert(dladdr(__builtin_return_address(0),&owner));
        assert(strstr(owner.dli_fname,modern?"/libbirnam_map.dylib":"_collection.dylib"));
    }
    Oop result=api->alloc(type,size);
    if(type==behavior) {
        assert(size==16);
        if(phase==0 && !prototype) candidate=result;
        /* Both words must be initialized explicitly. */
        memset(result,0xA5,16);
    }
    if(capture) printf("alloc result=%d\n",id(result));
    return result;
}
static unsigned live_allocations;
static Oop second_allocator(Oop type,size_t size) {
    live_allocations++; api->gc_collect(); return api->alloc(type,size);
}
static Oop forbidden_allocator(Oop type,size_t size) { (void)type; (void)size; assert(!"reload replaced the selected table"); return NULL; }
static Oop publish(void *context,const char *name,Oop value) {
    (void)context;
    if(capture) printf("export %s=%d\n",name,id(value));
    if(!strcmp(name,"Map")) {
        prototype=value; assert(lookup(behavior,"new") && lookup(behavior,"at:put:"));
        Oop other=lookup(behavior,"new")(value,NULL,0); assert(api->vtable_of(other)==behavior && ((Payload *)other)->count==0);
        api->gc_collect(); assert(fixture_integer_value(send0(other,"size"))==0);
    }
    return fixture_namespace_export(name,value);
}
static void validate(void *image,int attached) {
    if(!modern) return;
    if(fixture_map_v2) {
    MapDependencyV5 attach=(MapDependencyV5)api->dlsym_fn(image,"birnam_map_dependency_v5"); assert(attach && !attach(NULL));
    MapApiV5 bad=*fixture_map_v2; bad.version++; assert(!attach(&bad)); bad=*fixture_map_v2; bad.byte_size--; assert(!attach(&bad));
#define MISSING(field) bad=*fixture_map_v2; bad.field=NULL; assert(!attach(&bad))
    MISSING(prepare); MISSING(finish_prototype); MISSING(set_layout); MISSING(publish); MISSING(install);
#undef MISSING
    if(attached) { bad=*fixture_map_v2; assert(!attach(&bad) && attach(fixture_map_v2)==1); }
    } else {
    MapDependencyV1 attach=(MapDependencyV1)api->dlsym_fn(image,"birnam_map_dependency_v1"); assert(attach && !attach(NULL));
    MapApiV1 bad=*fixture_map; bad.version++; assert(!attach(&bad)); bad=*fixture_map; bad.byte_size--; assert(!attach(&bad));
#define MISSING(field) bad=*fixture_map; bad.field=NULL; assert(!attach(&bad))
    MISSING(prepare); MISSING(finish_prototype); MISSING(set_layout); MISSING(publish); MISSING(empty); MISSING(put);
    MISSING(find); MISSING(count); MISSING(root); MISSING(slot_count); MISSING(slot); MISSING(behavior);
#undef MISSING
    if(attached) { bad=*fixture_map; assert(!attach(&bad) && attach(fixture_map)==1); }
    }
}
static void mount(const char *path,IdApi *table,const NamespacePresenceV1 *presence,const NamespaceImportV1 *import_cap,const NamespaceExportV1 *export_cap,const RootRegistryV1 *root_registry) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    if(strstr(path,"collection")) {
        modern=api->dlsym_fn(image,"birnam_map_dependency_v1")!=NULL || api->dlsym_fn(image,"birnam_map_dependency_v5")!=NULL;
        if(modern && !fixture_map && !fixture_map_v2) { fixture_prepare_map(api,fixture_root_registry()); validate(image,0); }
    }
    fixture_attach_integer(api,image,fixture_root_registry());
    if(strstr(path,"collection")) validate(image,1);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init); init(table,NULL,presence,import_cap,export_cap,fixture_vtable_enumeration(),fixture_method_observation(),root_registry);
}
static void seed(int map_only) {
    const char *names[]={"CollectionVt","VectorNode","VectorVt","RangeVt","MapVt","MapNode"}; Oop types[6];
    for(size_t i=0;i<6;i++) { types[i]=api->proto(api->object_vt); if(!map_only || i>=4) fixture_namespace_export(names[i],types[i]); }
    if(!map_only) {
        fixture_namespace_export("Collection",types[0]);
        FixtureVectorPayload *vector=api->alloc(types[2],40); *vector=(FixtureVectorPayload){0,5,api->alloc(types[1],256),api->alloc(types[1],256),0}; fixture_namespace_export("Vector",vector);
        Oop range=api->alloc(types[3],16); memset(range,0,16); fixture_namespace_export("Range",range);
    }
    Oop map=api->alloc(types[4],16); *(Payload *)map=(Payload){NULL,0};
    fixture_namespace_export("Map",missing?NULL:(rebound?api->nil_obj:map));
}
static Oop empty(void) { return fixture_map?fixture_map->empty():new_method(prototype,NULL,0); }
int main(int argc,char **argv) {
    assert(argc==5); adopting=strcmp(argv[4],"normal") && strcmp(argv[4],"map-only-exports");
    missing=!strcmp(argv[4],"missing-prototype"); rebound=!strcmp(argv[4],"rebound-prototype");
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor); api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1],fixture_root_registry()); fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor)); mount(argv[2],api,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());
    if(adopting || !strcmp(argv[4],"map-only-exports")) seed(!adopting);
    selected=*api; selected.proto=proto; selected.alloc=allocate; selected_import_slot=import; selected_presence_slot=exported;
    selected.intern=intern; selected.set_layout=layout; selected.method=method; selected.send=send; NamespaceExportV1 selected_exports={1,sizeof(NamespaceExportV1),NULL,publish};
    selected_root_slot=roots; selected.set_finalizer=finalize;
    capture=1; mount(argv[3],&selected,&selected_presence,&selected_imports,&selected_exports,&selected_roots); capture=0; phase=1;
    if(!prototype) prototype=candidate;
    assert(prototype && !finalizers); partial(3); if(modern) assert(partial_checks>=3); if(fixture_map) assert(fixture_map->behavior()==behavior);
    if(!rebound) assert(!((Payload *)prototype)->root && !((Payload *)prototype)->count); else assert(prototype==api->nil_obj);
    new_method=lookup(behavior,"new"); allocation_count=0; operation_probe=1; Oop map=empty(); operation_probe=0;
    assert(allocation_count==1 && api->vtable_of(map)==behavior && !((Payload *)map)->root && !((Payload *)map)->count);
    if(fixture_map) assert(!fixture_map->count(map) && !fixture_map->root(map));
    fixture_namespace_export("MapVt",api->nil_obj); fixture_namespace_export("Map",api->nil_obj); fixture_namespace_export("MapNode",api->nil_obj);
    IdApi other=selected;NamespaceExportV1 other_exports=selected_exports;FixtureImportSlot other_import_slot=selected_import_slot;NamespaceImportV1 other_imports=FIXTURE_IMPORT(other_import_slot); other.alloc=forbidden_allocator; capture=1; mount(argv[3],&other,&selected_presence,&other_imports,&other_exports,&selected_roots); capture=0;
    assert(fixture_namespace_import("Map")==api->nil_obj);
    selected.alloc=second_allocator; live_allocations=0; map=empty(); assert(live_allocations==1 && api->vtable_of(map)==behavior);
    /* Forget temporary trace identities before precise GC can recycle their addresses. */
    identity_count=0;
    selected.alloc=allocate; api->gc_set_precise(1); api->gc_collect(); api->gc_set_precise(0); partial(3);
    fixture_namespace_export("Vector",NULL); capture=1; mount(argv[3],&selected,&selected_presence,&selected_imports,&selected_exports,&selected_roots); capture=0;
    assert(fixture_namespace_import("MapVt")==behavior && fixture_namespace_import("Map")==prototype && fixture_namespace_import("MapNode")==node_type && !finalizers);
    printf("map-storage: construction, adoption, allocation, partial roots, live allocator and reload passed mode=%s\n",argv[4]);
}
