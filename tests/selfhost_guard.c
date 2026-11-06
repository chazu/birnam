#include "birnam_native.h"
#include <assert.h>

static Oop world;
static unsigned forbidden_calls;
static Oop named(const char *name) { return bn_send1(world, "at:", bn_symbol(name)); }
static Oop forbidden(Oop m, Oop r, Oop a) {
    ++forbidden_calls;
    return bn_send2(named("Error"), "kind:message:", bn_symbol("interpreterForbidden"),
                    bn_bytes("interpreter entry used during native bootstrap", strlen("interpreter entry used during native bootstrap")));
}
static void disable(Oop selected, Oop receiver, size_t arity) {
    bn_header(selected)->entry = forbidden;
    Oop values[] = {world, world};
    Oop result = birnam_enter(selected, receiver, bn_arguments(arity, values));
    assert(bn_send0(result, "kind") == bn_symbol("interpreterForbidden"));
}
Oop birnam_module_init_v1(Oop m, Oop receiver, Oop a) {
    if (world) { assert(forbidden_calls == 0); return world; }
    world = receiver;
    Oop seed = named("Seed");
    const char *selectors[] = {"parse:", "eval:", "eval:in:", "evalFile:"};
    for (unsigned i = 0; i < 4; ++i)
        disable(birnam_bind(seed, bn_symbol(selectors[i])), seed, i == 2 ? 2 : 1);
    Oop syntax = bn_send0(seed, "syntax");
    const char *families[] = {"Literal", "Name", "Declaration", "Send", "Block", "Vector", "Sequence"};
    for (unsigned i = 0; i < 7; ++i) {
        Oop vt = bn_send1(syntax, "at:", bn_symbol(families[i]));
        disable(bn_send2(vt, "lookup:for:", bn_symbol("evalIn:"), vt), vt, 1);
    }
    assert(forbidden_calls == 11);
    forbidden_calls = 0;
    return world;
}
