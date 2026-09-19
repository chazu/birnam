#ifndef BIRNAM_INTEGER_V1_H
#define BIRNAM_INTEGER_V1_H
#include <stddef.h>
#include <stdint.h>
/* Independently maintained C mirror of the native Integer dependency. */
typedef struct {
    uint32_t version;
    size_t byte_size, payload_size, payload_alignment;
    void (*initialize)(uint8_t *, int64_t);
    int64_t (*read)(const uint8_t *);
} IntegerStorageV1;
typedef struct {
    uint32_t version;
    size_t byte_size;
    void *context;
    void *behavior, *prototype;
    void *(*make)(void *, int64_t);
    int64_t (*read)(void *, void *);
    int (*is_exact)(void *, void *);
    const IntegerStorageV1 *storage;
    uintptr_t (*allocation_context)(void *);
} IntegerApiV1;
typedef int (*IntegerDependencyV1)(IntegerApiV1 *);
typedef IntegerApiV1 *(*IntegerProviderV1)(void);
_Static_assert(sizeof(IntegerStorageV1) == 48, "Integer storage extent");
_Static_assert(sizeof(IntegerApiV1) == 80, "Integer operations extent");
_Static_assert(offsetof(IntegerApiV1, make) == 40, "Integer constructor offset");
#endif
