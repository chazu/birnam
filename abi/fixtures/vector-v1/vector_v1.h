/* Independent C mirror of the immutable 81AB native storage boundary. */
#ifndef BIRNAM_VECTOR_V1_H
#define BIRNAM_VECTOR_V1_H
#include <stddef.h>
#include <stdint.h>
typedef struct { Oop node, vector; } VectorBehaviorsV1;
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
    size_t (*count)(Oop);
    Oop (*at)(Oop, size_t);
    Oop (*behavior)(void);
} VectorApiV1;
typedef const VectorApiV1 *(*VectorProviderV1)(void);
typedef int (*VectorDependencyV1)(const VectorApiV1 *);
_Static_assert(sizeof(VectorApiV1)==96, "Vector boundary size");
_Static_assert(offsetof(VectorApiV1,behavior)==88, "Vector operation offset");
_Static_assert(sizeof(VectorBehaviorsV1)==16, "Vector identities size");
#endif
