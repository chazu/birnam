/* Loaded by the installed host and source-free compiled/image callers. */
#define _DARWIN_C_SOURCE
#include "idapi_v19.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdlib.h>
#include <string.h>

static IdApi *api;
static const FinalizersV1 *finalizers;
static const GcSettingsV1 *settings;
static Oop retained, owner;
static unsigned cleaned;
struct Payload { uint64_t magic; char *bytes; };

static void mark_roots(MarkFn mark) { mark(retained); }
static void forbidden(Oop object) { (void)object;assert(!"replaced or ignored callback ran"); }
static void cleanup(Oop object) {
    assert(object==owner && !cleaned && !api->vtable_of(object));
    struct Payload *payload=object;
    assert(payload->magic==0xf1a11e && !strcmp(payload->bytes,"owned native bytes"));
    finalizers_set_v1(finalizers,object,forbidden); /* Detached registration is ignored. */
    free(payload->bytes);payload->bytes=NULL;payload->magic=0;cleaned++;
}
__attribute__((noinline)) static void allocate_owner(void) {
    owner=api->alloc(api->object_vt,sizeof(struct Payload));assert(owner);
    struct Payload *payload=owner;payload->magic=0xf1a11e;
    payload->bytes=malloc(sizeof("owned native bytes"));assert(payload->bytes);
    memcpy(payload->bytes,"owned native bytes",sizeof("owned native bytes"));
    cleaned=0;
    finalizers_set_v1(finalizers,owner,forbidden);
    finalizers_set_v1(finalizers,owner,cleanup);
    finalizers_set_v1(finalizers,NULL,forbidden);
    finalizers_set_v1(finalizers,api->object_vt,forbidden);
    finalizers_set_v1(finalizers,(char *)owner+1,forbidden);
    assert(!cleaned);
}
__attribute__((noinline)) static void collect_after_return(void) {
    /* Overwrite the returned allocation helper's inactive stack slots. The
       installed tracer and all live native roots remain in use unchanged. */
    volatile uintptr_t scratch[8192];
    for(size_t i=0;i<8192;i++)scratch[i]=0;
    api->gc_collect();assert(scratch[8191]==0);
}
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==1 && args[0]);retained=args[0];
    gc_settings_precise_v1(settings,1);
    allocate_owner();collect_after_return();
    assert(cleaned==1 && !api->vtable_of(owner));
    api->gc_collect();assert(cleaned==1);
    owner=NULL;
    assert(api->vtable_of(retained));Oop result=retained;retained=NULL;return result;
}
__attribute__((visibility("default")))
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *roots,const GcSettingsV1 *selected_settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,const FinalizersV1 *selected_finalizers) {
    (void)prior;(void)presence;(void)exports;(void)vtables;(void)observation;(void)layouts;(void)hooks;
    if(api) assert(api==selected && finalizers==selected_finalizers);
    api=selected;finalizers=selected_finalizers;settings=selected_settings;
    Dl_info location;assert(dladdr((void *)finalizers->set,&location) && strstr(location.dli_fname,"libbirnam_finalizers.dylib"));
    root_registry_register_v1(roots,mark_roots);
    Oop system=namespace_import_v1(imports,"System");assert(system);
    api->method(api->vtable_of(system),api->intern("finalizersExercise:"),exercise);
    return NULL;
}
__attribute__((visibility("default"))) int birnam_symbol_borrow_v1(void) { return 1; }
