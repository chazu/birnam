#include "birnam_native.h"
#include <assert.h>
#include <stdio.h>

typedef struct { Oop previous; unsigned calls; } Lookup;
static Oop lookup(Oop method, Oop receiver, Oop args) {
    Lookup *state = (Lookup *)method;
    ++state->calls;
    return birnam_enter(state->previous, receiver, args);
}
static Oop value(Oop method, Oop receiver, Oop args) { return *(Oop *)method; }
static Oop called(Oop method, Oop receiver, Oop args) {
    return ((Lookup *)*(Oop *)method)->calls ? receiver : *(Oop *)method;
}

Oop birnam_module_init_v1(Oop method, Oop world, Oop args) {
    assert(birnam_bootstrap() == world);
    Oop root = bn_send1(world, "at:", bn_symbol("Vtable"));
    Oop replacement = bn_method(lookup, 2, sizeof(Lookup));
    ((Lookup *)replacement)->previous = birnam_bind(root, bn_symbol("lookup:for:"));
    bn_send2(root, "at:put:", bn_symbol("lookup:for:"), replacement);
    Oop phase = bn_method(value, 0, sizeof(Oop));
    *(Oop *)phase = bn_send1(world, "at:", bn_symbol("newToken"));
    bn_send2(birnam_object_behavior(), "at:put:", bn_symbol("phase"), phase);
    Oop check = bn_method(called, 0, sizeof(Oop));
    *(Oop *)check = replacement;
    bn_send2(world, "define:as:", bn_symbol("handoverObserved"), check);
    puts("PASS native handover: replaced root lookup and behavior in the existing world");
    return world;
}
