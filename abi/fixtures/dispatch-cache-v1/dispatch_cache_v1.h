#ifndef BIRNAM_DISPATCH_CACHE_V1_H
#define BIRNAM_DISPATCH_CACHE_V1_H
#include <stddef.h>
#include <stdint.h>
typedef struct {
    uint32_t version; size_t byte_size;
    void *(*create)(void);
    void (*destroy)(void *);
    void *(*lookup)(const void *,uint64_t,uintptr_t,uintptr_t);
    void (*insert)(void *,uint64_t,uintptr_t,uintptr_t,void *);
    void (*clear)(void *);
} DispatchCacheFactoryV1;
typedef const DispatchCacheFactoryV1 *(*DispatchCacheFactoryProviderV1)(void);
typedef int (*DispatchCacheDependencyV1)(const DispatchCacheFactoryV1 *);
_Static_assert(sizeof(DispatchCacheFactoryV1)==56 && offsetof(DispatchCacheFactoryV1,clear)==48,"dispatch cache v1 layout");
#endif
