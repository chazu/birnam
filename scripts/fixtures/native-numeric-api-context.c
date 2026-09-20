#include "idapi_v17.h"
#include "namespace-support.h"
#include "integer-support.h"
static int forbidden_presence(void *context,const char *name) { (void)context;(void)name;assert(!"numeric admission invoked presence");return 0; }
static Oop forbidden_import(void *context,const char *name) { (void)context;(void)name;assert(!"numeric admission invoked import");return NULL; }
static unsigned boxes;
static Oop (*original_make)(void *, int64_t);
static Oop counted_box(void *context, int64_t value) { ++boxes; return original_make(context, value); }
int main(int argc, char **argv) {
    assert(argc == 3);
    void *floor = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL); assert(floor);
    IdInitFn initialize = (IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")); assert(initialize);
    IdApi *floor_api = initialize();
    IdApi module_api = *floor_api;
    NamespacePresenceV1 module_presence=*fixture_namespace_presence();
    NamespaceImportV1 module_imports=*fixture_namespace_imports();
    void *integer = floor_api->dlopen_fn(argv[2], RTLD_NOW | RTLD_LOCAL); assert(integer);
    ModuleInitFn init = (ModuleInitFn)floor_api->dlsym_fn(integer, "module_init"); assert(init);
    fixture_prepare_float(floor_api, argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());
    fixture_prepare_word(floor_api, argv[1],fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_root_registry());
    assert(strlen(argv[1])<sizeof(fixture_floor_path));
    strcpy(fixture_floor_path,argv[1]);
    fixture_attach_integer(floor_api, integer,fixture_root_registry());
    init(&module_api,NULL,&module_presence,&module_imports,fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),fixture_payload_layout());
    IntegerProviderV1 provide = (IntegerProviderV1)floor_api->dlsym_fn(integer, "birnam_integer_provider_v1"); assert(provide);
    fixture_integer = provide(); assert(fixture_integer);
    fixture_attach_integer(floor_api, floor,fixture_root_registry());
    typedef unsigned (*Query)(const void *, Oop, const uintptr_t *);
    Query query = (Query)floor_api->dlsym_fn(integer, "birnam_native_numeric_semantics_v4"); assert(query);
    assert(!floor_api->dlsym_fn(integer, "birnam_native_numeric_semantics_v1"));
    Oop vt = fixture_integer->behavior;
    Oop selector = floor_api->intern("+");
    Method add = (Method)floor_api->send(vt, floor_api->intern("lookup"), &selector, 1); assert(add);
    original_make = fixture_integer->make;
    uintptr_t functions[] = {
        (uintptr_t)fixture_integer->is_exact, (uintptr_t)fixture_integer->read,
        (uintptr_t)fixture_namespace_presence()->contains, (uintptr_t)fixture_namespace_imports()->import,
        (uintptr_t)floor_api->vtable_of, (uintptr_t)original_make,
        (uintptr_t)fixture_integer->context, (uintptr_t)fixture_integer->storage,
        (uintptr_t)fixture_integer->prototype, (uintptr_t)floor_api->alloc,
        (uintptr_t)fixture_namespace_presence()->context,
        (uintptr_t)fixture_namespace_imports()->context,
    };
    assert(query((void *)add, vt, functions) == 1);
    assert(query((void *)add, fixture_float->behavior, functions) == 0);
    NamespacePresenceV1 original_presence=module_presence;
    module_presence.contains=forbidden_presence;
    assert(query((void *)add, vt, functions) == 0);
    module_presence=original_presence;
    uintptr_t independent_context=0;
    module_presence.context=&independent_context;
    assert(query((void *)add, vt, functions) == 0);
    module_presence=original_presence;
    assert(query((void *)add, vt, functions) == 1);
    NamespaceImportV1 original_imports=module_imports;
    module_imports.import=forbidden_import;
    assert(query((void *)add, vt, functions)==0);
    module_imports=original_imports;
    module_imports.context=&independent_context;
    assert(query((void *)add, vt, functions)==0);
    module_imports.import=forbidden_import;
    assert(query((void *)add, vt, functions)==0);
    module_imports=original_imports;
    assert(query((void *)add, vt, functions)==1);
    assert(!floor_api->dlsym_fn(integer,"birnam_native_numeric_semantics_v3"));
    assert(!floor_api->dlsym_fn(integer,"birnam_native_numeric_semantics_v2"));
    Oop seven = fixture_integer_make(7);
    fixture_integer->make = counted_box;
    assert(query((void *)add, vt, functions) == 0);
    Oop answer = add(seven, &seven, 1);
    assert(fixture_integer_value(answer) == 14 && boxes == 1);
    fixture_integer->make = original_make;
    assert(query((void *)add, vt, functions) == 1);
    for (unsigned i = 0; i < sizeof(functions)/sizeof(*functions); ++i) {
        uintptr_t saved = functions[i]; functions[i] = 0;
        assert(query((void *)add, vt, functions) == 0);
        functions[i] = saved;
    }
    puts("native-numeric-api-context: copied Id API, live Integer wrapper, all context operands and restoration passed");
}
