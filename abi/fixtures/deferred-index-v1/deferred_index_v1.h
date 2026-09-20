#ifndef BIRNAM_DEFERRED_INDEX_V1_H
#define BIRNAM_DEFERRED_INDEX_V1_H
#include <stddef.h>
#include <stdint.h>
typedef struct { uint64_t identity; uintptr_t index; } DeferredIndexRowV1;
typedef struct {
    uint32_t version; size_t byte_size;
    void (*destroy)(void *);
    size_t (*len)(const void *);
    size_t (*capacity)(const void *);
    void (*push)(void **, DeferredIndexRowV1);
    int (*read)(const void *, size_t, DeferredIndexRowV1 *);
    int (*write)(void *, size_t, DeferredIndexRowV1);
    int (*swap_remove)(void *, size_t, DeferredIndexRowV1 *);
    void (*clear)(void *);
} DeferredIndexFactoryV1;
typedef const DeferredIndexFactoryV1 *(*DeferredIndexFactoryProviderV1)(void);
typedef int (*DeferredIndexDependencyV1)(const DeferredIndexFactoryV1 *);
_Static_assert(sizeof(DeferredIndexRowV1)==16 && _Alignof(DeferredIndexRowV1)==8 && offsetof(DeferredIndexRowV1,index)==8,"deferred index row");
_Static_assert(sizeof(DeferredIndexFactoryV1)==80 && offsetof(DeferredIndexFactoryV1,clear)==72,"deferred index factory");
#endif
