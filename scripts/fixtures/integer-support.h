#ifndef FIXTURE_INTEGER_SUPPORT_H
#define FIXTURE_INTEGER_SUPPORT_H
#include "../../abi/fixtures/integer-v1/integer_v1.h"
#include "float-support.h"
#include "word-support.h"
#include "string-support.h"
#include "../../abi/fixtures/range-methods-v1/range_methods_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static IntegerApiV1 *fixture_integer;
static char fixture_floor_path[PATH_MAX];
static const RangeMethodsApiV1 *fixture_range_methods;
static inline Oop fixture_integer_make(int64_t value) {
    assert(fixture_integer);
    return fixture_integer->make(fixture_integer->context, value);
}
static inline int64_t fixture_integer_value(Oop value) {
    assert(fixture_integer);
    return fixture_integer->read(fixture_integer->context, value);
}
static inline int fixture_is_integer(Oop value) {
    assert(fixture_integer);
    return fixture_integer->is_exact(fixture_integer->context, value);
}
static inline void fixture_attach_integer(IdApi *api, void *image) {
    fixture_attach_string(api, image);
    fixture_attach_float(api, image);
    fixture_attach_word(api, image);
    IntegerDependencyV1 attach = (IntegerDependencyV1)api->dlsym_fn(image, "birnam_integer_dependency_v1");
    if (attach) { assert(fixture_integer); assert(attach(fixture_integer) == 1); }
    RangeMethodsDependencyV1 range_attach=(RangeMethodsDependencyV1)api->dlsym_fn(image,"birnam_range_methods_dependency_v1");
    if (range_attach) {
        if (!fixture_range_methods) {
            const char *slash=strrchr(fixture_floor_path,'/'); assert(slash);
            char path[PATH_MAX];
            int n=snprintf(path,sizeof(path),"%.*s/libbirnam_range.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
            assert(n>0 && (size_t)n<sizeof(path));
            void *range=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(range);
            fixture_attach_integer(api,range);
            ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(range,"module_init"); assert(init); init(api,NULL);
            RangeMethodsProviderV1 provide=(RangeMethodsProviderV1)api->dlsym_fn(range,"birnam_range_methods_provider_v1"); assert(provide);
            fixture_range_methods=provide(); assert(fixture_range_methods);
        }
        assert(range_attach(fixture_range_methods)==1);
    }
}
static inline void fixture_mount_integer(IdApi *api, const char *floor_path) {
    assert(strlen(floor_path)<sizeof(fixture_floor_path));
    strcpy(fixture_floor_path,floor_path);
    fixture_prepare_string(api, floor_path);
    assert(!fixture_integer);
    fixture_prepare_word(api, floor_path);
    fixture_prepare_float(api, floor_path);
    const char *slash = strrchr(floor_path, '/'); assert(slash);
    char path[PATH_MAX];
    int length = snprintf(path, sizeof(path), "%.*s/libbirnam_integer.dylib", (int)(slash-floor_path), floor_path);
    assert(length > 0 && (size_t)length < sizeof(path));
    void *image = api->dlopen_fn(path, RTLD_NOW | RTLD_LOCAL);
    if (!image) fprintf(stderr, "%s\n", dlerror());
    assert(image);
    ModuleInitFn init = (ModuleInitFn)api->dlsym_fn(image, "module_init"); assert(init);
    fixture_attach_float(api, image);
    fixture_attach_string(api, image);
    init(api, NULL);
    IntegerProviderV1 provide = (IntegerProviderV1)api->dlsym_fn(image, "birnam_integer_provider_v1"); assert(provide);
    fixture_integer = provide(); assert(fixture_integer);
    assert(fixture_integer->version == 1 && fixture_integer->byte_size == sizeof(IntegerApiV1));
    void *floor = api->dlopen_fn(floor_path, RTLD_NOW | RTLD_LOCAL); assert(floor);
    fixture_attach_integer(api, floor);
    fixture_attach_integer(api, fixture_float_image);
    fixture_attach_integer(api, fixture_word_image);
}
#endif
