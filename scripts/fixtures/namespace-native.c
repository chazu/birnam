#define _DARWIN_C_SOURCE
#include "idapi_v8.h"
#include "../../abi/fixtures/namespace-names-v1/namespace_names_v1.h"
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
    assert(api->export_obj("NamespaceFirst",value)==value);
    assert(api->export_obj("NamespaceSecond",value)==value);
    assert(api->export_obj("NamespaceOnly",value)==value);
    assert(count_exports()==before+added && contains(first) && contains(second) && contains(only));
    assert(api->import_fn("NamespaceSecond")==value && api->is_exported("NamespaceOnly"));
    assert(!strcmp(name_of(value),"NamespaceFirst"));
    api->export_obj("NamespaceFirst",api->nil_obj);
    assert(api->is_exported("NamespaceFirst") && api->import_fn("NamespaceFirst")==api->nil_obj);
    assert(!strcmp(name_of(value),"NamespaceSecond"));
    api->gc_collect();
    assert(api->import_fn("NamespaceSecond")==value && api->vtable_of(value));
    Oop globals=api->import_fn("Globals"),missing=api->send(globals,api->intern("at:"),&only,1);
    assert(missing==api->nil_obj); /* Raw publication does not bind Globals. */
    api->export_obj("NamespaceFirst",NULL);api->export_obj("NamespaceSecond",NULL);api->export_obj("NamespaceOnly",NULL);
    assert(!api->import_fn("NamespaceFirst") && !api->is_exported("NamespaceFirst"));
    assert(count_exports()==before+added && contains(first) && contains(second) && contains(only));
    assert(!name_of(value));
    return value;
}
void *module_init(IdApi *selected,void *state) {
    assert(selected && !state);if(api)assert(api==selected);else api=selected;
    Dl_info info;assert(dladdr((void *)api->import_fn,&info) && info.dli_fname);
    void *floor=api->dlopen_fn(info.dli_fname,RTLD_NOW|RTLD_LOCAL);assert(floor);
    NamespaceNamesProviderV1 provide=(NamespaceNamesProviderV1)api->dlsym_fn(floor,"birnam_namespace_names_provider_v1");assert(provide);names=provide();assert(names);
    count_exports=(size_t(*)(void))api->dlsym_fn(floor,"birnam_id_export_count_v1");
    name_at=(Oop(*)(size_t))api->dlsym_fn(floor,"birnam_id_export_name_at_v1");assert(count_exports && name_at);
    api->method(api->import_fn("SystemVt"),api->intern("namespaceExercise:"),exercise);
    api->method(api->import_fn("SystemVt"),api->intern("namespaceObserveNames"),observe_names);
    api->method(api->import_fn("SystemVt"),api->intern("namespaceRename:"),rename_names);
    api->method(api->import_fn("SystemVt"),api->intern("namespaceHide:"),hide_names);
    api->method(api->import_fn("SystemVt"),api->intern("namespaceRestoreNames"),restore_names);
    return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
