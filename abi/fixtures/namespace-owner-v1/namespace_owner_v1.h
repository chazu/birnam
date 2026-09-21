#ifndef BIRNAM_NAMESPACE_OWNER_V1_H
#define BIRNAM_NAMESPACE_OWNER_V1_H
#include <stddef.h>
#include <stdint.h>
typedef void *Oop;
#include "../namespace-v8/namespace_v8.h"
typedef struct {
    uint32_t version; size_t byte_size;
    void *context;
    const NamespaceApiV8 *api;
    void *storage, *receiver;
    void (*destroy)(void *);
} NamespaceOwnedV1;
_Static_assert(sizeof(NamespaceOwnedV1)==56 && offsetof(NamespaceOwnedV1,context)==16
    && offsetof(NamespaceOwnedV1,api)==24 && offsetof(NamespaceOwnedV1,storage)==32
    && offsetof(NamespaceOwnedV1,receiver)==40 && offsetof(NamespaceOwnedV1,destroy)==48,
    "native Namespace owner layout");
#endif
