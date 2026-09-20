#ifndef FIXTURE_TRACE_HOOKS_SUPPORT_H
#define FIXTURE_TRACE_HOOKS_SUPPORT_H
#include "../../abi/fixtures/trace-hooks-v1/trace_hooks_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static void *fixture_trace_hooks_floor;
static inline void fixture_prepare_trace_hooks(void *floor,const char *floor_path) {
    TraceHooksDependencyV1 attach=(TraceHooksDependencyV1)dlsym(floor,"birnam_trace_hooks_dependency_v1"); assert(attach);
    const char *slash=strrchr(floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_trace_hooks.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    TraceHooksFactoryProviderV1 provide=(TraceHooksFactoryProviderV1)dlsym(image,"birnam_trace_hooks_factory_v1"); assert(provide);
    const TraceHooksFactoryV1 *factory=provide(); assert(factory && attach(factory)==1);
    fixture_trace_hooks_floor=floor;
}
static inline TraceHooksV1 *fixture_trace_hooks(void) {
    assert(fixture_trace_hooks_floor);
    TraceHooksProviderV1 provide=(TraceHooksProviderV1)dlsym(fixture_trace_hooks_floor,"birnam_trace_hooks_provider_v1"); assert(provide);
    TraceHooksV1 *api=provide(); assert(api && api->version==1 && api->byte_size==sizeof(*api)); return api;
}
static inline void fixture_set_trace_hook(void *vt,TraceHookFnV1 hook) {
    trace_hooks_set_v1(fixture_trace_hooks(),vt,hook);
}
#endif
