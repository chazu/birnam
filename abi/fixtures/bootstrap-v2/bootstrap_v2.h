#ifndef BIRNAM_BOOTSTRAP_V2_H
#define BIRNAM_BOOTSTRAP_V2_H
#include "../id-protocol-v1/id_protocol.h"
#include "../root-registry-owner-v1/root_registry_owner_v1.h"
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    BirnamOutcome (*load)(void *,const uint8_t *,size_t,const BirnamOperand *,size_t);
    uint32_t (*adopt_roots)(void *,const RootRegistryOwnedV1 *);
    size_t (*release_result)(void *,BirnamOperand);
    void (*destroy)(void *);
    BirnamOutcome (*pause)(void *,BirnamNativeEntry);
} BootstrapOwnedV2;
_Static_assert(sizeof(BootstrapOwnedV2)==64 && offsetof(BootstrapOwnedV2,load)==24 && offsetof(BootstrapOwnedV2,destroy)==48,"Bootstrap owner ABI");
#endif
