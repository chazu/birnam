#ifndef BIRNAM_COLLECTION_PHASE_V1_H
#define BIRNAM_COLLECTION_PHASE_V1_H
#include <stddef.h>
#include <stdint.h>
enum { COLLECTION_PHASE_IDLE=0, COLLECTION_PHASE_MARKING=1, COLLECTION_PHASE_FINALIZING=2 };
typedef struct {
    uint32_t version; size_t byte_size;
    void *(*create)(void);
    void (*destroy)(void *);
    uint32_t (*read)(const void *);
    int (*set)(void *,uint32_t);
} CollectionPhaseFactoryV1;
typedef const CollectionPhaseFactoryV1 *(*CollectionPhaseFactoryProviderV1)(void);
typedef int (*CollectionPhaseDependencyV1)(const CollectionPhaseFactoryV1 *);
_Static_assert(sizeof(CollectionPhaseFactoryV1)==48 && offsetof(CollectionPhaseFactoryV1,set)==40,"collection phase v1 layout");
#endif
