/* Independent C mirror of Map construction and method installation. */
#ifndef BIRNAM_MAP_V6_H
#define BIRNAM_MAP_V6_H
#include "../map-v1/map_v1.h"
typedef struct {
    uint32_t version; size_t byte_size;
    MapBehaviorsV1 (*prepare)(IdApi *,const NamespaceImportV1 *,const RootRegistryV1 *,Oop,int);
    void (*finish_prototype)(IdApi *);
    void (*set_layout)(IdApi *,const PayloadLayoutV1 *);
    void (*publish)(IdApi *,const NamespaceExportV1 *);
    void (*install)(IdApi *,Oop,Oop,Oop);
} MapApiV6;
typedef const MapApiV6 *(*MapProviderV6)(void);
typedef int (*MapDependencyV6)(const MapApiV6 *);
_Static_assert(sizeof(MapApiV6)==56 && offsetof(MapApiV6,install)==48,"Map v5 boundary shape");
#endif
