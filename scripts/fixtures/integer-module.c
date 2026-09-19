#include "idapi_v3.h"
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
    assert(!lookup(api->smallint_vt, "+"));
    mount(argv[2]);
    assert(lookup(api->smallint_vt, "+"));
    mount(argv[3]); /* Errors are independently mounted, without Prelude. */
    const int64_t inputs[] = {INT64_MIN, INT64_MIN + 1, -17, -1, 0, 1, 17,
                              INT64_MAX - 1, INT64_MAX};
    const char *operators[] = {"+", "-", "*", "/", "mod:"};
    unsigned checked = 0;
    for (unsigned i = 0; i < sizeof(inputs)/sizeof(*inputs); ++i) {
        for (unsigned j = 0; j < sizeof(inputs)/sizeof(*inputs); ++j) {
            Oop a = api->mk_smallint(inputs[i]), b = api->mk_smallint(inputs[j]);
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
                    assert(api->is_smallint(value));
                    assert(api->smallint_value(value) == (int64_t)answer);
                    assert(value != a && value != b);
                }
                assert(api->smallint_value(a) == inputs[i]);
                assert(api->smallint_value(b) == inputs[j]);
                ++checked;
            }
        }
    }
    Oop two = api->mk_smallint(2), half = api->mk_float(0.5);
    Oop mixed = send(two, "+", half);
    assert(api->is_float(mixed) && api->float_value(mixed) == 2.5);
    Method original = lookup(api->smallint_vt, "+");
    api->method(api->smallint_vt, api->intern("+"), right_identity);
    assert(send(two, "+", half) == half);
    mount(argv[4]); /* Private behavior is a prerequisite of Prelude. */
    mount(argv[5]);
    assert(lookup(api->smallint_vt, "+") == right_identity);
    assert(send(two, "+", half) == half);
    api->method(api->smallint_vt, api->intern("+"), original);
    assert(api->smallint_value(send(two, "+", two)) == 4);
    printf("integer-module: %u oracle cases, mixed values, identity, errors and replacement before/after Prelude passed\n", checked);
    return 0;
}
