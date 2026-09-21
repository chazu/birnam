#ifndef BIRNAM_REFLECTION_FIXTURE_H
#define BIRNAM_REFLECTION_FIXTURE_H
#ifndef _DARWIN_C_SOURCE
#define _DARWIN_C_SOURCE
#endif
#include "storage_v1.h"
#include "native-fixture.h"
#include "../../abi/fixtures/native-stack-v1/native_stack_v1.h"
#include <dlfcn.h>
#include <stdio.h>
/* Explicit completed services supplied by the production host. The selected
 * reflection capability is args[4]; each oracle decodes its own ABI. */
static IdApi *api;
static const RootRegistryV1 *registry;
static const GcSettingsV1 *settings;
static const NativeStackV1 *native_stack;
static StorageObserveV1 observe;
static char *mode;
static void reflection_start(const BirnamOperand *args,size_t count) {
    assert(count>=7);api=fixture_word(args[0]);registry=fixture_word(args[1]);
    settings=fixture_word(args[2]);native_stack=fixture_word(args[3]);
    char *path=fixture_text(args[5]);void *runtime=dlopen(path,RTLD_NOW|RTLD_NOLOAD);free(path);assert(runtime);
    observe=(StorageObserveV1)dlsym(runtime,"birnam_id_observe_storage_v1");assert(observe);
    mode=fixture_text(args[6]);
}
static inline void fixture_set_auto(int value) { gc_settings_auto_v1(settings,value); }
static inline void fixture_set_precise(int value) { gc_settings_precise_v1(settings,value); }
static inline void fixture_set_stack_bottom(uintptr_t value) { native_stack_set_v1(native_stack,value); }
static inline void fixture_register_root(RootProvider provider) { root_registry_register_v1(registry,provider); }
#endif
