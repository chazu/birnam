#ifndef FIXTURE_NAMESPACE_SUPPORT_H
#define FIXTURE_NAMESPACE_SUPPORT_H
#include "../../abi/fixtures/namespace-v1/namespace_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static inline void fixture_prepare_namespace(void *floor, const char *floor_path) {
    NamespaceDependencyV1 attach=(NamespaceDependencyV1)dlsym(floor,"birnam_namespace_dependency_v1");
    /* Independent tests also run against the pre-extraction baseline. */
    if(!attach) return;
    const char *slash=strrchr(floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_namespace.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    NamespaceProviderV1 provide=(NamespaceProviderV1)dlsym(image,"birnam_namespace_provider_v1"); assert(provide);
    const NamespaceApiV1 *api=provide(); assert(api && attach(api)==1);
    /* Both handles remain pinned for the process-owned floor. */
}
#endif
