#include "birnam_native.h"
#include <assert.h>

static Oop world;
static unsigned parser_calls, evaluator_calls;
static unsigned constructor_calls[4];
typedef struct { Oop previous; unsigned *calls; } Observer;
static Oop named(const char *s) { return bn_send1(world, "at:", bn_symbol(s)); }
static Oop denied(const char *kind) {
    return bn_send2(named("Error"), "kind:message:", bn_symbol(kind), bn_bytes(kind, strlen(kind)));
}
static Oop parser_denied(Oop m, Oop r, Oop a) { ++parser_calls; return denied("nativeParserForbidden"); }
static Oop evaluator_denied(Oop m, Oop r, Oop a) { ++evaluator_calls; return denied("seedEvaluatorForbidden"); }
static Oop untouched(Oop m, Oop r, Oop a) { return named(parser_calls == 0 && evaluator_calls == 0 ? "true" : "false"); }
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
    Oop seed = named("Seed");
    Oop parser = birnam_bind(seed, bn_symbol("parse:"));
    bn_header(parser)->entry = parser_denied;
    Oop bytes = bn_bytes("1", 1);
    assert(bn_send0(birnam_enter(parser, seed, bn_arguments(1, &bytes)), "kind") == bn_symbol("nativeParserForbidden"));
    Oop syntax = bn_send0(seed, "syntax");
    const char *families[] = {"Literal", "Name", "Declaration", "Send", "Block", "Vector", "Sequence"};
    for (size_t i = 0; i < sizeof(families)/sizeof(*families); ++i) {
        Oop vt = bn_send1(syntax, "at:", bn_symbol(families[i]));
        Oop selected = bn_send2(vt, "lookup:for:", bn_symbol("evalIn:"), vt);
        bn_header(selected)->entry = evaluator_denied;
        assert(bn_send0(birnam_enter(selected, vt, bn_arguments(1, &world)), "kind") == bn_symbol("seedEvaluatorForbidden"));
    }
    assert(parser_calls == 1 && evaluator_calls == 7);
    parser_calls = evaluator_calls = 0;
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
