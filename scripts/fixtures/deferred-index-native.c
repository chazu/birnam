#define _DARWIN_C_SOURCE
#include "storage_v1.h"
#include "../../abi/fixtures/deferred-index-v1/deferred_index_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static IdApi *api;
static StorageObserveV1 observe;
static const FinalizersV1 *finalizer_api;
static Oop identity_probe,final_owner;
static unsigned cleaned;
static int allocation_view(void *context,const StorageViewV1 *view) {
    uint64_t *result=context;uint64_t previous=0;unsigned found=0;
    for(size_t i=0;i<view->allocation_count;i++) {
        const StorageAllocationV1 *entry=&view->allocations[i];
        assert(entry->identity>previous && entry->identity<=(UINT64_MAX>>5));previous=entry->identity;
        if(entry->object==identity_probe) {
            assert(entry->requested_size==9 && entry->padded_size==16 && entry->allocation_bytes==24);
            assert(entry->behavior==api->object_vt);*result=entry->identity;found++;
        }
    }
    assert(found==1);return STORAGE_OK;
}
static uint64_t identity_snapshot(void) {
    uint64_t result=0;StorageRequestV1 request={1,sizeof(request),&result,allocation_view};
    assert(observe(&request)==STORAGE_OK && result);return result;
}
static void cleanup(Oop object) {
    assert(object==final_owner && !api->vtable_of(object) && !cleaned);
    assert(*(uint64_t *)object==0xa110ca7e);cleaned++;
}
__attribute__((noinline)) static void allocate_final_owner(void) {
    final_owner=api->alloc(api->object_vt,8);*(uint64_t *)final_owner=0xa110ca7e;cleaned=0;
    finalizers_set_v1(finalizer_api,final_owner,cleanup);
}
__attribute__((noinline)) static void collect_after_return(void) {
    volatile uintptr_t scratch[8192];for(size_t i=0;i<8192;i++)scratch[i]=0;
    api->gc_collect();assert(scratch[8191]==0);
}
static const GcSettingsV1 *settings;
static Oop probe;
static Oop held_block,held_value,machine_vt,read_selector,write_selector,lookup_selector;
static Method canonical_read,canonical_write,canonical_lookup,frame_view;
static unsigned registered,read_calls,write_calls,lookup_reads,lookup_writes;
static void (*fixture_counts)(uint64_t *);
static uint64_t phase_start[3];
static Oop checkpoint(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==1);int64_t phase;memcpy(&phase,args[0],sizeof(phase));
    if(fixture_counts) {
        uint64_t current[3];fixture_counts(current);
        if(phase==0 || phase==2)memcpy(phase_start,current,sizeof(current));
        else {
            assert(phase==1 || phase==3);
            uint64_t pushed=current[0]-phase_start[0],removed=current[1]-phase_start[1],repaired=current[2]-phase_start[2];
            const char *enabled=getenv("BIRNAM_DEV_CLOSURE_ELISION");
            if(phase==1 && enabled && !strcmp(enabled,"1"))assert(pushed>0 && removed>0);
            if(phase==3)assert(!pushed && !removed && !repaired);
            fprintf(stderr,"deferred-index: phase=%lld pushes=%llu removes=%llu writes=%llu\n",(long long)phase,(unsigned long long)pushed,(unsigned long long)removed,(unsigned long long)repaired);
        }
    }
    return api->nil_obj;
}
static Oop identity(Oop self,const Oop *args,size_t count) { (void)args;assert(!count);return self; }
static Oop nil(Oop self,const Oop *args,size_t count) { (void)self;(void)args;assert(!count);return api->nil_obj; }
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
static void roots(MarkFn mark) { mark(held_block);mark(held_value);mark(probe);mark(identity_probe); }
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==2 && args[0] && args[1]);held_block=args[0];held_value=args[1];
    gc_settings_auto_v1(settings,0);gc_settings_precise_v1(settings,1);
    identity_probe=api->alloc(api->object_vt,9);
    uint64_t busy_result=0;StorageRequestV1 request={1,sizeof(request),&busy_result,allocation_view};
    assert(observe(&request)==STORAGE_BUSY && !busy_result);
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
    Oop parent=api->proto(api->object_vt),shape=api->proto(parent),other=api->proto(api->object_vt);
    probe=api->alloc(shape,0);Oop answer=api->intern("deferredIndexImageAnswer");
    Oop indexed_keys[24];char indexed_name[64];
    for(unsigned i=0;i<24;i++) {
        snprintf(indexed_name,sizeof(indexed_name),"deferredIndexImage%u",i);indexed_keys[i]=api->intern(indexed_name);
        api->method(parent,indexed_keys[i],identity);api->method(other,indexed_keys[i],nil);
    }
    api->method(parent,answer,identity);api->method(other,answer,nil);
    for(unsigned i=0;i<24;i++)assert(api->send(probe,indexed_keys[i],NULL,0)==probe);
    api->method(parent,indexed_keys[7],nil);assert(api->send(probe,indexed_keys[7],NULL,0)==api->nil_obj);
    api->method(parent,indexed_keys[7],identity);assert(api->send(probe,indexed_keys[7],NULL,0)==probe);
    Oop remove=api->intern("removeMethod:");
    // Remove through the 17 -> 16 boundary, then rebuild using the same keys.
    for(unsigned i=0;i<10;i++)api->send(parent,remove,&indexed_keys[i],1);
    for(unsigned i=10;i<24;i++)assert(api->send(probe,indexed_keys[i],NULL,0)==probe);
    for(unsigned i=0;i<10;i++)api->method(parent,indexed_keys[i],identity);
    for(unsigned i=0;i<24;i++)assert(api->send(probe,indexed_keys[i],NULL,0)==probe);
    for(unsigned i=0;i<3;i++)assert(api->send(probe,answer,NULL,0)==probe);
    Oop parents[]={shape,other};api->send(api->vtable_vt,api->intern("parentOf:put:"),parents,2);
    for(unsigned i=0;i<3;i++)assert(api->send(probe,answer,NULL,0)==api->nil_obj);
    assert(api->send(held_block,value_selector,&held_value,1)==held_value);
    Oop behavior[]={probe,parent};api->send(api->vtable_vt,api->intern("reassociateBehaviorOf:with:"),behavior,2);
    assert(api->send(probe,answer,NULL,0)==probe);
    assert(api->send(held_block,value_selector,&held_value,1)==held_value);
    Oop missing=api->intern("deferredIndexImageAbsent");api->send(parent,api->intern("removeMethod:"),&missing,1);
    assert(api->send(probe,answer,NULL,0)==probe);
    allocate_final_owner();collect_after_return();assert(cleaned==1 && !api->vtable_of(final_owner));
    final_owner=NULL;
    api->gc_collect();assert(cleaned==1);
    assert(api->send(probe,answer,NULL,0)==probe);
    for(unsigned i=0;i<24;i++)assert(api->send(probe,indexed_keys[i],NULL,0)==probe);
    assert(api->send(held_block,value_selector,&held_value,1)==held_value);probe=NULL;
    Oop result=held_value;held_block=held_value=identity_probe=NULL;return result;
}
__attribute__((visibility("default")))
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *selected_settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,const FinalizersV1 *finalizers) {
    (void)prior;(void)presence;(void)exports;(void)vtables;(void)observation;(void)layouts;(void)hooks;(void)finalizers;
    if(api)assert(api==selected);api=selected;settings=selected_settings;finalizer_api=finalizers;
    Dl_info owner;assert(dladdr((void *)api->dlsym_fn,&owner));
    void *floor=dlopen(owner.dli_fname,RTLD_NOW|RTLD_NOLOAD);assert(floor);
    observe=(StorageObserveV1)dlsym(floor,"birnam_id_observe_storage_v1");assert(observe);
    const char *slash=strrchr(owner.dli_fname,'/');assert(slash);char path[PATH_MAX];
    int length=snprintf(path,sizeof(path),"%.*s/libbirnam_storage.dylib",(int)(slash-owner.dli_fname),owner.dli_fname);
    assert(length>0 && (size_t)length<sizeof(path));void *image=dlopen(path,RTLD_NOW|RTLD_NOLOAD);assert(image);
    DeferredIndexFactoryProviderV1 provide=(DeferredIndexFactoryProviderV1)dlsym(image,"birnam_deferred_index_factory_v1");assert(provide);
    fixture_counts=(void (*)(uint64_t *))dlsym(image,"deferred_index_fixture_counts_v1");
    const DeferredIndexFactoryV1 *factory=provide();assert(factory && factory->version==1 && factory->byte_size==sizeof(*factory));
    assert(dladdr((void *)factory->read,&owner) && strstr(owner.dli_fname,"libbirnam_storage.dylib"));
    machine_vt=api->vtable_of(namespace_import_v1(imports,"Machine"));
    lookup_selector=api->intern("lookup");read_selector=api->intern("oopAt:");write_selector=api->intern("oopAt:put:");
    canonical_lookup=(Method)api->send(api->vtable_vt,lookup_selector,&lookup_selector,1);
    canonical_read=(Method)api->send(machine_vt,lookup_selector,&read_selector,1);
    canonical_write=(Method)api->send(machine_vt,lookup_selector,&write_selector,1);
    Oop view_selector=api->intern("currentFrameView");frame_view=(Method)api->send(machine_vt,lookup_selector,&view_selector,1);
    assert(canonical_lookup && canonical_read && canonical_write && frame_view);
    if(!registered) { root_registry_register_v1(root_registry,roots);registered=1; }
    Oop system=namespace_import_v1(imports,"System");assert(system);
    api->method(api->vtable_of(system),api->intern("deferredIndexCheckpoint:"),checkpoint);
    api->method(api->object_vt,api->intern("deferredIndexIdentity"),identity);
    api->method(api->vtable_of(system),api->intern("deferredIndexExercise:value:"),exercise);return NULL;
}
__attribute__((visibility("default"))) int birnam_symbol_borrow_v1(void) { return 1; }

/* Host checks the idle view before work and the retained retirement guard after eviction. */
__attribute__((visibility("default"))) void deferred_index_image_idle_v1(int after_main) {
    assert(api && observe);gc_settings_auto_v1(settings,0);gc_settings_precise_v1(settings,1);
    if(after_main) {
        uint64_t result=0;StorageRequestV1 request={1,sizeof(request),&result,allocation_view};
        assert(observe(&request)==STORAGE_BUSY && !result);return;
    }
    identity_probe=api->alloc(api->object_vt,9);Oop alias=identity_probe;
    uint64_t first=identity_snapshot();
    allocate_final_owner();collect_after_return();assert(cleaned==1 && !api->vtable_of(final_owner));
    assert(identity_probe==alias && identity_snapshot()==first);final_owner=NULL;
    identity_probe=api->alloc(api->object_vt,9);assert(identity_snapshot()>first);
    api->gc_collect();assert(identity_snapshot()>first && cleaned==1);identity_probe=NULL;
}
