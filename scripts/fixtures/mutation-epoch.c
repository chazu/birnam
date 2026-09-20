/* Observe the actual loaded Vtables owner; only Id advances its epoch. */
#define _DARWIN_C_SOURCE
#include "../../abi/fixtures/idapi-v20/idapi_v20.h"
#include "../../abi/fixtures/mutation-epoch-v1/mutation_epoch_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>
static const MutationEpochFactoryV1 *factory;
static MutationEpochFactoryV1 watched;
static void *owner;
static unsigned creates,reads,writes;
static void *create(void) {
    assert(!creates++);owner=factory->create();assert(owner && factory->read(owner)==0);return owner;
}
static uint64_t read(const void *context) { assert(context==owner);reads++;return factory->read(context); }
static void write(void *context,uint64_t epoch) {
    assert(context==owner);uint64_t before=factory->read(context);
    assert(epoch==(before==UINT64_MAX?before:before+1));writes++;factory->write(context,epoch);
}
static const MutationEpochFactoryV1 *provide(void) { return &watched; }
static void *capture_dlsym(void *image,const char *name) {
    void *symbol=dlsym(image,name);
    if(!strcmp(name,"birnam_mutation_epoch_factory_v1")) {
        assert(symbol);factory=((MutationEpochFactoryProviderV1)symbol)();watched=*factory;
        watched.create=create;watched.read=read;watched.write=write;return (void *)provide;
    }
    return symbol;
}
#define dlsym capture_dlsym
#include "namespace-support.h"
#undef dlsym
static IdApi *api;
static Oop identity(Oop self,const Oop *args,size_t count) { (void)args;assert(!count);return self; }
static Oop nil(Oop self,const Oop *args,size_t count) { (void)self;(void)args;assert(!count);return api->nil_obj; }
static uint64_t reset(void) { reads=writes=0;return factory->read(owner); }
static void advanced(uint64_t before,const char *label) {
    assert(writes==1 && reads>=1 && factory->read(owner)==before+1);printf("epoch loaded %s: exactly one advance\n",label);
}
int main(int argc,char **argv) {
    assert(argc==2);void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);assert(!creates && !reads && !writes);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api && creates==1 && reads && writes);
    Dl_info library;assert(dladdr((void *)factory->read,&library) && strstr(library.dli_fname,"libbirnam_object_model.dylib"));
    fixture_set_auto(0);
    Oop selector=api->intern("epochLoadedAnswer"),missing=api->intern("epochLoadedMissing");
    Oop remove=api->intern("removeMethod:"),parent_put=api->intern("parentOf:put:"),reassociate=api->intern("reassociateBehaviorOf:with:");
    uint64_t before=reset();Oop parent=api->proto(api->object_vt),other=api->proto(api->object_vt),shape=api->proto(parent);
    assert(!writes && factory->read(owner)==before);
    Oop receiver=api->alloc(shape,0);assert(!writes && factory->read(owner)==before);
    puts("epoch loaded inert attachment, one zero-initialized registry and creation without advancement");
    before=reset();api->method(parent,selector,identity);advanced(before,"method add");
    before=reset();api->method(parent,selector,nil);advanced(before,"method overwrite");
    before=reset();api->method(parent,selector,identity);advanced(before,"method restore");
    before=reset();for(unsigned i=0;i<3;i++)assert(api->send(receiver,selector,NULL,0)==receiver);
    assert(reads>=3 && !writes && factory->read(owner)==before);puts("epoch loaded warm inherited lookup reads current owner without writes");
    before=reset();api->send(parent,remove,&missing,1);advanced(before,"absent removal");
    Oop parents[]={shape,parent};before=reset();api->send(api->vtable_vt,parent_put,parents,2);advanced(before,"identical parent");
    api->method(other,selector,nil);parents[1]=other;before=reset();api->send(api->vtable_vt,parent_put,parents,2);advanced(before,"parent replacement");
    assert(api->send(receiver,selector,NULL,0)==api->nil_obj);
    Oop operands[]={receiver,parent};before=reset();api->send(api->vtable_vt,reassociate,operands,2);advanced(before,"behavior reassociation");
    assert(api->send(receiver,selector,NULL,0)==receiver);
    before=reset();api->send(api->vtable_vt,reassociate,operands,2);advanced(before,"identical behavior reassociation");
    before=reset();api->send(parent,remove,&selector,1);advanced(before,"present removal");
}
