/* Independent C mirror of the immutable 81AE native Vector boundary. */
#ifndef BIRNAM_VECTOR_V3_H
#define BIRNAM_VECTOR_V3_H
#include "../vector-v1/vector_v1.h"
typedef struct {
    uint32_t version;
    size_t byte_size;
    VectorBehaviorsV1 (*prepare)(IdApi *, const NamespaceImportV1 *, Oop, int);
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
} VectorApiV3;
typedef const VectorApiV3 *(*VectorProviderV3)(void);
typedef int (*VectorDependencyV3)(const VectorApiV3 *);
_Static_assert(sizeof(VectorApiV3)==104, "Vector v3 boundary size");
_Static_assert(offsetof(VectorApiV3,behavior)==72 && offsetof(VectorApiV3,install)==80 && offsetof(VectorApiV3,inject)==96, "Vector v3 operation offsets");
#endif
