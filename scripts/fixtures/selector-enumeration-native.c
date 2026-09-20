#define _DARWIN_C_SOURCE
#include "idapi_v13.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>
static IdApi *api;
static VtableEnumerationV2 *enumeration;
static Oop rooted;
static int rooted_registered,observing;
static VtableEnumerationV2 canonical;
static size_t count_reads,at_reads,selector_reads;
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
    assert(context==&canonical && observing);selector_reads++;
    return canonical.selector_at(canonical.context,vtable,index);
}
static Oop observe(Oop self,const Oop *args,size_t count) {
    (void)args;assert(count==0 && !observing);
    canonical=*enumeration;count_reads=at_reads=selector_reads=0;observing=1;
    enumeration->context=&canonical;enumeration->count=observe_count;enumeration->at=observe_at;enumeration->selector_at=observe_selector;
    return self;
}
static Oop restore(Oop self,const Oop *args,size_t count) {
    (void)self;(void)args;assert(count==0 && observing);
    *enumeration=canonical;observing=0;
    return selector_reads?api->true_obj:api->false_obj;
}
static Oop answer(Oop self,const Oop *args,size_t count) { (void)args;(void)count;return self; }
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==1 && args && args[0]);
    rooted=api->proto(api->object_vt);assert(rooted);
    Oop a=api->intern("selectorImageA"),b=api->intern("selectorImageB");
    api->method(rooted,a,answer);api->method(rooted,b,answer);
    assert(vtable_selector_at_v2(enumeration,rooted,0)==a && vtable_selector_at_v2(enumeration,rooted,1)==b);
    api->method(rooted,a,answer);
    assert(vtable_selector_at_v2(enumeration,rooted,0)==a && !vtable_selector_at_v2(enumeration,rooted,2));
    assert(!vtable_selector_at_v2(enumeration,rooted,SIZE_MAX));
    api->gc_collect();
    assert(vtable_selector_at_v2(enumeration,rooted,0)==a && vtable_selector_at_v2(enumeration,rooted,1)==b);
    api->send(rooted,api->intern("removeMethod:"),&a,1);
    assert(vtable_selector_at_v2(enumeration,rooted,0)==b && !vtable_selector_at_v2(enumeration,rooted,1));
    rooted=NULL;return args[0];
}
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables) {
    (void)presence;(void)exports;assert(selected && !prior && vtables && !observing);
    if(api)assert(api==selected && enumeration==vtables);else { api=selected;enumeration=(VtableEnumerationV2 *)vtables; }
    assert(enumeration->version==2 && enumeration->byte_size==sizeof(*enumeration));
    Dl_info owner;
    assert(dladdr((void *)enumeration->count,&owner) && strstr(owner.dli_fname,"libbirnam_object_model.dylib"));
    assert(dladdr((void *)enumeration->at,&owner) && strstr(owner.dli_fname,"libbirnam_object_model.dylib"));
    assert(dladdr((void *)enumeration->selector_at,&owner) && strstr(owner.dli_fname,"libbirnam_object_model.dylib"));
    if(!rooted_registered) { api->gc_register_root_provider(mark_root);rooted_registered=1; }
    Oop system=namespace_import_v1(imports,"SystemVt");assert(system);
    api->method(system,api->intern("selectorExercise:"),exercise);
    api->method(system,api->intern("selectorObserve"),observe);
    api->method(system,api->intern("selectorRestore"),restore);
    return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
