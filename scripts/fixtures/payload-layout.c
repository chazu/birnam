#define _DARWIN_C_SOURCE
#ifdef PAYLOAD_LAYOUT_BASELINE
#include "idapi_v16.h"
#include "../../abi/fixtures/idapi-v16/storage_v1.h"
#define SET(v,k,n,r) api->set_layout(v,k,n,r)
#define SLOT(v,i) api->slot_kind(v,i)
#else
#include "idapi_v20.h"
#include "../../abi/fixtures/idapi-v20/storage_v1.h"
static const PayloadLayoutV1 *layouts;
#define SET(v,k,n,r) payload_layout_set_v1(layouts,v,k,n,r)
#define SLOT(v,i) payload_layout_slot_v1(layouts,v,i)
#endif
#include "namespace-support.h"
#include <pthread.h>
#include <stdlib.h>

static IdApi *api;
static Oop parent,child,object;
static const char *mode;
static unsigned observed;
typedef struct {
    size_t size,tally; Oop *keys,*values; Oop parent; TraceHook hook;
    const uint8_t *kinds; size_t count; uint8_t rest,is_set; Oop owner;
} RawVtable;
_Static_assert(sizeof(RawVtable)==80 && offsetof(RawVtable,kinds)==48 && offsetof(RawVtable,owner)==72,"raw vtable mirror");
static void access_layout(void) {
    if(strstr(mode,"slot")) (void)SLOT(parent,0);
    else SET(parent,NULL,0,0);
}
static void *wrong_thread(void *context) { (void)context;access_layout();return NULL; }
static int observe(void *context,const StorageViewV1 *view) {
    assert(context==&observed);observed++;
    if(strstr(mode,"observation")) access_layout();
    unsigned found=0;
    for(size_t i=0;i<view->allocation_count;i++) if(view->allocations[i].object==object) {
        assert(view->allocations[i].flags&STORAGE_HAS_GC_LAYOUT);found++;
    }
    assert(found==1);return 0;
}
static Oop answer(Oop self,const Oop *args,size_t count) { (void)args;(void)count;return self; }

#ifndef PAYLOAD_LAYOUT_BASELINE
typedef struct { PayloadLayoutOwnedV1 owned; unsigned admissions,replacements; } TestFloor;
static const PayloadLayoutFactoryV1 *factory;
static void admit(void *context) { ((TestFloor *)context)->admissions++; }
static void replace(void *context,void *vt,PayloadLayoutOwnedV1 owned) {
    (void)vt;TestFloor *floor=context;factory->release(floor->owned);floor->owned=owned;floor->replacements++;
}
static PayloadLayoutViewV1 view(void *context,void *vt) { (void)vt;return ((TestFloor *)context)->owned.view; }
static void factory_probe(void) {
    Dl_info owner;assert(dladdr((void *)layouts->set,&owner) && strstr(owner.dli_fname,"libbirnam_payload_layout.dylib"));
    assert(dladdr((void *)layouts->slot_kind,&owner) && strstr(owner.dli_fname,"libbirnam_payload_layout.dylib"));
    void *image=dlopen(owner.dli_fname,RTLD_NOW|RTLD_LOCAL);assert(image);
    PayloadLayoutFactoryProviderV1 provide=(PayloadLayoutFactoryProviderV1)dlsym(image,"birnam_payload_layout_factory_v1");assert(provide);
    factory=provide();assert(factory && factory->version==1 && factory->byte_size==sizeof(*factory));
    TestFloor a={0},b={0};PayloadLayoutFloorV1 floor={1,sizeof(floor),&a,admit,replace,view};
    PayloadLayoutV1 *one=factory->create(&floor);floor.context=&b;
    PayloadLayoutV1 *two=factory->create(&floor);assert(one && two && one!=two && !a.admissions && !b.admissions);
    const uint8_t kinds[]={1,9};payload_layout_set_v1(one,NULL,kinds,2,1);
    assert(a.admissions==1 && a.replacements==1 && b.admissions==0);
    one->context=NULL;one->set=NULL;one->slot_kind=NULL;
    assert(factory->has_layout(a.owned.view) && !factory->has_layout(b.owned.view));
    assert(factory->all_oop(a.owned.view,0) && factory->all_oop(a.owned.view,1));
    assert(!factory->all_oop(a.owned.view,2) && !factory->all_oop(a.owned.view,SIZE_MAX));
    assert(factory->slot_kind(a.owned.view,1)==9 && factory->slot_kind(a.owned.view,SIZE_MAX)==1);
    factory->destroy(one);factory->destroy(two);factory->release(a.owned);factory->release(b.owned);
    PayloadLayoutOwnedV1 empty=factory->copy((const uint8_t *)(uintptr_t)1,0,0);
    assert(!empty.view.kinds && factory->has_layout(empty.view) && factory->all_oop(empty.view,0));
    assert(!factory->all_oop(empty.view,1));factory->release(empty);
    ModuleInitFn initialize=(ModuleInitFn)dlsym(image,"module_init");assert(initialize);
    assert(!initialize(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),layouts,fixture_trace_hooks(),fixture_finalizers()));
}
#endif

