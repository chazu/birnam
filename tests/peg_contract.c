#include "birnam_native.h"
#include <assert.h>
#include <stdio.h>

static Oop world, peg, live_grammar, live_tree;
static unsigned retired_contexts, retired_trees, retired_grammars, matches;
static void finalized(void *object, void *counter) { ++*(unsigned *)counter; }
static Oop named(const char *name) { return bn_send1(world, "at:", bn_symbol(name)); }
static void success(Oop value) { assert(bn_send0(value, "isError") == named("false")); }
static void number(Oop value, const char *expected) {
    Oop bytes = bn_send0(bn_send0(value, "printString"), "asBytes");
    assert(bn_header(bytes)->payload_bytes == strlen(expected));
    assert(memcmp(bytes, expected, strlen(expected)) == 0);
}
static Oop observing_match(Oop m, Oop r, Oop args) {
    Oop context = bn_arg(args, 0);
    GC_register_finalizer_no_order(bn_header(context), finalized, &retired_contexts, NULL, NULL);
    if (++matches % 17 == 0) GC_gcollect();
    return birnam_enter(*(Oop *)m, r, args);
}
static Oop grammar(void) {
    Oop g = bn_send0(peg, "grammar");
    Oop literal = bn_send1(peg, "literal:", bn_bytes("abc", 3));
    Oop original = birnam_bind(literal, bn_symbol("match:at:"));
    success(bn_send0(original, "code")); /* source-defined Method */
    Oop observer = bn_method(observing_match, 2, sizeof(Oop));
    *(Oop *)observer = original;
    success(bn_send2(literal, "define:as:", bn_symbol("match:at:"), observer));
    success(bn_send2(g, "rule:is:", bn_symbol("root"), literal));
    return g;
}
__attribute__((noinline)) static void discarded_batch(void) {
    for (unsigned i = 0; i < 100; ++i) {
        Oop g = grammar();
        Oop tree = bn_send2(g, "parse:from:", bn_bytes("abc", 3), bn_symbol("root"));
        success(tree);
        number(bn_send0(tree, "end"), "3");
        GC_register_finalizer_no_order(bn_header(g), finalized, &retired_grammars, NULL, NULL);
        GC_register_finalizer_no_order(bn_header(tree), finalized, &retired_trees, NULL, NULL);
    }
}
Oop birnam_module_init_v1(Oop m, Oop receiver, Oop args) {
    world = receiver;
    peg = named("Peg");
    live_grammar = grammar();
    live_tree = bn_send2(live_grammar, "parse:from:", bn_bytes("abc", 3), bn_symbol("root"));
    success(live_tree);
    GC_gcollect();
    size_t heap_before = GC_get_heap_size();
    for (unsigned batch = 0; batch < 4; ++batch) {
        discarded_batch();
        GC_gcollect();
        GC_invoke_finalizers();
    }
    for (unsigned i = 0; i < 5; ++i) { GC_gcollect(); GC_invoke_finalizers(); }
    assert(retired_contexts >= 360);
    assert(retired_trees >= 360);
    assert(retired_grammars >= 360);
    assert(GC_get_heap_size() <= heap_before + 16 * 1024 * 1024);
    number(bn_send0(live_tree, "end"), "3");
    success(bn_send2(live_grammar, "parse:from:", bn_bytes("abc", 3), bn_symbol("root")));
    printf("PASS PEG GC: %u/401 contexts, %u/400 trees, %u/400 grammars finalized; live grammar/CST survive; heap %zu -> %zu bytes\n",
           retired_contexts, retired_trees, retired_grammars, heap_before, GC_get_heap_size());
    return world;
}
