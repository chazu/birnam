#ifndef BIRNAM_RANGE_METHODS_V1_H
#define BIRNAM_RANGE_METHODS_V1_H
/* Independent construction ABI. Include the current IdApi header first. */
typedef void (*RangeMethodsInstallV1)(IdApi *,Oop,Oop,Oop,Oop,Method,Method);
typedef struct { uint32_t version; size_t byte_size; RangeMethodsInstallV1 install; } RangeMethodsApiV1;
typedef const RangeMethodsApiV1 *(*RangeMethodsProviderV1)(void);
typedef int (*RangeMethodsDependencyV1)(const RangeMethodsApiV1 *);
_Static_assert(sizeof(RangeMethodsApiV1)==24,"Range methods construction descriptor");
_Static_assert(offsetof(RangeMethodsApiV1,install)==16,"Range installer offset");
#endif
