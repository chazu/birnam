#ifndef BIRNAM_TRACE_HOOKS_V1_H
#define BIRNAM_TRACE_HOOKS_V1_H
#include <stddef.h>
#include <stdint.h>
typedef void (*TraceHookFnV1)(void *,void (*)(void *));
typedef struct { TraceHookFnV1 hook; } TraceHookSlotV1;
typedef struct { void *parent; TraceHookSlotV1 slot; } TraceHookEntryV1;
typedef struct {
    const void *context; size_t budget;
    int (*read)(const void *,void *,TraceHookEntryV1 *);
} TraceHookSourceV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void (*admit)(void *);
    void (*replace)(void *,void *,TraceHookSlotV1);
} TraceHooksFloorV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void (*set)(void *,void *,TraceHookFnV1);
} TraceHooksV1;
typedef struct {
    uint32_t version; size_t byte_size;
    TraceHooksV1 *(*create)(const TraceHooksFloorV1 *);
    void (*destroy)(TraceHooksV1 *);
    TraceHookSlotV1 (*slot)(TraceHookFnV1);
    TraceHookFnV1 (*resolve)(const TraceHookSourceV1 *,void *);
} TraceHooksFactoryV1;
typedef const TraceHooksFactoryV1 *(*TraceHooksFactoryProviderV1)(void);
typedef int (*TraceHooksDependencyV1)(const TraceHooksFactoryV1 *);
typedef TraceHooksV1 *(*TraceHooksProviderV1)(void);
_Static_assert(sizeof(TraceHookSlotV1)==8,"trace-hook slot");
_Static_assert(sizeof(TraceHookEntryV1)==16 && offsetof(TraceHookEntryV1,slot)==8,"trace-hook entry");
_Static_assert(sizeof(TraceHookSourceV1)==24 && offsetof(TraceHookSourceV1,read)==16,"trace-hook source");
_Static_assert(sizeof(TraceHooksFloorV1)==40 && offsetof(TraceHooksFloorV1,replace)==32,"trace-hooks floor");
_Static_assert(sizeof(TraceHooksV1)==32 && offsetof(TraceHooksV1,set)==24,"trace-hooks API");
_Static_assert(sizeof(TraceHooksFactoryV1)==48 && offsetof(TraceHooksFactoryV1,resolve)==40,"trace-hooks factory");
static inline void trace_hooks_set_v1(const TraceHooksV1 *selected,void *vt,TraceHookFnV1 hook) {
    TraceHooksV1 api=*selected; api.set(api.context,vt,hook);
}
#endif
