#ifndef BIRNAM_TRACE_GRAPH_V2_H
#define BIRNAM_TRACE_GRAPH_V2_H
#include "../trace-live-v1/trace_live_v1.h"
#include "../idapi-v20/storage_v1.h"
typedef struct {
    const StorageAllocationV1 *allocations; size_t allocation_count;
    const Oop *roots; size_t root_count;
    const StorageEdgeV1 *edges; size_t edge_count;
    const Oop *symbols; size_t symbol_count;
} TraceSnapshotV1;
typedef int (*TraceInspectV1)(void *, const TraceSnapshotV1 *);
_Static_assert(sizeof(TraceSnapshotV1)==64, "Trace snapshot ABI");
enum { TRACE_GRAPH_CORE_ONLY=1, TRACE_GRAPH_PRECISE=2 };
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    int (*roots)(void *,const TraceLiveSinkV1 *);
    int (*edges)(void *,void *,uint32_t,const TraceLiveSinkV1 *);
    int (*validate)(void *);
    TraceLiveV1 trace;
    int (*inspect)(void *,void *,TraceInspectV1);
} TraceGraphV2;
_Static_assert(sizeof(TraceGraphV2)==64 && offsetof(TraceGraphV2,roots)==24
    && offsetof(TraceGraphV2,trace)==48 && offsetof(TraceGraphV2,inspect)==56,"Trace graph ABI");
#endif
