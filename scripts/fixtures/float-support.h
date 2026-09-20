#ifndef FIXTURE_FLOAT_SUPPORT_H
#define FIXTURE_FLOAT_SUPPORT_H
#include "../../abi/fixtures/float-v1/float_v1.h"
#include "string-support.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static FloatApiV1 *fixture_float;
static void *fixture_float_image;
static inline Oop fixture_float_make(double value) {
    assert(fixture_float);
    return fixture_float->make(fixture_float->context, value);
}
static inline double fixture_float_value(Oop value) {
    assert(fixture_float);
    return fixture_float->read(fixture_float->context, value);
}
static inline int fixture_is_float(Oop value) {
    assert(fixture_float);
    return fixture_float->is_exact(fixture_float->context, value);
}
static inline void fixture_attach_float(IdApi *api, void *image) {
    fixture_attach_string(api, image);
    FloatDependencyV1 attach = (FloatDependencyV1)api->dlsym_fn(image, "birnam_float_dependency_v1");
    if (attach) { assert(fixture_float); assert(attach(fixture_float) == 1); }
}
static inline void fixture_prepare_float(IdApi *api, const char *floor_path, const NamespacePresenceV1 *presence, const NamespaceImportV1 *imports) {
    fixture_prepare_string(api, floor_path);
    if (fixture_float) return;
    const char *slash = strrchr(floor_path, '/'); assert(slash);
    char path[PATH_MAX];
    int length = snprintf(path, sizeof(path), "%.*s/libbirnam_float.dylib", (int)(slash-floor_path), floor_path);
    assert(length > 0 && (size_t)length < sizeof(path));
    fixture_float_image = api->dlopen_fn(path, RTLD_NOW | RTLD_LOCAL);
    if (!fixture_float_image) fprintf(stderr, "%s\n", dlerror());
    assert(fixture_float_image);
    FloatPrepareV3 prepare = (FloatPrepareV3)api->dlsym_fn(fixture_float_image, "birnam_float_prepare_v3"); assert(prepare);
    fixture_float = prepare(api,presence,imports); assert(fixture_float);
    assert(fixture_float->version == 1 && fixture_float->byte_size == sizeof(FloatApiV1));
    void *floor = api->dlopen_fn(floor_path, RTLD_NOW | RTLD_LOCAL); assert(floor);
    fixture_attach_float(api, floor);
}
#endif
