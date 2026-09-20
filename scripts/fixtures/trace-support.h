#ifndef FIXTURE_TRACE_SUPPORT_H
#define FIXTURE_TRACE_SUPPORT_H
#include "../../abi/fixtures/trace-live-v1/trace_live_v1.h"
#include "collect-support.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static inline void fixture_mount_trace(IdApi *api, const char *floor_path) {
    const char *slash=strrchr(floor_path,'/'); assert(slash);
    char path[PATH_MAX];
    int length=snprintf(path,sizeof(path),"%.*s/libbirnam_trace.dylib",(int)(slash-floor_path),floor_path);
    assert(length>0 && (size_t)length<sizeof(path));
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);
    if(!image) fprintf(stderr,"%s\n",dlerror());
    assert(image);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init); init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
    TraceLiveProviderV1 provide=(TraceLiveProviderV1)api->dlsym_fn(image,"birnam_trace_provider_v1"); assert(provide);
    void *floor=api->dlopen_fn(floor_path,RTLD_NOW|RTLD_LOCAL); assert(floor);
    TraceLiveDependencyV1 attach=(TraceLiveDependencyV1)api->dlsym_fn(floor,"birnam_trace_dependency_v1"); assert(attach);
    assert(attach(provide())==1);
    fixture_mount_collect(api,floor_path);
}
#endif
