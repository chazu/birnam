/* Independent C mirror of Map construction and method installation. */
#ifndef BIRNAM_MAP_V5_H
#define BIRNAM_MAP_V5_H
#include "../map-v1/map_v1.h"
typedef struct {
    uint32_t version; size_t byte_size;
    MapBehaviorsV1 (*prepare)(IdApi *,const NamespaceImportV1 *,const RootRegistryV1 *,Oop,int);
    void (*finish_prototype)(IdApi *);
    void (*set_layout)(IdApi *);
    void (*publish)(IdApi *,const NamespaceExportV1 *);
    void (*install)(IdApi *,Oop,Oop,Oop);
} MapApiV5;
typedef const MapApiV5 *(*MapProviderV5)(void);
typedef int (*MapDependencyV5)(const MapApiV5 *);
_Static_assert(sizeof(MapApiV5)==56 && offsetof(MapApiV5,install)==48,"Map v5 boundary shape");
#endif
