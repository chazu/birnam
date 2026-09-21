#ifndef BIRNAM_OBJECT_MODEL_OWNER_V1_H
#define BIRNAM_OBJECT_MODEL_OWNER_V1_H
#include <stddef.h>
#include <stdint.h>
typedef void *Oop;
#include "../vtable-enumeration-v2/vtable_enumeration_v2.h"
typedef struct {
    uint32_t version; size_t byte_size;
    void *context;
    VtableEnumerationV2 *enumeration;
    void *(*owner)(const void *,void *);
    void (*set_owner)(void *,void *,void *);
    void (*retire)(void *,const size_t *,size_t);
    void (*destroy)(void *);
} ObjectModelOwnedV1;
_Static_assert(sizeof(ObjectModelOwnedV1)==64 && offsetof(ObjectModelOwnedV1,context)==16
    && offsetof(ObjectModelOwnedV1,enumeration)==24 && offsetof(ObjectModelOwnedV1,owner)==32
    && offsetof(ObjectModelOwnedV1,set_owner)==40 && offsetof(ObjectModelOwnedV1,retire)==48
    && offsetof(ObjectModelOwnedV1,destroy)==56,"Object Model owner layout");
#endif
