#ifndef BIRNAM_COLLECTOR_V1_H
#define BIRNAM_COLLECTOR_V1_H
#include "../trace-live-v1/trace_live_v1.h"
typedef struct {
    void *context;
    int (*inspect)(void *);
    void (*settle)(void *);
    void (*retire)(void *);
    size_t (*reclaim)(void *);
    void (*prune)(void *);
} CollectorRetirementV1;
typedef struct {
    uintptr_t start,bottom; void *context;
    int (*roots)(void *,const TraceLiveSinkV1 *);
    const CollectorRetirementV1 *retirement;
} CollectorInputV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    int (*trace)(void *,const CollectorInputV1 *);
    int (*collect)(void *,const CollectorInputV1 *,size_t *);
} CollectorV1;
_Static_assert(sizeof(CollectorV1)==40 && sizeof(CollectorInputV1)==40 && sizeof(CollectorRetirementV1)==48,"collector ABI");
#endif
