#define _DARWIN_C_SOURCE
#include "idapi_v7.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>
static IdApi *api;
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
    assert(!strcmp(api->exported_name_of(value),"NamespaceFirst"));
    api->export_obj("NamespaceFirst",api->nil_obj);
    assert(api->is_exported("NamespaceFirst") && api->import_fn("NamespaceFirst")==api->nil_obj);
    assert(!strcmp(api->exported_name_of(value),"NamespaceSecond"));
    api->gc_collect();
    assert(api->import_fn("NamespaceSecond")==value && api->vtable_of(value));
    Oop globals=api->import_fn("Globals"),missing=api->send(globals,api->intern("at:"),&only,1);
    assert(missing==api->nil_obj); /* Raw publication does not bind Globals. */
    api->export_obj("NamespaceFirst",NULL);api->export_obj("NamespaceSecond",NULL);api->export_obj("NamespaceOnly",NULL);
    assert(!api->import_fn("NamespaceFirst") && !api->is_exported("NamespaceFirst"));
    assert(count_exports()==before+added && contains(first) && contains(second) && contains(only));
    assert(!api->exported_name_of(value));
    return value;
}
void *module_init(IdApi *selected,void *state) {
    assert(selected && !state);if(api)assert(api==selected);else api=selected;
    Dl_info info;assert(dladdr((void *)api->import_fn,&info) && info.dli_fname);
    void *floor=api->dlopen_fn(info.dli_fname,RTLD_NOW|RTLD_LOCAL);assert(floor);
    count_exports=(size_t(*)(void))api->dlsym_fn(floor,"birnam_id_export_count_v1");
    name_at=(Oop(*)(size_t))api->dlsym_fn(floor,"birnam_id_export_name_at_v1");assert(count_exports && name_at);
    api->method(api->import_fn("SystemVt"),api->intern("namespaceExercise:"),exercise);
    return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
