#ifdef FIXTURE_IDAPI_V12
#include "idapi_v12.h"
#define Enumeration VtableEnumerationV1
#define CONTEXT
#define CHECK_CONTEXT(expected) ((void)(expected))
#else
#include "idapi_v15.h"
#define Enumeration VtableEnumerationV2
#define CONTEXT void *context,
#define CHECK_CONTEXT(expected) assert(context==(expected))
#endif
#include "namespace-support.h"
#include "object-model-support.h"
#include "trace-support.h"
#ifdef FIXTURE_IDAPI_V12
#define OBSERVATION_ARGUMENT
#else
#define OBSERVATION_ARGUMENT ,fixture_method_observation(),fixture_root_registry()
#endif
static IdApi *api,selected,other;
static Enumeration enumeration,alternate;
static int first_context,second_context,active;
static Oop roots[4];
static void mark_roots(MarkFn mark) { for(size_t i=0;i<4;i++)mark(roots[i]); }
static Oop answer(Oop self,const Oop *args,size_t count) { (void)args;(void)count;return self; }
static Oop forbidden(CONTEXT Oop vtable,size_t index) {
#ifndef FIXTURE_IDAPI_V12
    (void)context;
#endif
    (void)vtable;(void)index;assert(!"selected later registration");return NULL;
}
static Oop second(CONTEXT Oop vtable,size_t index) {
    CHECK_CONTEXT(&second_context);assert(active && vtable==roots[0] && index>0 && index<=2);
    printf("second selector %zu\n",index);return index==1?roots[3]:NULL;
}
static Oop first(CONTEXT Oop vtable,size_t index) {
    CHECK_CONTEXT(&first_context);assert(active && vtable==roots[0] && index==0);
    puts("first selector 0");
#ifdef FIXTURE_IDAPI_V12
    selected.vtable_selector_at=second;
#else
    enumeration.context=&second_context;enumeration.selector_at=second;
#endif
    api->gc_collect();return roots[2];
}
static size_t count(void *context) { (void)context;assert(active);puts("count");return 1; }
static Oop at(void *context,size_t index) { (void)context;assert(active && index==0);puts("vtable 0");return roots[0]; }
static void begin(void) {
    active=1;
#ifdef FIXTURE_IDAPI_V12
    selected.vtable_selector_at=first;
#else
    enumeration.context=&first_context;enumeration.selector_at=first;
#endif
}
int main(int argc,char **argv) {
    assert(argc==3);setvbuf(stdout,NULL,_IONBF,0);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);fixture_prepare_namespace(floor,argv[1]);api=((IdInitFn)dlsym(floor,"id_init"))();
    fixture_mount_object_model(api,argv[1],fixture_root_registry());fixture_mount_trace(api,argv[1],fixture_root_registry());api->gc_set_auto(0);
    uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));fixture_register_root(mark_roots);
    selected=*api;enumeration=*fixture_vtable_enumeration();
    void *store_image=dlopen(argv[2],RTLD_NOW|RTLD_LOCAL);assert(store_image);fixture_attach_integer(api,store_image,fixture_root_registry());
    ModuleInitFn init=(ModuleInitFn)dlsym(store_image,"module_init");assert(init);
    init(&selected,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),&enumeration OBSERVATION_ARGUMENT);
    other=selected;alternate=enumeration;
#ifdef FIXTURE_IDAPI_V12
    other.vtable_selector_at=forbidden;
#else
    alternate.selector_at=forbidden;
#endif
    init(&other,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),&alternate OBSERVATION_ARGUMENT);
    roots[0]=api->proto(api->object_vt);roots[1]=api->alloc(roots[0],0);set_owner(api,roots[0],roots[1]);
    roots[2]=api->intern("selector-first");roots[3]=api->intern("selector-second");
    api->method(roots[0],roots[2],answer);api->method(roots[0],roots[3],answer);
    Oop store=fixture_namespace_import("Store");assert(store);
    Oop basis_selector=api->intern("basis"),retained_selector=api->intern("oldestRetainedBasis");
    int64_t basis=fixture_integer_value(api->send(store,basis_selector,NULL,0));
    int64_t retained=fixture_integer_value(api->send(store,retained_selector,NULL,0));
    enumeration.count=count;enumeration.at=at;
    puts("backfill live selector/context selection");begin();assert(api->send(store,api->intern("backfillMirror"),NULL,0)==store);active=0;
    assert(fixture_integer_value(api->send(store,basis_selector,NULL,0))==basis);
    assert(fixture_integer_value(api->send(store,retained_selector,NULL,0))==retained);
    puts("owner refusal skips selector callback");
#ifdef FIXTURE_IDAPI_V12
    selected.vtable_selector_at=forbidden;
#else
    enumeration.selector_at=forbidden;
#endif
    set_owner(api,roots[0],NULL);
    Oop refused[]={roots[3],roots[0]};assert(api->send(store,api->intern("removeMethod:on:"),refused,2)==api->nil_obj);
    set_owner(api,roots[0],roots[1]);
    puts("missing direct method scans to null");begin();
    Oop missing[]={api->intern("selector-missing"),roots[0]};assert(api->send(store,api->intern("removeMethod:on:"),missing,2)==api->nil_obj);active=0;
    puts("remove uses first saved execution and fresh selector/context");begin();
    Oop args[]={roots[3],roots[0]};assert(api->send(store,api->intern("removeMethod:on:"),args,2)==store);active=0;
    Oop lookup=api->intern("lookup");assert(!api->send(roots[0],lookup,&roots[3],1));
    assert(api->send(roots[1],roots[2],NULL,0)==roots[1]);
    puts("selector consumers: backfill basis/retention, live replacement, GC, reload and removal passed");
}
