/* Independent live export capability; no IdApi dependency. */
#ifndef BIRNAM_NAMESPACE_EXPORT_V1_H
#define BIRNAM_NAMESPACE_EXPORT_V1_H
#include <stdint.h>
#include <stddef.h>
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void *(*export)(void *,const char *,void *);
} NamespaceExportV1;
_Static_assert(sizeof(NamespaceExportV1)==32 && offsetof(NamespaceExportV1,export)==24,"Namespace export ABI");
typedef NamespaceExportV1 *(*NamespaceExportProviderV1)(void);
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void *(*access)(void *);
    void *(*intern)(void *,const uint8_t *,size_t);
    void (*mirror)(void *,void *,void *);
} NamespaceExportFloorV1;
_Static_assert(sizeof(NamespaceExportFloorV1)==48 && offsetof(NamespaceExportFloorV1,access)==24 && offsetof(NamespaceExportFloorV1,intern)==32 && offsetof(NamespaceExportFloorV1,mirror)==40,"Namespace export floor ABI");
static inline void *namespace_export_v1(const NamespaceExportV1 *api,const char *name,void *value) {
    NamespaceExportV1 selected=*api;
    return selected.export(selected.context,name,value);
}
#endif
