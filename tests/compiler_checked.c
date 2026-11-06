#include "birnam_native.h"
#include <assert.h>
#include <stdio.h>
Oop birnam_module_init_v1(Oop m, Oop world, Oop a) {
    Oop yes = bn_send1(world, "at:", bn_symbol("true"));
    Oop probe = bn_send1(world, "at:", bn_symbol("Probe"));
    assert(bn_send0(probe, "checked") == yes);
    printf("PASS effectful lookup and direct selected entry\n");
    return world;
}
