#ifndef BIRNAM_ROOT_REGISTRY_V1_H
#define BIRNAM_ROOT_REGISTRY_V1_H
#include <stddef.h>
#include <stdint.h>
typedef void (*RootRegistryMarkV1)(void *);
typedef void (*RootRegistryRootV1)(RootRegistryMarkV1);
typedef struct RootRegistryV1 {
    uint32_t version;
    size_t byte_size;
    void *context;
    void (*register_root)(void *, RootRegistryRootV1);
} RootRegistryV1;
typedef struct {
    uint32_t version;
    size_t byte_size;
    void *context;
    void (*admit)(void *);
    /* Must not return. */
    void (*overflow)(void *);
} RootRegistryFloorV1;
typedef struct {
    size_t count;
    RootRegistryRootV1 entries[32];
} RootRegistrySnapshotV1;
typedef struct {
    uint32_t version;
    size_t byte_size;
    RootRegistryV1 *(*create)(const RootRegistryFloorV1 *);
    void (*destroy)(RootRegistryV1 *);
    int (*append)(RootRegistryV1 *, RootRegistryRootV1);
    RootRegistrySnapshotV1 (*snapshot)(const RootRegistryV1 *);
} RootRegistryFactoryV1;
typedef const RootRegistryFactoryV1 *(*RootRegistryFactoryProviderV1)(void);
typedef int (*RootRegistryDependencyV1)(const RootRegistryFactoryV1 *);
typedef RootRegistryV1 *(*RootRegistryProviderV1)(void);
static inline void root_registry_register_v1(const RootRegistryV1 *selected, RootRegistryRootV1 provider) {
    RootRegistryV1 api=*selected; api.register_root(api.context,provider);
}
_Static_assert(sizeof(RootRegistryV1)==32 && offsetof(RootRegistryV1,register_root)==24,"root registry public layout");
_Static_assert(sizeof(RootRegistryFloorV1)==40 && offsetof(RootRegistryFloorV1,overflow)==32,"root registry floor layout");
_Static_assert(sizeof(RootRegistryFactoryV1)==48 && offsetof(RootRegistryFactoryV1,snapshot)==40,"root registry factory layout");
_Static_assert(sizeof(RootRegistrySnapshotV1)==264 && offsetof(RootRegistrySnapshotV1,entries)==8,"root registry snapshot layout");
#endif
