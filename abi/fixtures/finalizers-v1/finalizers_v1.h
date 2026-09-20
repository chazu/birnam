#ifndef BIRNAM_FINALIZERS_V1_H
#define BIRNAM_FINALIZERS_V1_H
#include <stddef.h>
#include <stdint.h>
typedef void (*FinalizerFnV1)(void *);
typedef struct { FinalizerFnV1 callback; } FinalizerSlotV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void (*admit)(void *);
    int (*read_permanent)(void *,void *,int *);
    void (*replace)(void *,void *,FinalizerSlotV1);
} FinalizersFloorV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void (*set)(void *,void *,FinalizerFnV1);
} FinalizersV1;
typedef struct {
    uint32_t version; size_t byte_size;
    FinalizersV1 *(*create)(const FinalizersFloorV1 *);
    void (*destroy)(FinalizersV1 *);
    FinalizerSlotV1 (*slot)(int,FinalizerFnV1);
} FinalizersFactoryV1;
typedef const FinalizersFactoryV1 *(*FinalizersFactoryProviderV1)(void);
typedef int (*FinalizersDependencyV1)(const FinalizersFactoryV1 *);
typedef FinalizersV1 *(*FinalizersProviderV1)(void);
_Static_assert(sizeof(FinalizerSlotV1)==8,"finalizer slot");
_Static_assert(sizeof(FinalizersFloorV1)==48 && offsetof(FinalizersFloorV1,replace)==40,"finalizers floor");
_Static_assert(sizeof(FinalizersV1)==32 && offsetof(FinalizersV1,set)==24,"finalizers API");
_Static_assert(sizeof(FinalizersFactoryV1)==40 && offsetof(FinalizersFactoryV1,slot)==32,"finalizers factory");
static inline void finalizers_set_v1(const FinalizersV1 *selected,void *object,FinalizerFnV1 callback) {
    FinalizersV1 api=*selected; api.set(api.context,object,callback);
}
#endif
