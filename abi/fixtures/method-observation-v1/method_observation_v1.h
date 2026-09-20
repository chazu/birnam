#ifndef BIRNAM_METHOD_OBSERVATION_V1_H
#define BIRNAM_METHOD_OBSERVATION_V1_H
#include <stddef.h>
#include <stdint.h>
typedef void *Oop;
typedef void (*MethodObservationHookV1)(Oop, Oop);
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void (*admit)(void *);
} MethodObservationFloorV1;
typedef struct {
    uint32_t version; size_t byte_size; void *context;
    void (*replace)(void *, MethodObservationHookV1);
} MethodObservationV1;
typedef struct {
    uint32_t version; size_t byte_size;
    MethodObservationV1 *(*create)(const MethodObservationFloorV1 *);
    void (*destroy)(MethodObservationV1 *);
    void (*notify)(MethodObservationV1 *, int, Oop, Oop);
} MethodObservationFactoryV1;
typedef const MethodObservationFactoryV1 *(*MethodObservationFactoryProviderV1)(void);
typedef int (*MethodObservationDependencyV1)(const MethodObservationFactoryV1 *);
typedef MethodObservationV1 *(*MethodObservationProviderV1)(void);
_Static_assert(sizeof(MethodObservationFloorV1)==32,"floor extent");
_Static_assert(offsetof(MethodObservationFloorV1,admit)==24,"admission offset");
_Static_assert(sizeof(MethodObservationV1)==32,"capability extent");
_Static_assert(offsetof(MethodObservationV1,replace)==24,"replacement offset");
_Static_assert(sizeof(MethodObservationFactoryV1)==40,"factory extent");
_Static_assert(offsetof(MethodObservationFactoryV1,notify)==32,"notification offset");
static inline void method_observation_replace_v1(const MethodObservationV1 *api,MethodObservationHookV1 hook) {
    MethodObservationV1 selected=*api;selected.replace(selected.context,hook);
}
#endif
