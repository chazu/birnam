#ifndef FIXTURE_GC_SETTINGS_SUPPORT_H
#define FIXTURE_GC_SETTINGS_SUPPORT_H
#include "../../abi/fixtures/gc-settings-v1/gc_settings_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static void *fixture_gc_settings_floor;
static inline void fixture_prepare_gc_settings(void *floor,const char *floor_path) {
    GcSettingsDependencyV1 attach=(GcSettingsDependencyV1)dlsym(floor,"birnam_gc_settings_dependency_v1");assert(attach);
    const char *slash=strrchr(floor_path,'/');assert(slash);char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_gc_settings.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    GcSettingsFactoryProviderV1 provide=(GcSettingsFactoryProviderV1)dlsym(image,"birnam_gc_settings_factory_v1");assert(provide);
    const GcSettingsFactoryV1 *factory=provide();assert(factory && attach(factory)==1);
    fixture_gc_settings_floor=floor;
}
static inline GcSettingsV1 *fixture_gc_settings(void) {
    assert(fixture_gc_settings_floor);
    GcSettingsProviderV1 provide=(GcSettingsProviderV1)dlsym(fixture_gc_settings_floor,"birnam_gc_settings_provider_v1");assert(provide);
    GcSettingsV1 *api=provide();assert(api && api->version==1 && api->byte_size==sizeof(*api));return api;
}
static inline void fixture_set_auto(int enabled) { gc_settings_auto_v1(fixture_gc_settings(),enabled); }
static inline void fixture_set_precise(int enabled) { gc_settings_precise_v1(fixture_gc_settings(),enabled); }
/* Test-only adapters retain the independent transcript's setter instrumentation. */
typedef struct { void (*automatic)(int); void (*precise)(int); } FixtureSettingsSlots;
static inline void fixture_auto_slot(void *context,int enabled) { ((FixtureSettingsSlots *)context)->automatic(enabled); }
static inline void fixture_precise_slot(void *context,int enabled) { ((FixtureSettingsSlots *)context)->precise(enabled); }
#define FIXTURE_SETTINGS(slots) {1,sizeof(GcSettingsV1),&(slots),fixture_auto_slot,fixture_precise_slot}
#endif
