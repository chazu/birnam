#ifndef FIXTURE_STORAGE_OBSERVATION_SUPPORT_H
#define FIXTURE_STORAGE_OBSERVATION_SUPPORT_H
#include "../../abi/fixtures/storage-observation-scope-v1/storage_observation_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static inline void fixture_prepare_storage_observation(void *floor,const char *floor_path) {
    StorageObservationDependencyV1 attach=(StorageObservationDependencyV1)dlsym(floor,"birnam_storage_observation_dependency_v1");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_storage.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    StorageObservationFactoryProviderV1 provide=(StorageObservationFactoryProviderV1)dlsym(image,"birnam_storage_observation_factory_v1");assert(provide);
    const StorageObservationFactoryV1 *api=provide();assert(api && attach(api)==1);
}
#endif
