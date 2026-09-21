#ifndef BIRNAM_NATIVE_STACK_V4_H
#define BIRNAM_NATIVE_STACK_V4_H
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
    uintptr_t (*thread_bottom)(void);
} NativeStackFactoryV4;
typedef const NativeStackFactoryV4 *(*NativeStackFactoryProviderV4)(void);
typedef int (*NativeStackDependencyV4)(const NativeStackFactoryV4 *);
_Static_assert(sizeof(NativeStackFactoryV4)==72 && offsetof(NativeStackFactoryV4,scan)==48
    && offsetof(NativeStackFactoryV4,capture)==56,"native stack v4 factory layout");
#endif
