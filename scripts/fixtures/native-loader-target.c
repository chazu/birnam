#include "../../tests/fixtures/oop-id-v2/id.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#ifdef NATIVE_LOADER_NO_INIT
int not_a_module(void) { return 1; }
#else
static IdProtocol id;
static Oop names[6];
static unsigned calls;
enum {BEHAVIOR, GC, COUNT, METHOD, ADD, NAME};
static Oop send(Oop receiver, size_t selector, const Oop *args, size_t count) {
    return id.message(id.root, receiver, names[selector], args, count);
}
static Oop count_loads(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root; (void)context; (void)receiver; (void)args;
    if (count) return id.invalid;
    char text[64];
    int length = snprintf(text, sizeof(text), "native-load-%u", calls);
    assert(length > 0 && (size_t)length < sizeof(text));
    Oop bytes[64];
    for (int i = 0; i < length; ++i) bytes[i] = id_size((unsigned char)text[i]);
    Oop name = send(id.root, NAME, bytes, (size_t)length);
    if (id_fault(name)) return name;
    return id.message(id.root, name, id.intern, NULL, 0);
}
/* Neither dependency discovery nor the retired initializer may be invoked. */
int birnam_native_loader_dependency_v12(const void *provider) { (void)provider; abort(); }
void *module_init(void) { abort(); }
static Oop initialize(const IdProtocol *protocol, Oop image, const Oop *symbols,
                      size_t symbol_count, const Oop *args, size_t count) {
    if (symbol_count != 6 || (count != 1 && count != 2)) return protocol->invalid;
    if (calls) assert(protocol->root == id.root);
    id = *protocol;
    for (size_t i = 0; i < symbol_count; ++i) names[i] = symbols[i];
    Oop system = args[0];
    Oop behavior = send(id.root, BEHAVIOR, &system, 1);
    if (id_fault(behavior)) return behavior;
    Oop collected = send(system, GC, NULL, 0);
    if (id_fault(collected)) return collected;
    ++calls;
    Oop method = send(image, METHOD, &names[COUNT], 1);
    if (id_fault(method)) return method;
    Oop binding[] = {names[COUNT], method};
    Oop installed = send(behavior, ADD, binding, 2);
    if (id_fault(installed)) return installed;
    // Initializers now return an explicit Oop; the caller chooses the result.
    return args[count - 1];
}
static const IdLiteral literals[] = {
    LIT("behaviorOf:"), LIT("gc"), LIT("nativeLoaderCount"),
    LIT("methodAt:"), LIT("addMethod"), LIT("nameWithBytes")
};
static const IdMethodSpec methods[] = {{COUNT, count_loads}};
const IdModule birnam_native_module_v2 = {
    2, sizeof(IdModule), literals, 6, methods, 1, initialize
};
#endif
