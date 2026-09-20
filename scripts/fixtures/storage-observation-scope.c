/* Observe actual Runtime scope transitions through the loaded factory. */
#define _DARWIN_C_SOURCE
#include "../../abi/fixtures/idapi-v20/storage_v1.h"
#include "../../abi/fixtures/storage-observation-scope-v1/storage_observation_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <pthread.h>
#include <stdio.h>
#include <string.h>

static const StorageObservationFactoryV1 *factory;
static StorageObservationFactoryV1 watched;
static void *scope;
static const char *mode;
static char events[128];
static size_t used;
static unsigned creates,visits;
static void event(char value) { assert(used<sizeof(events)-1);events[used++]=value;events[used]=0; }
static StorageObservationStateV1 state(void) { StorageObservationStateV1 value;factory->read(scope,&value);return value; }
static void *create(void) { assert(!creates++);scope=factory->create();assert(scope);return scope; }
static void active(void *context,int enabled) { assert(context==scope);event(enabled?'A':'a');factory->set_active(context,enabled); }
static void permit(void *context,int enabled) { assert(context==scope);event(enabled?'P':'p');factory->set_permit(context,enabled); }
static int advance(void *context,uint64_t *out) {
    assert(context==scope);StorageObservationStateV1 before=state();assert(!before.active && !before.permit);event('E');
    if(!strcmp(mode,"advance-refusal"))return 0;
    return factory->advance_epoch(context,out);
}
static const StorageObservationFactoryV1 *provide(void) { return &watched; }
static void *capture_dlsym(void *image,const char *name) {
    void *symbol=dlsym(image,name);
    if(strcmp(name,"birnam_storage_observation_factory_v1"))return symbol;
    assert(symbol);factory=((StorageObservationFactoryProviderV1)symbol)();
    assert(factory && factory->version==1 && factory->byte_size==sizeof(*factory));
    watched=*factory;watched.create=create;watched.set_active=active;watched.set_permit=permit;watched.advance_epoch=advance;
    return (void *)provide;
}
#define dlsym capture_dlsym
#include "namespace-support.h"
#undef dlsym

static IdApi *api;
static StorageObserveV1 observe;
static Method original;
static uint64_t runtime_identity;
static int visitor_status;
static void *wrong_thread(void *context) { (void)context;observe(NULL);return NULL; }
static int visitor(void *context,const StorageViewV1 *view) {
    (void)context;StorageObservationStateV1 current=state();
    assert(current.active==1 && current.permit==0 && current.epoch==view->observation_identity);
    assert(view->runtime_identity && view->allocation_count>0);
    if(!runtime_identity)runtime_identity=view->runtime_identity;assert(runtime_identity==view->runtime_identity);
    assert(observe(NULL)==STORAGE_BUSY);event('V');visits++;
    if(!strcmp(mode,"active-read"))api->gc_live_count();
    if(!strcmp(mode,"owner-before-active")) { pthread_t thread;assert(!pthread_create(&thread,NULL,wrong_thread,NULL));assert(!pthread_join(thread,NULL)); }
    return visitor_status;
}
static Oop wrapper(Oop self,const Oop *args,size_t count) {
    assert(state().permit==1 && !state().active && observe(NULL)==STORAGE_BUSY);
    Oop result=original(self,args,count);
    assert((uintptr_t)original(self,args,count)==STORAGE_BUSY);
    return result;
}
int main(int argc,char **argv) {
    assert(argc==3);mode=argv[2];void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);assert(!creates);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api && creates==1);
    assert(!state().epoch && !state().active && !state().permit && !used);
    Dl_info owner;assert(dladdr((void *)factory->advance_epoch,&owner) && strstr(owner.dli_fname,"libbirnam_storage.dylib"));
    observe=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(observe);
    Oop selector=api->intern("withStorageView:");Oop lookup=api->intern("lookup");
    original=(Method)api->send(api->vtable_vt,lookup,&selector,1);assert(original);
    StorageRequestV1 request={1,sizeof(request),NULL,visitor},bad=request;bad.version++;
    assert(observe(&bad)==STORAGE_INVALID && !used && !state().epoch);
    Oop argument=&request;assert((uintptr_t)api->send(api->vtable_vt,selector,&argument,1)==STORAGE_BUSY);
    api->method(api->vtable_vt,selector,wrapper);
    if(!strcmp(mode,"advance-refusal")) {
        /* Injection checks caller ordering; module unit tests prove real overflow. */
        for(unsigned i=0;i<2;i++) {
            used=0;assert(observe(&request)==STORAGE_INVALID && !visits);
            assert(!strcmp(events,"PpEp") && !state().active && !state().permit && !state().epoch);
        }
        puts("advancement refusal consumes permission before returning INVALID and never visits");return 0;
    }
    for(unsigned i=1;i<=3;i++) {
        visitor_status=i==2?-77:0;used=0;
        assert(observe(&request)==visitor_status && visits==i);
        assert(!strcmp(events,"PpEAVap"));
        StorageObservationStateV1 current=state();assert(current.epoch==i && !current.active && !current.permit);
    }
    assert(creates==1);puts("one Runtime scope: grant, consume, advance, activate, visit, active reset, permit reset");
    puts("visitor failure retains its epoch; malformed requests and unpermitted sends do not advance");
}
