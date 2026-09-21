#ifndef BIRNAM_COLLECTOR_V3_H
#define BIRNAM_COLLECTOR_V3_H
#include "../collector-v2/collector_v2.h"
#include "../bootstrap-v2/bootstrap_v2.h"
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    int (*trace)(void *,const CollectorInputV1 *);
    int (*collect)(void *,const CollectorInputV1 *,size_t *);
    int (*extend)(void *,const CollectorExtensionV1 *);
    int (*activate)(void *,const BootstrapOwnedV2 *,const RootRegistryOwnedV1 *);
    int (*active)(const void *);
} CollectorV3;
_Static_assert(sizeof(CollectorV3)==64 && offsetof(CollectorV3,activate)==48,"collector v3 ABI");
#endif
