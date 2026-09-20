/* Compile against the corresponding independent historical/current fixture tree. */
#ifdef FIXTURE_IDAPI_V11
#include "idapi_v11.h"
#define CONTEXT
#define CHECK_CONTEXT() ((void)0)
#else
#include "idapi_v17.h"
#define CONTEXT void *context,
#define CHECK_CONTEXT() assert(context==&context_token)
static int context_token;
static VtableEnumerationV2 selected_vtables,other_vtables;
#endif
#include "namespace-support.h"
#include "object-model-support.h"
#include "trace-support.h"
static IdApi *api,selected,other;
static Oop roots[8],parent_selector,direct_selector;
static size_t count_result,count_calls,parent_calls;
static int watching,replace_at,cycle,unit_probe;
static void mark_roots(MarkFn mark) { for(size_t i=0;i<8;i++)mark(roots[i]); }
static size_t forbidden_count(
#ifndef FIXTURE_IDAPI_V11
    void *context
#else
    void
#endif
) {
#ifndef FIXTURE_IDAPI_V11
    (void)context;
#endif
    assert(0);return 0;
}
static size_t observed_count(
#ifndef FIXTURE_IDAPI_V11
    void *context
#else
    void
#endif
) {
    CHECK_CONTEXT();count_calls++;printf("count %zu\n",count_result);return count_result;
}
static Oop second_at(CONTEXT size_t index) {
    CHECK_CONTEXT();printf("second at %zu\n",index);assert(index==1);return roots[1];
}
static Oop first_at(CONTEXT size_t index) {
    CHECK_CONTEXT();printf("first at %zu\n",index);assert(index==0);return roots[0];
}
static void select_count(int forbidden) {
#ifdef FIXTURE_IDAPI_V11
    selected.vtable_count=forbidden?forbidden_count:observed_count;
#else
    selected_vtables.count=forbidden?forbidden_count:observed_count;
#endif
}
static void select_at(int second) {
#ifdef FIXTURE_IDAPI_V11
    selected.vtable_at=second?second_at:first_at;
#else
    selected_vtables.at=second?second_at:first_at;
#endif
}
static Oop send(Oop receiver,Oop selector,const Oop *args,size_t count) {
    if(watching && receiver==api->vtable_vt && selector==parent_selector) {
        assert(count==1);parent_calls++;puts("parent");
        if(replace_at) {
            replace_at=0;select_at(1);select_count(1);
            roots[2]=api->proto(api->object_vt);api->gc_collect();
        }
        if(cycle)return args[0];
    }
    return api->send(receiver,selector,args,count);
}
static Oop vtable(Oop object) {
    if(watching && unit_probe && object==roots[5])return roots[4];
    return api->vtable_of(object);
}
static Oop selector_at(CONTEXT Oop object,size_t index) {
#ifndef FIXTURE_IDAPI_V11
    (void)context;
#endif
    if(watching) { assert(object==roots[0] || object==roots[1]);printf("selector %zu\n",index); }
#ifdef FIXTURE_IDAPI_V11
    return api->vtable_selector_at(object,index);
#else
    return vtable_selector_at_v2(fixture_vtable_enumeration(),object,index);
#endif
}
static Oop answer(Oop self,const Oop *args,size_t count) { (void)args;(void)count;return self; }
static void initialize(void *image,IdApi *table,int alternate) {
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init");assert(init);
#ifdef FIXTURE_IDAPI_V11
    (void)alternate;init(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
#else
    init(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),alternate?&other_vtables:&selected_vtables,fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout());
#endif
}
static void *mount(const char *path) {
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    fixture_attach_integer(api,image,fixture_root_registry());initialize(image,&selected,0);return image;
}
static Oop invoke(Oop object,const char *selector) { return api->send(object,api->intern(selector),NULL,0); }
static Method method(Oop object,const char *name) {
    Oop selector=api->intern(name);
    Method result=(Method)api->send(api->vtable_of(object),api->intern("lookup"),&selector,1);assert(result);return result;
}
static void begin(const char *name,size_t bound) {
    puts(name);count_result=bound;count_calls=parent_calls=0;select_count(0);watching=1;
}
static void end(size_t counts,size_t parents) {
    watching=0;assert(count_calls==counts && parent_calls==parents);
}
int main(int argc,char **argv) {
    assert(argc==5);setvbuf(stdout,NULL,_IONBF,0);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);fixture_prepare_namespace(floor,argv[1]);
    IdInitFn init=(IdInitFn)dlsym(floor,"id_init");assert(init);api=init();assert(api);
    fixture_mount_object_model(api,argv[1],fixture_root_registry());fixture_mount_trace(api,argv[1],fixture_root_registry());fixture_set_auto(0);
    uintptr_t anchor=0;api->gc_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    fixture_register_root(mark_roots);
    selected=*api;selected.send=send;selected.vtable_of=vtable;
