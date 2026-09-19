/* Independent C mirror of Map's immutable native storage boundary. */
#ifndef BIRNAM_MAP_V1_H
#define BIRNAM_MAP_V1_H
typedef struct { Oop map,node; } MapBehaviorsV1;
typedef struct {
    uint32_t version; size_t byte_size;
    MapBehaviorsV1 (*prepare)(IdApi *,Oop,int);
    void (*finish_prototype)(IdApi *);
    void (*set_layout)(IdApi *);
    void (*publish)(IdApi *);
    Oop (*empty)(void);
    Oop (*put)(const IdApi *,Oop,Oop,Oop);
    Oop (*find)(const IdApi *,Oop,Oop);
    size_t (*count)(Oop);
    Oop (*root)(Oop);
    size_t (*slot_count)(Oop);
    Oop (*slot)(Oop,size_t);
    Oop (*behavior)(void);
} MapApiV1;
typedef const MapApiV1 *(*MapProviderV1)(void);
typedef int (*MapDependencyV1)(const MapApiV1 *);
_Static_assert(sizeof(MapApiV1)==112 && offsetof(MapApiV1,behavior)==104 && sizeof(MapBehaviorsV1)==16,"Map v1 boundary shape");
#endif