int main(int argc,char **argv) {
    assert(argc==3);mode=argv[2];
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);
#ifndef PAYLOAD_LAYOUT_BASELINE
    layouts=fixture_payload_layout();factory_probe();
#endif
    fixture_set_auto(0);parent=api->proto(api->object_vt);child=api->proto(parent);object=api->alloc(parent,0);
    if(strstr(mode,"owner")) { pthread_t t;assert(!pthread_create(&t,NULL,wrong_thread,NULL));assert(!pthread_join(t,NULL));abort(); }
    StorageObserveV1 inspect=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(inspect);
    StorageRequestV1 request={1,sizeof(request),&observed,observe};
    if(strstr(mode,"observation")) { (void)inspect(&request);abort(); }
    if(!strcmp(mode,"invalid-set")) { SET(NULL,NULL,0,0);abort(); }
    if(!strcmp(mode,"invalid-slot")) { (void)SLOT(NULL,0);abort(); }
    if(!strcmp(mode,"null-prefix")) { SET(parent,NULL,1,0);abort(); }
    if(!strcmp(mode,"null-prefix-invalid")) { SET(NULL,NULL,1,0);abort(); }
    assert(SLOT(parent,0)==255 && SLOT(child,SIZE_MAX)==255);
    RawVtable *raw=parent;assert(!raw->kinds && !raw->count && !raw->is_set);
    uint8_t input[]={1,0,2,9,255};SET(parent,input,5,9);input[0]=0;
    assert(raw->kinds!=input && raw->count==5 && raw->is_set && raw->rest==9);
    assert(SLOT(parent,0)==1 && SLOT(parent,2)==2 && SLOT(parent,3)==9 && SLOT(parent,4)==255 && SLOT(parent,SIZE_MAX)==9);
    assert(SLOT(child,0)==255);
    const uint8_t *old=raw->kinds;SET(parent,old,5,1);assert(raw->kinds!=old && SLOT(parent,0)==1 && SLOT(parent,5)==1);
    old=raw->kinds;api->method(parent,api->intern("payloadLayoutMirrorProbe"),answer);assert(raw->kinds==old && raw->count==5);
    assert(inspect(&request)==0 && observed==1);
    for(unsigned kind=0;kind<256;kind++) {
        SET(parent,(const uint8_t *)(uintptr_t)1,0,(uint8_t)kind);
        assert(!raw->kinds && !raw->count && raw->is_set && SLOT(parent,0)==kind && SLOT(parent,SIZE_MAX)==kind);
    }
    puts("unset, copied prefix, arbitrary tail, exact membership, borrowed replacement and storage observation agree");
    return 0;
}
