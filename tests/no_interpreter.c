#include "birnam_native.h"

/* Interpreter-absence witness. Load it before and after native work in the
 * same process, as many times as needed: every load requires that `Seed` is
 * unbound. If the interpreter is present, init answers an Error of kind
 * interpreterPresent, so the siphon exits 1 and names the kind. It keeps no
 * state between loads and prints nothing on success.
 */
static Oop named(Oop world, const char *name) { return bn_send1(world, "at:", bn_symbol(name)); }

Oop birnam_module_init_v1(Oop m, Oop world, Oop a) {
    Oop seed = named(world, "Seed");
    if (bn_send0(seed, "isError") == named(world, "true") && bn_send0(seed, "kind") == bn_symbol("unknownName"))
        return world;
    const char *message = "Seed is bound: the interpreter is loaded in a native-only process";
    return bn_send2(named(world, "Error"), "kind:message:", bn_symbol("interpreterPresent"),
                    bn_bytes(message, strlen(message)));
}
