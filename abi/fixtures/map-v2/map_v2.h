/* Independent C mirror of Map construction and method installation. */
#ifndef BIRNAM_MAP_V2_H
#define BIRNAM_MAP_V2_H
#include "../map-v1/map_v1.h"
typedef struct {
    uint32_t version; size_t byte_size;
    MapBehaviorsV1 (*prepare)(IdApi *,Oop,int);
    void (*finish_prototype)(IdApi *);
    void (*set_layout)(IdApi *);
    void (*publish)(IdApi *);
    void (*install)(IdApi *,Oop,Oop,Oop);
} MapApiV2;
typedef const MapApiV2 *(*MapProviderV2)(void);
typedef int (*MapDependencyV2)(const MapApiV2 *);
_Static_assert(sizeof(MapApiV2)==56 && offsetof(MapApiV2,install)==48,"Map v2 boundary shape");
#endif
