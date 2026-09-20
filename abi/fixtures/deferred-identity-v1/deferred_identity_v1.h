#ifndef BIRNAM_DEFERRED_IDENTITY_V1_H
#define BIRNAM_DEFERRED_IDENTITY_V1_H
#include <stddef.h>
#include <stdint.h>
typedef struct {
    uint32_t version; size_t byte_size;
    void *(*create)(void);
    void (*destroy)(void *);
    uint64_t (*read)(const void *);
    void (*write)(void *,uint64_t);
} DeferredIdentityFactoryV1;
typedef const DeferredIdentityFactoryV1 *(*DeferredIdentityFactoryProviderV1)(void);
typedef int (*DeferredIdentityDependencyV1)(const DeferredIdentityFactoryV1 *);
_Static_assert(sizeof(DeferredIdentityFactoryV1)==48 && offsetof(DeferredIdentityFactoryV1,write)==40,"allocation identity factory v1 layout");
#endif
