#ifndef BIRNAM_STRING_PREPARE_V3_H
#define BIRNAM_STRING_PREPARE_V3_H
#include "../string-v1/string_v1.h"
#include "../root-registry-v1/root_registry_v1.h"
typedef StringApiV1 *(*StringPrepareV3)(IdApi *, const NamespaceExportV1 *, const NativeAllocationV1 *, const RootRegistryV1 *);
#endif
