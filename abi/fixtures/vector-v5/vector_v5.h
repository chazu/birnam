/* Independent C mirror of the immutable 81AE native Vector boundary. */
#ifndef BIRNAM_VECTOR_V5_H
#define BIRNAM_VECTOR_V5_H
#include "../vector-v1/vector_v1.h"
typedef struct {
    uint32_t version;
    size_t byte_size;
    VectorBehaviorsV1 (*prepare)(IdApi *, const NamespaceImportV1 *, const RootRegistryV1 *, Oop, int);
    void (*finish_prototype)(IdApi *);
    void (*set_layout)(IdApi *);
    void (*set_owner)(IdApi *);
    void (*publish)(IdApi *,const NamespaceExportV1 *);
    Oop (*empty)(void);
    Oop (*push)(Oop, Oop);
    Oop (*behavior)(void);
    Method (*install)(IdApi *, Oop, Oop, Oop);
    Method for_each;
    Method inject;
} VectorApiV5;
typedef const VectorApiV5 *(*VectorProviderV5)(void);
typedef int (*VectorDependencyV5)(const VectorApiV5 *);
_Static_assert(sizeof(VectorApiV5)==104, "Vector v5 boundary size");
_Static_assert(offsetof(VectorApiV5,behavior)==72 && offsetof(VectorApiV5,install)==80 && offsetof(VectorApiV5,inject)==96, "Vector v5 operation offsets");
#endif
