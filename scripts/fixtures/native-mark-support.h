#ifndef FIXTURE_NATIVE_MARK_SUPPORT_H
#define FIXTURE_NATIVE_MARK_SUPPORT_H
#include "../../abi/fixtures/native-mark-v1/native_mark_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static inline void fixture_prepare_native_mark(void *floor,const char *floor_path) {
    NativeMarkDependencyV1 attach=(NativeMarkDependencyV1)dlsym(floor,"birnam_native_mark_dependency_v1");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_native_mark.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    NativeMarkProviderV1 provide=(NativeMarkProviderV1)dlsym(image,"birnam_native_mark_provider_v1");assert(provide);
    const NativeMarkV1 *api=provide();assert(api && attach(api)==1);
}
#endif
