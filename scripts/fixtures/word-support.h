#ifndef FIXTURE_WORD_SUPPORT_H
#define FIXTURE_WORD_SUPPORT_H
#include "../../abi/fixtures/word-v1/word_v1.h"
#include "string-support.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static WordApiV1 *fixture_word;
static void *fixture_word_image;
static inline Oop fixture_word_make(uint64_t value) {
    assert(fixture_word);
    return fixture_word->make(fixture_word->context, value);
}
static inline uint64_t fixture_word_value(Oop value) {
    assert(fixture_word);
    return fixture_word->read(fixture_word->context, value);
}
static inline int fixture_is_word(Oop value) {
    assert(fixture_word);
    return fixture_word->is_exact(fixture_word->context, value);
}
static inline void fixture_attach_word(IdApi *api, void *image) {
    fixture_attach_string(api, image);
    WordDependencyV1 attach = (WordDependencyV1)api->dlsym_fn(image, "birnam_word_dependency_v1");
    if (attach) { assert(fixture_word); assert(attach(fixture_word) == 1); }
}
static inline void fixture_prepare_word(IdApi *api, const char *floor_path, const NamespacePresenceV1 *presence, const NamespaceImportV1 *imports,const NamespaceExportV1 *exports) {
    fixture_prepare_string(api, floor_path, exports);
    if (fixture_word) return;
    const char *slash = strrchr(floor_path, '/'); assert(slash);
    char path[PATH_MAX];
    int length = snprintf(path, sizeof(path), "%.*s/libbirnam_word.dylib", (int)(slash-floor_path), floor_path);
    assert(length > 0 && (size_t)length < sizeof(path));
    fixture_word_image = api->dlopen_fn(path, RTLD_NOW | RTLD_LOCAL);
    if (!fixture_word_image) fprintf(stderr, "%s\n", dlerror());
    assert(fixture_word_image);
    WordPrepareV4 prepare = (WordPrepareV4)api->dlsym_fn(fixture_word_image, "birnam_word_prepare_v4"); assert(prepare);
    fixture_word = prepare(api,presence,imports,exports); assert(fixture_word);
    assert(fixture_word->version == 1 && fixture_word->byte_size == sizeof(WordApiV1));
    void *floor = api->dlopen_fn(floor_path, RTLD_NOW | RTLD_LOCAL); assert(floor);
    fixture_attach_word(api, floor);
}
#endif
