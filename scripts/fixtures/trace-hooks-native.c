#define _DARWIN_C_SOURCE
#include "idapi_v19.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>

static IdApi *api;
static const RootRegistryV1 *roots;
static const GcSettingsV1 *settings;
static const PayloadLayoutV1 *layouts;
static const TraceHooksV1 *hooks;
static Oop retained[3];
static unsigned calls;
static void mark_roots(MarkFn mark) { for(size_t i=0;i<3;i++)mark(retained[i]); }
static void trace(Oop object,MarkFn mark) { calls++;mark(*(Oop *)object); }
static Oop exercise(Oop self,const Oop *args,size_t count) {
    assert(count==1 && args[0]);retained[0]=args[0];
    gc_settings_precise_v1(settings,1);
    Oop vt=api->proto(api->object_vt);retained[1]=vt;
    payload_layout_set_v1(layouts,vt,NULL,0,IDAPI_V19_SLOT_RAW);
    trace_hooks_set_v1(hooks,vt,trace);
    Oop object=api->alloc(vt,sizeof(Oop));retained[2]=object;
    Oop hidden=api->alloc(api->object_vt,0);*(Oop *)object=hidden;
    calls=0;api->gc_collect();assert(calls==1 && api->vtable_of(hidden)==api->object_vt);
    trace_hooks_set_v1(hooks,vt,trace);
    api->gc_collect();assert(calls==2 && api->vtable_of(hidden)==api->object_vt);
    Oop result=retained[0];retained[1]=retained[2]=NULL;api->gc_collect(); /* C locals remain conservative roots until this call returns. */
    retained[0]=NULL;(void)self;return result;
}
__attribute__((visibility("default")))
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *selected_roots,const GcSettingsV1 *selected_settings,const PayloadLayoutV1 *selected_layouts,const TraceHooksV1 *selected_hooks,const FinalizersV1 *finalizer_cap) { (void)finalizer_cap;
    (void)prior;(void)presence;(void)exports;(void)vtables;(void)observation;
    api=selected;roots=selected_roots;settings=selected_settings;layouts=selected_layouts;hooks=selected_hooks;
    Dl_info owner;assert(dladdr((void *)hooks->set,&owner) && strstr(owner.dli_fname,"libbirnam_trace_hooks.dylib"));
    root_registry_register_v1(roots,mark_roots);
    Oop system=namespace_import_v1(imports,"System");assert(system);
    api->method(api->vtable_of(system),api->intern("traceHooksExercise:"),exercise);
    return NULL;
}
__attribute__((visibility("default"))) int birnam_symbol_borrow_v1(void) { return 1; }
