#ifndef BIRNAM_STORAGE_OBSERVATION_V1_H
#define BIRNAM_STORAGE_OBSERVATION_V1_H
#include <stddef.h>
#include <stdint.h>
typedef struct { uint64_t epoch; uint32_t active; uint32_t permit; } StorageObservationStateV1;
typedef struct {
    uint32_t version; size_t byte_size;
    void *(*create)(void);
    void (*destroy)(void *);
    void (*read)(const void *,StorageObservationStateV1 *);
    void (*set_active)(void *,int);
    void (*set_permit)(void *,int);
    int (*advance_epoch)(void *,uint64_t *);
} StorageObservationFactoryV1;
typedef const StorageObservationFactoryV1 *(*StorageObservationFactoryProviderV1)(void);
typedef int (*StorageObservationDependencyV1)(const StorageObservationFactoryV1 *);
_Static_assert(sizeof(StorageObservationStateV1)==16 && offsetof(StorageObservationStateV1,permit)==12,"storage observation state v1 layout");
_Static_assert(sizeof(StorageObservationFactoryV1)==64 && offsetof(StorageObservationFactoryV1,advance_epoch)==56,"storage observation factory v1 layout");
#endif
