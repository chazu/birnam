#ifndef BIRNAM_ALLOCATION_IDENTITY_V1_H
#define BIRNAM_ALLOCATION_IDENTITY_V1_H
#include <stddef.h>
#include <stdint.h>
typedef struct {
    uint32_t version; size_t byte_size;
    void *(*create)(void);
    void (*destroy)(void *);
    uint64_t (*read)(const void *);
    void (*write)(void *,uint64_t);
} AllocationIdentityFactoryV1;
typedef const AllocationIdentityFactoryV1 *(*AllocationIdentityFactoryProviderV1)(void);
typedef int (*AllocationIdentityDependencyV1)(const AllocationIdentityFactoryV1 *);
_Static_assert(sizeof(AllocationIdentityFactoryV1)==48 && offsetof(AllocationIdentityFactoryV1,write)==40,"allocation identity factory v1 layout");
#endif
