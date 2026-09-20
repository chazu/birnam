#ifndef FIXTURE_COLLECTION_PHASE_SUPPORT_H
#define FIXTURE_COLLECTION_PHASE_SUPPORT_H
#include "../../abi/fixtures/collection-phase-v1/collection_phase_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static inline void fixture_prepare_collection_phase(void *floor,const char *floor_path) {
    CollectionPhaseDependencyV1 attach=(CollectionPhaseDependencyV1)dlsym(floor,"birnam_collection_phase_dependency_v1");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_collect.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    CollectionPhaseFactoryProviderV1 provide=(CollectionPhaseFactoryProviderV1)dlsym(image,"birnam_collection_phase_factory_v1");assert(provide);
    const CollectionPhaseFactoryV1 *api=provide();assert(api && attach(api)==1);
}
#endif
