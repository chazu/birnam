/* Seed otherwise unconstructible nonzero Words through the public native API.
   Save unexported clones so the image encodes values, not external export names.
   The fixture library can then be removed before image boot. */
#include "idapi_v18.h"
#include "word_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdlib.h>
void *module_init(IdApi *api,void *state,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks) { (void)hooks; (void)layouts; (void)settings; (void)root_registry; (void)observation; (void)vtables;
    (void)exports;
    (void)presence;(void)imports;
    assert(api && !state);
    const char *path=getenv("BIRNAM_WORD_ERROR_DYLIB");assert(path);
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL);assert(image);
    WordProviderV1 provide=(WordProviderV1)api->dlsym_fn(image,"birnam_word_provider_v1");assert(provide);
    WordApiV1 *word=provide();assert(word && word->version==1 && word->byte_size==sizeof(*word));
    namespace_export_v1(exports,"OverflowWord",word->make(word->context,UINT64_MAX));
    namespace_export_v1(exports,"BoundaryWord",word->make(word->context,INT64_MAX));
    return NULL;
}
