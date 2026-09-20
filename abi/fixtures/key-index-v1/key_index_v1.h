#ifndef BIRNAM_KEY_INDEX_V1_H
#define BIRNAM_KEY_INDEX_V1_H
#include <stddef.h>
#include <stdint.h>
typedef struct {
    uint32_t version; size_t byte_size;
    void (*destroy)(void *);
    int (*is_empty)(const void *);
    int (*get)(const void *,uintptr_t,uintptr_t *);
    void (*insert)(void **,uintptr_t,uintptr_t);
    void (*clear)(void *);
} KeyIndexFactoryV1;
typedef const KeyIndexFactoryV1 *(*KeyIndexFactoryProviderV1)(void);
typedef int (*KeyIndexDependencyV1)(const KeyIndexFactoryV1 *);
_Static_assert(sizeof(KeyIndexFactoryV1)==56 && offsetof(KeyIndexFactoryV1,clear)==48,"key index factory v1 layout");
#endif
