#ifndef BIRNAM_ALLOCATION_BUDGET_V1_H
#define BIRNAM_ALLOCATION_BUDGET_V1_H
#include <stddef.h>
#include <stdint.h>
typedef struct {
    uint32_t version; size_t byte_size;
    void *(*create)(void);
    void (*destroy)(void *);
    size_t (*read)(const void *);
    void (*replace)(void *,size_t);
    int (*charge)(void *,size_t);
    int (*preview)(const void *,size_t,size_t *);
} AllocationBudgetFactoryV1;
typedef const AllocationBudgetFactoryV1 *(*AllocationBudgetFactoryProviderV1)(void);
typedef int (*AllocationBudgetDependencyV1)(const AllocationBudgetFactoryV1 *);
_Static_assert(sizeof(AllocationBudgetFactoryV1)==64 && offsetof(AllocationBudgetFactoryV1,preview)==56,"allocation budget v1 layout");
#endif
