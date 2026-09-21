#ifndef BIRNAM_COLLECTION_STATE_V1_H
#define BIRNAM_COLLECTION_STATE_V1_H
#include "../native-stack-v3/native_stack_v3.h"
#include "../gc-settings-v1/gc_settings_v1.h"
#include "../allocation-budget-v1/allocation_budget_v1.h"
#include "../collection-phase-v1/collection_phase_v1.h"
#include "../collect-v1/collect_v1.h"
typedef struct { void *object; uint32_t flags,reserved; } CollectionRecordV1;
typedef struct { const CollectionRecordV1 *entries; size_t count; } CollectionRecordsViewV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    int (*insert)(void *,void *,uint32_t);
    uint32_t (*get)(const void *,uintptr_t);
    int (*retain)(void *,uintptr_t);
    int (*mark)(void *,uintptr_t);
    void (*clear)(void *);
    CollectionRecordsViewV1 (*snapshot)(void *);
    int (*remove_dead)(void *,uintptr_t);
} CollectionRecordsV1;
typedef struct { void *context; void (*settings)(void *); void (*stack)(void *); } CollectionAccessV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    const CollectionRecordsV1 *records;
    NativeStackV1 *stack; const NativeStackFactoryV3 *stack_factory;
    GcSettingsV1 *settings; const GcSettingsFactoryV1 *settings_factory;
    void *budget_context; const AllocationBudgetFactoryV1 *budget_factory;
    void *phase_context; const CollectionPhaseFactoryV1 *phase_factory;
    void (*set_access)(void *,CollectionAccessV1);
    void (*destroy)(void *);
    CollectV1 collect;
} CollectionStateV1;
_Static_assert(sizeof(CollectionRecordV1)==16 && sizeof(CollectionRecordsViewV1)==16,"record ABI");
_Static_assert(sizeof(CollectionRecordsV1)==80 && offsetof(CollectionRecordsV1,remove_dead)==72,"records ABI");
_Static_assert(sizeof(CollectionStateV1)==120 && offsetof(CollectionStateV1,records)==24
    && offsetof(CollectionStateV1,set_access)==96 && offsetof(CollectionStateV1,destroy)==104 && offsetof(CollectionStateV1,collect)==112,"collection state ABI");
#endif
