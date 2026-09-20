#ifndef BIRNAM_STORAGE_V1_H
#define BIRNAM_STORAGE_V1_H
/* Independent C mirror of the storage boundary. Includes current Id types. */
#include "idapi_v17.h"
enum { STORAGE_OK=0, STORAGE_INVALID=1, STORAGE_BUSY=2, STORAGE_INCOMPLETE=3 };
enum { STORAGE_PERMANENT=1, STORAGE_CORE_PAYLOAD=2, STORAGE_HAS_FINALIZER=4, STORAGE_HAS_GC_LAYOUT=8 };
enum { STORAGE_NATIVE_ROOTS=1, STORAGE_LEGACY_ROOTS=2, STORAGE_LEGACY_HOOKS=4, STORAGE_DEFERRED=8, STORAGE_MISSING=16 };
typedef struct {
    Oop object; uint64_t identity; size_t requested_size, padded_size, allocation_bytes;
    Oop behavior; uint32_t flags; TraceHook trace_hook;
} StorageAllocationV1;
typedef struct { Oop from, to; } StorageEdgeV1;
typedef struct {
    uint32_t version; size_t byte_size; uint64_t runtime_identity, observation_identity;
    const StorageAllocationV1 *allocations; size_t allocation_count;
    const Oop *roots; size_t root_count;
    const StorageEdgeV1 *edges; size_t edge_count;
    const RootProvider *legacy_root_providers; size_t legacy_root_provider_count;
    size_t deferred_count, deferred_bytes; uint32_t coverage_gaps, gc_precise;
} StorageViewV1;
typedef int (*StorageVisitorV1)(void *, const StorageViewV1 *);
typedef struct { uint32_t version; size_t byte_size; void *context; StorageVisitorV1 visitor; } StorageRequestV1;
typedef int (*StorageObserveV1)(const StorageRequestV1 *);
typedef struct {
    uint64_t runtime_identity, allocation_identity; Oop object; size_t extent;
    const size_t *strong_offsets; size_t strong_offset_count;
} StorageDescriptionV1;
typedef struct {
    void *context; int (*root)(void *, Oop); int (*describe)(void *, const StorageDescriptionV1 *);
} StorageSinkV1;
typedef struct {
    void *context; int (*provide)(void *, const StorageViewV1 *, const StorageSinkV1 *);
} StorageProviderV1;
typedef struct {
    const StorageViewV1 *storage; const Oop *roots; size_t root_count;
    const StorageDescriptionV1 *descriptions; size_t description_count;
    const StorageEdgeV1 *payload_edges; size_t payload_edge_count;
    size_t missing_description_count; uint32_t coverage_gaps;
} StorageDescribedViewV1;
typedef struct {
    uint32_t version; size_t byte_size; const StorageProviderV1 *providers; size_t provider_count;
    uint32_t reject_gaps; void *context; int (*visitor)(void *, const StorageDescribedViewV1 *);
} StorageDescribeRequestV1;
typedef int (*StorageDescribeV1)(const StorageViewV1 *, const StorageDescribeRequestV1 *);
_Static_assert(sizeof(StorageAllocationV1)==64, "allocation carrier");
_Static_assert(sizeof(StorageViewV1)==120, "view carrier");
_Static_assert(sizeof(StorageRequestV1)==32, "request carrier");
_Static_assert(sizeof(StorageDescriptionV1)==48, "description carrier");
_Static_assert(sizeof(StorageSinkV1)==24, "sink carrier");
_Static_assert(sizeof(StorageProviderV1)==16, "provider carrier");
_Static_assert(sizeof(StorageDescribedViewV1)==72, "described view carrier");
_Static_assert(sizeof(StorageDescribeRequestV1)==56, "describe request carrier");
#endif
