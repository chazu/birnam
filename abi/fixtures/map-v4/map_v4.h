/* Independent C mirror of Map construction and method installation. */
#ifndef BIRNAM_MAP_V4_H
#define BIRNAM_MAP_V4_H
#include "../map-v1/map_v1.h"
typedef struct {
    uint32_t version; size_t byte_size;
    MapBehaviorsV1 (*prepare)(IdApi *,const NamespaceImportV1 *,Oop,int);
    void (*finish_prototype)(IdApi *);
    void (*set_layout)(IdApi *);
    void (*publish)(IdApi *,const NamespaceExportV1 *);
    void (*install)(IdApi *,Oop,Oop,Oop);
} MapApiV4;
typedef const MapApiV4 *(*MapProviderV4)(void);
typedef int (*MapDependencyV4)(const MapApiV4 *);
_Static_assert(sizeof(MapApiV4)==56 && offsetof(MapApiV4,install)==48,"Map v4 boundary shape");
#endif
