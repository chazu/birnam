#ifndef FIXTURE_DEFERRED_INDEX_SUPPORT_H
#define FIXTURE_DEFERRED_INDEX_SUPPORT_H
#include "../../abi/fixtures/deferred-index-v1/deferred_index_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static inline void fixture_prepare_deferred_index(void *floor,const char *floor_path) {
    DeferredIndexDependencyV1 attach=(DeferredIndexDependencyV1)dlsym(floor,"birnam_deferred_index_dependency_v1");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_storage.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    DeferredIndexFactoryProviderV1 provide=(DeferredIndexFactoryProviderV1)dlsym(image,"birnam_deferred_index_factory_v1");assert(provide);
    const DeferredIndexFactoryV1 *api=provide();assert(api && attach(api)==1);
}
#endif
