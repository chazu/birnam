/* Independent C mirror of Map construction and method installation. */
#ifndef BIRNAM_MAP_V3_H
#define BIRNAM_MAP_V3_H
#include "../map-v1/map_v1.h"
typedef struct {
    uint32_t version; size_t byte_size;
    MapBehaviorsV1 (*prepare)(IdApi *,const NamespaceImportV1 *,Oop,int);
    void (*finish_prototype)(IdApi *);
    void (*set_layout)(IdApi *);
    void (*publish)(IdApi *);
    void (*install)(IdApi *,Oop,Oop,Oop);
} MapApiV3;
typedef const MapApiV3 *(*MapProviderV3)(void);
typedef int (*MapDependencyV3)(const MapApiV3 *);
_Static_assert(sizeof(MapApiV3)==56 && offsetof(MapApiV3,install)==48,"Map v3 boundary shape");
#endif
