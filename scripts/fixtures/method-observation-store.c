#ifdef FIXTURE_IDAPI_V13
#include "idapi_v13.h"
#define CONTEXT
#define CONTEXT_CHECK(expected) ((void)(expected))
#else
#include "idapi_v14.h"
#define CONTEXT void *context,
#define CONTEXT_CHECK(expected) assert(context==(expected))
#endif
#include "namespace-support.h"
#include "object-model-support.h"
#include "trace-support.h"
static IdApi *api,first,second;
static ModuleInitFn initialize;
static int mode,registered,hook_first,hook_second,methods,first_context,second_context;
#ifndef FIXTURE_IDAPI_V13
static MethodObservationV1 first_observation,second_observation;
#endif
static void init(IdApi *table) {
    initialize(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration()
#ifndef FIXTURE_IDAPI_V13
        ,table==&first?&first_observation:&second_observation
#endif
    );
}
static void root_registration(RootProvider provider) {
    assert(!registered++ && methods);puts("root registration after methods");api->gc_register_root_provider(provider);
    if(mode==1) { puts("reenter from root registration");init(&second); }
}
static void replace_real(void (*hook)(Oop,Oop)) {
#ifdef FIXTURE_IDAPI_V13
    api->set_method_overwrite_hook(hook);
#else
    method_observation_replace_v1(fixture_method_observation(),hook);
#endif
}
static void register_first(CONTEXT void (*hook)(Oop,Oop)) {
    CONTEXT_CHECK(&first_context);assert(hook && registered);hook_first++;puts("register first");
    if(mode==2) { puts("reenter from overwrite registration");init(&second); }
    replace_real(hook);
}
static void register_second(CONTEXT void (*hook)(Oop,Oop)) {
    CONTEXT_CHECK(&second_context);assert(hook && registered);hook_second++;puts("register second");replace_real(hook);
}
static void bind(Oop type,Oop selector,Method method) { methods++;api->method(type,selector,method); }
static void forbidden(Oop type,Oop selector,Method method) { (void)type;(void)selector;(void)method;assert(!"selected later execution API"); }
int main(int argc,char **argv) {
    assert(argc==4);setvbuf(stdout,NULL,_IONBF,0);mode=!strcmp(argv[3],"root-reentry")?1:!strcmp(argv[3],"hook-reentry")?2:0;
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);fixture_prepare_namespace(floor,argv[1]);api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);api->gc_set_auto(0);
    fixture_mount_object_model(api,argv[1]);fixture_mount_trace(api,argv[1]);
    void *image=dlopen(argv[2],RTLD_NOW|RTLD_LOCAL);assert(image);fixture_attach_integer(api,image);initialize=(ModuleInitFn)dlsym(image,"module_init");assert(initialize);
    first=*api;first.method=bind;first.gc_register_root_provider=root_registration;second=first;second.method=forbidden;
#ifdef FIXTURE_IDAPI_V13
    first.set_method_overwrite_hook=register_first;second.set_method_overwrite_hook=register_second;
#else
    first_observation=(MethodObservationV1){1,sizeof(first_observation),&first_context,register_first};
    second_observation=(MethodObservationV1){1,sizeof(second_observation),&second_context,register_second};
#endif
    init(&first);init(&second);init(&first);
    assert(registered==1 && hook_first==(mode!=1) && hook_second==(mode==1));
    Oop store=fixture_namespace_import("Store");assert(store && api->send(store,api->intern("basis"),NULL,0));
    puts("Store incoming registration, saved execution, once flags and reentry passed");
}
