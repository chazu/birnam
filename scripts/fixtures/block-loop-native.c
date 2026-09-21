/* Image smoke guard: called only after explicitly reloading Block Loop. */
#include "idapi_v20.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>
#ifndef EXPECT_BLOCK_LOOP_LIBRARY
#define EXPECT_BLOCK_LOOP_LIBRARY "libbirnam_block_loop.dylib"
#endif
void *module_init(IdApi *api,void *state,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,const FinalizersV1 *finalizer_cap) { (void)finalizer_cap; (void)hooks; (void)layouts; (void)settings; (void)root_registry; (void)observation; (void)vtables;
    (void)exports;
    (void)presence;
    (void)state;assert(api);Oop type=namespace_import_v1(imports,"BlockVt");assert(type);
    const char *names[]={"whileTrue:","whileFalse:"};
    for(size_t i=0;i<2;i++) {
        Oop selector=api->intern(names[i]);Method method=(Method)api->send(type,api->intern("lookup"),&selector,1);
        Dl_info owner;assert(method && dladdr((const void *)method,&owner));
        const char *base=strrchr(owner.dli_fname,'/');assert(base && !strcmp(base+1,EXPECT_BLOCK_LOOP_LIBRARY));
    }
    return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
