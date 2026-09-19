#ifndef BIRNAM_TRACE_V1_H
#define BIRNAM_TRACE_V1_H
/* Independent C mirror of the scoped production tracer ABI. */
#include "../storage-v1/storage_v1.h"
typedef struct { Oop object; uint64_t allocation_identity; } TraceAllocationRefV1;
typedef struct {
    uint32_t version; size_t byte_size;
    uint64_t runtime_identity, observation_identity;
    const StorageDescribedViewV1 *described;
    const TraceAllocationRefV1 *reached; size_t reached_count;
    const TraceAllocationRefV1 *not_reached; size_t not_reached_count;
    uint32_t coverage_gaps;
} TraceReportV1;
typedef struct {
    uint32_t version; size_t byte_size; uint32_t accept_gaps;
    void *context; int (*visitor)(void *, const TraceReportV1 *);
} TraceRequestV1;
typedef int (*TraceDescribedV1)(const StorageDescribedViewV1 *, const TraceRequestV1 *);
_Static_assert(sizeof(TraceAllocationRefV1)==16, "allocation reference carrier");
_Static_assert(sizeof(TraceRequestV1)==40, "trace request carrier");
_Static_assert(sizeof(TraceReportV1)==80, "trace report carrier");
_Static_assert(offsetof(TraceRequestV1, visitor)==32, "trace visitor offset");
_Static_assert(offsetof(TraceReportV1, reached)==40, "reached offset");
_Static_assert(offsetof(TraceReportV1, coverage_gaps)==72, "coverage offset");
#endif
