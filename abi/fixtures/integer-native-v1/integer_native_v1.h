#ifndef BIRNAM_INTEGER_NATIVE_V1_H
#define BIRNAM_INTEGER_NATIVE_V1_H
#include "../id-protocol-v1/id_protocol.h"
#include "../integer-v1/integer_v1.h"
#include "../root-registry-owner-v1/root_registry_owner_v1.h"
#include "../namespace-owner-v1/namespace_owner_v1.h"
typedef struct {
    BirnamProtocol id;
    BirnamNativeEntry allocation_size, behavior_of;
    void *allocate, *namespace_receiver, *binding, *error_behavior_name;
    RootRegistryOwnedV1 roots;
} IntegerConstructionV1;
typedef int (*IntegerConstructionQueryV1)(const IntegerApiV1 *, IntegerConstructionV1 *);
typedef uint32_t (*NativeNumericQueryV5)(BirnamNativeEntry, const IntegerApiV1 *);
typedef int (*NamespaceLookupQueryV1)(BirnamNativeEntry, const NamespaceOwnedV1 *, const BirnamProtocol *);
_Static_assert(sizeof(IntegerConstructionV1)==184,"Integer construction layout");
_Static_assert(offsetof(IntegerConstructionV1,roots)==112,"Integer construction roots");
#endif
