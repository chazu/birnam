/* Independent old/new oracle for caller export selection during construction.
   Compile this source in the corresponding historical/current fixture tree. */
#ifdef FIXTURE_IDAPI_V10
#include "idapi_v10.h"
#else
#include "idapi_v14.h"
#endif
#include "namespace-support.h"
#include "integer-support.h"
#include "trace-support.h"
#include <stdlib.h>
static IdApi *api;
static IdApi selected;
#ifndef FIXTURE_IDAPI_V10
static NamespaceExportV1 exports;
static int first_context, second_context;
#define CONTEXT void *context,
#define CHECK_CONTEXT(expected) assert(context==&(expected))
#else
#define CONTEXT
#define CHECK_CONTEXT(expected) ((void)0)
#endif
static const char *family;
static unsigned publications;
static int during_construction;
static Oop second(CONTEXT const char *name,Oop value);
static void replace(void) {
#ifdef FIXTURE_IDAPI_V10
    selected.export_obj=second;
#else
    exports.export=second;exports.context=&second_context;
#endif
}
static Oop original(const char *name,Oop value) {
#ifdef FIXTURE_IDAPI_V10
    return api->export_obj(name,value);
#else
    return fixture_namespace_export(name,value);
#endif
}
static Oop first(CONTEXT const char *name,Oop value) {
    CHECK_CONTEXT(first_context);
    printf("first %s\n",name);publications++;
    replace();
    (void)api->gc_collect();
    return original(name,value);
}
static Oop second(CONTEXT const char *name,Oop value) {
    CHECK_CONTEXT(second_context);
    printf("second %s\n",name);publications++;
    (void)api->gc_collect();
    return original(name,value);
}
static Oop proto(Oop parent) {
    if(during_construction) { during_construction=0;replace();puts("construction replacement"); }
    return api->proto(parent);
}
static void mount_integer(const char *path,int selected_api) {
    IdApi *table=selected_api?&selected:api;
#ifdef FIXTURE_IDAPI_V10
    fixture_mount_integer(table,path,fixture_namespace_presence(),fixture_namespace_imports());
#else
    fixture_mount_integer(table,path,fixture_namespace_presence(),fixture_namespace_imports(),selected_api?&exports:fixture_namespace_exports());
#endif
}
static void prepare(const char *path) {
#ifdef FIXTURE_IDAPI_V10
    if(!strcmp(family,"Float"))fixture_prepare_float(&selected,path,fixture_namespace_presence(),fixture_namespace_imports());
    else if(!strcmp(family,"Word"))fixture_prepare_word(&selected,path,fixture_namespace_presence(),fixture_namespace_imports());
    else fixture_prepare_string(&selected,path);
#else
    if(!strcmp(family,"Float"))fixture_prepare_float(&selected,path,fixture_namespace_presence(),fixture_namespace_imports(),&exports);
    else if(!strcmp(family,"Word"))fixture_prepare_word(&selected,path,fixture_namespace_presence(),fixture_namespace_imports(),&exports);
    else fixture_prepare_string(&selected,path,&exports);
#endif
}
int main(int argc,char **argv) {
    assert(argc==4);
    family=argv[2];during_construction=!strcmp(argv[3],"construction");
    assert(during_construction || !strcmp(argv[3],"between"));
    assert(!strcmp(family,"Integer") || !strcmp(family,"Float") || !strcmp(family,"Word") || !strcmp(family,"String"));
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);
    api->gc_set_auto(0);uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    fixture_mount_trace(api,argv[1]);
    selected=*api;selected.proto=proto;
#ifdef FIXTURE_IDAPI_V10
    selected.export_obj=first;
#else
    exports=(NamespaceExportV1){1,sizeof(exports),&first_context,first};
#endif
    if(!strcmp(family,"Integer")) {
        /* Other families are prepared before the Integer snapshot under test. */
#ifdef FIXTURE_IDAPI_V10
        fixture_prepare_string(api,argv[1]);
        fixture_prepare_word(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports());
        fixture_prepare_float(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports());
#else
        fixture_prepare_string(api,argv[1],fixture_namespace_exports());
        fixture_prepare_word(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
        fixture_prepare_float(api,argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
#endif
        mount_integer(argv[1],1);
    } else {
        prepare(argv[1]);mount_integer(argv[1],0);
        void *image=!strcmp(family,"Float")?fixture_float_image:!strcmp(family,"Word")?fixture_word_image:fixture_string_image;
        fixture_attach_integer(api,image);
        ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);
        /* Execution must retain the earlier preparation's API/capability. */
#ifdef FIXTURE_IDAPI_V10
        init(&selected,NULL,fixture_namespace_presence(),fixture_namespace_imports());
#else
        init(&selected,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation());
#endif
    }
    assert(publications==2 && !during_construction);
    Oop prototype=fixture_namespace_import(family);assert(prototype);
    char vtable[32];int n=snprintf(vtable,sizeof(vtable),"%sVt",family);assert(n>0 && (size_t)n<sizeof(vtable));
    Oop behavior=fixture_namespace_import(vtable);assert(behavior && api->vtable_of(prototype)==behavior);
    (void)api->gc_collect();assert(fixture_namespace_import(family)==prototype && fixture_namespace_import(vtable)==behavior);
    puts("export selection and rooted identities passed");
}
