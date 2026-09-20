#define _DARWIN_C_SOURCE
#include "idapi_v15.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>
static IdApi *api;
static VtableEnumerationV2 *enumeration;
static Oop rooted;
static int rooted_registered,observing;
static VtableEnumerationV2 canonical;
static size_t count_reads,at_reads;
static void mark_root(MarkFn mark) { mark(rooted); }
static size_t observe_count(void *context) {
    assert(context==&canonical && observing);count_reads++;
    return canonical.count(canonical.context);
}
static Oop observe_at(void *context,size_t index) {
    assert(context==&canonical && observing);at_reads++;
    return canonical.at(canonical.context,index);
}
static Oop observe_selector(void *context,Oop vtable,size_t index) {
    assert(context==&canonical && observing);
    return canonical.selector_at(canonical.context,vtable,index);
}
static Oop observe(Oop self,const Oop *args,size_t count) {
    (void)args;assert(count==0 && !observing);
    canonical=*enumeration;count_reads=at_reads=0;observing=1;
    enumeration->context=&canonical;enumeration->count=observe_count;enumeration->at=observe_at;enumeration->selector_at=observe_selector;
    return self;
}
static Oop restore(Oop self,const Oop *args,size_t count) {
    (void)self;(void)args;assert(count==0 && observing);
    *enumeration=canonical;observing=0;
    return count_reads?api->true_obj:api->false_obj;
}
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==1 && args && args[0]);
    size_t before=vtable_count_v2(enumeration);assert(before && !vtable_at_v2(enumeration,before) && !vtable_at_v2(enumeration,SIZE_MAX));
    rooted=api->proto(api->object_vt);assert(rooted);
    assert(vtable_count_v2(enumeration)==before+1 && vtable_at_v2(enumeration,before)==rooted);
    api->gc_collect();
    int found=0;
    for(size_t i=0;i<vtable_count_v2(enumeration);i++)if(vtable_at_v2(enumeration,i)==rooted)found++;
    assert(found==1);rooted=NULL;
    return args[0];
}
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry) { (void)root_registry; (void)observation;
    (void)presence;(void)exports;assert(selected && !prior && vtables && !observing);
    if(api)assert(api==selected && enumeration==vtables);else { api=selected;enumeration=(VtableEnumerationV2 *)vtables; }
    assert(enumeration->version==2 && enumeration->byte_size==sizeof(*enumeration));
    Dl_info owner;
    assert(dladdr((void *)enumeration->count,&owner) && strstr(owner.dli_fname,"libbirnam_object_model.dylib"));
    assert(dladdr((void *)enumeration->at,&owner) && strstr(owner.dli_fname,"libbirnam_object_model.dylib"));
    if(!rooted_registered) { root_registry_register_v1(root_registry,mark_root);rooted_registered=1; }
    Oop system=namespace_import_v1(imports,"SystemVt");assert(system);
    api->method(system,api->intern("enumerationExercise:"),exercise);
    api->method(system,api->intern("enumerationObserve"),observe);
    api->method(system,api->intern("enumerationRestore"),restore);
    return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
