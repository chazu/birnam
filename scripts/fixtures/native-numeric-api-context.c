#include "idapi_v3.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>

static IdApi *floor_api;
static unsigned boxes;
static Oop counted_box(int64_t value) { ++boxes; return floor_api->mk_smallint(value); }

int main(int argc, char **argv) {
    assert(argc == 3);
    void *floor = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    assert(floor);
    IdInitFn initialize = (IdInitFn)dlsym(floor, "id_init");
    assert(initialize);
    floor_api = initialize();
    IdApi module_api = *floor_api;
    void *integer = floor_api->dlopen_fn(argv[2], RTLD_NOW | RTLD_LOCAL);
    assert(integer);
    ModuleInitFn module_init = (ModuleInitFn)floor_api->dlsym_fn(integer, "module_init");
    assert(module_init);
    module_init(&module_api, NULL);
    typedef unsigned (*Query)(const void *, Oop, const uintptr_t *);
    Query query = (Query)floor_api->dlsym_fn(integer, "birnam_native_numeric_semantics_v1");
    assert(query);
    Oop selector = floor_api->intern("+");
    Method add = (Method)floor_api->send(floor_api->smallint_vt, floor_api->intern("lookup"), &selector, 1);
    assert(add);
    uintptr_t functions[] = {
        (uintptr_t)floor_api->is_smallint, (uintptr_t)floor_api->smallint_value,
        (uintptr_t)floor_api->is_exported, (uintptr_t)floor_api->import_fn,
        (uintptr_t)floor_api->vtable_of, (uintptr_t)floor_api->mk_smallint,
    };
    assert(query((void *)add, floor_api->smallint_vt, functions) == 1);
    assert(query((void *)add, floor_api->float_vt, functions) == 0);
    module_api.mk_smallint = counted_box;
    assert(query((void *)add, floor_api->smallint_vt, functions) == 0);
    Oop seven = floor_api->mk_smallint(7);
    Oop answer = add(seven, &seven, 1);
    assert(floor_api->smallint_value(answer) == 14 && boxes == 1);
    module_api.mk_smallint = floor_api->mk_smallint;
    assert(query((void *)add, floor_api->smallint_vt, functions) == 1);
    for (unsigned i = 0; i < 6; ++i) {
        uintptr_t saved = functions[i];
        functions[i] = 0;
        assert(query((void *)add, floor_api->smallint_vt, functions) == 0);
        functions[i] = saved;
    }
    puts("native-numeric-api-context: copied API, boxing override and restored context passed");
    return 0;
}
