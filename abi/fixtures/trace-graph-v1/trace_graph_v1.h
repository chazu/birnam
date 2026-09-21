#ifndef BIRNAM_TRACE_GRAPH_V1_H
#define BIRNAM_TRACE_GRAPH_V1_H
#include "../trace-live-v1/trace_live_v1.h"
enum { TRACE_GRAPH_CORE_ONLY=1, TRACE_GRAPH_PRECISE=2 };
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    int (*roots)(void *,const TraceLiveSinkV1 *);
    int (*edges)(void *,void *,uint32_t,const TraceLiveSinkV1 *);
    int (*validate)(void *);
    TraceLiveV1 trace;
} TraceGraphV1;
_Static_assert(sizeof(TraceGraphV1)==56 && offsetof(TraceGraphV1,roots)==24
    && offsetof(TraceGraphV1,trace)==48,"Trace graph ABI");
#endif
