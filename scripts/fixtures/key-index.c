/* Observe the real loaded dictionary's index, not a standalone substitute. */
#define _DARWIN_C_SOURCE
#include "../../abi/fixtures/idapi-v20/idapi_v20.h"
#include "../../abi/fixtures/key-index-v1/key_index_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>
static const KeyIndexFactoryV1 *factory;
static KeyIndexFactoryV1 watched;
static void *owner;
static unsigned operations,inserts,clears,map_reads,allocations;
static int tracking;
static uintptr_t last_slot;
static void insert(void **slot,uintptr_t key,uintptr_t position) {
    operations++;
    if(tracking) { inserts++;allocations+=!*slot;if(owner)assert(*slot==owner);last_slot=(uintptr_t)slot; }
    factory->insert(slot,key,position);
    if(tracking) { assert(*slot);if(!owner)owner=*slot;assert(owner==*slot); }
}
static int is_empty(const void *context) { operations++;return factory->is_empty(context); }
static int get(const void *context,uintptr_t key,uintptr_t *out) {
    operations++;if(tracking && context==owner)map_reads++;return factory->get(context,key,out);
}
static void clear(void *context) { operations++;if(tracking){clears++;if(context)assert(context==owner);}factory->clear(context); }
static void destroy(void *context) { operations++;factory->destroy(context); }
static const KeyIndexFactoryV1 *provide(void) { return &watched; }
static void *capture_dlsym(void *image,const char *name) {
    void *symbol=dlsym(image,name);
    if(!strcmp(name,"birnam_key_index_factory_v1")) {
        assert(symbol);factory=((KeyIndexFactoryProviderV1)symbol)();watched=*factory;
        watched.insert=insert;watched.is_empty=is_empty;watched.get=get;watched.clear=clear;watched.destroy=destroy;
        return (void *)provide;
    }
    return symbol;
}
#define dlsym capture_dlsym
#include "namespace-support.h"
#undef dlsym
static IdApi *api;
static Oop identity(Oop self,const Oop *args,size_t count) { (void)args;assert(!count);return self; }
static Oop nil(Oop self,const Oop *args,size_t count) { (void)self;(void)args;assert(!count);return api->nil_obj; }
static void reset(void) { inserts=clears=map_reads=allocations=0; }
static void verify(const Oop *keys,const unsigned *order,unsigned length) {
    uintptr_t output=91;
    for(unsigned i=0;i<length;i++) { assert(factory->get(owner,(uintptr_t)keys[order[i]],&output)==1);assert(output==i); }
}
int main(int argc,char **argv) {
    assert(argc==2);void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);assert(!operations);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api && operations);fixture_set_auto(0);
    Dl_info library;assert(dladdr((void *)factory->get,&library) && strstr(library.dli_fname,"libbirnam_object_model.dylib"));
    Oop keys[24];unsigned order[24];char text[64];
    for(unsigned i=0;i<24;i++) { snprintf(text,sizeof(text),"keyIndexLoaded%u",i);keys[i]=api->intern(text);order[i]=i; }
    Oop remove=api->intern("removeMethod:");
    tracking=1;reset();Oop parent=api->proto(api->object_vt),child=api->proto(parent),receiver=api->alloc(child,0);
    for(unsigned i=0;i<16;i++)api->method(parent,keys[i],identity);
    assert(!inserts && !clears && !allocations && !owner);
    puts("key-index loaded: empty and 1..16 keys allocate no owner");
    reset();api->method(parent,keys[16],identity);assert(inserts==17 && clears==1 && allocations==1 && owner);verify(keys,order,17);
    reset();api->method(parent,keys[17],identity);assert(inserts==1 && !clears && !allocations);verify(keys,order,18);
    puts("key-index loaded: 17 keys build the complete map, 18 appends once");
    reset();api->method(parent,keys[7],nil);assert(!inserts && !clears && map_reads);
    for(unsigned i=0;i<3;i++)assert(api->send(receiver,keys[7],NULL,0)==api->nil_obj);
    api->method(parent,keys[7],identity);assert(api->send(receiver,keys[7],NULL,0)==receiver);
    assert(!inserts && !clears);puts("key-index loaded: overwrite reads the current Method");
    reset();api->send(parent,remove,&keys[23],1);assert(!inserts && !clears);
    reset();api->send(parent,remove,&keys[0],1);assert(inserts==17 && clears==1 && !allocations);
    for(unsigned i=0;i<17;i++)order[i]=i+1;verify(keys,order,17);
    reset();api->send(parent,remove,&keys[9],1);assert(!inserts && clears==1 && !allocations && factory->is_empty(owner));
    for(unsigned i=0,j=0;i<17;i++)if(order[i]!=9)order[j++]=order[i];
    reset();for(unsigned i=0;i<16;i++)assert(api->send(receiver,keys[order[i]],NULL,0)==receiver);assert(!map_reads);
    reset();api->method(parent,keys[0],identity);order[16]=0;assert(inserts==17 && clears==1 && !allocations);verify(keys,order,17);
    puts("key-index loaded: removals rebuild positions, shrink scans, regrowth reuses ownership");
    uintptr_t previous_slot=last_slot;
    tracking=0;for(unsigned i=0;i<4096;i++)api->proto(api->object_vt);tracking=1;
    reset();api->method(parent,keys[22],identity);order[17]=22;
    assert(inserts==1 && !allocations && last_slot!=previous_slot);verify(keys,order,18);
    assert(api->send(receiver,keys[22],NULL,0)==receiver);
    puts("key-index loaded: actual registry movement preserves the owner without retaining slot addresses");
}
