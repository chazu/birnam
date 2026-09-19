#ifndef BIRNAM_NAMESPACE_V1_H
#define BIRNAM_NAMESPACE_V1_H
#include <stdint.h>
#include <stddef.h>
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
} NamespaceApiV1;
typedef const NamespaceApiV1 *(*NamespaceProviderV1)(void);
typedef int (*NamespaceDependencyV1)(const NamespaceApiV1 *);
_Static_assert(sizeof(NamespaceApiV1)==80 && offsetof(NamespaceApiV1,create)==16 && offsetof(NamespaceApiV1,bindings)==72, "Namespace ABI");
_Static_assert(sizeof(NamespaceBindingV1)==16 && sizeof(NamespaceViewV1)==16, "Namespace view ABI");
#endif
