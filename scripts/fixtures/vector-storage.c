#include "idapi_v18.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
static FixtureRootSlot selected_root_slot;
static RootRegistryV1 selected_roots=FIXTURE_ROOTS(selected_root_slot);

/* Independent C representation and constructor/allocation trace oracle. */
typedef struct { size_t count,shift; Oop root,tail; size_t tail_len; } Payload;
_Static_assert(sizeof(Payload)==40 && offsetof(Payload,root)==16 && offsetof(Payload,tail)==24,"Vector payload");
static FixturePresenceSlot selected_presence_slot;
static NamespacePresenceV1 selected_presence=FIXTURE_PRESENCE(selected_presence_slot);
static FixtureImportSlot selected_import_slot;
static NamespaceImportV1 selected_imports=FIXTURE_IMPORT(selected_import_slot);

static IdApi *api, selected;
static Oop node_type, behavior, prototype, candidate, identities[256];
static size_t identity_count, marks, partial_checks, allocation_count, allocation_sizes[16];
static unsigned proto_calls, finalizers;
static RootProvider owner_roots;
static Method new_method,add_method,at_method,size_method;
static int capture, phase, adopting, missing, rebound, operation_probe, modern;
static void *collection;
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
    else assert(fixture_vector_v2 && (value==api->intern("value:") || value==api->intern("value:value:") || value==api->intern("printString")));
}
static void partial(size_t expected) {
    if(!modern) return;
    assert(owner_roots); marks=0; owner_roots(mark); assert(marks==expected); partial_checks++;
}
static void roots(RootProvider provider) {
    Dl_info owner; assert(dladdr((const void *)provider,&owner));
    if(strstr(owner.dli_fname,"/libbirnam_vector.dylib")) owner_roots=provider;
    fixture_register_root(provider);
}
static Oop proto(Oop parent) {
    if(proto_calls==2) partial(1);
    if(proto_calls==3) partial(2);
    if(capture) printf("proto parent=%d\n",id(parent));
    Oop result=api->proto(parent); proto_calls++;
    if(proto_calls==2) node_type=result;
    if(proto_calls==3) behavior=result;
    if(capture) printf("proto result=%d\n",id(result));
    return result;
}
static Oop import(const char *name) {
    if(!strcmp(name,"VectorVt")) partial(1);
    if(!strcmp(name,"Vector")) partial(2);
    if(!strcmp(name,"RangeVt")) partial(missing?2:3);
    if(capture) printf("import %s\n",name);
    Oop result=fixture_namespace_import(name);
    if(!strcmp(name,"VectorNode")) node_type=result;
    if(!strcmp(name,"VectorVt")) behavior=result;
    if(!strcmp(name,"Vector")) prototype=result;
    if(capture) printf("import result=%d\n",id(result));
    return result;
}
static int exported(const char *name) { int result=fixture_namespace_contains(name); if(capture) printf("isExported %s=%d\n",name,result); return result; }
static Oop intern(const char *name) { if(capture) printf("intern %s\n",name); return api->intern(name); }
static void layout(Oop type,const uint8_t *kinds,size_t count,uint8_t rest) {
    if(capture) { printf("layout %d rest=%u",id(type),rest); for(size_t i=0;i<count;i++) printf(" %u",kinds[i]); putchar('\n'); }
    if(type==node_type) assert(count==0 && rest==IDAPI_V18_SLOT_OOP);
    if(type==behavior) { const uint8_t expected[]={IDAPI_V18_SLOT_RAW,IDAPI_V18_SLOT_RAW,IDAPI_V18_SLOT_OOP,IDAPI_V18_SLOT_OOP,IDAPI_V18_SLOT_RAW}; assert(count==5 && rest==IDAPI_V18_SLOT_RAW && !memcmp(kinds,expected,5)); }
    fixture_set_layout(type,kinds,count,rest);
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
    if(phase==0 && candidate) { prototype=candidate; candidate=NULL; partial(3); }
    if(capture) printf("alloc %d %zu\n",id(type),size);
    if(operation_probe) {
        assert(type==node_type || type==behavior);
        assert(size==(type==node_type?256:40));
        assert(allocation_count<16); allocation_sizes[allocation_count++]=size;
        Dl_info owner; assert(dladdr(__builtin_return_address(0),&owner));
        assert(strstr(owner.dli_fname,modern?"/libbirnam_vector.dylib":"_collection.dylib"));
    }
    Oop result=api->alloc(type,size);
    if(type==behavior) {
        assert(size==40);
        if(phase==0 && !prototype) candidate=result;
        /* All five words must be initialized explicitly. */
        memset(result,0xA5,40);
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
    if(!strcmp(name,"Vector")) {
        prototype=value; assert(lookup(behavior,"new") && lookup(behavior,"add:"));
        Oop other=lookup(behavior,"new")(value,NULL,0); assert(api->vtable_of(other)==behavior && ((Payload *)other)->count==0);
        api->gc_collect(); assert(fixture_integer_value(send0(other,"size"))==0);
    }
    return fixture_namespace_export(name,value);
}
static void validate_dependency(void *image, int attached) {
    if(fixture_vector_v2) {
    VectorDependencyV6 attach=(VectorDependencyV6)api->dlsym_fn(image,"birnam_vector_dependency_v6"); assert(attach && attach(NULL)==0);
    VectorApiV6 bad=*fixture_vector_v2; bad.version++; assert(attach(&bad)==0); bad=*fixture_vector_v2; bad.byte_size--; assert(attach(&bad)==0);
#define MISSING(field) bad=*fixture_vector_v2; bad.field=NULL; assert(attach(&bad)==0)
    MISSING(prepare); MISSING(finish_prototype); MISSING(set_layout); MISSING(set_owner); MISSING(publish);
    MISSING(empty); MISSING(push); MISSING(behavior); MISSING(install); MISSING(for_each); MISSING(inject);
#undef MISSING
    if(attached) { bad=*fixture_vector_v2; assert(attach(&bad)==0 && attach(fixture_vector_v2)==1); }
    } else {
    VectorDependencyV1 attach=(VectorDependencyV1)api->dlsym_fn(image,"birnam_vector_dependency_v1"); assert(attach && attach(NULL)==0);
    VectorApiV1 bad=*fixture_vector_v1; bad.version++; assert(attach(&bad)==0); bad=*fixture_vector_v1; bad.byte_size--; assert(attach(&bad)==0);
#define MISSING(field) bad=*fixture_vector_v1; bad.field=NULL; assert(attach(&bad)==0)
    MISSING(prepare); MISSING(finish_prototype); MISSING(set_layout); MISSING(set_owner); MISSING(publish);
    MISSING(empty); MISSING(push); MISSING(count); MISSING(at); MISSING(behavior);
#undef MISSING
    if(attached) { bad=*fixture_vector_v1; assert(attach(&bad)==0 && attach(fixture_vector_v1)==1); }
    }
}
static void mount(const char *path,IdApi *table,const NamespacePresenceV1 *presence,const NamespaceImportV1 *import_cap,const NamespaceExportV1 *export_cap,const RootRegistryV1 *root_registry,const PayloadLayoutV1 *layouts) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    if(modern && strstr(path,"collection") && !collection) {
        fixture_prepare_vector(api,fixture_root_registry());
        /* Reject malformed descriptors before the dependency has been selected. */
        validate_dependency(image,0);
    }
    fixture_attach_integer(api,image,fixture_root_registry());
    if(modern && strstr(path,"collection")) {
        validate_dependency(image,1);
    }
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init); init(table,NULL,presence,import_cap,export_cap,fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),layouts,fixture_trace_hooks());
    if(strstr(path,"collection")) collection=image;
}
static void seed(void) {
    const char *names[]={"CollectionVt","VectorNode","VectorVt","RangeVt","MapVt","MapNode"}; Oop types[6];
    for(size_t i=0;i<6;i++) { types[i]=api->proto(api->object_vt); fixture_namespace_export(names[i],types[i]); }
    fixture_namespace_export("Collection",types[0]); node_type=types[1]; behavior=types[2];
    Oop vector=api->alloc(behavior,40),range=api->alloc(types[3],16),map=api->alloc(types[4],16);
    *(Payload *)vector=(Payload){0,5,api->alloc(node_type,256),api->alloc(node_type,256),0};
    memset(range,0,16); memset(map,0,16);
    prototype=missing?NULL:(rebound?api->nil_obj:vector);
    fixture_namespace_export("Vector",prototype); fixture_namespace_export("Range",range); fixture_namespace_export("Map",map);
    /* isExported uses nonnull values; force the adoption predicate for the missing prototype mode below. */
}
static int adoption_exported(const char *name) {
    if(phase==0 && missing && !strcmp(name,"Vector")) { if(capture) puts("isExported Vector=1"); return 1; }
    return exported(name);
}
static Oop empty(void) { return modern?fixture_vector->empty():new_method(prototype,NULL,0); }
static Oop push(Oop vector,Oop value) { return modern?fixture_vector->push(vector,value):add_method(vector,&value,1); }
static void trace_allocations(size_t count) {
    printf("append %zu allocations",count); for(size_t i=0;i<allocation_count;i++) printf(" %zu",allocation_sizes[i]); putchar('\n');
}
static void verify(Oop vector,size_t count) {
    assert(api->vtable_of(vector)==behavior && ((Payload *)vector)->count==count);
    assert(fixture_integer_value(size_method(vector,NULL,0))==(int64_t)count);
    if(modern) assert(fixture_vector->count(vector)==count && fixture_vector->at(vector,count)==NULL && fixture_vector->at(vector,SIZE_MAX)==NULL);
    for(size_t i=0;i<count;i++) {
        Oop index=fixture_integer_make((int64_t)i),value=at_method(vector,&index,1);
        assert(fixture_integer_value(value)==(int64_t)i);
        if(modern) assert(fixture_vector->at(vector,i)==value);
    }
}
static Oop forbidden(Oop receiver,const Oop *args,size_t count) { (void)receiver; (void)args; (void)count; assert(!"private helper became an overridable send"); return NULL; }
int main(int argc,char **argv) {
    assert(argc==5 || (argc==6 && !strcmp(argv[5],"baseline"))); modern=argc==5;
    int ignored=!strcmp(argv[4],"vector-only-exports"); adopting=strcmp(argv[4],"normal")!=0 && !ignored;
    missing=!strcmp(argv[4],"missing-prototype"); rebound=!strcmp(argv[4],"rebound-prototype");
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor); api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api);
    fixture_set_auto(0); fixture_mount_trace(api,argv[1],fixture_root_registry()); fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor)); mount(argv[2],api,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry(),fixture_payload_layout());
    if(adopting || ignored) seed();
    if(ignored) { fixture_namespace_export("Vector",NULL); node_type=behavior=prototype=NULL; }
    Oop old_node=node_type,old_behavior=behavior,old_prototype=prototype;
    selected=*api; selected.proto=proto; selected_import_slot=import; selected_presence_slot=adoption_exported;
    selected.intern=intern; selected.alloc=allocate; selected.method=method; selected.send=send;
    NamespaceExportV1 selected_exports={1,sizeof(NamespaceExportV1),NULL,publish}; FixtureLayoutSlots layout_slots={layout,fixture_slot_kind}; PayloadLayoutV1 selected_layouts=FIXTURE_LAYOUTS(layout_slots); selected.set_finalizer=finalize; selected_root_slot=roots;
    capture=1; mount(argv[3],&selected,&selected_presence,&selected_imports,&selected_exports,&selected_roots,&selected_layouts); capture=0; phase=1;
    if(candidate) { prototype=candidate; candidate=NULL; }
    if(adopting) assert(node_type==old_node && behavior==old_behavior && (!old_prototype || prototype==old_prototype));
    else assert(fixture_namespace_import("Vector")==prototype && vtable_parent(api,behavior)==fixture_namespace_import("CollectionVt"));
    assert(prototype && !finalizers); partial(3); if(modern) assert(partial_checks>=3 && fixture_vector->behavior()==behavior);
    if(!rebound) assert(((Payload *)prototype)->count==0 && ((Payload *)prototype)->shift==5 && ((Payload *)prototype)->tail_len==0);
    else assert(prototype==api->nil_obj);
    new_method=lookup(behavior,"new"); add_method=lookup(behavior,"add:"); at_method=lookup(behavior,"at:"); size_method=lookup(behavior,"size");
    operation_probe=1; allocation_count=0; Oop vector=empty(); trace_allocations(0); operation_probe=0;
    verify(vector,0); assert(allocation_count==3 && allocation_sizes[0]==256 && allocation_sizes[1]==256 && allocation_sizes[2]==40);
    Oop versions[4]={0}; const size_t retained[]={32,33,1056,1057};
    for(size_t i=0;i<32801;i++) {
        Oop old=vector; Payload previous=*(Payload *)old; Oop value=fixture_integer_make((int64_t)i);
        allocation_count=0; operation_probe=1; vector=push(old,value); operation_probe=0;
        assert(!memcmp(&previous,old,sizeof(previous)) && ((Payload *)vector)->tail!=previous.tail);
        if(previous.tail_len<32) assert(((Payload *)vector)->root==previous.root && allocation_count==2);
        else assert(((Payload *)vector)->root!=previous.root && allocation_sizes[0]==256);
        if(i==0 || i==30 || i==31 || i==32 || i==1023 || i==1055 || i==1056 || i==32767 || i==32799 || i==32800) trace_allocations(i+1);
        for(size_t j=0;j<4;j++) if(i+1==retained[j]) versions[j]=vector;
    }
    verify(vector,32801); assert(((Payload *)vector)->shift==15);
    fixture_namespace_export("VectorStorageRoot",vector);
    for(size_t j=0;j<4;j++) { char name[32]; snprintf(name,sizeof(name),"VectorVersion%zu",j); fixture_namespace_export(name,versions[j]); verify(versions[j],retained[j]); }
    assert(((Payload *)versions[1])->root && ((Oop *)((Payload *)versions[1])->root)[0]==((Payload *)versions[0])->tail);
    assert(((Oop *)((Payload *)versions[3])->root)[0]==((Payload *)versions[2])->root);
    Oop alias=vector; fixture_namespace_export("VectorStorageAlias",alias);
    Oop inherited=api->alloc(api->proto(behavior),40); *(Payload *)inherited=*(Payload *)vector;
    assert(fixture_integer_value(size_method(inherited,NULL,0))==32801);
    if(modern) assert(fixture_vector->count(inherited)==32801 && fixture_vector->at(inherited,32)==fixture_vector->at(vector,32));
    Oop item=fixture_integer_make(32801),extended=push(inherited,item);
    assert(api->vtable_of(extended)==behavior && ((Payload *)extended)->count==32802);
    Oop with_null=push(vector,NULL),index=fixture_integer_make(32801);
    assert(at_method(with_null,&index,1)==api->nil_obj);
    assert(add_method(vector,NULL,1)==api->nil_obj && new_method(vector,NULL,1)==api->nil_obj);
    assert(at_method(vector,NULL,1)==api->nil_obj && size_method(vector,NULL,1)==api->nil_obj);
    Oop clone=send0(vector,"clone"); assert(clone!=vector && api->vtable_of(clone)==behavior && ((Payload *)clone)->count==0);
    /* Overridden public Vector methods must not affect private construction. */
    const char *names[]={"new","add:","at:","size",","};
    Method previous[5];
    for(size_t i=0;i<5;i++) { previous[i]=lookup(behavior,names[i]); api->method(behavior,api->intern(names[i]),forbidden); }
    Oop range=send1(fixture_integer_make(1),"to:",fixture_integer_make(3));
    Oop reversed=send0(range,"reversed"); assert(((Payload *)reversed)->count==3);
    Oop joined=send1(range,",",reversed); assert(((Payload *)joined)->count==6);
    Oop separator=fixture_string_make((const uint8_t *)",",1);
    Oop split=send1(fixture_string_make((const uint8_t *)"a,b",3),"split:",separator); assert(((Payload *)split)->count==2);
    Oop map=send0(fixture_namespace_import("Map"),"new");
    Oop pair[]={fixture_integer_make(7),fixture_integer_make(8)};
    map=api->send(map,api->intern("at:put:"),pair,2);
    assert(((Payload *)send0(map,"keys"))->count==1 && ((Payload *)send0(map,"values"))->count==1);
    for(size_t i=0;i<5;i++) api->method(behavior,api->intern(names[i]),previous[i]);
    fixture_namespace_export("VectorVt",api->nil_obj); fixture_namespace_export("Vector",api->nil_obj); fixture_namespace_export("VectorNode",api->nil_obj);
    IdApi other=selected;NamespaceExportV1 other_exports=selected_exports;FixtureImportSlot other_import_slot=selected_import_slot;NamespaceImportV1 other_imports=FIXTURE_IMPORT(other_import_slot); other.alloc=forbidden_allocator;
    capture=1; mount(argv[3],&other,&selected_presence,&other_imports,&other_exports,&selected_roots,&selected_layouts); capture=0;
    assert(fixture_namespace_import("Vector")==api->nil_obj);
    selected.alloc=second_allocator; live_allocations=0;
    Oop fresh=empty(); assert(live_allocations==3 && api->vtable_of(fresh)==behavior);
    fresh=push(fresh,api->true_obj); assert(live_allocations==5 && ((Payload *)fresh)->count==1);
    selected.alloc=allocate;
    fixture_set_precise(1); api->gc_collect(); partial(3);
    vector=fixture_namespace_import("VectorStorageRoot"); assert(vector==fixture_namespace_import("VectorStorageAlias")); verify(vector,32801);
    for(size_t j=0;j<4;j++) verify(versions[j],retained[j]);
    /* GC may recycle temporary addresses recorded by an earlier trace phase. */
    identity_count=0;
    fixture_namespace_export("Vector",NULL); capture=1; mount(argv[3],&selected,&selected_presence,&selected_imports,&selected_exports,&selected_roots,&selected_layouts); capture=0;
    assert(fixture_namespace_import("VectorVt")==behavior && fixture_namespace_import("Vector")==prototype && fixture_namespace_import("VectorNode")==node_type && !finalizers);
    printf("vector-storage: construction, persistence through 32801, native consumers, allocation order, roots, live callbacks and reload passed mode=%s\n",argv[4]);
}
