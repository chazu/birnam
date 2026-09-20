#include "idapi_v10.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <inttypes.h>

/* Independent method, callback-order and precise-root oracle, runnable on 81AD. */
static IdApi *api,selected;
static RootProvider vector_roots,collection_roots;
static Oop type,block_type,block,key_type,input,nested;
static const char *names[]={"size","at:","add:","do:","inject:into:","printString",",","indexOf:","copyFrom:to:","reversed","sortBy:","new","clone"};
static const size_t arities[]={0,1,1,1,2,0,1,1,2,0,1,0,0};
static Method methods[13];
static size_t marks,base_marks,extra_roots,key_count,finalized,key_calls[2],visits;
static int depth,nest,fail_key,replace_key,non_true,trace;
static size_t comparisons;
typedef struct { int64_t rank,identity; } Key;
static Oop send1(Oop self,const char *selector,Oop value) { return api->send(self,api->intern(selector),&value,1); }
static Method lookup(Oop self,const char *selector) { return (Method)send1(self,"lookup",api->intern(selector)); }
static void mark(Oop value) { assert(value); marks++; }
static void check_roots(void) {
    marks=0; (fixture_vector_v2?vector_roots:collection_roots)(mark);
    assert(marks==base_marks+extra_roots);
}
static void register_roots(RootProvider provider) {
    Dl_info owner; assert(dladdr((const void *)provider,&owner));
    if(strstr(owner.dli_fname,"/libbirnam_vector.dylib")) vector_roots=provider;
    if(strstr(owner.dli_fname,"_collection.dylib")) collection_roots=provider;
    api->gc_register_root_provider(provider);
}
static Oop counted_send(Oop self,Oop selector,const Oop *args,size_t count) {
    if(trace) printf("send %s %zu\n",api->symbol_name(selector),count);
    return api->send(self,selector,args,count);
}
static void gc(void) { api->gc_set_precise(1); api->gc_collect(); api->gc_set_precise(0); }
static void finalize(Oop object) { assert(((Key *)object)->identity>=0); finalized++; }
static Oop vector(const int64_t *values,size_t count) {
    Oop result=fixture_vector->empty();
    for(size_t i=0;i<count;i++) result=fixture_vector->push(result,fixture_integer_make(values[i]));
    return result;
}
static void expect(Oop result,const int64_t *values,size_t count) {
    assert(result && fixture_vector->count(result)==count);
    for(size_t i=0;i<count;i++) assert(fixture_integer_value(fixture_vector->at(result,i))==values[i]);
}
static Oop alternate_key(Oop self,const Oop *args,size_t count);
static Oop make_key(Oop self,const Oop *args,size_t count) {
    assert(self==block && args && count==1); check_roots(); gc();
    int64_t value=fixture_integer_value(args[0]);
    printf("key depth=%d value=%" PRId64 "\n",depth,value);
    if(nest && !depth && !key_calls[0]) {
        depth=1; key_calls[1]=0; extra_roots+=2;
        Oop result=methods[10](nested,&block,1); const int64_t expected[]={12,10,11}; expect(result,expected,3);
        extra_roots-=2+2*key_calls[1]; depth=0; check_roots();
    }
    if(fail_key && key_calls[depth]==1) return NULL;
    if(replace_key && !key_calls[depth]) api->method(block_type,api->intern("value:"),alternate_key);
    Oop key=api->alloc(key_type,sizeof(Key));
    *(Key *)key=(Key){value%3,value}; api->set_finalizer(key,finalize); key_count++;
    key_calls[depth]++; extra_roots+=2;
    return key;
}
static Oop alternate_key(Oop self,const Oop *args,size_t count) {
    puts("replacement key"); return make_key(self,args,count);
}
static Oop alternate_less(Oop self,const Oop *args,size_t count);
static Oop less(Oop self,const Oop *args,size_t count) {
    assert(args && count==1); check_roots(); gc();
    Key a=*(Key *)self,b=*(Key *)args[0];
    printf("less %" PRId64 " %" PRId64 "\n",a.identity,b.identity);
    if(replace_key && !comparisons++) api->method(key_type,api->intern("<"),alternate_less);
    if(non_true) return block;
    return a.rank<b.rank?api->true_obj:api->false_obj;
}
static Oop alternate_less(Oop self,const Oop *args,size_t count) {
    puts("replacement comparator"); return less(self,args,count);
}
static void sort_case(const char *name,int nesting,int failure,int replacement,int truth) {
    printf("case %s\n",name); nest=nesting; fail_key=failure; replace_key=replacement; non_true=truth;
    api->method(block_type,api->intern("value:"),make_key);
    api->method(key_type,api->intern("<"),less); comparisons=0;
    key_count=finalized=key_calls[0]=key_calls[1]=0; extra_roots=2; trace=1;
    Oop result=methods[10](input,&block,1); trace=0;
    if(failure) assert(result==api->nil_obj);
    else if(truth) { const int64_t expected[]={5,2,4,1,3,0}; expect(result,expected,6); }
    else { const int64_t expected[]={3,0,4,1,5,2}; expect(result,expected,6); }
    extra_roots=0; check_roots(); gc(); assert(finalized==key_count);
    printf("keys released %zu\n",finalized);
}
static Oop forbidden(Oop self,const Oop *args,size_t count) { (void)self; (void)args; (void)count; assert(!"native consumer or copied Method used public replacement"); return NULL; }
static Oop visit(Oop self,const Oop *args,size_t count) {
    assert(self==block && args && count==1); visits++;
    printf("visit %" PRId64 "\n",fixture_integer_value(args[0])); return NULL;
}
static Oop sum(Oop self,const Oop *args,size_t count) {
    assert(self==block && args && count==2); visits++;
    return fixture_integer_make(fixture_integer_value(args[0])+fixture_integer_value(args[1]));
}
static Oop null_sum(Oop self,const Oop *args,size_t count) { (void)self; (void)args; assert(count==2); visits++; return NULL; }
static void mount(const char *path,IdApi *table) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init); init(table,NULL,fixture_namespace_presence(),fixture_namespace_imports());
}
int main(int argc,char **argv) {
    assert(argc==4); void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]); fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports());
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor)); mount(argv[2],api);
    selected=*api; selected.send=counted_send; selected.gc_register_root_provider=register_roots; mount(argv[3],&selected);
    type=fixture_vector->behavior();
    for(size_t i=0;i<13;i++) {
        methods[i]=lookup(type,names[i]); assert(methods[i]);
        Dl_info owner; assert(dladdr((const void *)methods[i],&owner));
        assert(strstr(owner.dli_fname,fixture_vector_v2?"/libbirnam_vector.dylib":"_collection.dylib"));
        Oop arg=api->nil_obj; assert(methods[i](fixture_namespace_import("Vector"),&arg,arities[i]+1)==api->nil_obj);
        if(arities[i]) assert(methods[i](fixture_namespace_import("Vector"),NULL,arities[i])==api->nil_obj);
    }
    assert(methods[11]==methods[12]);
    block_type=api->proto(api->object_vt); block=api->alloc(block_type,0); key_type=api->proto(api->object_vt);
    api->set_layout(key_type,NULL,0,IDAPI_V10_SLOT_RAW); api->method(key_type,api->intern("<"),less);
    api->export_obj("VectorTestBlock",block); api->export_obj("VectorTestKeys",key_type);
    const int64_t values[]={5,2,4,1,3,0},inner[]={12,11,10};
    input=vector(values,6); nested=vector(inner,3);
    api->export_obj("VectorTestInput",input); api->export_obj("VectorTestNested",nested);
    marks=0; (fixture_vector_v2?vector_roots:collection_roots)(mark); base_marks=marks;
    assert(base_marks==(fixture_vector_v2?6:8));
    sort_case("stable",0,0,0,0); sort_case("nested precise GC",1,0,0,0);
    sort_case("null key cleanup",0,1,0,0); sort_case("fresh key and comparator lookup",0,0,1,0);
    sort_case("exact true only",0,0,0,1);
    Oop empty=methods[11](type,NULL,0),single=fixture_vector->push(empty,api->nil_obj);
    assert(methods[10](empty,&block,1)==empty && methods[10](single,&block,1)==single); check_roots();
    assert(fixture_integer_value(methods[0](input,NULL,0))==6);
    for(int64_t index=-1;index<=6;index++) {
        Oop arg=fixture_integer_make(index),result=methods[1](input,&arg,1);
        assert(index<0 || index==6 ? result==api->nil_obj : fixture_integer_value(result)==values[index]);
    }
    Oop bad=api->true_obj; assert(methods[1](input,&bad,1)==api->nil_obj);
    Oop bounds[]={fixture_integer_make(1),fixture_integer_make(4)}; const int64_t slice[]={2,4,1};
    expect(methods[8](input,bounds,2),slice,3);
    bounds[1]=fixture_integer_make(7); assert(methods[8](input,bounds,2)==api->nil_obj);
    const int64_t reversed[]={0,3,1,4,2,5}; expect(methods[9](input,NULL,0),reversed,6);
    Oop inherited=api->alloc(api->proto(type),sizeof(FixtureVectorPayload));
    memcpy(inherited,input,sizeof(FixtureVectorPayload));
    assert(fixture_integer_value(methods[0](inherited,NULL,0))==6);
    expect(methods[9](inherited,NULL,0),reversed,6);
    assert(methods[6](input,&inherited,1)==api->nil_obj);
    Oop joined=methods[6](empty,&input,1); expect(joined,values,6);
    assert(methods[6](input,&bad,1)==api->nil_obj);
    Oop with_null=methods[2](input,(Oop[]){NULL},1); assert(fixture_vector->count(with_null)==7);
    assert(methods[1](with_null,(Oop[]){fixture_integer_make(6)},1)==api->nil_obj);
    Oop printed=methods[5](input,NULL,0); size_t length=0; const uint8_t *bytes=fixture_string->bytes(fixture_string->context,printed,&length);
    assert(length==14 && !memcmp(bytes,"#[5 2 4 1 3 0]",14));
    api->method(block_type,api->intern("value:"),visit); visits=0; trace=1;
    assert(methods[3](input,&block,1)==input && visits==6); trace=0;
    api->method(block_type,api->intern("value:value:"),sum); Oop arguments[]={fixture_integer_make(10),block};
    assert(fixture_integer_value(methods[4](input,arguments,2))==25);
    api->method(block_type,api->intern("value:value:"),null_sum); visits=0;
    assert(methods[4](input,arguments,2)==api->nil_obj && visits==1);
    /* Direct Map iteration and retained Methods ignore public Vector replacements. */
    Oop map=fixture_namespace_import("Map"),pair[]={fixture_integer_make(1),fixture_integer_make(7)};
    map=api->send(map,api->intern("at:put:"),pair,2);
    pair[0]=fixture_integer_make(2); pair[1]=fixture_integer_make(8); map=api->send(map,api->intern("at:put:"),pair,2);
    api->method(type,api->intern("do:"),forbidden); api->method(type,api->intern("inject:into:"),forbidden);
    visits=0; assert(send1(map,"do:",block)==map && visits==2);
    api->method(block_type,api->intern("value:value:"),sum);
    assert(fixture_integer_value(api->send(map,api->intern("inject:into:"),arguments,2))==25);
    api->method(type,api->intern("size"),forbidden); assert(fixture_integer_value(methods[0](input,NULL,0))==6);
    IdApi other=selected; other.nil_obj=block; mount(argv[3],&other);
    if(fixture_vector_v2) {
        Method comma=fixture_vector_v2->install(&other,api->nil_obj,api->nil_obj,api->nil_obj);
        assert(comma==methods[6] && fixture_vector_v2->for_each==methods[3] && fixture_vector_v2->inject==methods[4]);
    }
    assert(methods[0](input,NULL,1)==api->nil_obj);
    for(size_t i=0;i<13;i++) assert(lookup(type,names[i])==methods[i]);
    api->method(block_type,api->intern("value:"),visit); visits=0;
    assert(methods[3](input,&block,1)==input && visits==6); check_roots();
    puts("vector-methods: 13 bindings, copied methods, stable sorting, nested precise GC, root cleanup, live callbacks, Map native iteration and reload passed");
}
