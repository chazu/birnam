#include "idapi_v4.h"
#include "integer-support.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>

static IdApi *api;
static void mount(const char *path) {
    void *image = api->dlopen_fn(path, RTLD_NOW | RTLD_LOCAL);
    if (!image) fprintf(stderr, "%s\n", dlerror());
    assert(image);
    ModuleInitFn init = (ModuleInitFn)api->dlsym_fn(image, "module_init");
    assert(init);
    IntegerDependencyV1 attach = (IntegerDependencyV1)api->dlsym_fn(image, "birnam_integer_dependency_v1");
    if (attach) {
        assert(attach(NULL) == 0);
        IntegerApiV1 copied = *fixture_integer;
        copied.version = 999;
        assert(attach(&copied) == 0);
        assert(attach(fixture_integer) == 1 && attach(fixture_integer) == 1);
        copied = *fixture_integer;
        assert(attach(&copied) == 0); /* no silent remount onto another table */
    }
    init(api, NULL);
}
static Method lookup(Oop behavior, const char *name) {
    Oop selector = api->intern(name);
    return (Method)api->send(behavior, api->intern("lookup"), &selector, 1);
}
static Oop send(Oop object, const char *name, Oop arg) {
    return api->send(object, api->intern(name), &arg, 1);
}
static Oop right_identity(Oop receiver, const Oop *args, size_t count) {
    (void)receiver;
    assert(count == 1);
    return args[0];
}
int main(int argc, char **argv) {
    assert(argc == 6);
    void *floor = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    assert(floor);
    IdInitFn initialize = (IdInitFn)dlsym(floor, "id_init");
    assert(initialize);
    api = initialize();
    api->gc_set_auto(0);
    assert(!api->import_fn("Integer"));
    assert(!api->import_fn("IntegerVt"));
    Oop lower = api->alloc(api->object_vt, 0);
    Oop cloned = api->send(lower, api->intern("clone"), NULL, 0);
    assert(cloned != lower && api->vtable_of(cloned) == api->object_vt);
    assert(api->send(lower, api->intern("=="), &lower, 1) == api->true_obj);
    assert(!api->import_fn("Integer"));
    fixture_mount_integer(api, argv[1]);
    assert(lookup(fixture_integer->behavior, "+"));
    mount(argv[3]); /* Errors are independently mounted, without Prelude. */
    const int64_t inputs[] = {INT64_MIN, INT64_MIN + 1, -17, -1, 0, 1, 17,
                              INT64_MAX - 1, INT64_MAX};
    const char *operators[] = {"+", "-", "*", "/", "mod:"};
    unsigned checked = 0;
    for (unsigned i = 0; i < sizeof(inputs)/sizeof(*inputs); ++i) {
        for (unsigned j = 0; j < sizeof(inputs)/sizeof(*inputs); ++j) {
            Oop a = fixture_integer_make(inputs[i]), b = fixture_integer_make(inputs[j]);
            for (unsigned op = 0; op < 5; ++op) {
                __int128 x = inputs[i], y = inputs[j], answer = 0;
                int zero_divide = op >= 3 && y == 0;
                if (!zero_divide) {
                    switch (op) {
                    case 0: answer = x + y; break;
                    case 1: answer = x - y; break;
                    case 2: answer = x * y; break;
                    case 3: answer = x / y; break;
                    default: answer = x % y; break;
                    }
                }
                Oop value = send(a, operators[op], b);
                if (zero_divide || answer < INT64_MIN || answer > INT64_MAX) {
                    assert(api->vtable_of(value) == api->import_fn("ErrorVt"));
                    Oop kind = api->send(value, api->intern("kind"), NULL, 0);
                    assert(kind == api->intern(zero_divide ? "zeroDivide" : "overflow"));
                    assert(send(a, operators[op], value) == value);
                } else {
                    assert(fixture_is_integer(value));
                    assert(fixture_integer_value(value) == (int64_t)answer);
                    assert(value != a && value != b);
                }
                assert(fixture_integer_value(a) == inputs[i]);
                assert(fixture_integer_value(b) == inputs[j]);
                ++checked;
            }
        }
    }
    Oop two = fixture_integer_make(2), half = api->mk_float(0.5);
    Oop mixed = send(two, "+", half);
    assert(api->is_float(mixed) && api->float_value(mixed) == 2.5);
    Method original = lookup(fixture_integer->behavior, "+");
    api->method(fixture_integer->behavior, api->intern("+"), right_identity);
    assert(send(two, "+", half) == half);
    mount(argv[4]); /* Private behavior is a prerequisite of Prelude. */
    mount(argv[5]);
    assert(lookup(fixture_integer->behavior, "+") == right_identity);
    assert(send(two, "+", half) == half);
    api->method(fixture_integer->behavior, api->intern("+"), original);
    assert(fixture_integer_value(send(two, "+", two)) == 4);
    /* Exact recognition remains distinct from inherited image handling. */
    Oop child = api->proto(fixture_integer->behavior);
    Oop object = fixture_integer_make(-42);
    Oop args[] = {object, child};
    api->send(api->vtable_vt, api->intern("reassociateBehaviorOf:with:"), args, 2);
    assert(!fixture_is_integer(object));
    assert(fixture_integer_value(object) == -42);
    assert(!fixture_is_integer(NULL) && !fixture_is_integer((void *)(uintptr_t)0x12345));
    /* Module roots survive namespace rebinding in precise collection. */
    Oop prototype = fixture_integer->prototype, behavior = fixture_integer->behavior;
    api->export_obj("Integer", api->nil_obj);
    api->export_obj("IntegerVt", api->nil_obj);
    api->gc_set_precise(1); api->gc_set_stack_bottom(0); api->gc_collect();
    assert(api->vtable_of(prototype) == behavior && fixture_integer_value(prototype) == 0);
    assert(fixture_integer_value(fixture_integer_make(INT64_MIN)) == INT64_MIN);
    api->export_obj("Integer", prototype); api->export_obj("IntegerVt", behavior);
    printf("integer-module: %u oracle cases, mixed values, identity, errors and replacement before/after Prelude passed\n", checked);
    return 0;
}
