#ifndef BIRNAM_FLOAT_V1_H
#define BIRNAM_FLOAT_V1_H
#include "../namespace-import-v1/namespace_import_v1.h"
#include <stddef.h>
#include <stdint.h>
/* Independent C mirror of Float's live operations and immutable bits codec. */
typedef struct {
    uint32_t version;
    size_t byte_size, payload_size, payload_alignment;
    void (*initialize_bits)(uint8_t *, uint64_t);
    uint64_t (*read_bits)(const uint8_t *);
} FloatStorageV1;
typedef struct {
    uint32_t version;
    size_t byte_size;
    void *context;
    void *behavior, *prototype;
    void *(*make)(void *, double);
    double (*read)(void *, void *);
    int (*is_exact)(void *, void *);
    const FloatStorageV1 *storage;
} FloatApiV1;
typedef int (*FloatDependencyV1)(FloatApiV1 *);
typedef FloatApiV1 *(*FloatProviderV1)(void);
typedef FloatApiV1 *(*FloatPrepareV1)(IdApi *);
typedef FloatApiV1 *(*FloatPrepareV2)(IdApi *,const NamespacePresenceV1 *);
typedef FloatApiV1 *(*FloatPrepareV3)(IdApi *,const NamespacePresenceV1 *,const NamespaceImportV1 *);
_Static_assert(sizeof(FloatStorageV1) == 48, "Float storage extent");
_Static_assert(sizeof(FloatApiV1) == 72, "Float operations extent");
_Static_assert(offsetof(FloatApiV1, make) == 40, "Float constructor offset");
#endif
