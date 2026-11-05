#include "birnam_native.h"
#include <assert.h>
#include <stdio.h>

static Oop world;
static unsigned finalized;

static Oop named(const char *name) { return bn_send1(world, "at:", bn_symbol(name)); }
static void publish(const char *name, Oop value) {
    assert(bn_send2(world, "at:put:", bn_symbol(name), value) == world);
}
static void put(Oop vt, const char *name, Oop method) {
    assert(bn_send2(vt, "at:put:", bn_symbol(name), method) == vt);
}
static void error_is(Oop error, const char *kind) {
    assert(bn_send0(error, "isError") == named("true"));
    assert(bn_send0(error, "kind") == bn_symbol(kind));
}

static Oop identity(Oop method, Oop receiver, Oop args) { return receiver; }
static Oop capture(Oop method, Oop receiver, Oop args) { return *(Oop *)method; }
static Oop returning(Oop value) {
    Oop m = bn_method(capture, 0, sizeof(Oop));
    *(Oop *)m = value;
    return m;
}
static Oop first(Oop method, Oop receiver, Oop args) { return bn_arg(args, 0); }

typedef struct { Oop value; } Slot;
static Oop slot_get(Oop method, Oop receiver, Oop args) { return ((Slot *)receiver)->value; }
static Oop slot_put(Oop method, Oop receiver, Oop args) {
    ((Slot *)receiver)->value = bn_arg(args, 0);
    return receiver;
}

typedef struct { Oop fallback, selector, a, b, ma, mb; unsigned calls; } Choice;
static Oop choose(Oop method, Oop receiver, Oop args) {
    Choice *c = (Choice *)method;
    ++c->calls;
    if (bn_arg(args, 0) == c->selector) {
        if (bn_arg(args, 1) == c->a) return c->ma;
        if (bn_arg(args, 1) == c->b) return c->mb;
    }
    return birnam_enter(c->fallback, receiver, args);
}

typedef struct { Oop vt, selector, next, result; } Replacement;
static Oop replace_while_entered(Oop method, Oop receiver, Oop args) {
    Replacement *state = (Replacement *)method;
    bn_send2(state->vt, "at:put:", state->selector, state->next);
    bn_send0(named("GC"), "collect");
    return state->result;
}

typedef struct { Oop lookup; unsigned calls; } RootLookup;
static Oop root_lookup(Oop method, Oop receiver, Oop args) {
    RootLookup *state = (RootLookup *)method;
    ++state->calls;
    return birnam_enter(state->lookup, receiver, args);
}

static Oop collect_and_return_argument(Oop method, Oop receiver, Oop args) {
    bn_send0(named("GC"), "collect");
    return bn_arg(args, 0);
}
static void finalizer(void *object, void *data) { ++finalized; }
__attribute__((noinline)) static void make_garbage(void) {
    for (unsigned i = 0; i < 1024; ++i) {
        Oop object = bn_native_object(birnam_object_behavior(), 1024);
        GC_register_finalizer_no_order(bn_header(object), finalizer, NULL, NULL, NULL);
    }
}
__attribute__((noinline)) static void scrub_and_collect(void) {
    volatile uintptr_t scrub[4096] = {0};
    (void)scrub;
    bn_send0(named("GC"), "collect");
    GC_invoke_finalizers();
}