#ifndef FIXTURE_IDAPI_V11
    selected_vtables=*fixture_vtable_enumeration();selected_vtables.selector_at=selector_at;
#else
    selected.vtable_selector_at=selector_at;
#endif
    void *store_image=mount(argv[2]);mount(argv[3]);mount(argv[4]);
    other=selected;
#ifdef FIXTURE_IDAPI_V11
    other.vtable_count=forbidden_count;
#else
    other_vtables=selected_vtables;other_vtables.count=forbidden_count;
#endif
    initialize(store_image,&other,1);
    parent_selector=api->intern("parentOf:");direct_selector=api->intern("enumerationProbe");
    for(size_t i=0;i<2;i++) { roots[i]=api->proto(api->object_vt);api->method(roots[i],direct_selector,answer); }
    Oop store=fixture_namespace_import("Store");assert(store);
    int64_t basis=fixture_integer_value(invoke(store,"basis"));
    int64_t retained=fixture_integer_value(invoke(store,"oldestRetainedBasis"));
#ifndef FIXTURE_IDAPI_V11
    selected_vtables.context=&context_token;
#endif
    select_at(0);replace_at=1;
    begin("store live selection and saved execution",2);assert(invoke(store,"backfillMirror")==store);end(1,2);
    assert(fixture_integer_value(invoke(store,"basis"))==basis);
    assert(fixture_integer_value(invoke(store,"oldestRetainedBasis"))==retained);
    puts("mirror basis and retention unchanged");
    Oop codegen=fixture_namespace_import("RustCodegen");assert(codegen);
    Method diagnostic=method(codegen,"liveVtableCount");
    begin("diagnostic arity refusal",0);assert(diagnostic(codegen,NULL,1)==api->nil_obj);end(0,0);
    begin("diagnostic clamping",SIZE_MAX);assert(fixture_integer_value(diagnostic(codegen,NULL,0))==INT64_MAX);end(1,0);
    void *tcc=dlopen(argv[3],RTLD_NOW|RTLD_LOCAL);assert(tcc);
    int64_t (*is_address)(Oop)=(int64_t (*)(Oop))dlsym(tcc,"birnam_isForeignAddress");assert(is_address);
    Oop family=fixture_namespace_import("ForeignAddressVt");assert(family);
    roots[4]=api->proto(family);roots[3]=api->alloc(roots[4],2*sizeof(uintptr_t));
    ((uintptr_t *)roots[3])[0]=123;((uintptr_t *)roots[3])[1]=1;
    begin("foreign early refusals",0);assert(!is_address(NULL) && !is_address(api->nil_obj));end(0,0);
    begin("foreign count plus one refusal",0);assert(!is_address(roots[3]));end(1,1);
    begin("foreign count plus one acceptance",1);assert(is_address(roots[3]));end(1,1);
    cycle=1;begin("foreign cycle bound",2);assert(!is_address(roots[3]));end(1,3);cycle=0;
    /* A real Unit supplies the existing payload extent. An invalid parent chain
       must be bounded before attempting to use its unconfigured native state. */
    Oop tcc_object=fixture_namespace_import("Tcc");assert(tcc_object);
    roots[5]=invoke(tcc_object,"new");assert(roots[5] && roots[5]!=api->nil_obj);
    Method bind=method(roots[5],"bindVtableCache");
    begin("unit arity refusal",2);assert(bind(roots[5],NULL,1)==api->nil_obj);end(0,0);
    unit_probe=cycle=1;begin("unit count without addition",2);assert(bind(roots[5],NULL,0)==api->nil_obj);end(1,2);
    watching=unit_probe=cycle=0;
    puts("consumer selection, early refusals, clamping and traversal bounds passed");
}
