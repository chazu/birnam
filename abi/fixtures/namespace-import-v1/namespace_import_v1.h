/* Independent live import capability; include IdApi first. */
#ifndef BIRNAM_NAMESPACE_IMPORT_V1_H
#define BIRNAM_NAMESPACE_IMPORT_V1_H
#include <stdint.h>
#include <stddef.h>
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    Oop (*import)(void *,const char *);
} NamespaceImportV1;
_Static_assert(sizeof(NamespaceImportV1)==32 && offsetof(NamespaceImportV1,import)==24,"Namespace import ABI");
#endif
