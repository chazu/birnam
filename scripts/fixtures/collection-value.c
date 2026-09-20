#include "idapi_v10.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <math.h>
#include <inttypes.h>

typedef struct { Oop root; size_t count; } Map;
typedef struct { uint32_t tag,n,hash,pad; Oop key,value; } Collision;
enum Kind { IDENTITY, INTEGER, FLOAT, STRING, SYMBOL };
typedef struct { Oop object; enum Kind kind; int64_t integer; double real; const uint8_t *bytes; size_t length; } Sample;
static IdApi *api,selected;
static Sample samples[48]; static Oop vectors[48],maps[48],active_map,old_map; static size_t used;
static Method index_of,map_at,map_put,map_new;
static int capture,force_integer,force_float,force_bytes,reenter;
static unsigned sends,roots_registered,checks;
static int (*integer_exact)(void *,Oop),(*float_exact)(void *,Oop);
static int64_t (*integer_read)(void *,Oop);
static double (*float_read)(void *,Oop);
static const uint8_t *(*string_bytes)(void *,Oop,size_t *);
static Oop send1(Oop object,const char *name,Oop arg) { return api->send(object,api->intern(name),&arg,1); }
static Method lookup(Oop type,const char *name) { return (Method)send1(type,"lookup",api->intern(name)); }
static int id(Oop object) { for(size_t i=0;i<used;i++) if(samples[i].object==object) return (int)i; return -1; }
static void mark_roots(MarkFn mark) {
    for(size_t i=0;i<used;i++) { mark(samples[i].object); mark(vectors[i]); mark(maps[i]); }
    mark(active_map); mark(old_map);
}
static void roots(RootProvider provider) { roots_registered++; api->gc_register_root_provider(provider); }
static int ie(void *context,Oop value) { if(capture) printf("integer? %d\n",id(value)); return integer_exact(context,value); }
static int fe(void *context,Oop value) { if(capture) printf("float? %d\n",id(value)); return float_exact(context,value); }
static int64_t ir(void *context,Oop value) { if(capture) printf("integer %d\n",id(value)); return force_integer?7:integer_read(context,value); }
static double fr(void *context,Oop value) { if(capture) printf("float %d\n",id(value)); return force_float?7.0:float_read(context,value); }
static const uint8_t *sb(void *context,Oop value,size_t *length) {
    if(capture) printf("bytes %d\n",id(value));
    if(reenter) {
        reenter=0; Oop needle=samples[8].object;
        assert(index_of(vectors[8],&needle,1)!=api->nil_obj); api->gc_collect();
    }
    static const uint8_t replacement[]={'z'};
    if(force_bytes) { *length=force_bytes==3?0:1; return force_bytes==1?replacement:NULL; }
    return string_bytes(context,value,length);
}
static Oop vt(Oop value) { if(capture) printf("vtable %d\n",id(value)); return api->vtable_of(value); }
static const char *sn(Oop value) { if(capture) printf("symbol %d\n",id(value)); return api->symbol_name(value); }
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) { if(capture) sends++; return api->send(self,selector,args,count); }
static Oop forbidden(Oop self,const Oop *args,size_t count) { (void)self; (void)args; (void)count; assert(!"collection policy used a public send"); return NULL; }
static const char *alternate_name(Oop value) { (void)value; return "alias"; }
static const char *null_name(Oop value) { (void)value; return NULL; }
static uint32_t address_hash(uint64_t input) {
    /* Independent unsigned arithmetic oracle for the specified bit mixer. */
    uint64_t x=input+UINT64_C(0x9e3779b97f4a7c15);
    x^=x>>30; x*=UINT64_C(0xbf58476d1ce4e5b9);
    x^=x>>27; x*=UINT64_C(0x94d049bb133111eb); x^=x>>31;
    return (uint32_t)x;
}
static uint32_t byte_hash(const uint8_t *bytes,size_t count) {
    uint32_t result=UINT32_C(2166136261);
    for(size_t i=0;i<count;i++) result=(result^bytes[i])*UINT32_C(16777619);
    return result;
}
static uint32_t expected_hash(Sample value) {
    if(value.kind==INTEGER) return address_hash((uint64_t)value.integer);
    if(value.kind==FLOAT) { uint64_t bits=0; if(value.real!=0) memcpy(&bits,&value.real,8); return address_hash(bits); }
    if(value.kind==STRING || value.kind==SYMBOL) return byte_hash(value.bytes,value.length);
    return address_hash((uintptr_t)value.object);
}
static int equal(Sample a,Sample b) {
    if(a.object==b.object) return 1;
    if(!a.object || !b.object || a.kind!=b.kind) return 0;
    switch(a.kind) {
        case INTEGER:return a.integer==b.integer;
        case FLOAT:return a.real==b.real;
        case STRING:return a.length==b.length && !memcmp(a.bytes,b.bytes,a.length);
        default:return 0;
    }
}
static void add(Sample sample) { assert(used<48); samples[used++]=sample; }
static Oop collision(Oop key,uint32_t hash) {
    Collision *node=api->alloc(fixture_namespace_import("MapNode"),sizeof(Collision));
    *node=(Collision){1,1,hash,0,key,api->true_obj};
    Map *map=api->alloc(fixture_namespace_import("MapVt"),sizeof(Map)); *map=(Map){node,1}; return map;
}
static void validate(void *collection,int attached) {
    CollectionValueDependencyV1 attach=(CollectionValueDependencyV1)api->dlsym_fn(collection,"birnam_collection_value_dependency_v1");
    if(!attach) {
        const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
        int n=snprintf(path,sizeof(path),"%.*s/libbirnam_map.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
        assert(n>0 && (size_t)n<sizeof(path)); void *map=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(map);
        attach=(CollectionValueDependencyV1)api->dlsym_fn(map,"birnam_collection_value_dependency_v1");
    }
    assert(attach);
    CollectionValueApiV1 bad=*fixture_collection_value;
    assert(!attach(NULL)); bad.version++; assert(!attach(&bad)); bad=*fixture_collection_value;
    bad.byte_size--; assert(!attach(&bad)); bad=*fixture_collection_value;
    bad.same_value=NULL; assert(!attach(&bad)); bad=*fixture_collection_value;
    bad.key_hash=NULL; assert(!attach(&bad));
    if(attached) { bad=*fixture_collection_value; assert(!attach(&bad) && attach(fixture_collection_value)==1); }
}
static void mount(const char *path,IdApi *table) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    if(strstr(path,"collection") && !fixture_collection_value && api->dlsym_fn(image,"birnam_collection_value_dependency_v1")) {
        const char *slash=strrchr(fixture_floor_path,'/'); char library[PATH_MAX]; assert(slash);
        int n=snprintf(library,sizeof(library),"%.*s/libbirnam_collection_value.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
        assert(n>0 && (size_t)n<sizeof(library));
        void *value=api->dlopen_fn(library,RTLD_NOW|RTLD_LOCAL); assert(value); fixture_attach_integer(api,value);
        CollectionValueProviderV1 provide=(CollectionValueProviderV1)api->dlsym_fn(value,"birnam_collection_value_provider_v1"); assert(provide && !provide());
        ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(value,"module_init"); assert(init); unsigned before=roots_registered;
        init(table,NULL,fixture_namespace_presence(),fixture_namespace_imports()); assert(roots_registered==before); fixture_collection_value=provide(); assert(fixture_collection_value); validate(image,0);
    }
    fixture_attach_integer(api,image);
    if(strstr(path,"collection") && fixture_collection_value) validate(image,1);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init); init(table,NULL,fixture_namespace_presence(),fixture_namespace_imports());
}
static void begin(const char *name) { capture=1; sends=0; printf("case %s\n",name); }
static void end(void) { capture=0; assert(!sends); checks++; }
static void pair(size_t left,size_t right,int expected) {
    Oop needle=samples[right].object; Oop result=index_of(vectors[left],&needle,1);
    assert((result!=api->nil_obj)==expected);
    if(expected) assert(integer_read(fixture_integer->context,result)==0);
}
int main(int argc,char **argv) {
    assert(argc==4); void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]); fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports());
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor)); mount(argv[2],api);
    selected=*api; selected.vtable_of=vt; selected.symbol_name=sn; selected.send=send; selected.gc_register_root_provider=roots; mount(argv[3],&selected);
    api->gc_register_root_provider(mark_roots);
    index_of=lookup(fixture_vector->behavior(),"indexOf:"); Oop map_type=fixture_namespace_import("MapVt");
    map_at=lookup(map_type,"at:"); map_put=lookup(map_type,"at:put:"); map_new=lookup(map_type,"new");
    add((Sample){.object=NULL}); add((Sample){.object=api->nil_obj}); add((Sample){.object=api->true_obj}); add((Sample){.object=api->false_obj});
    add((Sample){.object=api->alloc(api->object_vt,0)}); add((Sample){.object=api->alloc(api->object_vt,0)});
    add((Sample){.object=api->intern("alpha"),.kind=SYMBOL,.bytes=(const uint8_t *)"alpha",.length=5});
    add((Sample){.object=api->intern("beta"),.kind=SYMBOL,.bytes=(const uint8_t *)"beta",.length=4});
    const int64_t numbers[]={0,0,1,-1,INT64_MIN,INT64_MAX,9007199254740993LL};
    for(size_t i=0;i<7;i++) add((Sample){.object=fixture_integer_make(numbers[i]),.kind=INTEGER,.integer=numbers[i]});
    const double floats[]={0.0,-0.0,1.0,1.0,INFINITY,-INFINITY,NAN,NAN,9007199254740992.0};
    for(size_t i=0;i<9;i++) add((Sample){.object=fixture_float_make(floats[i]),.kind=FLOAT,.real=floats[i]});
    static const uint8_t empty[]={0},alpha[]="alpha",beta[]="beta",binary[]={'a',0,'b'},binary2[]={'a',0,'c'},utf8[]={0xc3,0xa9};
    const uint8_t *strings[]={empty,empty,alpha,alpha,beta,binary,binary,binary2,utf8}; const size_t lengths[]={0,0,5,5,4,3,3,3,2};
    for(size_t i=0;i<9;i++) add((Sample){.object=fixture_string_make(strings[i],lengths[i]),.kind=STRING,.bytes=strings[i],.length=lengths[i]});
    Oop inherited=api->alloc(api->proto(fixture_integer->behavior),8); fixture_integer->storage->initialize(inherited,0); add((Sample){.object=inherited});
    inherited=api->alloc(api->proto(fixture_float->behavior),8); fixture_float->storage->initialize_bits(inherited,0); add((Sample){.object=inherited});
    /* The collision record exposes all 32 hash bits through the existing Map lookup. */
    for(size_t i=0;i<used;i++) { vectors[i]=fixture_vector->push(fixture_vector->empty(),samples[i].object); maps[i]=collision(samples[i].object,expected_hash(samples[i])); }
    integer_exact=fixture_integer->is_exact; integer_read=fixture_integer->read; float_exact=fixture_float->is_exact; float_read=fixture_float->read; string_bytes=fixture_string->bytes;
    fixture_integer->is_exact=ie; fixture_integer->read=ir; fixture_float->is_exact=fe; fixture_float->read=fr; fixture_string->bytes=sb;
    Oop types[]={api->object_vt,fixture_integer->behavior,fixture_float->behavior,fixture_string->behavior,api->symbol_vt};
    for(size_t i=0;i<5;i++) { api->method(types[i],api->intern("="),forbidden); api->method(types[i],api->intern("hash"),forbidden); }
    for(size_t i=0;i<used;i++) for(size_t j=0;j<used;j++) {
        printf("pair %zu %zu\n",i,j); begin("equality"); pair(i,j,equal(samples[i],samples[j])); end();
        if(fixture_collection_value) assert(fixture_collection_value->same_value(&selected,samples[i].object,samples[j].object)==equal(samples[i],samples[j]));
    }
    for(size_t i=0;i<used;i++) {
        printf("key %zu\n",i); begin("hash"); assert(map_at(maps[i],&samples[i].object,1)==api->true_obj); end();
        if(fixture_collection_value) assert(fixture_collection_value->key_hash(&selected,samples[i].object)==expected_hash(samples[i]));
    }
    force_integer=1; begin("live Integer reads"); pair(8,10,1); end();
    Oop probe=collision(samples[8].object,address_hash(7)); assert(map_at(probe,&samples[8].object,1)==api->true_obj); force_integer=0;
    force_float=1; begin("live Float reads"); pair(15,17,1); end(); force_float=0;
    force_bytes=1; begin("live String bytes"); pair(26,28,1); end();
    probe=collision(samples[26].object,byte_hash((const uint8_t *)"z",1)); assert(map_at(probe,&samples[26].object,1)==api->true_obj);
    force_bytes=2; probe=collision(samples[26].object,0); begin("null String hash"); assert(map_at(probe,&samples[26].object,1)==api->true_obj); end();
    force_bytes=3; probe=collision(samples[26].object,2166136261U); begin("empty String hash"); assert(map_at(probe,&samples[26].object,1)==api->true_obj); end(); force_bytes=0;
    reenter=1; begin("String callback reentry and GC"); pair(26,27,1); end();
    selected.symbol_name=alternate_name; probe=collision(samples[6].object,byte_hash((const uint8_t *)"alias",5)); assert(map_at(probe,&samples[6].object,1)==api->true_obj);
    selected.symbol_name=null_name; probe=collision(samples[6].object,0); assert(map_at(probe,&samples[6].object,1)==api->true_obj); selected.symbol_name=sn;
    selected.symbol_vt=api->object_vt; selected.symbol_name=alternate_name;
    probe=collision(samples[4].object,byte_hash((const uint8_t *)"alias",5)); assert(map_at(probe,&samples[4].object,1)==api->true_obj);
    if(fixture_collection_value) {
        IdApi alternate=*api; alternate.symbol_vt=api->object_vt; alternate.symbol_name=null_name;
        assert(fixture_collection_value->key_hash(&alternate,samples[4].object)==0);
        assert(fixture_collection_value->key_hash(&selected,samples[4].object)==byte_hash((const uint8_t *)"alias",5));
    }
    selected.symbol_vt=api->symbol_vt; selected.symbol_name=sn;
    /* Real Map updates preserve old versions, equal keys, and unequal same-hash keys. */
    active_map=map_new(fixture_namespace_import("Map"),NULL,0); const size_t keys[]={8,15,6,26,29};
    for(size_t i=0;i<5;i++) { Oop args[]={samples[keys[i]].object,fixture_integer_make((int64_t)i)}; active_map=map_put(active_map,args,2); }
    assert(((Map *)active_map)->count==5); old_map=active_map;
    Oop args[]={samples[27].object,fixture_integer_make(99)}; active_map=map_put(active_map,args,2);
    assert(((Map *)active_map)->count==5 && fixture_integer_value(map_at(old_map,&args[0],1))==3 && fixture_integer_value(map_at(active_map,&args[0],1))==99);
    api->gc_set_precise(1); api->gc_collect(); api->gc_set_precise(0);
    for(size_t i=0;i<5;i++) assert(fixture_is_integer(map_at(active_map,&samples[keys[i]].object,1)));
    IdApi other=selected; other.symbol_vt=api->object_vt; other.symbol_name=null_name; mount(argv[3],&other);
    begin("reload retains caller table"); assert(map_at(maps[6],&samples[6].object,1)==api->true_obj); pair(26,27,1); end();
    printf("collection-value: %zu samples, %u cases, full hash oracle, live callbacks, reentry, persistence, roots and reload passed\n",used,checks);
}
