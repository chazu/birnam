#include "birnam_native.h"
#include <assert.h>
#include <stdio.h>

static Oop seed, world;
static unsigned retired_closures;
static Oop eval(const char *source) {
    return bn_send1(seed, "eval:", bn_bytes(source, strlen(source)));
}
static Oop named(const char *name) { return bn_send1(world, "at:", bn_symbol(name)); }
static void error_kind(Oop value, const char *kind) {
    assert(bn_send0(value, "isError") == named("true"));
    assert(bn_send0(value, "kind") == bn_symbol(kind));
}
static void rendered(Oop value, const char *expected) {
    Oop text = bn_send0(bn_send0(value, "printString"), "asBytes");
    assert(bn_header(text)->payload_bytes == strlen(expected));
    assert(memcmp(text, expected, strlen(expected)) == 0);
}
static void closure_finalized(void *object, void *data) { ++retired_closures; }
typedef struct { Oop fallback, selector, result; unsigned calls; } Policy;
typedef struct { Oop previous; unsigned calls; } Intern;
static Oop collecting_intern(Oop m, Oop r, Oop args) {
    Intern *state = (Intern *)m;
    ++state->calls;
    GC_gcollect();
    return birnam_enter(state->previous, r, args);
}
static Oop constant(Oop m, Oop r, Oop args) { return *(Oop *)m; }
static Oop lookup(Oop m, Oop r, Oop args) {
    Policy *p = (Policy *)m;
    if (bn_arg(args, 0) == p->selector) {
        ++p->calls;
        return p->result;
    }
    return birnam_enter(p->fallback, r, args);
}
__attribute__((noinline)) static void allocate_discarded_syntax(void) {
    for (unsigned i = 0; i < 150; ++i) {
        Oop value = eval("[[:x | [| x]] value: 42]");
        assert(bn_send0(value, "isError") == named("false"));
        GC_register_finalizer_no_order(bn_header(value), closure_finalized, NULL, NULL, NULL);
    }
}

Oop birnam_module_init_v1(Oop m, Oop receiver, Oop args) {
    world = receiver;
    seed = named("Seed");
    Oop value = eval("[Namespace at: #beforeFailure put: true]; [1 / 0]; [Namespace at: #afterFailure put: true]");
    error_kind(value, "zeroDivide");
    assert(named("beforeFailure") == named("true"));
    error_kind(named("afterFailure"), "unknownName");

    Oop target = bn_native_object(birnam_object_behavior(), 0);
    Oop result = bn_native_object(birnam_object_behavior(), 0);
    Oop vt = bn_send0(birnam_object_behavior(), "delegated");
    bn_header(target)->behavior = vt;
    Oop chosen = bn_method(constant, 0, sizeof(Oop));
    *(Oop *)chosen = result;
    Oop policy = bn_method(lookup, 2, sizeof(Policy));
    *(Policy *)policy = (Policy){birnam_bind(vt, bn_symbol("lookup:for:")), bn_symbol("probe"), chosen, 0};
    bn_send2(vt, "define:as:", bn_symbol("lookup:for:"), policy);
    bn_send2(world, "at:put:", bn_symbol("Probe"), target);
    assert(eval("[Probe probe]") == result);
    assert(eval("[Probe probe]") == result);
    assert(((Policy *)policy)->calls == 2);

    // Syntax-node evaluation must use the current evalIn: Method.
    Oop syntax = bn_send0(seed, "syntax");
    Oop literal_vt = bn_send1(syntax, "at:", bn_symbol("Literal"));
    Oop original = bn_send2(literal_vt, "lookup:for:", bn_symbol("evalIn:"), world);
    Oop replacement = bn_method(constant, 1, sizeof(Oop));
    *(Oop *)replacement = result;
    bn_send2(literal_vt, "at:put:", bn_symbol("evalIn:"), replacement);
    assert(eval("42") == result);
    bn_send2(literal_vt, "at:put:", bn_symbol("evalIn:"), original);

    // Collect during parsing/interning, while partially built syntax and
    // parser scratch lists are the only owners of many nodes.
    Oop symbols = named("Symbols");
    Oop intern_selector = bn_symbol("intern:");
    Oop define_selector = bn_symbol("define:as:");
    Oop old_intern = birnam_bind(symbols, intern_selector);
    Oop interner = bn_method(collecting_intern, 1, sizeof(Intern));
    *(Intern *)interner = (Intern){old_intern, 0};
    Oop define_args[] = {intern_selector, interner};
    birnam_send(symbols, define_selector, bn_arguments(2, define_args));
    const char *gc_source = "let parserGcNewBinding := [[:parserGcParameter | [| parserGcParameter]] value: 42]; [parserGcNewBinding value]";
    Oop source = bn_bytes(gc_source, strlen(gc_source));
    Oop parsed = bn_send1(seed, "eval:", source);
    error_kind(eval("missingAfterParserGc"), "unknownName");
    assert(bn_send0(parsed, "isError") == named("false"));
    rendered(parsed, "42");
    assert(((Intern *)interner)->calls > 0);
    Oop restore_args[] = {intern_selector, old_intern};
    birnam_send(symbols, define_selector, bn_arguments(2, restore_args));

    bn_send0(named("GC"), "collect");
    size_t heap_before = GC_get_heap_size();
    for (unsigned batch = 0; batch < 4; ++batch) {
        allocate_discarded_syntax();
        bn_send0(named("GC"), "collect");
        GC_invoke_finalizers();
    }
    for (unsigned i = 0; i < 5 && retired_closures < 580; ++i) {
        bn_send0(named("GC"), "collect");
        GC_invoke_finalizers();
    }
    assert(retired_closures >= 540); /* room for conservative false roots */
    assert(GC_get_heap_size() <= heap_before + 16 * 1024 * 1024);
    printf("PASS seed C witnesses: failed-source effects, lookup effects, replaceable syntax, parser GC, %u/600 closures finalized\n", retired_closures);
    return world;
}
