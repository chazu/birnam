#ifndef BIRNAM_STRING_V1_H
#define BIRNAM_STRING_V1_H
#include <stddef.h>
#include <stdint.h>
/* Independent C mirror of the admitted native allocation and String contracts. */
typedef struct {
    void *context, *behavior;
    size_t payload_size;
    void (*finalizer)(void *);
    void (*prepare)(void *);
    void (*initialize)(void *, void *);
} NativeAllocationPlanV1;
typedef struct {
    uint32_t version;
    size_t byte_size;
    void *context;
    void *(*allocate)(void *, const NativeAllocationPlanV1 *);
} NativeAllocationV1;
typedef const NativeAllocationV1 *(*NativeAllocationProviderV1)(void);
typedef struct {
    uint32_t version;
    size_t byte_size, payload_size, payload_alignment;
    void *(*construct)(void *, const uint8_t *, size_t, Finalizer);
    void *(*clone)(void *, void *, Finalizer);
    const uint8_t *(*borrow)(void *, void *, size_t *);
    int (*release)(void *, void *);
    Finalizer cleanup;
    void *(*cleanup_owner)(Finalizer);
} StringStorageV1;
typedef struct {
    uint32_t version;
    size_t byte_size;
    void *context, *behavior, *prototype;
    void *(*make)(void *, const uint8_t *, size_t);
    const uint8_t *(*bytes)(void *, void *, size_t *);
    const StringStorageV1 *storage;
} StringApiV1;
typedef int (*StringDependencyV1)(StringApiV1 *);
typedef StringApiV1 *(*StringProviderV1)(void);
typedef StringApiV1 *(*StringPrepareV1)(IdApi *, const NativeAllocationV1 *);
_Static_assert(sizeof(NativeAllocationV1)==32, "native allocation extent");
_Static_assert(sizeof(NativeAllocationPlanV1)==48, "native allocation plan extent");
_Static_assert(sizeof(StringStorageV1)==80, "String storage extent");
_Static_assert(sizeof(StringApiV1)==64, "String operations extent");
#endif
