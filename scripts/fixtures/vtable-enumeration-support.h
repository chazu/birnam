#ifndef FIXTURE_VTABLE_ENUMERATION_SUPPORT_H
#define FIXTURE_VTABLE_ENUMERATION_SUPPORT_H
#include "../../abi/fixtures/vtable-enumeration-v2/vtable_enumeration_v2.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static void *fixture_vtable_floor;
static inline void fixture_prepare_vtable_enumeration(void *floor,const char *floor_path) {
    VtableEnumerationDependencyV2 attach=(VtableEnumerationDependencyV2)dlsym(floor,"birnam_vtable_enumeration_dependency_v2");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_object_model.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    VtableEnumerationFactoryProviderV2 provide=(VtableEnumerationFactoryProviderV2)dlsym(image,"birnam_vtable_enumeration_factory_v2");assert(provide);
    const VtableEnumerationFactoryV2 *factory=provide();assert(factory && attach(factory)==1);
    fixture_vtable_floor=floor;
}
static inline VtableEnumerationV2 *fixture_vtable_enumeration(void) {
    assert(fixture_vtable_floor);
    VtableEnumerationProviderV2 provide=(VtableEnumerationProviderV2)dlsym(fixture_vtable_floor,"birnam_vtable_enumeration_provider_v2");assert(provide);
    VtableEnumerationV2 *api=provide();assert(api && api->version==2 && api->byte_size==sizeof(*api));
    return api;
}
#endif
