#ifndef BIRNAM_NATIVE_STACK_V1_H
#define BIRNAM_NATIVE_STACK_V1_H
#include <stddef.h>
#include <stdint.h>
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void (*admit)(void *);
} NativeStackFloorV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void (*set)(void *,size_t);
} NativeStackV1;
typedef struct {
    uint32_t version; size_t byte_size;
    NativeStackV1 *(*create)(const NativeStackFloorV1 *);
    void (*destroy)(NativeStackV1 *);
    void (*set)(NativeStackV1 *,size_t);
    size_t (*get)(const NativeStackV1 *);
} NativeStackFactoryV1;
typedef const NativeStackFactoryV1 *(*NativeStackFactoryProviderV1)(void);
typedef int (*NativeStackDependencyV1)(const NativeStackFactoryV1 *);
typedef NativeStackV1 *(*NativeStackProviderV1)(void);
static inline void native_stack_set_v1(const NativeStackV1 *selected,size_t address) {
    NativeStackV1 api=*selected;api.set(api.context,address);
}
_Static_assert(sizeof(NativeStackFloorV1)==32 && offsetof(NativeStackFloorV1,admit)==24,"native stack floor layout");
_Static_assert(sizeof(NativeStackV1)==32 && offsetof(NativeStackV1,set)==24,"native stack capability layout");
_Static_assert(sizeof(NativeStackFactoryV1)==48 && offsetof(NativeStackFactoryV1,get)==40,"native stack factory layout");
#endif
