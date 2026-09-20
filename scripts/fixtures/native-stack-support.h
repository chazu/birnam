#ifndef FIXTURE_NATIVE_STACK_SUPPORT_H
#define FIXTURE_NATIVE_STACK_SUPPORT_H
#include "../../abi/fixtures/native-stack-v2/native_stack_v2.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static void *fixture_native_stack_floor;
static inline void fixture_prepare_native_stack(void *floor,const char *floor_path) {
    NativeStackDependencyV2 attach=(NativeStackDependencyV2)dlsym(floor,"birnam_native_stack_dependency_v2");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_native_stack.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    NativeStackFactoryProviderV2 provide=(NativeStackFactoryProviderV2)dlsym(image,"birnam_native_stack_factory_v2");assert(provide);
    const NativeStackFactoryV2 *factory=provide();assert(factory && attach(factory)==1);
    fixture_native_stack_floor=floor;
}
static inline NativeStackV1 *fixture_native_stack(void) {
    assert(fixture_native_stack_floor);
    NativeStackProviderV1 provide=(NativeStackProviderV1)dlsym(fixture_native_stack_floor,"birnam_native_stack_provider_v1");assert(provide);
    NativeStackV1 *api=provide();assert(api && api->version==1 && api->byte_size==sizeof(*api));return api;
}
static inline void fixture_set_stack_bottom(size_t address) { native_stack_set_v1(fixture_native_stack(),address); }
#endif
