#ifndef BIRNAM_NAMESPACE_NAMES_V1_H
#define BIRNAM_NAMESPACE_NAMES_V1_H
#include <stdint.h>
#include <stddef.h>
typedef struct {
    uint32_t version;size_t byte_size;void *context;
    void (*preflight)(void *);
    const char *(*symbol_name)(void *,Oop);
} NamespaceNameFloorV1;
typedef struct {
    uint32_t version;size_t byte_size;void *context;
    const char *(*name_of)(void *,Oop);
} NamespaceNamesApiV1;
typedef NamespaceNamesApiV1 *(*NamespaceNamesProviderV1)(void);
typedef int (*NamespaceNamesDependencyV1)(NamespaceNamesApiV1 *);
_Static_assert(sizeof(NamespaceNamesApiV1)==32 && offsetof(NamespaceNamesApiV1,name_of)==24, "Namespace names ABI");
_Static_assert(sizeof(NamespaceNameFloorV1)==40, "Namespace name floor ABI");
#endif
