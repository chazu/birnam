#ifndef FIXTURE_METHOD_OBSERVATION_SUPPORT_H
#define FIXTURE_METHOD_OBSERVATION_SUPPORT_H
#include "../../abi/fixtures/method-observation-v1/method_observation_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static void *fixture_observation_floor;
static inline void fixture_prepare_method_observation(void *floor,const char *floor_path) {
    MethodObservationDependencyV1 attach=(MethodObservationDependencyV1)dlsym(floor,"birnam_method_observation_dependency_v1");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_method_observation.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    MethodObservationFactoryProviderV1 provide=(MethodObservationFactoryProviderV1)dlsym(image,"birnam_method_observation_factory_v1");assert(provide);
    const MethodObservationFactoryV1 *factory=provide();assert(factory && attach(factory)==1);
    fixture_observation_floor=floor;
}
static inline MethodObservationV1 *fixture_method_observation(void) {
    assert(fixture_observation_floor);
    MethodObservationProviderV1 provide=(MethodObservationProviderV1)dlsym(fixture_observation_floor,"birnam_method_observation_provider_v1");assert(provide);
    MethodObservationV1 *api=provide();assert(api && api->version==1 && api->byte_size==sizeof(*api));
    return api;
}
#endif
