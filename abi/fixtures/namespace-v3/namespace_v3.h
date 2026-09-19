#ifndef BIRNAM_NAMESPACE_V3_H
#define BIRNAM_NAMESPACE_V3_H
#include <stdint.h>
#include "../namespace-names-v1/namespace_names_v1.h"
#include <stddef.h>
#include "../namespace-enumeration-v1/namespace_enumeration_v1.h"
typedef struct { Oop name, value; } NamespaceBindingV1;
typedef struct { const NamespaceBindingV1 *bindings; size_t count; } NamespaceViewV1;
typedef struct {
    uint32_t version; size_t byte_size;
    void *(*create)(void);
    void (*destroy)(void *);
    Oop (*bind)(void *, Oop, Oop);
    Oop (*find)(const void *, Oop);
    Oop (*reverse)(const void *, Oop);
    size_t (*count)(const void *);
    Oop (*name_at)(const void *, size_t);
    NamespaceViewV1 (*bindings)(const void *);
    NamespaceNamesApiV1 *(*create_names)(void *,Oop (*)(const void *,Oop),const NamespaceNameFloorV1 *);
    void (*destroy_names)(NamespaceNamesApiV1 *);
    NamespaceEnumerationV1 *(*create_enumeration)(const void *,size_t (*)(const void *),Oop (*)(const void *,size_t),const NamespaceEnumerationFloorV1 *);
    void (*destroy_enumeration)(NamespaceEnumerationV1 *);
} NamespaceApiV3;
typedef const NamespaceApiV3 *(*NamespaceProviderV3)(void);
typedef int (*NamespaceDependencyV3)(const NamespaceApiV3 *);
_Static_assert(sizeof(NamespaceApiV3)==112 && offsetof(NamespaceApiV3,create)==16 && offsetof(NamespaceApiV3,bindings)==72, "Namespace ABI");
_Static_assert(sizeof(NamespaceBindingV1)==16 && sizeof(NamespaceViewV1)==16, "Namespace view ABI");
#endif
