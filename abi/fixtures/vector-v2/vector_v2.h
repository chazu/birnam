/* Independent C mirror of the immutable 81AE native Vector boundary. */
#ifndef BIRNAM_VECTOR_V2_H
#define BIRNAM_VECTOR_V2_H
#include "../vector-v1/vector_v1.h"
typedef struct {
    uint32_t version;
    size_t byte_size;
    VectorBehaviorsV1 (*prepare)(IdApi *, Oop, int);
    void (*finish_prototype)(IdApi *);
    void (*set_layout)(IdApi *);
    void (*set_owner)(IdApi *);
    void (*publish)(IdApi *);
    Oop (*empty)(void);
    Oop (*push)(Oop, Oop);
    Oop (*behavior)(void);
    Method (*install)(IdApi *, Oop, Oop, Oop);
    Method for_each;
    Method inject;
} VectorApiV2;
typedef const VectorApiV2 *(*VectorProviderV2)(void);
typedef int (*VectorDependencyV2)(const VectorApiV2 *);
_Static_assert(sizeof(VectorApiV2)==104, "Vector v2 boundary size");
_Static_assert(offsetof(VectorApiV2,behavior)==72 && offsetof(VectorApiV2,install)==80 && offsetof(VectorApiV2,inject)==96, "Vector v2 operation offsets");
#endif
