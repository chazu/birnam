#ifndef FIXTURE_ROOT_REGISTRY_SUPPORT_H
#define FIXTURE_ROOT_REGISTRY_SUPPORT_H
#include "../../abi/fixtures/root-registry-v1/root_registry_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static void *fixture_root_registry_floor;
static inline void fixture_prepare_root_registry(void *floor,const char *floor_path) {
    RootRegistryDependencyV1 attach=(RootRegistryDependencyV1)dlsym(floor,"birnam_root_registry_dependency_v1");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_root_registry.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    RootRegistryFactoryProviderV1 provide=(RootRegistryFactoryProviderV1)dlsym(image,"birnam_root_registry_factory_v1");assert(provide);
    const RootRegistryFactoryV1 *factory=provide();assert(factory && attach(factory)==1);
    fixture_root_registry_floor=floor;
}
static inline RootRegistryV1 *fixture_root_registry(void) {
    assert(fixture_root_registry_floor);
    RootRegistryProviderV1 provide=(RootRegistryProviderV1)dlsym(fixture_root_registry_floor,"birnam_root_registry_provider_v1");assert(provide);
    RootRegistryV1 *api=provide();assert(api && api->version==1 && api->byte_size==sizeof(*api));
    return api;
}
static inline void fixture_register_root(RootProvider provider) {
    root_registry_register_v1(fixture_root_registry(),provider);
}
/* Only test fixtures use callback-slot loans to preserve independent old traces. */
typedef void (*FixtureRootSlot)(RootProvider);
static inline void fixture_root_slot(void *context,RootProvider provider) {
    FixtureRootSlot selected=*(const FixtureRootSlot *)context;selected(provider);
}
#define FIXTURE_ROOTS(slot) {1,sizeof(RootRegistryV1),&(slot),fixture_root_slot}
#endif