Oop birnam_module_init_v1(Oop method, Oop receiver, Oop args) {
    world = receiver;
    assert(birnam_bootstrap() == world);
    assert(GC_get_all_interior_pointers());
    assert(!GC_is_incremental_mode());
    assert(bn_symbol("hello") == bn_symbol("hello"));
    assert(bn_send0(bn_symbol("hello"), "clone") == bn_symbol("hello"));
    assert(bn_send0(named("nil"), "clone") == named("nil"));
    Oop alternate_name = bn_native_object(birnam_object_behavior(), 0);
    bn_send2(alternate_name, "define:as:", bn_symbol("intern"), returning(bn_symbol("hello")));
    assert(birnam_intern(alternate_name) == bn_symbol("hello"));
    Oop ns_copy = bn_send0(world, "clone");
    bn_send2(ns_copy, "at:put:", bn_symbol("onlyInCopy"), ns_copy);
    error_is(bn_send1(world, "at:", bn_symbol("onlyInCopy")), "unknownName");
    Oop vt = bn_send0(birnam_object_behavior(), "delegated");
    Oop a = bn_native_object(vt, sizeof(Slot));
    Oop before = bn_native_object(birnam_object_behavior(), 8);
    Oop after = bn_native_object(birnam_object_behavior(), 8);
    ((Slot *)a)->value = before;
    put(vt, "value", bn_method(slot_get, 0, 0));
    put(vt, "value:", bn_method(slot_put, 1, 0));
    put(vt, "yourself", bn_method(identity, 0, 0));
    Oop b = bn_send0(a, "clone");
    assert(a != b && bn_send0(a, "behavior") == bn_send0(b, "behavior"));
    assert(bn_send0(a, "value") == before && bn_send0(b, "value") == before);
    assert(bn_send1(b, "value:", after) == b);
    assert(bn_send0(a, "value") == before && bn_send0(b, "value") == after);
    assert(bn_send0(b, "yourself") == b); /* inherited self is the application object */

    Oop old = returning(before), newer = returning(after);
    put(vt, "greet", old);
    Oop captured = birnam_bind(a, bn_symbol("greet"));
    assert(captured == old);
    bn_send2(b, "define:as:", bn_symbol("greet"), newer);
    put(vt, "greet", returning(a));
    assert(bn_send0(a, "greet") == a && bn_send0(b, "greet") == after);
    assert(birnam_enter(captured, a, bn_arguments(0, NULL)) == before);
    Oop c = bn_send0(b, "clone");
    bn_send2(b, "define:as:", bn_symbol("greet"), returning(b));
    assert(bn_send0(b, "greet") == b && bn_send0(c, "greet") == after);

    Oop never_installed = bn_method(identity, 0, 0);
    error_is(bn_send0(a, "notInstalled"), "messageNotUnderstood");
    put(vt, "notInstalled", never_installed);
    assert(bn_send0(a, "notInstalled") == a);
    bn_send2(old, "define:as:", bn_symbol("apply"), returning(after));
    bn_send2(old, "define:as:", bn_symbol("invoke"), returning(after));
    put(vt, "direct", old);
    assert(bn_send0(a, "direct") == before); /* no implicit second invocation */

    Oop pick_vt = bn_send0(birnam_object_behavior(), "delegated");
    Oop left = bn_native_object(pick_vt, 0), right = bn_native_object(pick_vt, 0);
    Oop chooser = bn_method(choose, 2, sizeof(Choice));
    Choice *choice = (Choice *)chooser;
    choice->fallback = birnam_bind(pick_vt, bn_symbol("lookup:for:"));
    choice->selector = bn_symbol("choose");
    choice->a = left; choice->b = right; choice->ma = old; choice->mb = newer;
    bn_send2(pick_vt, "define:as:", bn_symbol("lookup:for:"), chooser);
    assert(birnam_bind(left, choice->selector) == old);
    assert(birnam_bind(right, choice->selector) == newer);
    assert(bn_send0(left, "choose") == before);
    unsigned count = choice->calls;
    assert(bn_send0(left, "choose") == before && choice->calls > count);
    assert(bn_send0(right, "choose") == after);
    assert(bn_send0(left, "clone") != left); /* nested fallback is not a recursion shortcut */
    choice->ma = after; /* invalid custom lookup result is a language error */
    error_is(bn_send0(left, "choose"), "notMethod");
    choice->ma = old;

    Oop root = named("Vtable"), lookup_sel = bn_symbol("lookup:for:");
    Oop original_lookup = birnam_bind(root, lookup_sel);
    Oop replacement = bn_method(root_lookup, 2, sizeof(RootLookup));
    ((RootLookup *)replacement)->lookup = original_lookup;
    put(root, "lookup:for:", replacement);
    assert(bn_send0(a, "direct") == before);
    assert(((RootLookup *)replacement)->calls > 0);
    put(root, "lookup:for:", original_lookup);

    Oop active = bn_method(replace_while_entered, 0, sizeof(Replacement));
    *(Replacement *)active = (Replacement){vt, bn_symbol("active"), newer, before};
    put(vt, "active", active);
    assert(bn_send0(a, "active") == before);
    assert(bn_send0(a, "active") == after);
    assert(birnam_enter(active, a, bn_arguments(0, NULL)) == before);

    put(vt, "collect:", bn_method(collect_and_return_argument, 1, 0));
    Oop child = bn_native_object(birnam_object_behavior(), 32);
    assert(bn_send1(a, "collect:", child) == child);
    assert(bn_send0(a, "value") == before);
    error_is(bn_send1(a, "direct", child), "arityMismatch");
    error_is(birnam_enter(old, a, bn_native_object(birnam_vector_behavior(), 1)), "invalidArguments");
    error_is(birnam_send(NULL, bn_symbol("direct"), bn_arguments(0, NULL)), "invalidCall");
    Oop error = bn_send0(a, "missing");
    assert(bn_send1(error, "ordinary:", child) == error);
    put(birnam_object_behavior(), "globalAnswer", returning(after));
    assert(bn_send0(error, "globalAnswer") == error); /* inherited methods must not swallow Errors */

    Oop identity_method = bn_method(first, 1, 0);
    put(vt, "identity:", identity_method);
    Oop source_vector = bn_arguments(1, &a);
    assert(bn_send1(a, "identity:", source_vector) == source_vector); /* not flattened */

    size_t heap_before = GC_get_heap_size();
    for (unsigned round = 0; round < 4; ++round) {
        make_garbage();
        for (unsigned i = 0; i < 8 && finalized < (round + 1) * 1000; ++i) scrub_and_collect();
        assert(finalized >= (round + 1) * 900); /* allowance for conservative false roots */
    }
    scrub_and_collect();
    assert(GC_get_heap_size() <= heap_before + 8 * 1024 * 1024);
    assert(birnam_enter(captured, a, bn_arguments(0, NULL)) == before);

    Oop siphon = named("Siphon");
    const char *failed_path = "build/fail_init.dylib";
    Oop failed = bn_send1(siphon, "load:", bn_bytes(failed_path, strlen(failed_path)));
    error_is(failed, "unknownName");
    assert(birnam_enter(named("partialMethod"), world, bn_arguments(0, NULL)) == world);
    Oop nulpath = bn_bytes("x\0y", 3);
    error_is(bn_send1(siphon, "load:", nulpath), "invalidModulePath");

    Oop pre = bn_native_object(birnam_object_behavior(), 0);
    put(birnam_object_behavior(), "phase", old);
    publish("beforeObject", pre);
    publish("beforePhase", birnam_bind(pre, bn_symbol("phase")));
    publish("oldToken", before);
    publish("newToken", after);
    publish("originalWorld", world);
    printf("PASS C contract: dispatch, clone, overrides, captures, errors, GC (%u/4096 finalized), failed initialization\n", finalized);
    return world;
}
