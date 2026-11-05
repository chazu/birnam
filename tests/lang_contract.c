#include "birnam_native.h"
#include <assert.h>
#include <stdio.h>

static Oop world, seed, retained_code, retained_closure;
static unsigned retired_code, retired_closures;
static Oop named(const char *name) { return bn_send1(world, "at:", bn_symbol(name)); }
static Oop parse(const char *source) { return bn_send1(seed, "parse:", bn_bytes(source, strlen(source))); }
static Oop eval(const char *source) { return bn_send1(seed, "eval:", bn_bytes(source, strlen(source))); }
static void success(Oop value) { assert(bn_send0(value, "isError") == named("false")); }
static void number(Oop value, const char *expected) {
    Oop bytes = bn_send0(bn_send0(value, "printString"), "asBytes");
    assert(bn_header(bytes)->payload_bytes == strlen(expected));
    assert(memcmp(bytes, expected, strlen(expected)) == 0);
}
static void finalized(void *object, void *counter) { ++*(unsigned *)counter; }
__attribute__((noinline)) static void discarded_batch(void) {
    for (unsigned i = 0; i < 25; ++i) {
        Oop code = parse("[1 + 2]");
        success(code);
        GC_register_finalizer_no_order(bn_header(code), finalized, &retired_code, NULL, NULL);
        Oop closure = eval("[[:x | [| x]] value: 42]");
        success(closure);
        GC_register_finalizer_no_order(bn_header(closure), finalized, &retired_closures, NULL, NULL);
    }
}
Oop birnam_module_init_v1(Oop m, Oop receiver, Oop args) {
    world = receiver;
    seed = named("Seed");
    retained_code = parse("[1 + 2]");
    retained_closure = eval("[[:x | [| x]] value: 42]");
    success(retained_code);
    success(retained_closure);
    GC_gcollect();
    size_t before = GC_get_heap_size();
    for (unsigned batch = 0; batch < 4; ++batch) {
        discarded_batch();
        GC_gcollect(); GC_invoke_finalizers();
    }
    for (unsigned i = 0; i < 5; ++i) { GC_gcollect(); GC_invoke_finalizers(); }
    assert(retired_code >= 90 && retired_closures >= 90);
    assert(GC_get_heap_size() <= before + 16 * 1024 * 1024);
    Oop env = bn_send1(named("Environment"), "forNamespace:", world);
    number(bn_send1(retained_code, "evalIn:", env), "3");
    number(bn_send0(retained_closure, "value"), "42");
    assert(bn_send0(named("NativeParserGuard"), "untouched") == named("true"));
    printf("PASS Lang GC: %u/100 syntax roots and %u/100 closures finalized; retained code/captures survive; heap %zu -> %zu bytes; no native parser calls\n",
           retired_code, retired_closures, before, GC_get_heap_size());
    return world;
}
