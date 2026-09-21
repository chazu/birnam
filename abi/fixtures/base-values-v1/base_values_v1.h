#ifndef BIRNAM_BASE_VALUES_V1_H
#define BIRNAM_BASE_VALUES_V1_H
#include <stddef.h>
#include <stdint.h>
/* Completed identities, owned by the base module through registered roots. */
typedef struct {
    uint32_t version;
    size_t byte_size;
    void *nil_behavior, *boolean_behavior, *nil, *true_object, *false_object;
} BaseValuesV1;
_Static_assert(sizeof(BaseValuesV1)==56 && offsetof(BaseValuesV1,byte_size)==8
    && offsetof(BaseValuesV1,nil_behavior)==16 && offsetof(BaseValuesV1,boolean_behavior)==24
    && offsetof(BaseValuesV1,nil)==32 && offsetof(BaseValuesV1,true_object)==40
    && offsetof(BaseValuesV1,false_object)==48, "base values layout");
#endif
