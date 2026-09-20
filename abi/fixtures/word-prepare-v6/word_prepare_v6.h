#ifndef BIRNAM_WORD_PREPARE_V6_H
#define BIRNAM_WORD_PREPARE_V6_H
#include "../word-v1/word_v1.h"
#include "../root-registry-v1/root_registry_v1.h"
#include "../payload-layout-v1/payload_layout_v1.h"
typedef WordApiV1 *(*WordPrepareV6)(IdApi *, const NamespacePresenceV1 *, const NamespaceImportV1 *, const NamespaceExportV1 *, const RootRegistryV1 *,const PayloadLayoutV1 *);
#endif
