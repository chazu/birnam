#ifndef BIRNAM_TRACE_LIVE_V1_H
#define BIRNAM_TRACE_LIVE_V1_H
/* Independent native mirror; no private Id structures. */
#include <stddef.h>
#include <stdint.h>
typedef struct {
    void *context;
    int (*emit)(void *, uintptr_t);
} TraceLiveSinkV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    int (*roots)(void *, const TraceLiveSinkV1 *);
    int (*admit)(void *, uintptr_t);
    int (*edges)(void *, uintptr_t, const TraceLiveSinkV1 *);
} TraceLiveSourceV1;
typedef int (*TraceLiveV1)(const TraceLiveSourceV1 *);
typedef struct {
    uint32_t version; size_t byte_size; TraceLiveV1 trace;
} TraceLiveApiV1;
typedef const TraceLiveApiV1 *(*TraceLiveProviderV1)(void);
typedef int (*TraceLiveDependencyV1)(const TraceLiveApiV1 *);
_Static_assert(sizeof(TraceLiveSinkV1)==16, "live sink size");
_Static_assert(sizeof(TraceLiveSourceV1)==48, "live source size");
_Static_assert(sizeof(TraceLiveApiV1)==24, "live API size");
_Static_assert(offsetof(TraceLiveSourceV1, admit)==32, "admission offset");
#endif
