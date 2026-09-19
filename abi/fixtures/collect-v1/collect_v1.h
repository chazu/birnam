#ifndef BIRNAM_COLLECT_V1_H
#define BIRNAM_COLLECT_V1_H
/* Independent native mirror; uses only C values, never private Rust types. */
#include <stddef.h>
#include <stdint.h>
typedef struct { const uintptr_t *addresses; size_t count; } CollectDeadV1;
typedef struct { void *object; void (*finalizer)(void *); } CollectDetachedV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void (*phase)(void *, uint32_t);
    void (*clear_marks)(void *);
    int (*trace)(void *);
    int (*inspect_retirement)(void *);
    void (*settle_deferred)(void *);
    void (*retire_metadata)(void *);
    void (*snapshot_dead)(void *, CollectDeadV1 *);
    int (*detach)(void *, uintptr_t, CollectDetachedV1 *);
    int (*reclaim)(void *, uintptr_t);
    size_t (*reclaim_deferred)(void *);
    void (*reset_accounting)(void *);
    void (*prune)(void *);
} CollectSourceV1;
typedef int (*CollectV1)(const CollectSourceV1 *, size_t *);
typedef struct { uint32_t version; size_t byte_size; CollectV1 collect; } CollectApiV1;
typedef const CollectApiV1 *(*CollectProviderV1)(void);
typedef int (*CollectDependencyV1)(const CollectApiV1 *);
_Static_assert(sizeof(CollectDeadV1)==16, "dead size");
_Static_assert(sizeof(CollectDetachedV1)==16, "detached size");
_Static_assert(sizeof(CollectSourceV1)==120, "source size");
_Static_assert(sizeof(CollectApiV1)==24, "API size");
_Static_assert(offsetof(CollectSourceV1, detach)==80, "detach offset");
#endif
