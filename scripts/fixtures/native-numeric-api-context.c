#include "idapi_v7.h"
#include "integer-support.h"
static unsigned boxes;
static Oop (*original_make)(void *, int64_t);
static Oop counted_box(void *context, int64_t value) { ++boxes; return original_make(context, value); }
int main(int argc, char **argv) {
    assert(argc == 3);
    void *floor = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL); assert(floor);
    IdInitFn initialize = (IdInitFn)dlsym(floor, "id_init"); assert(initialize);
    IdApi *floor_api = initialize();
    IdApi module_api = *floor_api;
    void *integer = floor_api->dlopen_fn(argv[2], RTLD_NOW | RTLD_LOCAL); assert(integer);
    ModuleInitFn init = (ModuleInitFn)floor_api->dlsym_fn(integer, "module_init"); assert(init);
    fixture_prepare_float(floor_api, argv[1]);
    fixture_prepare_word(floor_api, argv[1]);
    fixture_attach_float(floor_api, integer);
    init(&module_api, NULL);
    IntegerProviderV1 provide = (IntegerProviderV1)floor_api->dlsym_fn(integer, "birnam_integer_provider_v1"); assert(provide);
    fixture_integer = provide(); assert(fixture_integer);
    fixture_attach_integer(floor_api, floor);
    typedef unsigned (*Query)(const void *, Oop, const uintptr_t *);
    Query query = (Query)floor_api->dlsym_fn(integer, "birnam_native_numeric_semantics_v2"); assert(query);
    assert(!floor_api->dlsym_fn(integer, "birnam_native_numeric_semantics_v1"));
    Oop vt = fixture_integer->behavior;
    Oop selector = floor_api->intern("+");
    Method add = (Method)floor_api->send(vt, floor_api->intern("lookup"), &selector, 1); assert(add);
    original_make = fixture_integer->make;
    uintptr_t functions[] = {
        (uintptr_t)fixture_integer->is_exact, (uintptr_t)fixture_integer->read,
        (uintptr_t)floor_api->is_exported, (uintptr_t)floor_api->import_fn,
        (uintptr_t)floor_api->vtable_of, (uintptr_t)original_make,
        (uintptr_t)fixture_integer->context, (uintptr_t)fixture_integer->storage,
        (uintptr_t)fixture_integer->prototype, (uintptr_t)floor_api->alloc,
    };
    assert(query((void *)add, vt, functions) == 1);
    assert(query((void *)add, fixture_float->behavior, functions) == 0);
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
