#define _DARWIN_C_SOURCE
#include "idapi_v14.h"
#include "namespace-support.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>
static IdApi *api;
static NamespaceNamesApiV1 *names;
static const char *name_of(Oop value) { NamespaceNamesApiV1 selected=*names;return selected.name_of(selected.context,value); }
static struct { NamespaceNamesApiV1 original; size_t calls; Oop target; int active, hide; } observation;
static const char *observe_name(void *context,Oop value) {
    assert(context==&observation && observation.active);observation.calls++;
    if(observation.target && value==observation.target)return observation.hide?NULL:"NamespaceReplacement";
    return observation.original.name_of(observation.original.context,value);
}
static Oop observe_names(Oop self,const Oop *args,size_t count) {
    (void)args;assert(count==0 && !observation.active);
    observation.original=*names;observation.calls=0;observation.target=NULL;observation.hide=0;observation.active=1;
    names->context=&observation;names->name_of=observe_name;return self;
}
static Oop rename_names(Oop self,const Oop *args,size_t count) {
    assert(count==1);observe_names(self,NULL,0);observation.target=args[0];return self;
}
static Oop hide_names(Oop self,const Oop *args,size_t count) {
    rename_names(self,args,count);observation.hide=1;return self;
}
static Oop restore_names(Oop self,const Oop *args,size_t count) {
    (void)self;(void)args;assert(count==0 && observation.active);
    *names=observation.original;observation.active=0;
    return observation.calls?api->true_obj:api->false_obj;
}
static size_t (*count_exports)(void);
static Oop (*name_at)(size_t);
static int contains(Oop name) {
    for(size_t i=0;i<count_exports();i++)if(name_at(i)==name)return 1;
    return 0;
}
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(count==1 && args && args[0]);Oop value=args[0];
    Oop first=api->intern("NamespaceFirst"),second=api->intern("NamespaceSecond"),only=api->intern("NamespaceOnly");
    size_t before=count_exports(),added=(!contains(first))+(!contains(second))+(!contains(only));
    assert(fixture_namespace_export("NamespaceFirst",value)==value);
    assert(fixture_namespace_export("NamespaceSecond",value)==value);
    assert(fixture_namespace_export("NamespaceOnly",value)==value);
    assert(count_exports()==before+added && contains(first) && contains(second) && contains(only));
    assert(fixture_namespace_import("NamespaceSecond")==value && fixture_namespace_contains("NamespaceOnly"));
    assert(!strcmp(name_of(value),"NamespaceFirst"));
    fixture_namespace_export("NamespaceFirst",api->nil_obj);
    assert(fixture_namespace_contains("NamespaceFirst") && fixture_namespace_import("NamespaceFirst")==api->nil_obj);
    assert(!strcmp(name_of(value),"NamespaceSecond"));
    api->gc_collect();
    assert(fixture_namespace_import("NamespaceSecond")==value && api->vtable_of(value));
    Oop globals=fixture_namespace_import("Globals"),missing=api->send(globals,api->intern("at:"),&only,1);
    assert(missing==api->nil_obj); /* Raw publication does not bind Globals. */
    fixture_namespace_export("NamespaceFirst",NULL);fixture_namespace_export("NamespaceSecond",NULL);fixture_namespace_export("NamespaceOnly",NULL);
    assert(!fixture_namespace_import("NamespaceFirst") && !fixture_namespace_contains("NamespaceFirst"));
    assert(count_exports()==before+added && contains(first) && contains(second) && contains(only));
    assert(!name_of(value));
    return value;
}
void *module_init(IdApi *selected,void *state,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation) { (void)observation; (void)vtables;
    fixture_exports=(NamespaceExportV1 *)exports;
    fixture_presence=(NamespacePresenceV1 *)presence;
    fixture_imports=(NamespaceImportV1 *)imports;
    assert(selected && !state);if(api)assert(api==selected);else api=selected;
    Dl_info info;assert(dladdr((void *)api->dlsym_fn,&info) && info.dli_fname);
    void *floor=api->dlopen_fn(info.dli_fname,RTLD_NOW|RTLD_LOCAL);assert(floor);
    NamespaceNamesProviderV1 provide=(NamespaceNamesProviderV1)api->dlsym_fn(floor,"birnam_namespace_names_provider_v1");assert(provide);names=provide();assert(names);
    const char *slash=strrchr(info.dli_fname,'/');assert(slash);char path[PATH_MAX];
    int length=snprintf(path,sizeof(path),"%.*s/libbirnam_namespace.dylib",(int)(slash-info.dli_fname),info.dli_fname);
    assert(length>0 && (size_t)length<sizeof(path));
    void *namespace=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(namespace);
    NamespaceProviderV8 factory=(NamespaceProviderV8)api->dlsym_fn(namespace,"birnam_namespace_provider_v8");assert(factory);
    const NamespaceApiV8 *operations=factory();assert(operations && operations->version==8 && operations->byte_size==sizeof(*operations));
    assert(dladdr((void *)operations->lookup,&info) && strstr(info.dli_fname,"libbirnam_namespace.dylib"));
    assert(dladdr((void *)operations->publish,&info) && strstr(info.dli_fname,"libbirnam_namespace.dylib"));
    assert(dladdr((void *)imports->import,&info) && strstr(info.dli_fname,"libbirnam_namespace.dylib"));
    assert(dladdr((void *)exports->export,&info) && strstr(info.dli_fname,"libbirnam_namespace.dylib"));
    fixture_namespace_floor=floor;count_exports=fixture_namespace_count;name_at=fixture_namespace_name_at;
    api->method(fixture_namespace_import("SystemVt"),api->intern("namespaceExercise:"),exercise);
    api->method(fixture_namespace_import("SystemVt"),api->intern("namespaceObserveNames"),observe_names);
    api->method(fixture_namespace_import("SystemVt"),api->intern("namespaceRename:"),rename_names);
    api->method(fixture_namespace_import("SystemVt"),api->intern("namespaceHide:"),hide_names);
    api->method(fixture_namespace_import("SystemVt"),api->intern("namespaceRestoreNames"),restore_names);
    return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
