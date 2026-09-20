/* Independent lazy Error value descriptor; include IdApi first. */
#ifndef BIRNAM_ERROR_VALUE_V3_H
#define BIRNAM_ERROR_VALUE_V3_H
#include "../namespace-import-v1/namespace_import_v1.h"
#include "../namespace-presence-v1/namespace_presence_v1.h"
typedef struct {
    uint32_t version; size_t byte_size;
    int (*is_error)(const IdApi *,const NamespacePresenceV1 *,const NamespaceImportV1 *,Oop);
    Oop (*from_bytes)(const IdApi *,const NamespacePresenceV1 *,const NamespaceImportV1 *,const uint8_t *,size_t,const uint8_t *,size_t);
} ErrorValueApiV3;
typedef const ErrorValueApiV3 *(*ErrorValueProviderV3)(void);
typedef int (*ErrorValueDependencyV3)(const ErrorValueApiV3 *);
_Static_assert(sizeof(ErrorValueApiV3)==32 && offsetof(ErrorValueApiV3,from_bytes)==24,"Error value descriptor layout");
#endif
