/* Independent live import capability; no IdApi dependency. */
#ifndef BIRNAM_NAMESPACE_IMPORT_V1_H
#define BIRNAM_NAMESPACE_IMPORT_V1_H
#include <stdint.h>
#include <stddef.h>
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void *(*import)(void *,const char *);
} NamespaceImportV1;
_Static_assert(sizeof(NamespaceImportV1)==32 && offsetof(NamespaceImportV1,import)==24,"Namespace import ABI");
typedef NamespaceImportV1 *(*NamespaceImportProviderV1)(void);
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void *(*access)(void *);
    void *(*intern)(void *,const uint8_t *,size_t);
} NamespaceImportFloorV1;
_Static_assert(sizeof(NamespaceImportFloorV1)==40 && offsetof(NamespaceImportFloorV1,access)==24 && offsetof(NamespaceImportFloorV1,intern)==32,"Namespace import floor ABI");
static inline void *namespace_import_v1(const NamespaceImportV1 *api,const char *name) {
    NamespaceImportV1 selected=*api;
    return selected.import(selected.context,name);
}
#endif
