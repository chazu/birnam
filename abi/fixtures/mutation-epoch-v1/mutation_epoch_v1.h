#ifndef BIRNAM_MUTATION_EPOCH_V1_H
#define BIRNAM_MUTATION_EPOCH_V1_H
#include <stddef.h>
#include <stdint.h>
typedef struct {
    uint32_t version; size_t byte_size;
    void *(*create)(void);
    void (*destroy)(void *);
    uint64_t (*read)(const void *);
    void (*write)(void *,uint64_t);
} MutationEpochFactoryV1;
typedef const MutationEpochFactoryV1 *(*MutationEpochFactoryProviderV1)(void);
typedef int (*MutationEpochDependencyV1)(const MutationEpochFactoryV1 *);
_Static_assert(sizeof(MutationEpochFactoryV1)==48 && offsetof(MutationEpochFactoryV1,write)==40,"mutation epoch factory v1 layout");
#endif
