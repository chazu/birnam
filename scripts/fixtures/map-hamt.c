#include "idapi_v18.h"
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <inttypes.h>
#include <math.h>
#include <stdlib.h>

typedef struct { Oop root; size_t count; } Map;
typedef struct { uint32_t tag,n,hash,pad; } Header;
static IdApi *api,selected;
static Oop type,node_type,prototype,active,versions[4],kept[64],inner,inner_key,inner_value,mutated_source,new_root;
static size_t kept_count,allocations,reads,entries;
static Method new_method,put_method,at_method;
static int capture,probe,reenter_alloc,mutate_count,mutate_root,callback_gc;
static int64_t (*integer_read)(void *,Oop);
static Oop send0(Oop self,const char *name) { return api->send(self,api->intern(name),NULL,0); }
static Oop send1(Oop self,const char *name,Oop arg) { return api->send(self,api->intern(name),&arg,1); }
static Method lookup(Oop self,const char *name) { return (Method)send1(self,"lookup",api->intern(name)); }
static void roots(MarkFn mark) {
    mark(active); mark(inner); mark(inner_key); mark(inner_value); mark(mutated_source); mark(new_root);
    for(size_t i=0;i<4;i++) mark(versions[i]);
    for(size_t i=0;i<kept_count;i++) mark(kept[i]);
}
static Oop keep(Oop value) { assert(kept_count<64); kept[kept_count++]=value; return value; }
static Oop empty(void) { return fixture_map?fixture_map->empty():new_method(prototype,NULL,0); }
static Oop put(Oop object,Oop key,Oop value) { return fixture_map?fixture_map->put(&selected,object,key,value):put_method(object,(Oop[]){key,value},2); }
static Oop get(Oop object,Oop key) { return fixture_map?fixture_map->find(&selected,object,key):at_method(object,&key,1); }
static Oop allocate(Oop behavior,size_t size) {
    if(probe) {
        assert(behavior==type || behavior==node_type); allocations++;
        assert(behavior==type?size==16:(size>=32 && (size-16)%16==0));
        Dl_info owner; assert(dladdr(__builtin_return_address(0),&owner));
        assert(strstr(owner.dli_fname,(fixture_map || fixture_map_v2)?"/libbirnam_map.dylib":"_collection.dylib"));
    }
    if(capture) printf("alloc %s %zu\n",behavior==type?"Map":"Node",size);
    if(behavior==type && reenter_alloc) {
        reenter_alloc=0; puts("nested overwrite during Map allocation");
        inner=put(inner,inner_key,inner_value);
    }
    if(behavior==type && mutate_count) { ((Map *)mutated_source)->count=999; mutate_count=0; }
    if(callback_gc) api->gc_collect();
    Oop result=api->alloc(behavior,size);
    if(behavior==type || behavior==node_type) memset(result,0xA5,size);
    return result;
}
static int64_t read_integer(void *context,Oop value) {
    int64_t result=integer_read(context,value); reads++;
    if(capture) printf("integer %" PRId64 "\n",result);
    if(mutate_root) { mutate_root=0; ((Map *)mutated_source)->root=new_root; }
    if(callback_gc) api->gc_collect();
    return result;
}
static void mount(const char *path,IdApi *table) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image,fixture_root_registry());
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init); init(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks());
}
static uint32_t hash(uint64_t input) {
    uint64_t x=input+UINT64_C(0x9e3779b97f4a7c15); x^=x>>30; x*=UINT64_C(0xbf58476d1ce4e5b9);
    x^=x>>27; x*=UINT64_C(0x94d049bb133111eb); x^=x>>31; return (uint32_t)x;
}
static size_t slots(Header h) { return h.tag==0?2*(size_t)__builtin_popcount(h.n):2*(size_t)h.n; }
static Oop *data(Oop node) { return (Oop *)((uint8_t *)node+16); }
static uint64_t digest(Oop object) {
    if(!object) return 0;
    Header h=*(Header *)object; assert((h.tag==0 || h.tag==1) && h.pad==0);
    size_t n=slots(h); uint64_t result=h.n;
    if(fixture_map) assert(fixture_map->slot_count(object)==n);
    for(size_t i=0;i<n;i+=2) {
        Oop key=data(object)[i],value=data(object)[i+1];
        if(fixture_map) assert(fixture_map->slot(object,i)==key && fixture_map->slot(object,i+1)==value);
        if(key) { assert(value); entries++; result=(result<<1)^(uintptr_t)key^(uintptr_t)value; }
        else result=(result<<1)^digest(value);
    }
    return result;
}
static void verify(Oop object,size_t count) {
    entries=0; (void)digest(((Map *)object)->root); assert(entries==count && ((Map *)object)->count==count);
    if(fixture_map) assert(fixture_map->root(object)==((Map *)object)->root && fixture_map->count(object)==count);
}
static void begin(const char *name) { printf("case %s\n",name); capture=probe=1; allocations=reads=0; }
static void end(void) { capture=probe=0; printf("done allocations=%zu reads=%zu\n",allocations,reads); }
typedef struct { uint32_t bits,number; } Candidate;
static int compare(const void *aa,const void *bb) {
    const Candidate *a=aa,*b=bb; uint32_t x=a->bits&0x3fffffff,y=b->bits&0x3fffffff;
    if(x!=y) return x<y?-1:1; return a->number<b->number?-1:(a->number!=b->number);
}
static Oop forbidden(Oop self,const Oop *args,size_t count) { (void)self; (void)args; (void)count; assert(!"native Map operation used public dispatch"); return NULL; }
int main(int argc,char **argv) {
    setvbuf(stdout,NULL,_IONBF,0);
    assert(argc==4); void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL); assert(floor);
    api=((IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")))(); assert(api); fixture_set_auto(0);
    fixture_mount_trace(api,argv[1],fixture_root_registry()); fixture_mount_integer(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());
    uintptr_t anchor=0; api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor)); mount(argv[2],api);
    selected=*api; selected.alloc=allocate; mount(argv[3],&selected); fixture_register_root(roots);
    type=fixture_namespace_import("MapVt"); node_type=fixture_namespace_import("MapNode"); prototype=fixture_namespace_import("Map");
    new_method=lookup(type,"new"); put_method=lookup(type,"at:put:"); at_method=lookup(type,"at:");
    Method methods[]={new_method,put_method,at_method};
    for(size_t i=0;i<3;i++) { Dl_info owner; assert(dladdr((const void *)methods[i],&owner) && strstr(owner.dli_fname,fixture_map_v2?"/libbirnam_map.dylib":"_collection.dylib")); }
    integer_read=fixture_integer->read; fixture_integer->read=read_integer;
    active=empty(); const size_t retained[]={32,33,1024,4096};
    for(size_t i=0;i<4096;i++) {
        Oop old=active; Map previous=*(Map *)old; uint64_t before=digest(previous.root);
        Oop key=fixture_integer_make((int64_t)i),value=fixture_integer_make((int64_t)(2*i));
        allocations=0; probe=1; active=put(old,key,value); probe=0;
        assert(!memcmp(&previous,old,sizeof(previous)) && digest(previous.root)==before);
        assert(active!=old && ((Map *)active)->root!=previous.root && allocations>=2);
        for(size_t j=0;j<4;j++) if(i+1==retained[j]) versions[j]=active;
    }
    for(size_t j=0;j<4;j++) verify(versions[j],retained[j]);
    for(size_t i=0;i<4096;i++) assert(fixture_integer_value(get(active,fixture_integer_make((int64_t)i)))==(int64_t)(2*i));
    Oop key=keep(fixture_integer_make(17)),same=get(active,key);
    begin("identical update"); assert(put(active,key,same)==active); end(); assert(!allocations);
    Oop value=keep(fixture_integer_make(999)); begin("copied update"); Oop replaced=keep(put(active,key,value)); end(); verify(replaced,4096);
    assert(fixture_integer_value(get(active,key))==34 && get(replaced,key)==value);
    Header head=*(Header *)((Map *)active)->root; assert(head.tag==0); size_t shared=0;
    for(size_t i=0;i<slots(head);i+=2) if(data(((Map *)active)->root)[i+1]==data(((Map *)replaced)->root)[i+1]) shared++;
    assert(shared>=30);
    /* Find two real Integer keys sharing 30 low hash bits, independently. */
    size_t count=160000; Candidate *c=malloc(count*sizeof(*c)); assert(c);
    for(size_t i=0;i<count;i++) c[i]=(Candidate){hash(i),(uint32_t)i}; qsort(c,count,sizeof(*c),compare);
    size_t found=0; for(size_t i=1;i<count;i++) if((c[i-1].bits&0x3fffffff)==(c[i].bits&0x3fffffff) && c[i-1].bits!=c[i].bits) { found=i; break; } assert(found);
    Oop first=keep(fixture_integer_make(c[found-1].number)),second=keep(fixture_integer_make(c[found].number)); free(c);
    Oop deep=keep(put(empty(),first,api->true_obj)); begin("30 shared hash bits"); deep=keep(put(deep,second,api->false_obj)); end(); verify(deep,2);
    Oop node=((Map *)deep)->root; size_t levels=0;
    while(data(node)[0]==NULL) { assert(((Header *)node)->tag==0 && slots(*(Header *)node)==2); node=data(node)[1]; levels++; }
    assert(levels==6 && slots(*(Header *)node)==4 && get(deep,first)==api->true_obj && get(deep,second)==api->false_obj);
    /* Distinct NaNs share all hash bits and compare unequal; identity is retained. */
    Oop collision=empty(),nans[8];
    for(size_t i=0;i<8;i++) {
        nans[i]=keep(fixture_float_make(NAN));
        if(i==1 || i==7) begin("collision append");
        collision=put(collision,nans[i],fixture_integer_make((int64_t)i));
        if(i==1 || i==7) end();
    }
    collision=keep(collision); verify(collision,8);
    begin("collision replacement"); Oop updated=keep(put(collision,nans[3],value)); end(); verify(updated,8);
    assert(get(updated,nans[3])==value && fixture_integer_value(get(collision,nans[3]))==3);
    begin("collision identical value"); assert(put(updated,nans[3],value)==updated); end(); assert(!allocations);
    double nan_value=fixture_float->read(fixture_float->context,nans[0]); uint64_t nan_bits; memcpy(&nan_bits,&nan_value,8);
    uint64_t collision_number=0; while((hash(collision_number)&1023)!=(hash(nan_bits)&1023) || hash(collision_number)==hash(nan_bits)) collision_number++;
    Oop collision_key=keep(fixture_integer_make((int64_t)collision_number));
    begin("collision with different hash sharing ten bits"); Oop mixed=keep(put(collision,collision_key,value)); end(); verify(mixed,9);
    for(size_t i=0;i<8;i++) assert(fixture_integer_value(get(mixed,nans[i]))==(int64_t)i);
    Oop projection=send0(deep,"keys"); assert(fixture_vector->count(projection)==2);
    assert(fixture_vector->at(projection,0)==data(node)[0] && fixture_vector->at(projection,1)==data(node)[2]);
    projection=send0(deep,"values"); assert(fixture_vector->at(projection,0)==data(node)[1] && fixture_vector->at(projection,1)==data(node)[3]);
    Oop printable=put(put(empty(),first,key),second,value);
    Oop printed=send0(printable,"printString"); size_t length=0; const uint8_t *bytes=fixture_string_bytes(printed,&length);
    printf("deep print %.*s\n",(int)length,bytes);
    Oop nil_map=keep(put(empty(),key,api->nil_obj)); assert(send1(nil_map,"includesKey:",key)==api->true_obj && at_method(nil_map,&key,1)==api->nil_obj);
    assert(put_method(active,NULL,2)==api->nil_obj && put_method(active,(Oop[]){NULL,value},2)==api->nil_obj && put_method(active,(Oop[]){key,NULL},2)==api->nil_obj);
    Oop inherited=keep(api->alloc(api->proto(type),16)); *(Map *)inherited=*(Map *)active;
    Oop extended=keep(put(inherited,fixture_integer_make(5000),value)); verify(extended,4097); assert(api->vtable_of(extended)==type);
    /* Root lookup is snapshotted before the hash callback. */
    Oop original=keep(put(empty(),key,api->true_obj)),replacement=keep(put(empty(),key,api->false_obj));
    mutated_source=keep(api->alloc(type,16)); *(Map *)mutated_source=*(Map *)original; new_root=((Map *)replacement)->root; mutate_root=1;
    begin("root snapshot before hash"); assert(get(mutated_source,key)==api->true_obj); end(); assert(((Map *)mutated_source)->root==new_root);
    /* Old count is read before the final allocation callback. */
    mutated_source=keep(empty()); mutate_count=1; begin("count snapshot before allocation"); Oop counted=keep(put(mutated_source,key,value)); end(); assert(((Map *)counted)->count==1 && ((Map *)mutated_source)->count==999);
    inner_key=keep(fixture_integer_make(80)); inner=keep(put(empty(),inner_key,api->true_obj)); inner_value=api->false_obj;
    Oop outer=keep(empty()); reenter_alloc=1; begin("shared added flag under reentry"); Oop reentrant=keep(put(outer,key,value)); end();
    assert(((Map *)reentrant)->count==0 && get(reentrant,key)==value && ((Map *)inner)->count==1);
    callback_gc=1; begin("live allocator and hash callback GC"); Oop live=keep(put(active,fixture_integer_make(6000),value)); end(); callback_gc=0; verify(live,4097);
    if(fixture_map || fixture_map_v2) {
        const char *names[]={"new","at:","at:put:","size"};
        for(size_t i=0;i<4;i++) api->method(type,api->intern(names[i]),forbidden);
        Oop direct=put(empty(),key,value); assert(get(direct,key)==value && ((Map *)direct)->count==1);
    }
    IdApi other=selected; other.nil_obj=value; mount(argv[3],&other); assert(put_method(active,NULL,1)==api->nil_obj);
    fixture_set_precise(1); api->gc_collect(); fixture_set_precise(0);
    for(size_t i=0;i<4;i++) verify(versions[i],retained[i]); verify(deep,2); verify(collision,8); verify(mixed,9);
    assert(fixture_integer_value(get(active,key))==34 && get(replaced,key)==value);
    fixture_integer->read=integer_read;
    puts("map-hamt: 4096 persistent entries, shared paths, deep hashes, collisions, native readers, callback order, reentry, GC and reload passed");
}
