#ifndef FIXTURE_KEY_INDEX_SUPPORT_H
#define FIXTURE_KEY_INDEX_SUPPORT_H
#include "../../abi/fixtures/key-index-v1/key_index_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static inline void fixture_prepare_key_index(void *floor,const char *floor_path) {
    KeyIndexDependencyV1 attach=(KeyIndexDependencyV1)dlsym(floor,"birnam_key_index_dependency_v1");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_object_model.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    KeyIndexFactoryProviderV1 provide=(KeyIndexFactoryProviderV1)dlsym(image,"birnam_key_index_factory_v1");assert(provide);
    const KeyIndexFactoryV1 *api=provide();assert(api && attach(api)==1);
}
#endif
