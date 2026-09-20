#ifndef FIXTURE_ALLOCATION_IDENTITY_SUPPORT_H
#define FIXTURE_ALLOCATION_IDENTITY_SUPPORT_H
#include "../../abi/fixtures/allocation-identity-v1/allocation_identity_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static inline void fixture_prepare_allocation_identity(void *floor,const char *floor_path) {
    AllocationIdentityDependencyV1 attach=(AllocationIdentityDependencyV1)dlsym(floor,"birnam_allocation_identity_dependency_v1");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_storage.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    AllocationIdentityFactoryProviderV1 provide=(AllocationIdentityFactoryProviderV1)dlsym(image,"birnam_allocation_identity_factory_v1");assert(provide);
    const AllocationIdentityFactoryV1 *api=provide();assert(api && attach(api)==1);
}
#endif
