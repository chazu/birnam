#include "birnam_native.h"
#include <assert.h>

static unsigned forbidden_calls;
static Oop world;
static Oop named(const char *name) { return bn_send1(world, "at:", bn_symbol(name)); }
static Oop forbidden(Oop m, Oop r, Oop args) {
    ++forbidden_calls;
    return bn_send2(named("Error"), "kind:message:", bn_symbol("nativeParserForbidden"),
                    bn_bytes("native parser called after authored takeover", strlen("native parser called after authored takeover")));
}
static Oop untouched(Oop m, Oop r, Oop args) { return named(forbidden_calls == 0 ? "true" : "false"); }
Oop birnam_module_init_v1(Oop m, Oop receiver, Oop args) {
    world = receiver;
    Oop seed = named("Seed");
    Oop original = birnam_bind(seed, bn_symbol("parse:"));
    /* Disable the selected native Method itself, including captured references,
       not just its dictionary slot. This is test-only envelope instrumentation. */
    bn_header(original)->entry = forbidden;
    Oop text = bn_bytes("1", 1);
    Oop result = birnam_enter(original, seed, bn_arguments(1, &text));
    assert(bn_send0(result, "kind") == bn_symbol("nativeParserForbidden"));
    assert(forbidden_calls == 1);
    forbidden_calls = 0;
    Oop guard = bn_native_object(birnam_object_behavior(), 0);
    bn_send2(guard, "define:as:", bn_symbol("untouched"), bn_method(untouched, 0, 0));
    bn_send2(world, "at:put:", bn_symbol("NativeParserGuard"), guard);
    Oop installed = bn_send1(named("Lang"), "installOn:", seed);
    assert(installed == seed);
    assert(birnam_bind(seed, bn_symbol("parse:")) != original);
    return world;
}
