#include "idapi_v7.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <inttypes.h>

typedef struct { Oop root; size_t count; } Map;
typedef struct { uint32_t tag,n,hash,pad; Oop items[6]; } Node;
static IdApi *api,selected;
static Oop type,node_type,block_type,block,piece_type,subject,leaf,alternate,empty_map,pieces[8],kept[32];
static size_t kept_count,marks,visits,allocations,make_calls,bytes_calls;
static int capture,mutation,allocation_mutation,null_result,null_bytes,null_make,changed_bytes,reentry,make_offset;
static RootProvider owner_roots;
static const char *names[]={"new","clone","at:put:","at:","at:ifAbsent:","includesKey:","size","keys","values","do:","inject:into:","keysAndValuesDo:","printString"};
static const size_t arities[]={0,0,2,1,2,1,0,0,0,1,2,1,0};
static Method methods[13];
static Oop (*original_make)(void *,const uint8_t *,size_t);
static const uint8_t *(*original_bytes)(void *,Oop,size_t *);
static Oop (*original_integer_make)(void *,int64_t);
static Oop send1(Oop self,const char *name,Oop value) { return api->send(self,api->intern(name),&value,1); }
static Method lookup(Oop self,const char *name) { return (Method)send1(self,"lookup",api->intern(name)); }
static Oop keep(Oop value) { assert(kept_count<32); kept[kept_count++]=value; return value; }
static void roots(MarkFn mark) {
    mark(subject); mark(leaf); mark(alternate); mark(empty_map); mark(block); mark(piece_type);
    for(size_t i=0;i<8;i++) mark(pieces[i]); for(size_t i=0;i<kept_count;i++) mark(kept[i]);
}
static void mark(Oop value) { assert(value); marks++; }
static void register_roots(RootProvider provider) {
    Dl_info owner; assert(dladdr((const void *)provider,&owner));
    if(strstr(owner.dli_fname,"/libbirnam_map.dylib")) owner_roots=provider;
    api->gc_register_root_provider(provider);
}
static int identity(Oop object) { for(int i=0;i<8;i++) if(pieces[i]==object) return i; return -1; }
static void reset(void) {
    *(Node *)leaf=(Node){0,7,0,0,{pieces[0],pieces[1],pieces[2],pieces[3],pieces[4],pieces[5]}};
    *(Node *)alternate=(Node){0,1,0,0,{pieces[6],pieces[7],NULL,NULL,NULL,NULL}};
    *(Map *)subject=(Map){leaf,3};
    mutation=allocation_mutation=null_result=null_bytes=null_make=changed_bytes=reentry=0;
    visits=allocations=make_calls=bytes_calls=0;
}
static Oop allocate(Oop behavior,size_t size) {
    if(capture) { allocations++; printf("alloc %zu\n",size); }
    if(allocation_mutation==1) { allocation_mutation=0; ((Map *)subject)->root=alternate; }
    else if(allocation_mutation==2 && allocations==4) {
        ((Node *)leaf)->n=1; ((Node *)leaf)->items[2]=pieces[6]; ((Node *)leaf)->items[3]=pieces[7]; allocation_mutation=0;
    }
    return api->alloc(behavior,size);
}
static Oop send(Oop self,Oop selector,const Oop *args,size_t count) {
    if(capture) printf("send %s %d %zu\n",api->symbol_name(selector),identity(self),count);
    return api->send(self,selector,args,count);
}
static Oop make(void *context,const uint8_t *bytes,size_t length) {
    make_calls++; if(capture) printf("make %zu\n",length);
    if(null_make) return NULL;
    return original_make(context,bytes,length);
}
static const uint8_t *bytes(void *context,Oop object,size_t *length) {
    bytes_calls++; if(capture) puts("bytes");
    if(null_bytes) { *length=1; return NULL; }
    if(changed_bytes) { *length=1; return (const uint8_t *)"x"; }
    return original_bytes(context,object,length);
}
static Oop integer_make(void *context,int64_t value) { return original_integer_make(context,value+make_offset); }
static Oop replacement_visit(Oop self,const Oop *args,size_t count);
static Oop visit(Oop self,const Oop *args,size_t count) {
    assert(self==block && args && (count==1 || count==2));
    printf("visit"); for(size_t i=0;i<count;i++) printf(" %d",identity(args[i])); putchar('\n'); visits++;
    if(mutation && visits==1) {
        ((Node *)leaf)->n=1; ((Node *)leaf)->items[2]=pieces[6]; ((Node *)leaf)->items[3]=pieces[7];
        api->method(block_type,api->intern(count==1?"value:":"value:value:"),replacement_visit);
    }
    if(reentry && visits==1) {
        reentry=0; Oop projected=methods[7](subject,NULL,0); assert(fixture_vector->count(projected)==(mutation?1u:3u)); api->gc_collect();
    }
    if(null_result) return NULL;
    return count==2?args[1]:args[0];
}
static Oop replacement_visit(Oop self,const Oop *args,size_t count) { puts("replacement visit"); return visit(self,args,count); }
static Oop fallback(Oop self,const Oop *args,size_t count) { assert(self==block && !args && !count); visits++; return null_result?NULL:pieces[7]; }
static Oop print_replacement(Oop self,const Oop *args,size_t count);
static Oop print(Oop self,const Oop *args,size_t count) {
    assert(!args && !count); int id=identity(self); assert(id>=0); visits++; printf("print %d\n",id);
    if(mutation && visits==1) {
        ((Node *)leaf)->n=1; ((Node *)leaf)->items[1]=pieces[7]; ((Node *)leaf)->items[2]=pieces[6];
        api->method(piece_type,api->intern("printString"),print_replacement);
    }
    if(reentry && visits==1) { reentry=0; Oop projected=methods[8](subject,NULL,0); assert(fixture_vector->count(projected)==(mutation?1u:3u)); api->gc_collect(); }
    if(null_result) return NULL;
    uint8_t text[2]={'p',(uint8_t)('0'+id)}; return original_make(fixture_string->context,text,2);
}
static Oop print_replacement(Oop self,const Oop *args,size_t count) { puts("replacement print"); return print(self,args,count); }
static Oop forbidden(Oop self,const Oop *args,size_t count) { (void)self; (void)args; (void)count; assert(!"private operation used public lookup or reloaded selectors"); return NULL; }
static void mount(const char *path,IdApi *table) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init); init(table,NULL);
}
static void expect_vector(Oop object,const int *indices,size_t count) {
    assert(fixture_vector->count(object)==count);
    for(size_t i=0;i<count;i++) assert(fixture_vector->at(object,i)==pieces[indices[i]]);
}
static void expect_print(Oop object,const char *text) {
    size_t length=0; const uint8_t *content=original_bytes(fixture_string->context,object,&length);
    assert(length==strlen(text) && !memcmp(content,text,length));
}
static void begin(const char *name) { printf("case %s\n",name); capture=1; }
static void end(void) { capture=0; }
int main(int argc,char **argv) {
    assert(argc==4); void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); api->gc_set_auto(0);
    fixture_mount_trace(api,argv[1]); fixture_mount_integer(api,argv[1]);
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor)); mount(argv[2],api);
    selected=*api; selected.alloc=allocate; selected.send=send; selected.gc_register_root_provider=register_roots; mount(argv[3],&selected);
    type=api->import_fn("MapVt"); node_type=api->import_fn("MapNode");
    for(size_t i=0;i<13;i++) {
        methods[i]=lookup(type,names[i]); assert(methods[i]); Dl_info owner; assert(dladdr((const void *)methods[i],&owner));
        assert(strstr(owner.dli_fname,fixture_map_v2?"/libbirnam_map.dylib":"_collection.dylib"));
    }
    assert(methods[0]==methods[1]); api->gc_register_root_provider(roots);
    block_type=api->proto(api->object_vt); block=api->alloc(block_type,0); piece_type=api->proto(api->object_vt);
    for(size_t i=0;i<8;i++) pieces[i]=api->alloc(piece_type,0);
    api->method(block_type,api->intern("value"),fallback);
    api->method(block_type,api->intern("value:"),visit); api->method(block_type,api->intern("value:value:"),visit);
    api->method(piece_type,api->intern("printString"),print);
    empty_map=methods[0](type,NULL,0); subject=api->alloc(type,16); leaf=api->alloc(node_type,sizeof(Node)); alternate=api->alloc(node_type,sizeof(Node)); reset();
    original_make=fixture_string->make; original_bytes=fixture_string->bytes; fixture_string->make=make; fixture_string->bytes=bytes;
    original_integer_make=fixture_integer->make; fixture_integer->make=integer_make;
    for(size_t i=0;i<13;i++) {
        assert(methods[i](subject,(Oop[]){pieces[0],pieces[1],pieces[2]},arities[i]+1)==api->nil_obj);
        if(arities[i] && i!=9) assert(methods[i](subject,NULL,arities[i])==api->nil_obj);
    }
    begin("do null carrier still projects"); allocations=0; assert(methods[9](subject,NULL,1)==subject); end(); assert(allocations==9);
    begin("inject null carrier still projects"); allocations=0; assert(methods[10](subject,NULL,2)==api->nil_obj); end(); assert(allocations==9);
    Oop table=keep(methods[2](empty_map,(Oop[]){pieces[0],pieces[1]},2));
    assert(methods[3](table,&pieces[0],1)==pieces[1]); visits=0;
    assert(methods[4](table,(Oop[]){pieces[0],block},2)==pieces[1] && !visits);
    assert(methods[4](table,(Oop[]){pieces[2],block},2)==pieces[7] && visits==1);
    null_result=1; assert(methods[4](table,(Oop[]){pieces[2],block},2)==NULL); null_result=0;
    selected.true_obj=pieces[6]; selected.false_obj=pieces[7];
    assert(methods[5](table,&pieces[0],1)==pieces[6] && methods[5](table,&pieces[2],1)==pieces[7]);
    selected.true_obj=api->true_obj; selected.false_obj=api->false_obj;
    make_offset=100; assert(fixture_integer_value(methods[6](table,NULL,0))==101); make_offset=0;
    reset(); allocation_mutation=1; begin("projection root after empty allocation"); expect_vector(methods[7](subject,NULL,0),(int[]){6},1); end();
    reset(); allocation_mutation=2; begin("keys header snapshot and later slot reads"); expect_vector(methods[7](subject,NULL,0),(int[]){0,6,4},3); end();
    reset(); allocation_mutation=2; begin("values snapshot and later slot reads"); expect_vector(methods[8](subject,NULL,0),(int[]){1,7,5},3); end();
    /* The actual Vector bodies and Map's private projection ignore public overrides. */
    const char *vector_names[]={"new","add:","do:","inject:into:"}; Method vector_methods[4];
    for(size_t i=0;i<4;i++) { vector_methods[i]=lookup(fixture_vector->behavior(),vector_names[i]); api->method(fixture_vector->behavior(),api->intern(vector_names[i]),forbidden); }
    api->method(type,api->intern("values"),forbidden);
    reset(); mutation=reentry=null_result=1; begin("do preprojects values and keeps fresh block lookup"); assert(methods[9](subject,&block,1)==subject && visits==3); end();
    api->method(block_type,api->intern("value:"),visit);
    reset(); mutation=1; begin("inject preprojects values"); assert(methods[10](subject,(Oop[]){pieces[0],block},2)==pieces[5] && visits==3); end();
    api->method(block_type,api->intern("value:value:"),visit);
    reset(); null_result=1; begin("inject null result stops"); assert(methods[10](subject,(Oop[]){pieces[0],block},2)==api->nil_obj && visits==1); end();
    reset(); mutation=reentry=null_result=1; begin("pairs retain header and reread later slots"); assert(methods[11](subject,&block,1)==subject && visits==3); end();
    api->method(block_type,api->intern("value:value:"),visit);
    for(size_t i=0;i<4;i++) api->method(fixture_vector->behavior(),api->intern(vector_names[i]),vector_methods[i]);
    api->method(type,api->intern("values"),methods[8]);
    reset(); mutation=reentry=1; begin("print snapshots pair and rereads later key"); expect_print(methods[12](subject,NULL,0),"{p0 -> p1, p6 -> p3, p4 -> p5}"); end(); assert(make_calls==1 && bytes_calls==6);
    api->method(piece_type,api->intern("printString"),print);
    reset(); null_result=1; assert(methods[12](subject,NULL,0)==api->nil_obj && !make_calls);
    reset(); null_bytes=1; assert(methods[12](subject,NULL,0)==api->nil_obj && !make_calls && bytes_calls==1);
    reset(); changed_bytes=1; expect_print(methods[12](subject,NULL,0),"{x -> x, x -> x, x -> x}");
    reset(); null_make=1; assert(methods[12](subject,NULL,0)==NULL && make_calls==1); reset();
    expect_print(methods[12](empty_map,NULL,0),"{}");
    marks=0; assert(owner_roots); owner_roots(mark); assert(marks==(fixture_map_v2?6:3));
    IdApi other=selected; other.nil_obj=pieces[7]; mount(argv[3],&other);
    if(fixture_map_v2) fixture_map_v2->install(&other,pieces[0],pieces[0],pieces[0]);
    assert(methods[6](subject,NULL,1)==api->nil_obj);
    for(size_t i=0;i<13;i++) assert(lookup(type,names[i])==methods[i]);
    assert(methods[4](table,(Oop[]){pieces[2],block},2)==pieces[7]);
    reset(); visits=0; assert(methods[11](subject,&block,1)==subject && visits==3); expect_print(methods[12](empty_map,NULL,0),"{}");
    api->gc_set_precise(1); api->gc_collect(); api->gc_set_precise(0); marks=0; owner_roots(mark); assert(marks==(fixture_map_v2?6:3));
    fixture_string->make=original_make; fixture_string->bytes=original_bytes; fixture_integer->make=original_integer_make;
    puts("map-methods: 13 bindings, null contracts, projection/traversal timing, fresh dispatch, native Vector calls, live operations, GC and reload passed");
}
