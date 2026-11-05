#ifndef BIRNAM_NATIVE_H
#define BIRNAM_NATIVE_H

/* Native object construction/entry support, not an additional message API.
 * Native methods may have arbitrary C payloads. These mechanical envelopes
 * contain no semantic type tag and no Odin-specific fields.
 */
#include "birnam.h"
#include <gc/gc.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    Oop behavior;
    uintptr_t payload_bytes;
    BirnamEntry entry;       /* NULL for a non-callable object */
    uintptr_t arity;
} BirnamHeader;
_Static_assert(sizeof(BirnamHeader) == 4 * sizeof(uintptr_t), "seed header layout mismatch");

/* Native call-Vector adapter. Its elements are Oops, never native values.
 * It is deliberately not the representation of every collection in Birnam.
 */
typedef struct { uintptr_t count; Oop elements[]; } BirnamArguments;

static inline BirnamHeader *bn_header(Oop object) {
    return ((BirnamHeader *)object) - 1;
}

static inline Oop bn_native_object(Oop behavior, size_t payload_bytes) {
    if (payload_bytes > SIZE_MAX - sizeof(BirnamHeader) - 1) abort();
    BirnamHeader *h = GC_malloc(sizeof(*h) + (payload_bytes ? payload_bytes : 1));
    if (!h) abort();
    h->behavior = behavior;
    h->payload_bytes = payload_bytes;
    h->entry = NULL;
    h->arity = 0;
    return (Oop)(h + 1);
}

static inline Oop bn_bytes(const void *bytes, size_t count) {
    Oop object = bn_native_object(birnam_bytes_behavior(), count);
    if (count) memcpy(object, bytes, count);
    return object;
}

static inline Oop bn_symbol(const char *name) {
    return birnam_intern(bn_bytes(name, strlen(name)));
}

static inline Oop bn_arguments(size_t count, const Oop *elements) {
    if (count > (SIZE_MAX - sizeof(BirnamHeader) - sizeof(uintptr_t) - 1) / sizeof(Oop)) abort();
    Oop object = bn_native_object(birnam_vector_behavior(), sizeof(uintptr_t) + count * sizeof(Oop));
    BirnamArguments *a = (BirnamArguments *)object;
    a->count = count;
    if (count) memcpy(a->elements, elements, count * sizeof(Oop));
    return object;
}

static inline Oop bn_method(BirnamEntry entry, size_t arity, size_t capture_bytes) {
    Oop object = bn_native_object(birnam_method_behavior(), capture_bytes);
    bn_header(object)->entry = entry;
    bn_header(object)->arity = arity;
    return object;
}

static inline Oop bn_arg(Oop arguments, size_t index) {
    BirnamArguments *a = (BirnamArguments *)arguments;
    if (index >= a->count) abort();
    return a->elements[index];
}

static inline Oop bn_send0(Oop receiver, const char *selector) {
    return birnam_send(receiver, bn_symbol(selector), bn_arguments(0, NULL));
}

static inline Oop bn_send1(Oop receiver, const char *selector, Oop a) {
    return birnam_send(receiver, bn_symbol(selector), bn_arguments(1, &a));
}

static inline Oop bn_send2(Oop receiver, const char *selector, Oop a, Oop b) {
    Oop args[] = {a, b};
    return birnam_send(receiver, bn_symbol(selector), bn_arguments(2, args));
}

#endif
