#ifndef BIRNAM_NAMESPACE_ENUMERATION_V1_H
#define BIRNAM_NAMESPACE_ENUMERATION_V1_H
#include <stdint.h>
#include <stddef.h>
typedef struct {
    uint32_t version;size_t byte_size;void *context;
    void (*preflight)(void *);
} NamespaceEnumerationFloorV1;
typedef struct {
    uint32_t version;size_t byte_size;void *context;
    size_t (*count)(void *);
} NamespaceCountV1;
typedef struct {
    uint32_t version;size_t byte_size;void *context;
    Oop (*name_at)(void *,size_t);
} NamespaceNameAtV1;
typedef struct { NamespaceCountV1 count;NamespaceNameAtV1 name_at; } NamespaceEnumerationV1;
typedef struct { uint32_t version;size_t byte_size;const NamespaceCountV1 *entry; } NamespaceCountPublicationV1;
typedef struct { uint32_t version;size_t byte_size;const NamespaceNameAtV1 *entry; } NamespaceNameAtPublicationV1;
_Static_assert(sizeof(NamespaceEnumerationFloorV1)==32,"Enumeration floor ABI");
_Static_assert(sizeof(NamespaceCountV1)==32 && offsetof(NamespaceCountV1,count)==24,"Count ABI");
_Static_assert(sizeof(NamespaceNameAtV1)==32 && offsetof(NamespaceNameAtV1,name_at)==24,"NameAt ABI");
_Static_assert(sizeof(NamespaceEnumerationV1)==64,"Enumeration ownership ABI");
_Static_assert(sizeof(NamespaceCountPublicationV1)==24 && sizeof(NamespaceNameAtPublicationV1)==24 && offsetof(NamespaceCountPublicationV1,entry)==16,"Enumeration publication ABI");
#endif
