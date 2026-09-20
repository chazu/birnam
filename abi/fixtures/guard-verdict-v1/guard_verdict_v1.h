#ifndef BIRNAM_GUARD_VERDICT_V1_H
#define BIRNAM_GUARD_VERDICT_V1_H
#include <stddef.h>
#include <stdint.h>
typedef struct { uint64_t epoch; uint32_t verified; } GuardVerdictStateV1;
typedef struct {
    uint32_t version; size_t byte_size;
    void *(*create)(void);
    void (*destroy)(void *);
    void (*read)(const void *,GuardVerdictStateV1 *);
    void (*write)(void *,uint64_t,int);
} GuardVerdictFactoryV1;
typedef const GuardVerdictFactoryV1 *(*GuardVerdictFactoryProviderV1)(void);
typedef int (*GuardVerdictDependencyV1)(const GuardVerdictFactoryV1 *);
_Static_assert(sizeof(GuardVerdictStateV1)==16 && offsetof(GuardVerdictStateV1,verified)==8,"guard verdict state v1 layout");
_Static_assert(sizeof(GuardVerdictFactoryV1)==48 && offsetof(GuardVerdictFactoryV1,write)==40,"guard verdict factory v1 layout");
#endif
