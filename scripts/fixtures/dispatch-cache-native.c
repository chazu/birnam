#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "../../abi/fixtures/dispatch-cache-v2/dispatch_cache_v2.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>

static BirnamOutcome cached_entry(void *context, BirnamObject receiver, const BirnamOperand *args, size_t count) {
    (void)context;(void)receiver;(void)args;(void)count;assert(!"cache invoked an entry");
    return (BirnamOutcome){0};
}
static IdApi *api;
static const DispatchCacheFactoryV2 *factory;
static const GcSettingsV1 *settings;
static Oop retained,receiver,shape,selector;
static Method original_lookup;
static unsigned registered,collections,lookup_calls;
static Oop identity(Oop self,const Oop *args,size_t count) { (void)args;assert(!count);return self; }
static Oop nil(Oop self,const Oop *args,size_t count) { (void)self;(void)args;assert(!count);return api->nil_obj; }
static Oop observing_lookup(Oop self,const Oop *args,size_t count) {
    if(self==shape && count==1 && args[0]==selector)lookup_calls++;
    return original_lookup(self,args,count);
}
static void roots(MarkFn mark) {
    collections++;mark(retained);mark(receiver);
    assert(api->gc_collect()==0);
    if(receiver)assert(api->send(receiver,selector,NULL,0)==receiver);
}
static void repeat(Oop expected) {
    for(unsigned i=0;i<8;i++)assert(api->send(receiver,selector,NULL,0)==expected);
}
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==1 && args[0]);retained=args[0];
    gc_settings_auto_v1(settings,0);gc_settings_precise_v1(settings,1);
    Oop parent=api->proto(api->object_vt);shape=api->proto(parent);
    receiver=api->alloc(shape,0);selector=api->intern("cacheImageAnswer");
    api->method(parent,selector,identity);repeat(receiver);
    api->method(parent,selector,nil);repeat(api->nil_obj);
    api->method(shape,selector,identity);repeat(receiver);
    api->send(shape,api->intern("removeMethod:"),&selector,1);repeat(api->nil_obj);
    Oop other=api->proto(api->object_vt);api->method(other,selector,identity);
    Oop parents[]={shape,other};api->send(api->vtable_vt,api->intern("parentOf:put:"),parents,2);repeat(receiver);
    Oop lookup=api->intern("lookup");original_lookup=(Method)api->send(api->vtable_vt,lookup,&lookup,1);assert(original_lookup);
    api->method(api->vtable_vt,lookup,observing_lookup);lookup_calls=0;repeat(receiver);assert(lookup_calls==8);
    api->method(api->vtable_vt,lookup,original_lookup);repeat(receiver);
    collections=0;api->gc_collect();assert(collections==1);repeat(receiver);
    api->alloc(api->object_vt,16u<<20);assert(collections==1);
    gc_settings_auto_v1(settings,1);api->alloc(api->object_vt,0);assert(collections==2);repeat(receiver);
    void *a=factory->create(),*b=factory->create();assert(a && b && a!=b);
    factory->insert(a,1,(uintptr_t)shape,(uintptr_t)selector,(BirnamNativeEntry){cached_entry,receiver});
    BirnamNativeEntry hit=factory->lookup(a,1,(uintptr_t)shape,(uintptr_t)selector);
    assert(hit.method==cached_entry && hit.context==receiver);
    assert(!factory->lookup(b,1,(uintptr_t)shape,(uintptr_t)selector).method);
    factory->clear(a);assert(!factory->lookup(a,1,(uintptr_t)shape,(uintptr_t)selector).method);
    factory->destroy(a);factory->destroy(b);
    api->send(other,api->intern("removeMethod:"),&selector,1);
    /* parent may have been reclaimed after reparenting; no use of it after GC. */
    Oop result=retained;retained=receiver=shape=selector=NULL;return result;
}
__attribute__((visibility("default")))
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *selected_settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,const FinalizersV1 *finalizers) {
    (void)prior;(void)presence;(void)exports;(void)vtables;(void)observation;(void)layouts;(void)hooks;(void)finalizers;
    if(api)assert(api==selected);api=selected;settings=selected_settings;
    Dl_info owner;assert(dladdr((void *)api->dlsym_fn,&owner));
    const char *slash=strrchr(owner.dli_fname,'/');assert(slash);char path[PATH_MAX];
    int length=snprintf(path,sizeof(path),"%.*s/libbirnam_dispatch_cache.dylib",(int)(slash-owner.dli_fname),owner.dli_fname);
    assert(length>0 && (size_t)length<sizeof(path));void *image=dlopen(path,RTLD_NOW|RTLD_NOLOAD);assert(image);
    DispatchCacheFactoryProviderV2 provide=(DispatchCacheFactoryProviderV2)dlsym(image,"birnam_dispatch_cache_factory_v2");assert(provide);
    factory=provide();assert(factory && factory->version==2 && factory->byte_size==sizeof(*factory));
    assert(dladdr((void *)factory->lookup,&owner) && strstr(owner.dli_fname,"libbirnam_dispatch_cache.dylib"));
    if(!registered) { root_registry_register_v1(root_registry,roots);registered=1; }
    Oop system=namespace_import_v1(imports,"System");assert(system);
    api->method(api->vtable_of(system),api->intern("dispatchCacheExercise:"),exercise);return NULL;
}
__attribute__((visibility("default"))) int birnam_symbol_borrow_v1(void) { return 1; }
