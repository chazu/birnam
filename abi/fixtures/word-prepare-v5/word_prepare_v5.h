#ifndef BIRNAM_WORD_PREPARE_V5_H
#define BIRNAM_WORD_PREPARE_V5_H
#include "../word-v1/word_v1.h"
#include "../root-registry-v1/root_registry_v1.h"
typedef WordApiV1 *(*WordPrepareV5)(IdApi *, const NamespacePresenceV1 *, const NamespaceImportV1 *, const NamespaceExportV1 *, const RootRegistryV1 *);
#endif
