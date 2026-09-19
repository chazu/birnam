/* Independent lazy Error value descriptor; include IdApi first. */
#ifndef BIRNAM_ERROR_VALUE_V2_H
#define BIRNAM_ERROR_VALUE_V2_H
#include "../namespace-presence-v1/namespace_presence_v1.h"
typedef struct {
    uint32_t version; size_t byte_size;
    int (*is_error)(const IdApi *,const NamespacePresenceV1 *,Oop);
    Oop (*from_bytes)(const IdApi *,const NamespacePresenceV1 *,const uint8_t *,size_t,const uint8_t *,size_t);
} ErrorValueApiV2;
typedef const ErrorValueApiV2 *(*ErrorValueProviderV2)(void);
typedef int (*ErrorValueDependencyV2)(const ErrorValueApiV2 *);
_Static_assert(sizeof(ErrorValueApiV2)==32 && offsetof(ErrorValueApiV2,from_bytes)==24,"Error value descriptor layout");
#endif
