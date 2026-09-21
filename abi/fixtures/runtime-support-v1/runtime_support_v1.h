#ifndef BIRNAM_RUNTIME_SUPPORT_V1_H
#define BIRNAM_RUNTIME_SUPPORT_V1_H
#include "../method-observation-v1/method_observation_v1.h"
#include "../storage-observation-scope-v1/storage_observation_v1.h"
#include "../deferred-identity-v1/deferred_identity_v1.h"
#include "../deferred-index-v1/deferred_index_v1.h"
#include "../dispatch-cache-v2/dispatch_cache_v2.h"
#include "../guard-verdict-v1/guard_verdict_v1.h"
typedef struct {
    uint32_t version; size_t byte_size;
    MethodObservationFactoryV1 observation;
    StorageObservationFactoryV1 storage;
    DeferredIdentityFactoryV1 identity;
    DeferredIndexFactoryV1 index;
    DispatchCacheFactoryV2 cache;
    GuardVerdictFactoryV1 verdict;
} RuntimeFactoriesV1;
_Static_assert(sizeof(RuntimeFactoriesV1)==352, "Runtime factories extent");
_Static_assert(offsetof(RuntimeFactoriesV1,observation)==16 && offsetof(RuntimeFactoriesV1,storage)==56 &&
    offsetof(RuntimeFactoriesV1,identity)==120 && offsetof(RuntimeFactoriesV1,index)==168 &&
    offsetof(RuntimeFactoriesV1,cache)==248 && offsetof(RuntimeFactoriesV1,verdict)==304, "Runtime factory offsets");
#endif
