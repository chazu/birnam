#ifndef BIRNAM_BOOTSTRAP_V1_H
#define BIRNAM_BOOTSTRAP_V1_H
#include "../id-protocol-v1/id_protocol.h"
#include "../root-registry-owner-v1/root_registry_owner_v1.h"
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    BirnamOutcome (*load)(void *,const uint8_t *,size_t,const BirnamOperand *,size_t);
    uint32_t (*adopt_roots)(void *,const RootRegistryOwnedV1 *);
    size_t (*release_result)(void *,BirnamOperand);
    void (*destroy)(void *);
} BootstrapOwnedV1;
_Static_assert(sizeof(BootstrapOwnedV1)==56 && offsetof(BootstrapOwnedV1,load)==24 && offsetof(BootstrapOwnedV1,destroy)==48,"Bootstrap owner ABI");
#endif
