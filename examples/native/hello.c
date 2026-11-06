#include "birnam_native.h"
#include <stdio.h>

static Oop greet(Oop method, Oop receiver, Oop arguments) {
    puts("Hello from a native Method in Birnam's bootstrap world.");
    return receiver;
}

Oop birnam_module_init_v1(Oop method, Oop world, Oop arguments) {
    Oop hello = bn_native_object(birnam_object_behavior(), 0);
    bn_send2(hello, "define:as:", bn_symbol("greet"), bn_method(greet, 0, 0));
    bn_send2(world, "at:put:", bn_symbol("Hello"), hello);
    return bn_send0(hello, "greet");
}
