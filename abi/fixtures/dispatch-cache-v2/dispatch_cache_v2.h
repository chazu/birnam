#ifndef BIRNAM_DISPATCH_CACHE_V2_H
#define BIRNAM_DISPATCH_CACHE_V2_H
#include <stddef.h>
#include <stdint.h>
#include "../id-protocol-v1/id_protocol.h"
typedef struct {
    uint32_t version; size_t byte_size;
    void *(*create)(void);
    void (*destroy)(void *);
    BirnamNativeEntry (*lookup)(const void *,uint64_t,uintptr_t,uintptr_t);
    void (*insert)(void *,uint64_t,uintptr_t,uintptr_t,BirnamNativeEntry);
    void (*clear)(void *);
} DispatchCacheFactoryV2;
typedef const DispatchCacheFactoryV2 *(*DispatchCacheFactoryProviderV2)(void);
typedef int (*DispatchCacheDependencyV2)(const DispatchCacheFactoryV2 *);
_Static_assert(sizeof(DispatchCacheFactoryV2)==56 && offsetof(DispatchCacheFactoryV2,clear)==48,"dispatch cache v2 layout");
#endif
