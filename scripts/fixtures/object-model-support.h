/* The current native stack loads the object-model rung immediately after Id. */
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static void fixture_mount_object_model(IdApi *api, const char *floor_path) {
    const char *slash = strrchr(floor_path, '/');
    assert(slash);
    char path[PATH_MAX];
    int length = snprintf(path, sizeof(path), "%.*s/libbirnam_object_model.dylib",
                          (int)(slash - floor_path), floor_path);
    assert(length > 0 && (size_t)length < sizeof(path));
    void *module = api->dlopen_fn(path, RTLD_NOW | RTLD_LOCAL);
    if (!module) fprintf(stderr, "%s\n", dlerror());
    assert(module);
    ModuleInitFn init = (ModuleInitFn)api->dlsym_fn(module, "module_init");
    assert(init);
    init(api, NULL);
    length = snprintf(path, sizeof(path), "%.*s/libbirnam_integer.dylib",
                      (int)(slash - floor_path), floor_path);
    assert(length > 0 && (size_t)length < sizeof(path));
    module = api->dlopen_fn(path, RTLD_NOW | RTLD_LOCAL);
    if (!module) fprintf(stderr, "%s\n", dlerror());
    assert(module);
    init = (ModuleInitFn)api->dlsym_fn(module, "module_init");
    assert(init);
    init(api, NULL);
}
