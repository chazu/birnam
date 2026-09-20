#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "../../abi/fixtures/guard-verdict-v1/guard_verdict_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>

static IdApi *api;
static const GcSettingsV1 *settings;
static Oop held_block,held_value,machine_vt,read_selector,write_selector,lookup_selector;
static Method canonical_read,canonical_write,canonical_lookup,frame_view;
static unsigned registered,read_calls,write_calls,lookup_reads,lookup_writes;
static Oop identity(Oop self,const Oop *args,size_t count) { (void)args;assert(!count);return self; }
static Oop read_override(Oop machine,const Oop *args,size_t count) {
    assert(frame_view(machine,NULL,0)!=api->nil_obj);read_calls++;
    return canonical_read(machine,args,count);
}
static Oop write_override(Oop machine,const Oop *args,size_t count) {
    assert(count==2 && frame_view(machine,NULL,0)!=api->nil_obj);write_calls++;
    Oop answer=canonical_write(machine,args,count);assert(answer==args[1]);return answer;
}
static Oop lookup_override(Oop self,const Oop *args,size_t count) {
    if(self==machine_vt && count==1) {
        lookup_reads+=args[0]==read_selector;lookup_writes+=args[0]==write_selector;
    }
    return canonical_lookup(self,args,count);
}
static void roots(MarkFn mark) { mark(held_block);mark(held_value); }
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==2 && args[0] && args[1]);held_block=args[0];held_value=args[1];
    gc_settings_auto_v1(settings,0);gc_settings_precise_v1(settings,1);
    Oop value_selector=api->intern("value:");
    for(unsigned mode=0;mode<4;mode++) {
        api->method(machine_vt,read_selector,mode==1?read_override:canonical_read);
        api->method(machine_vt,write_selector,mode==2?write_override:canonical_write);
        api->method(api->vtable_vt,lookup_selector,mode==3?lookup_override:canonical_lookup);
        read_calls=write_calls=lookup_reads=lookup_writes=0;
        for(unsigned repeat=0;repeat<3;repeat++)assert(api->send(held_block,value_selector,&held_value,1)==held_value);
        if(mode==1)assert(read_calls>=3 && !write_calls);
        else if(mode==2)assert(write_calls>=3 && !read_calls);
        else if(mode==3)assert(lookup_reads>=3 && lookup_writes>=3 && !read_calls && !write_calls);
        else assert(!read_calls && !write_calls && !lookup_reads && !lookup_writes);
    }
    api->method(api->vtable_vt,lookup_selector,canonical_lookup);
    assert(api->send(held_block,value_selector,&held_value,1)==held_value);
    api->gc_collect();assert(api->send(held_block,value_selector,&held_value,1)==held_value);
    Oop result=held_value;held_block=held_value=NULL;return result;
}
__attribute__((visibility("default")))
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *selected_settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,const FinalizersV1 *finalizers) {
    (void)prior;(void)presence;(void)exports;(void)vtables;(void)observation;(void)layouts;(void)hooks;(void)finalizers;
    if(api)assert(api==selected);api=selected;settings=selected_settings;
    Dl_info owner;assert(dladdr((void *)api->dlsym_fn,&owner));
    const char *slash=strrchr(owner.dli_fname,'/');assert(slash);char path[PATH_MAX];
    int length=snprintf(path,sizeof(path),"%.*s/libbirnam_dispatch_cache.dylib",(int)(slash-owner.dli_fname),owner.dli_fname);
    assert(length>0 && (size_t)length<sizeof(path));void *image=dlopen(path,RTLD_NOW|RTLD_NOLOAD);assert(image);
    GuardVerdictFactoryProviderV1 provide=(GuardVerdictFactoryProviderV1)dlsym(image,"birnam_guard_verdict_factory_v1");assert(provide);
    const GuardVerdictFactoryV1 *factory=provide();assert(factory && factory->version==1 && factory->byte_size==sizeof(*factory));
    assert(dladdr((void *)factory->read,&owner) && strstr(owner.dli_fname,"libbirnam_dispatch_cache.dylib"));
    machine_vt=api->vtable_of(namespace_import_v1(imports,"Machine"));
    lookup_selector=api->intern("lookup");read_selector=api->intern("oopAt:");write_selector=api->intern("oopAt:put:");
    canonical_lookup=(Method)api->send(api->vtable_vt,lookup_selector,&lookup_selector,1);
    canonical_read=(Method)api->send(machine_vt,lookup_selector,&read_selector,1);
    canonical_write=(Method)api->send(machine_vt,lookup_selector,&write_selector,1);
    Oop view_selector=api->intern("currentFrameView");frame_view=(Method)api->send(machine_vt,lookup_selector,&view_selector,1);
    assert(canonical_lookup && canonical_read && canonical_write && frame_view);
    if(!registered) { root_registry_register_v1(root_registry,roots);registered=1; }
    Oop system=namespace_import_v1(imports,"System");assert(system);
    api->method(api->object_vt,api->intern("guardVerdictIdentity"),identity);
    api->method(api->vtable_of(system),api->intern("guardVerdictExercise:value:"),exercise);return NULL;
}
__attribute__((visibility("default"))) int birnam_symbol_borrow_v1(void) { return 1; }
