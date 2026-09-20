#ifndef FIXTURE_DISPATCH_CACHE_SUPPORT_H
#define FIXTURE_DISPATCH_CACHE_SUPPORT_H
#include "../../abi/fixtures/dispatch-cache-v1/dispatch_cache_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static inline void fixture_prepare_dispatch_cache(void *floor,const char *floor_path) {
    DispatchCacheDependencyV1 attach=(DispatchCacheDependencyV1)dlsym(floor,"birnam_dispatch_cache_dependency_v1");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_dispatch_cache.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    DispatchCacheFactoryProviderV1 provide=(DispatchCacheFactoryProviderV1)dlsym(image,"birnam_dispatch_cache_factory_v1");assert(provide);
    const DispatchCacheFactoryV1 *api=provide();assert(api && attach(api)==1);
}
#endif
