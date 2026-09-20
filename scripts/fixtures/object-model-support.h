/* Compose the two independently owned families above bare Id. */
#include "integer-support.h"
static void fixture_mount_object_model(IdApi *api, const char *floor_path) {
    fixture_mount_integer(api, floor_path, fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
    const char *slash = strrchr(floor_path, '/'); assert(slash);
    char path[PATH_MAX];
    int length = snprintf(path, sizeof(path), "%.*s/libbirnam_object_model.dylib", (int)(slash-floor_path), floor_path);
    assert(length > 0 && (size_t)length < sizeof(path));
    void *module = api->dlopen_fn(path, RTLD_NOW | RTLD_LOCAL);
    if (!module) fprintf(stderr, "%s\n", dlerror());
    assert(module);
    ModuleInitFn init = (ModuleInitFn)api->dlsym_fn(module, "module_init"); assert(init);
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports());
}
