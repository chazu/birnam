#include "birnam_native.h"
#include <assert.h>

static unsigned lookups, entries;
static Oop world;
typedef struct { Oop fallback, selector, chosen, receiver; } Policy;
static Oop entry(Oop m, Oop r, Oop a) { ++entries; return r; }
static Oop poisoned(Oop m, Oop r, Oop a) { assert(!"implicit Method invocation lookup"); return r; }
static Oop lookup(Oop m, Oop r, Oop a) {
    Policy *p = (Policy *)m;
    if (bn_arg(a, 0) == p->selector) {
        assert(bn_arg(a, 1) == p->receiver);
        ++lookups; return p->chosen;
    }
    return birnam_enter(p->fallback, r, a);
}
static Oop checked(Oop m, Oop r, Oop a) {
    assert(lookups == 2 && entries == 2);
    return bn_send1(world, "at:", bn_symbol("true"));
}
Oop birnam_module_init_v1(Oop m, Oop receiver, Oop a) {
    world = receiver;
    Oop vt = bn_send0(birnam_object_behavior(), "delegated");
    Oop probe = bn_native_object(vt, 0);
    Oop chosen = bn_method(entry, 0, 0);
    bn_send2(chosen, "define:as:", bn_symbol("valueOn:with:"), bn_method(poisoned, 2, 0));
    bn_send2(chosen, "define:as:", bn_symbol("apply"), bn_method(poisoned, 0, 0));
    bn_send2(chosen, "define:as:", bn_symbol("invoke"), bn_method(poisoned, 0, 0));
    Oop policy = bn_method(lookup, 2, sizeof(Policy));
    *(Policy *)policy = (Policy){birnam_bind(vt, bn_symbol("lookup:for:")), bn_symbol("probe"), chosen, probe};
    bn_send2(vt, "define:as:", bn_symbol("lookup:for:"), policy);
    bn_send2(vt, "at:put:", bn_symbol("checked"), bn_method(checked, 0, 0));
    bn_send2(world, "at:put:", bn_symbol("Probe"), probe);
    return world;
}
