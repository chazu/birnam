#ifndef FIXTURE_GUARD_VERDICT_SUPPORT_H
#define FIXTURE_GUARD_VERDICT_SUPPORT_H
#include "../../abi/fixtures/guard-verdict-v1/guard_verdict_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static inline void fixture_prepare_guard_verdict(void *floor,const char *floor_path) {
    GuardVerdictDependencyV1 attach=(GuardVerdictDependencyV1)dlsym(floor,"birnam_guard_verdict_dependency_v1");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_dispatch_cache.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    GuardVerdictFactoryProviderV1 provide=(GuardVerdictFactoryProviderV1)dlsym(image,"birnam_guard_verdict_factory_v1");assert(provide);
    const GuardVerdictFactoryV1 *api=provide();assert(api && attach(api)==1);
}
#endif
