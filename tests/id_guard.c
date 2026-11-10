#include "birnam_native.h"
#include <assert.h>
#include <stdio.h>

/* Loaded three times. The first load captures the siphon's Methods for every
 * selector the authored Id replaces; the second, after takeover, checks that
 * none is still selected and makes each one fail loudly; the last checks that
 * nothing entered them in between.
 */
static Oop world;
static unsigned phase, forbidden_calls;
enum { REPLACED = 9 };
static struct { Oop vt; const char *selector; size_t arity; Oop original; } replaced[REPLACED];

static Oop named(const char *name) { return bn_send1(world, "at:", bn_symbol(name)); }
static Oop forbidden(Oop m, Oop r, Oop a) {
    ++forbidden_calls;
    return bn_send2(named("Error"), "kind:message:", bn_symbol("idTakeoverForbidden"),
                    bn_bytes("replaced siphon Id Method was entered", strlen("replaced siphon Id Method was entered")));
}
static Oop selected(unsigned i) {
    return bn_send2(replaced[i].vt, "lookup:for:", bn_symbol(replaced[i].selector), replaced[i].vt);
}

Oop birnam_module_init_v1(Oop m, Oop receiver, Oop a) {
    world = receiver;
    if (phase == 0) {
        Oop object_vt = birnam_object_behavior(), vtable = named("Vtable");
        Oop namespace_vt = bn_send0(named("Namespace"), "behavior"), error_vt = bn_send0(named("Error"), "behavior");
        struct { Oop vt; const char *selector; size_t arity; } rows[REPLACED] = {
            {object_vt, "define:as:", 2}, {object_vt, "clone", 0}, {error_vt, "define:as:", 2},
            {vtable, "at:put:", 2}, {vtable, "delegated", 0}, {vtable, "parent", 0}, {vtable, "clone", 0},
            {namespace_vt, "new", 0}, {namespace_vt, "clone", 0},
        };
        for (unsigned i = 0; i < REPLACED; ++i) {
            replaced[i].vt = rows[i].vt;
            replaced[i].selector = rows[i].selector;
            replaced[i].arity = rows[i].arity;
            replaced[i].original = selected(i);
            assert(bn_header(replaced[i].original)->entry != NULL);
        }
        /* Keep the captures reachable without relying on this library's statics. */
        Oop originals[REPLACED];
        for (unsigned i = 0; i < REPLACED; ++i) originals[i] = replaced[i].original;
        bn_send2(world, "at:put:", bn_symbol("idGuardOriginals"), bn_arguments(REPLACED, originals));
    } else if (phase == 1) {
        Oop values[] = {world, world};
        for (unsigned i = 0; i < REPLACED; ++i) {
            assert(selected(i) != replaced[i].original);
            bn_header(replaced[i].original)->entry = forbidden;
            Oop result = birnam_enter(replaced[i].original, replaced[i].vt, bn_arguments(replaced[i].arity, values));
            assert(bn_send0(result, "kind") == bn_symbol("idTakeoverForbidden"));
        }
        assert(forbidden_calls == REPLACED);
        forbidden_calls = 0;
    } else {
        assert(forbidden_calls == 0);
        puts("PASS authored Id takeover: 9 replaced siphon Methods poisoned and never entered");
    }
    ++phase;
    return world;
}
