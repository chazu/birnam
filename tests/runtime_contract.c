#include "birnam_native.h"
#include <assert.h>
#include <stdio.h>

/* Loaded as: siphon runtime.dylib runtime_contract.dylib, with no seed. */
static Oop world;
static Oop named(const char *name) { return bn_send1(world, "at:", bn_symbol(name)); }
static void error_kind(Oop value, const char *kind) {
    assert(bn_send0(value, "isError") == named("true"));
    assert(bn_send0(value, "kind") == bn_symbol(kind));
}
static void text_is(Oop value, const char *expected) {
    assert(bn_send0(value, "isError") == named("false"));
    Oop bytes = bn_send0(value, "asBytes");
    assert(bn_header(bytes)->payload_bytes == strlen(expected));
    assert(memcmp(bytes, expected, strlen(expected)) == 0);
}

static const char *services[] = {"Integer", "String", "Symbol", "Block", "Environment", "Reflect", "System", "Cell"};
#define SERVICE_COUNT (sizeof(services) / sizeof(*services))

Oop birnam_module_init_v1(Oop m, Oop receiver, Oop args) {
    world = receiver;
    Oop bound[SERVICE_COUNT];
    for (size_t i = 0; i < SERVICE_COUNT; ++i) {
        bound[i] = named(services[i]);
        assert(bn_send0(bound[i], "isError") == named("false"));
    }
    error_kind(named("Seed"), "unknownName");

    Oop integer = bn_send1(named("Integer"), "fromBytes:", bn_bytes("41", 2));
    Oop one = bn_send1(named("Integer"), "fromBytes:", bn_bytes("1", 1));
    text_is(bn_send0(bn_send1(integer, "+", one), "printString"), "42");
    assert(bn_send0(named("Block"), "value") == named("nil"));

    /* Reload the already loaded runtime by its own retained path. */
    Oop siphon = named("Siphon");
    Oop modules = bn_send0(siphon, "modules");
    assert(((BirnamArguments *)modules)->count >= 1);
    Oop path = bn_send0(bn_arg(modules, 0), "name");
    Oop plus = birnam_bind(integer, bn_symbol("+"));
    Oop value = birnam_bind(named("Block"), bn_symbol("value"));
    Oop reloaded = bn_send1(siphon, "load:", path);
    assert(bn_send0(reloaded, "isError") == named("false"));
    assert(bn_send0(bn_send0(reloaded, "result"), "isError") == named("false"));
    for (size_t i = 0; i < SERVICE_COUNT; ++i) assert(named(services[i]) == bound[i]);
    assert(birnam_bind(integer, bn_symbol("+")) == plus);
    assert(birnam_bind(named("Block"), bn_symbol("value")) == value);
    error_kind(named("Seed"), "unknownName");

    printf("PASS runtime C witnesses without seed: %zu services bound, Seed unbound, Integer arithmetic, "
           "prototype Block value is nil, second load is a no-op\n", (size_t)SERVICE_COUNT);
    return world;
}
