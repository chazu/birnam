#include "birnam_native.h"
#include <assert.h>
#include <stdio.h>

/* Runs after the authored Id takeover. Native callers reach authored policy
 * through ordinary sends, private layers stay bounded, and objects owning
 * their layers (an owner/behavior cycle) are still reclaimed.
 */
static Oop world;
static unsigned finalized;

static Oop named(const char *name) { return bn_send1(world, "at:", bn_symbol(name)); }
static Oop constant(Oop m, Oop r, Oop a) { return *(Oop *)m; }
static Oop returning(Oop value) {
    Oop m = bn_method(constant, 0, sizeof(Oop));
    *(Oop *)m = value;
    return m;
}
static void count_finalized(void *object, void *data) { ++finalized; }

static void discarded_batch(Oop prototype, Oop method, unsigned count) {
    for (unsigned i = 0; i < count; ++i) {
        Oop object = bn_send0(prototype, "clone");
        assert(bn_send2(object, "define:as:", bn_symbol("value"), method) == object);
        assert(bn_send2(object, "define:as:", bn_symbol("other"), method) == object);
        GC_register_finalizer_no_order(bn_header(object), count_finalized, NULL, NULL, NULL);
    }
}

Oop birnam_module_init_v1(Oop m, Oop receiver, Oop a) {
    world = receiver;
    Oop object_vt = birnam_object_behavior(), yes = named("true");
    Oop object = bn_send0(named("Object"), "clone");
    for (unsigned i = 0; i < 1000; ++i)
        assert(bn_send2(object, "define:as:", bn_symbol("value"), returning(named("nil"))) == object);
    Oop layer = bn_send0(object, "behavior");
    assert(bn_send0(layer, "parent") == object_vt);
    assert(bn_send1(layer, "includesSelector:", bn_symbol("value")) == yes);
    Oop error = bn_send2(object_vt, "at:put:", bn_symbol("value"), named("nil"));
    assert(bn_send0(error, "kind") == bn_symbol("notMethod"));

    Oop prototype = bn_send0(named("Object"), "clone");
    Oop method = returning(prototype);
    size_t before = GC_get_heap_size();
    for (unsigned batch = 0; batch < 8; ++batch) {
        discarded_batch(prototype, method, 512);
        GC_gcollect(); GC_invoke_finalizers();
    }
    for (unsigned i = 0; i < 5; ++i) { GC_gcollect(); GC_invoke_finalizers(); }
    size_t after = GC_get_heap_size();
    /* Conservative scanning may retain a few; the bound is about owner cycles. */
    assert(finalized >= 4096 - 64);
    assert(after <= before + 4 * 1024 * 1024);
    assert(bn_send0(prototype, "behavior") == object_vt);
    printf("PASS authored Id contract: 1000 definitions in one layer; %u/4096 specialized objects finalized; heap %zu -> %zu bytes\n",
           finalized, before, after);
    return world;
}
