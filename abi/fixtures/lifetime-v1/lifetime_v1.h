#ifndef BIRNAM_LIFETIME_V1_H
#define BIRNAM_LIFETIME_V1_H
#include "../payload-layout-v1/payload_layout_v1.h"
#include "../trace-hooks-v1/trace_hooks_v1.h"
#include "../finalizers-v1/finalizers_v1.h"
typedef struct {
    void *context;
    void (*admit)(void *);
    uint32_t (*facts)(void *,void *);
} LifetimeAccessV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    PayloadLayoutV1 *layouts; TraceHooksV1 *hooks; FinalizersV1 *finalizers;
    const PayloadLayoutFactoryV1 *layout_factory;
    const TraceHooksFactoryV1 *hook_factory;
    const FinalizersFactoryV1 *finalizer_factory;
    PayloadLayoutViewV1 (*layout)(void *,void *);
    void (*replace_layout)(void *,void *,PayloadLayoutOwnedV1);
    TraceHookSlotV1 (*hook)(void *,void *);
    void (*replace_hook)(void *,void *,TraceHookSlotV1);
    FinalizerSlotV1 (*finalizer)(void *,void *);
    void (*replace_finalizer)(void *,void *,FinalizerSlotV1);
    void (*retire)(void *,const size_t *,size_t);
    void (*set_access)(void *,LifetimeAccessV1);
    void (*destroy)(void *);
} LifetimeOwnedV1;
_Static_assert(sizeof(LifetimeAccessV1)==24,"Lifetime access ABI");
_Static_assert(sizeof(LifetimeOwnedV1)==144 && offsetof(LifetimeOwnedV1,layout)==72 && offsetof(LifetimeOwnedV1,destroy)==136,"Lifetime owner ABI");
#endif
