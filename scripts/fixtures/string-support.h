#ifndef FIXTURE_STRING_SUPPORT_H
#define FIXTURE_STRING_SUPPORT_H
#include "../../abi/fixtures/string-v1/string_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static StringApiV1 *fixture_string;
static void *fixture_string_image;
static inline Oop fixture_string_make(const uint8_t *bytes, size_t length) {
    assert(fixture_string); return fixture_string->make(fixture_string->context, bytes, length);
}
static inline const uint8_t *fixture_string_bytes(Oop object, size_t *length) {
    assert(fixture_string); return fixture_string->bytes(fixture_string->context, object, length);
}
static inline void fixture_attach_string(IdApi *api, void *image) {
    StringDependencyV1 attach=(StringDependencyV1)api->dlsym_fn(image,"birnam_string_dependency_v1");
    if(attach) { assert(fixture_string); assert(attach(fixture_string)==1); }
}
static inline void fixture_prepare_string(IdApi *api, const char *floor_path) {
    if(fixture_string) return;
    const char *slash=strrchr(floor_path,'/'); assert(slash);
    char path[PATH_MAX];
    int length=snprintf(path,sizeof(path),"%.*s/libbirnam_string.dylib",(int)(slash-floor_path),floor_path);
    assert(length>0 && (size_t)length<sizeof(path));
    fixture_string_image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(fixture_string_image);
    void *floor=api->dlopen_fn(floor_path,RTLD_NOW|RTLD_LOCAL); assert(floor);
    NativeAllocationProviderV1 allocation=(NativeAllocationProviderV1)api->dlsym_fn(floor,"birnam_native_allocation_provider_v1"); assert(allocation);
    StringPrepareV1 prepare=(StringPrepareV1)api->dlsym_fn(fixture_string_image,"birnam_string_prepare_v1"); assert(prepare);
    fixture_string=prepare(api,allocation()); assert(fixture_string);
    assert(fixture_string->version==1 && fixture_string->byte_size==sizeof(StringApiV1));
    fixture_attach_string(api,floor);
}
#endif
