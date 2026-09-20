#ifndef BIRNAM_NATIVE_STACK_V2_H
#define BIRNAM_NATIVE_STACK_V2_H
#include "../native-stack-v1/native_stack_v1.h"
#include "../trace-live-v1/trace_live_v1.h"
typedef struct {
    uint32_t version; size_t byte_size;
    NativeStackV1 *(*create)(const NativeStackFloorV1 *);
    void (*destroy)(NativeStackV1 *);
    void (*set)(NativeStackV1 *,size_t);
    size_t (*get)(const NativeStackV1 *);
    void (*scan)(const TraceLiveSinkV1 *,uintptr_t,uintptr_t);
} NativeStackFactoryV2;
typedef const NativeStackFactoryV2 *(*NativeStackFactoryProviderV2)(void);
typedef int (*NativeStackDependencyV2)(const NativeStackFactoryV2 *);
_Static_assert(sizeof(NativeStackFactoryV2)==56 && offsetof(NativeStackFactoryV2,scan)==48,"native stack v2 factory layout");
#endif
