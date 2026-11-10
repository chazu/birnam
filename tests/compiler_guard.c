#include "birnam_native.h"
#include <assert.h>

/* Loaded with no interpreter: the run is native only. Load fails unless Seed
 * is unbound, and CompilerGuard untouched answers true only while it still is.
 * The four runtime constructors are observed to show compiled code uses them.
 */
static Oop world;
static unsigned constructor_calls[4];
typedef struct { Oop previous; unsigned *calls; } Observer;
static Oop named(const char *s) { return bn_send1(world, "at:", bn_symbol(s)); }
static int interpreter_absent(void) {
    Oop seed = named("Seed");
    return bn_send0(seed, "isError") == named("true") && bn_send0(seed, "kind") == bn_symbol("unknownName");
}
static Oop untouched(Oop m, Oop r, Oop a) { return named(interpreter_absent() ? "true" : "false"); }
static Oop observed(Oop m, Oop r, Oop a) {
    Observer *observer = (Observer *)m;
    if (++*observer->calls % 17 == 0) GC_gcollect();
    return birnam_enter(observer->previous, r, a);
}
static Oop services_observed(Oop m, Oop r, Oop a) {
    for (unsigned i = 0; i < 4; ++i) assert(constructor_calls[i] > 0);
    return named("true");
}
Oop birnam_module_init_v1(Oop m, Oop receiver, Oop a) {
    world = receiver;
    if (!interpreter_absent()) {
        const char *message = "Seed is bound: the interpreter is loaded in a native-only process";
        return bn_send2(named("Error"), "kind:message:", bn_symbol("interpreterPresent"), bn_bytes(message, strlen(message)));
    }
    const char *objects[] = {"Integer", "Block", "Vector", "Environment"};
    const char *selectors[] = {"fromBytes:", "code:environment:parameters:", "fromElements:", "lookup:"};
    const unsigned arities[] = {1, 3, 1, 1};
    for (unsigned i = 0; i < 4; ++i) {
        Oop target = named(objects[i]);
        Oop selector = bn_symbol(selectors[i]);
        Oop observer = bn_method(observed, arities[i], sizeof(Observer));
        *(Observer *)observer = (Observer){birnam_bind(target, selector), &constructor_calls[i]};
        // Environment instances inherit this shared owner behavior.
        bn_send2(bn_send0(target, "behavior"), "at:put:", selector, observer);
    }
    Oop guard = bn_native_object(birnam_object_behavior(), 0);
    bn_send2(guard, "define:as:", bn_symbol("untouched"), bn_method(untouched, 0, 0));
    bn_send2(guard, "define:as:", bn_symbol("servicesObserved"), bn_method(services_observed, 0, 0));
    bn_send2(world, "at:put:", bn_symbol("CompilerGuard"), guard);
    return world;
}
