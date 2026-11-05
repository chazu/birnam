#include "birnam_native.h"
#include <assert.h>
#include <stdio.h>

Oop birnam_module_init_v1(Oop method, Oop world, Oop args) {
    assert(bn_send1(world, "at:", bn_symbol("originalWorld")) == world);
    Oop object = bn_send1(world, "at:", bn_symbol("beforeObject"));
    Oop captured = bn_send1(world, "at:", bn_symbol("beforePhase"));
    Oop before = bn_send1(world, "at:", bn_symbol("oldToken"));
    Oop after = bn_send1(world, "at:", bn_symbol("newToken"));
    Oop gc = bn_send1(world, "at:", bn_symbol("GC"));
    bn_send0(gc, "collect");
    assert(bn_send0(object, "phase") == after);
    assert(birnam_enter(captured, object, bn_arguments(0, NULL)) == before);
    assert(bn_send0(world, "handoverObserved") == world);
    Oop siphon = bn_send1(world, "at:", bn_symbol("Siphon"));
    Oop modules = bn_send0(siphon, "modules");
    assert(((BirnamArguments *)modules)->count == 4); /* includes failed initializer */
    puts("PASS post-handover: existing objects/captured code survived collection and replacement");
    return world;
}
