#ifndef FIXTURE_FINALIZERS_SUPPORT_H
#define FIXTURE_FINALIZERS_SUPPORT_H
#include "../../abi/fixtures/finalizers-v1/finalizers_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static void *fixture_finalizers_floor;
static inline void fixture_prepare_finalizers(void *floor,const char *floor_path) {
    FinalizersDependencyV1 attach=(FinalizersDependencyV1)dlsym(floor,"birnam_finalizers_dependency_v1"); assert(attach);
    const char *slash=strrchr(floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_finalizers.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    FinalizersFactoryProviderV1 provide=(FinalizersFactoryProviderV1)dlsym(image,"birnam_finalizers_factory_v1"); assert(provide);
    const FinalizersFactoryV1 *factory=provide(); assert(factory && attach(factory)==1);
    fixture_finalizers_floor=floor;
}
static inline FinalizersV1 *fixture_finalizers(void) {
    assert(fixture_finalizers_floor);
    FinalizersProviderV1 provide=(FinalizersProviderV1)dlsym(fixture_finalizers_floor,"birnam_finalizers_provider_v1"); assert(provide);
    FinalizersV1 *api=provide(); assert(api && api->version==1 && api->byte_size==sizeof(*api)); return api;
}
static inline void fixture_set_finalizer(void *object,FinalizerFnV1 hook) {
    finalizers_set_v1(fixture_finalizers(),object,hook);
}
/* Test-only slot loans preserve older independent callback transcripts. */
typedef void (*FixtureFinalizerSlot)(void *,FinalizerFnV1);
static inline void fixture_finalizer_slot(void *context,void *object,FinalizerFnV1 callback) {
    FixtureFinalizerSlot selected=*(const FixtureFinalizerSlot *)context;selected(object,callback);
}
#define FIXTURE_FINALIZERS(slot) {1,sizeof(FinalizersV1),&(slot),fixture_finalizer_slot}
#endif
