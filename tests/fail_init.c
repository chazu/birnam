#include "birnam_native.h"
static Oop retained_code(Oop method, Oop receiver, Oop args) { return receiver; }
Oop birnam_module_init_v1(Oop method, Oop world, Oop args) {
    bn_send2(world, "at:put:", bn_symbol("partialMethod"), bn_method(retained_code, 0, 0));
    return bn_send1(world, "at:", bn_symbol("deliberatelyMissing"));
}
