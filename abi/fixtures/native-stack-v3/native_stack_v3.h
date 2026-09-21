#ifndef BIRNAM_NATIVE_STACK_V3_H
#define BIRNAM_NATIVE_STACK_V3_H
#include "../native-stack-v1/native_stack_v1.h"
#include "../trace-live-v1/trace_live_v1.h"
typedef void (*NativeStackCapturedV1)(void *,uintptr_t,uintptr_t);
typedef int (*NativeStackCaptureV1)(uintptr_t,void *,NativeStackCapturedV1);
typedef struct {
    uint32_t version; size_t byte_size;
    NativeStackV1 *(*create)(const NativeStackFloorV1 *);
    void (*destroy)(NativeStackV1 *);
    void (*set)(NativeStackV1 *,size_t);
    size_t (*get)(const NativeStackV1 *);
    void (*scan)(const TraceLiveSinkV1 *,uintptr_t,uintptr_t);
    NativeStackCaptureV1 capture;
} NativeStackFactoryV3;
typedef const NativeStackFactoryV3 *(*NativeStackFactoryProviderV3)(void);
typedef int (*NativeStackDependencyV3)(const NativeStackFactoryV3 *);
_Static_assert(sizeof(NativeStackFactoryV3)==64 && offsetof(NativeStackFactoryV3,scan)==48
    && offsetof(NativeStackFactoryV3,capture)==56,"native stack v3 factory layout");
#endif
