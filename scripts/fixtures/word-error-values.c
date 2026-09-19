/* Seed otherwise unconstructible nonzero Words through the public native API.
   Save unexported clones so the image encodes values, not external export names.
   The fixture library can then be removed before image boot. */
#include "idapi_v8.h"
#include "word_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdlib.h>
void *module_init(IdApi *api,void *state) {
    assert(api && !state);
    const char *path=getenv("BIRNAM_WORD_ERROR_DYLIB");assert(path);
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    WordProviderV1 provide=(WordProviderV1)api->dlsym_fn(image,"birnam_word_provider_v1");assert(provide);
    WordApiV1 *word=provide();assert(word && word->version==1 && word->byte_size==sizeof(*word));
    api->export_obj("OverflowWord",word->make(word->context,UINT64_MAX));
    api->export_obj("BoundaryWord",word->make(word->context,INT64_MAX));
    return NULL;
}
