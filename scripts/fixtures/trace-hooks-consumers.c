/* Independent old/new saved-capability and live callback selection oracle. */
#ifdef TRACE_HOOKS_BASELINE
#include "idapi_v17.h"
#else
#include "idapi_v19.h"
#endif
#include "namespace-support.h"
#include "object-model-support.h"

static IdApi *api;
static unsigned phase,count,expected_count;
static Oop types[4];
static TraceHook hooks_seen[4];
static void apply(Oop vt,TraceHook hook) {
#ifdef TRACE_HOOKS_BASELINE
    api->set_trace_hook(vt,hook);
#else
    fixture_set_trace_hook(vt,hook);
#endif
}
static void first(Oop vt,TraceHook hook) {
    assert(phase==0 && count<expected_count);
    types[count]=vt;hooks_seen[count]=hook;count++;apply(vt,hook);
}
static void saved(Oop vt,TraceHook hook) {
    assert(phase && count<expected_count && types[count]==vt && hooks_seen[count]==hook);
    count++;apply(vt,hook);
}
static void forbidden(Oop vt,TraceHook hook) { (void)vt;(void)hook;assert(!"selected incoming hook registration"); }
#ifndef TRACE_HOOKS_BASELINE
static int first_context,saved_context,other_context;
static void first_set(void *context,void *vt,TraceHookFnV1 hook) { assert(context==&first_context);first(vt,hook); }
static void saved_set(void *context,void *vt,TraceHookFnV1 hook) { assert(context==&saved_context);saved(vt,hook); }
static void other_set(void *context,void *vt,TraceHookFnV1 hook) { assert(context==&other_context);forbidden(vt,hook); }
#endif
int main(int argc,char **argv) {
    assert(argc==4);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);
    fixture_set_auto(0);fixture_mount_object_model(api,argv[1],fixture_root_registry());
    int portable=!strcmp(argv[3],"portable");
    if(portable) {
        const char *dependencies[]={"collection","error"};
        const char *slash=strrchr(argv[1],'/');assert(slash);char path[PATH_MAX];
        for(size_t i=0;i<2;i++) {
            int n=snprintf(path,sizeof(path),"%.*s/libbirnam_%s.dylib",(int)(slash-argv[1]),argv[1],dependencies[i]);assert(n>0 && (size_t)n<sizeof(path));
            void *dependency=dlopen(path,RTLD_NOW|RTLD_LOCAL);
            if(!dependency) { n=snprintf(path,sizeof(path),"%.*s/libbirnam_id_%s.dylib",(int)(slash-argv[1]),argv[1],dependencies[i]);assert(n>0 && (size_t)n<sizeof(path));dependency=dlopen(path,RTLD_NOW|RTLD_LOCAL); }
            assert(dependency);fixture_attach_integer(api,dependency,fixture_root_registry());
            ModuleInitFn init_dependency=(ModuleInitFn)dlsym(dependency,"module_init");assert(init_dependency);
            init_dependency(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout()
#ifndef TRACE_HOOKS_BASELINE
                ,fixture_trace_hooks(),fixture_finalizers()
#endif
            );
        }
    }
    void *image=dlopen(argv[2],RTLD_NOW|RTLD_LOCAL);assert(image);
    fixture_attach_integer(api,image,fixture_root_registry());
    ModuleInitFn initialize=(ModuleInitFn)dlsym(image,"module_init");assert(initialize);
    IdApi selected=*api,other=*api;
    expected_count=portable?4:(!strcmp(argv[3],"peg")?3:(!strcmp(argv[3],"store")?2:1));
#ifdef TRACE_HOOKS_BASELINE
    selected.set_trace_hook=first;other.set_trace_hook=forbidden;
#define MOUNT(table,cap) initialize(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout())
#else
    TraceHooksV1 selected_hooks={1,sizeof(TraceHooksV1),&first_context,first_set},other_hooks={1,sizeof(TraceHooksV1),&other_context,other_set};
#define MOUNT(table,cap) initialize(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout(),cap,fixture_finalizers())
#endif
    MOUNT(&selected,&selected_hooks);assert(count==expected_count);
#ifdef TRACE_HOOKS_BASELINE
    selected.set_trace_hook=saved;
#else
    selected_hooks.context=&saved_context;selected_hooks.set=saved_set;
#endif
    for(phase=1;phase<=2;phase++) { count=0;MOUNT(&other,&other_hooks);assert(count==expected_count); }
    printf("%s: %u hook families retain first execution selection and observe saved callback replacement on reload\n",argv[3],expected_count);
}
