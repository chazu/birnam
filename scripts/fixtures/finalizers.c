#define _DARWIN_C_SOURCE
#ifdef FINALIZERS_BASELINE
#include "idapi_v18.h"
#define SET(o,f) api->set_finalizer(o,f)
#else
#include "idapi_v20.h"
static FinalizersV1 *finalizer_cap;
#define SET(o,f) finalizers_set_v1(finalizer_cap,o,f)
#endif
#include "storage_v1.h"
#include "namespace-support.h"
#include <pthread.h>
#include <stdlib.h>

static IdApi *api;
static StorageObserveV1 inspect;
static const char *mode;
static Oop query;
static StorageAllocationV1 observed;
static size_t found,allocation_count;
static void first(Oop self) { (void)self;assert(!"registration invoked first callback"); }
static void second(Oop self) { (void)self;assert(!"registration invoked replacement callback"); }
static void *wrong_thread(void *context) { (void)context;SET(NULL,first);return NULL; }
static int observe(void *context,const StorageViewV1 *view) {
    (void)context;
    if(!strcmp(mode,"observation")) SET(NULL,first);
    found=0;allocation_count=view->allocation_count;
    for(size_t i=0;i<view->allocation_count;i++) if(view->allocations[i].object==query) {
        observed=view->allocations[i];found++;
    }
    return 0;
}
static int lookup(Oop object) {
    query=object;StorageRequestV1 request={1,sizeof(request),NULL,observe};
    assert(inspect(&request)==0);assert(found<=1);return found==1;
}
#ifndef FINALIZERS_BASELINE
static void factory_probe(void) {
    Dl_info owner;assert(dladdr((void *)finalizer_cap->set,&owner));
    assert(strstr(owner.dli_fname,"libbirnam_finalizers.dylib"));
    void *image=dlopen(owner.dli_fname,RTLD_NOW|RTLD_LOCAL);assert(image);
    FinalizersFactoryProviderV1 provide=(FinalizersFactoryProviderV1)dlsym(image,"birnam_finalizers_factory_v1");assert(provide);
    const FinalizersFactoryV1 *factory=provide();assert(factory && factory->version==1 && factory->byte_size==sizeof(*factory));
    assert(factory->slot(0,first).callback==first && factory->slot(0,second).callback==second);
    assert(!factory->slot(1,first).callback && !factory->create(NULL));
    assert(dladdr((void *)factory->slot,&owner) && strstr(owner.dli_fname,"libbirnam_finalizers.dylib"));
}
#endif
int main(int argc,char **argv) {
    assert(argc==3);mode=argv[2];
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);
    fixture_set_auto(0);
#ifndef FINALIZERS_BASELINE
    finalizer_cap=fixture_finalizers();factory_probe();
#endif
    if(!strcmp(mode,"owner")) { pthread_t thread;assert(!pthread_create(&thread,NULL,wrong_thread,NULL));assert(!pthread_join(thread,NULL));abort(); }
    inspect=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(inspect);
    uintptr_t foreign=0;
    Oop object=api->alloc(api->object_vt,16),permanent=api->nil_obj;
    assert(lookup(object) && !(observed.flags&STORAGE_HAS_FINALIZER));
    uint64_t identity=observed.identity;size_t before=allocation_count;
    SET(object,first);SET(object,second);
    assert(lookup(object) && (observed.flags&STORAGE_HAS_FINALIZER));
    assert(observed.identity==identity && allocation_count==before);
    assert(lookup(permanent) && (observed.flags&STORAGE_PERMANENT) && !(observed.flags&STORAGE_HAS_FINALIZER));
    SET(permanent,first);assert(lookup(permanent) && !(observed.flags&STORAGE_HAS_FINALIZER));
    Oop ignored[]={NULL,(char *)object+1,(char *)object-8,&foreign,(Oop)(uintptr_t)1};
    for(size_t i=0;i<sizeof(ignored)/sizeof(*ignored);i++) {
        SET(ignored[i],first);assert(!lookup(ignored[i]) && allocation_count==before);
    }
    Oop clone=api->send(object,api->intern("clone"),NULL,0);
    assert(clone && clone!=object && lookup(clone) && !(observed.flags&STORAGE_HAS_FINALIZER));
    assert(lookup(object) && (observed.flags&STORAGE_HAS_FINALIZER) && observed.identity==identity);
    puts("collectable replacement, permanent/null/interior/foreign rejection, identity, allocation count, clone and callback presence agree");
}
