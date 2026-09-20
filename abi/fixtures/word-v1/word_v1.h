#ifndef BIRNAM_WORD_V1_H
#define BIRNAM_WORD_V1_H
#include "../namespace-export-v1/namespace_export_v1.h"
#include "../namespace-import-v1/namespace_import_v1.h"
#include <stddef.h>
#include <stdint.h>
/* Independent C mirror of Word's live operations and immutable bits codec. */
typedef struct {
    uint32_t version;
    size_t byte_size, payload_size, payload_alignment;
    void (*initialize)(uint8_t *, uint64_t);
    uint64_t (*read)(const uint8_t *);
} WordStorageV1;
typedef struct {
    uint32_t version;
    size_t byte_size;
    void *context;
    void *behavior, *prototype;
    void *(*make)(void *, uint64_t);
    uint64_t (*read)(void *, void *);
    int (*is_exact)(void *, void *);
    const WordStorageV1 *storage;
} WordApiV1;
typedef int (*WordDependencyV1)(WordApiV1 *);
typedef WordApiV1 *(*WordProviderV1)(void);
typedef WordApiV1 *(*WordPrepareV1)(IdApi *);
typedef WordApiV1 *(*WordPrepareV2)(IdApi *,const NamespacePresenceV1 *);
typedef WordApiV1 *(*WordPrepareV3)(IdApi *,const NamespacePresenceV1 *,const NamespaceImportV1 *);
typedef WordApiV1 *(*WordPrepareV4)(IdApi *,const NamespacePresenceV1 *,const NamespaceImportV1 *,const NamespaceExportV1 *);
_Static_assert(sizeof(WordStorageV1) == 48, "Word storage extent");
_Static_assert(sizeof(WordApiV1) == 72, "Word operations extent");
_Static_assert(offsetof(WordApiV1, make) == 40, "Word constructor offset");
#endif
