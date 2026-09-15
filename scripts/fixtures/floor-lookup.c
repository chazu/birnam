#define _DARWIN_C_SOURCE
#include "idapi_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifndef FIXTURE_ID
#define FIXTURE_ID 7
#endif
static IdApi copied;
static unsigned opens, lookups;
static int mode;
uint32_t fixture_target(uintptr_t ignored) { (void)ignored; return FIXTURE_ID; }
void *fixture_lookup(void *handle, const char *name) {
    ++lookups;
    void *answer = dlsym(handle, name);
    if (answer && strcmp(name, "fixture_target") == 0)
        assert(((uint32_t (*)(uintptr_t))answer)(0) == FIXTURE_ID);
    return answer;
}
static void *fixture_open(const char *path, int flags) {
    ++opens;
    Dl_info own;
    assert(dladdr((void *)&fixture_lookup, &own));
    assert(strcmp(path, own.dli_fname) == 0);
    assert(flags == (RTLD_NOW | RTLD_LOCAL));
    if (mode == 2) return NULL;
    return dlopen(path, flags);
}
IdApi *fixture_api(const IdApi *base, int selected_mode) {
    copied = *base;
    copied.dlopen_fn = fixture_open;
    copied.dlsym_fn = fixture_lookup;
    mode = selected_mode;
    return &copied;
}
unsigned fixture_count(unsigned which) { return which == 0 ? opens : lookups; }
__attribute__((destructor)) static void unloaded(void) {
    if (FIXTURE_ID == 7) {
        const char *path = getenv("BIRNAM_LOOKUP_UNLOADED");
        if (path) { FILE *f = fopen(path, "w"); assert(f); fputs("unloaded", f); fclose(f); }
    }
}
