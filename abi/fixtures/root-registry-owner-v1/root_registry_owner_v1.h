#ifndef BIRNAM_ROOT_REGISTRY_OWNER_V1_H
#define BIRNAM_ROOT_REGISTRY_OWNER_V1_H
#include "../root-registry-v1/root_registry_v1.h"
typedef struct {
    void *const *entries;
    size_t count;
} RootRegistryRootsV1;
typedef struct {
    uint32_t version;
    size_t byte_size;
    RootRegistryV1 *api;
    const RootRegistryFactoryV1 *factory;
    void *context;
    size_t (*retain)(void *, void *);
    int (*release)(void *, size_t);
    RootRegistryRootsV1 (*roots)(void *);
    int (*set_admission)(void *, const RootRegistryFloorV1 *);
} RootRegistryOwnedV1;
_Static_assert(sizeof(RootRegistryOwnedV1)==72 && offsetof(RootRegistryOwnedV1,set_admission)==64,"root owner layout");
_Static_assert(sizeof(RootRegistryRootsV1)==16,"retained roots layout");
#endif
