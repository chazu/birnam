#ifndef BIRNAM_FLOAT_PREPARE_V6_H
#define BIRNAM_FLOAT_PREPARE_V6_H
#include "../float-v1/float_v1.h"
#include "../root-registry-v1/root_registry_v1.h"
#include "../payload-layout-v1/payload_layout_v1.h"
typedef FloatApiV1 *(*FloatPrepareV6)(IdApi *, const NamespacePresenceV1 *, const NamespaceImportV1 *, const NamespaceExportV1 *, const RootRegistryV1 *,const PayloadLayoutV1 *);
#endif
