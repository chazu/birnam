/* CLI transport only: stdin bytes -> Compiler compile:as: -> System print:.
 * Grammar, lowering, and C generation belong entirely to authored Methods. */
#include "birnam_native.h"
#include <stdio.h>

static Oop named(Oop world, const char *name) { return bn_send1(world, "at:", bn_symbol(name)); }
static Oop failure(Oop world, const char *kind, const char *message) {
    return bn_send2(named(world, "Error"), "kind:message:", bn_symbol(kind), bn_bytes(message, strlen(message)));
}
Oop birnam_module_init_v1(Oop m, Oop world, Oop a) {
    size_t used = 0, capacity = 8192;
    unsigned char *buffer = malloc(capacity);
    if (!buffer) return failure(world, "sourceReadFailed", "cannot allocate source input buffer");
    for (;;) {
        size_t count = fread(buffer + used, 1, capacity - used, stdin);
        used += count;
        if (ferror(stdin)) { free(buffer); return failure(world, "sourceReadFailed", "cannot read source input"); }
        if (feof(stdin)) break;
        if (used == capacity) {
            if (capacity > SIZE_MAX / 2) { free(buffer); return failure(world, "sourceReadFailed", "source input is too large"); }
            capacity *= 2;
            unsigned char *grown = realloc(buffer, capacity);
            if (!grown) { free(buffer); return failure(world, "sourceReadFailed", "cannot grow source input buffer"); }
            buffer = grown;
        }
    }
    Oop source = bn_bytes(buffer, used);
    free(buffer);
    const char *name = getenv("BIRNAM_COMPILE_NAME");
    if (!name) name = "NativeProgram";
    Oop result = bn_send2(named(world, "Compiler"), "compile:as:", source, bn_bytes(name, strlen(name)));
    if (bn_send0(result, "isError") == named(world, "true")) return result;
    return bn_send1(named(world, "System"), "print:", result);
}
