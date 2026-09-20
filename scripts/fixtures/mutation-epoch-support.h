#ifndef FIXTURE_MUTATION_EPOCH_SUPPORT_H
#define FIXTURE_MUTATION_EPOCH_SUPPORT_H
#include "../../abi/fixtures/mutation-epoch-v1/mutation_epoch_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static inline void fixture_prepare_mutation_epoch(void *floor,const char *floor_path) {
    MutationEpochDependencyV1 attach=(MutationEpochDependencyV1)dlsym(floor,"birnam_mutation_epoch_dependency_v1");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_object_model.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    MutationEpochFactoryProviderV1 provide=(MutationEpochFactoryProviderV1)dlsym(image,"birnam_mutation_epoch_factory_v1");assert(provide);
    const MutationEpochFactoryV1 *api=provide();assert(api && attach(api)==1);
}
#endif
