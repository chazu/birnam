/* Independent lazy Error value descriptor; include IdApi first. */
#ifndef BIRNAM_ERROR_VALUE_V1_H
#define BIRNAM_ERROR_VALUE_V1_H
typedef struct {
    uint32_t version; size_t byte_size;
    int (*is_error)(const IdApi *,Oop);
    Oop (*from_bytes)(const IdApi *,const uint8_t *,size_t,const uint8_t *,size_t);
} ErrorValueApiV1;
typedef const ErrorValueApiV1 *(*ErrorValueProviderV1)(void);
typedef int (*ErrorValueDependencyV1)(const ErrorValueApiV1 *);
_Static_assert(sizeof(ErrorValueApiV1)==32 && offsetof(ErrorValueApiV1,from_bytes)==24,"Error value descriptor layout");
#endif
