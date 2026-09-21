#ifndef BIRNAM_COLLECTOR_V2_H
#define BIRNAM_COLLECTOR_V2_H
#include "../collector-v1/collector_v1.h"
/* Base and input are borrowed for the synchronous extension call only. */
typedef struct {
    void *context;
    int (*trace)(void *,const CollectorV1 *,const CollectorInputV1 *);
    int (*collect)(void *,const CollectorV1 *,const CollectorInputV1 *,size_t *);
} CollectorExtensionV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    int (*trace)(void *,const CollectorInputV1 *);
    int (*collect)(void *,const CollectorInputV1 *,size_t *);
    int (*extend)(void *,const CollectorExtensionV1 *);
} CollectorV2;
_Static_assert(sizeof(CollectorExtensionV1)==24 && sizeof(CollectorV2)==48
    && offsetof(CollectorV2,extend)==40,"collector v2 ABI");
#endif
