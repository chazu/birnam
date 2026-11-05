#include "birnam_native.h"
#include <assert.h>
#include <stdio.h>

static Oop world;
static unsigned retired;
static Oop named(const char *name) { return bn_send1(world, "at:", bn_symbol(name)); }
static void success(Oop value) { assert(bn_send0(value, "isError") == named("false")); }
static Oop constant(Oop m, Oop r, Oop a) { return *(Oop *)m; }
static void finalized(void *object, void *data) { ++retired; }
__attribute__((noinline)) static void discarded_batch(void) {
    for (unsigned i = 0; i < 50; ++i) {
        Oop closure = bn_send0(named("make"), "value");
        success(closure);
        GC_register_finalizer_no_order(bn_header(closure), finalized, NULL, NULL, NULL);
    }
}
Oop birnam_module_init_v1(Oop m, Oop receiver, Oop a) {
    world = receiver;
    Oop program = named("NativeProgram");
    Oop code = bn_send0(program, "code");
    success(code); success(bn_send0(program, "source"));
    Oop captured = birnam_bind(code, bn_symbol("evalIn:"));
    Oop marker = named("marker");
    Oop replacement = bn_method(constant, 1, sizeof(Oop));
    *(Oop *)replacement = marker;
    bn_send2(code, "define:as:", bn_symbol("evalIn:"), replacement);
    assert(bn_send1(program, "runIn:", world) == marker);
    // Explicit entry retains the native implementation despite replacement.
    Oop env = bn_send1(named("Environment"), "forNamespace:", world);
    Oop result = birnam_enter(captured, code, bn_arguments(1, &env));
    success(result);
    assert(result != marker);
    bn_send2(code, "define:as:", bn_symbol("evalIn:"), captured);
    GC_gcollect();
    size_t before = GC_get_heap_size();
    for (unsigned batch = 0; batch < 4; ++batch) {
        discarded_batch(); GC_gcollect(); GC_invoke_finalizers();
    }
    for (unsigned i = 0; i < 5; ++i) { GC_gcollect(); GC_invoke_finalizers(); }
    assert(retired >= 180);
    assert(GC_get_heap_size() <= before + 16 * 1024 * 1024);
    assert(bn_send0(named("closure"), "value") == named("marker"));
    assert(bn_send0(named("CompilerGuard"), "untouched") == named("true"));
    assert(bn_send0(named("CompilerGuard"), "servicesObserved") == named("true"));
    printf("PASS compiled code replacement, captured native entry, GC (%u/200 closures finalized; heap %zu -> %zu bytes), no interpreter fallback\n",
           retired, before, GC_get_heap_size());
    return world;
}
