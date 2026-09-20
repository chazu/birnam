#include "idapi_v15.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>
#ifndef EXPECT_SOURCE_LOADER_LIBRARY
#define EXPECT_SOURCE_LOADER_LIBRARY "libbirnam_source_loader.dylib"
#endif
void *module_init(IdApi *api,void *state,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry) { (void)root_registry; (void)observation; (void)vtables;
    (void)exports;
    (void)presence;
    assert(api && !state);Oop type=namespace_import_v1(imports,"SystemVt"),selector=api->intern("load:");
    Method method=(Method)api->send(type,api->intern("lookup"),&selector,1);Dl_info owner;
    assert(method && dladdr((const void *)method,&owner));const char *base=strrchr(owner.dli_fname,'/');
    assert(base && !strcmp(base+1,EXPECT_SOURCE_LOADER_LIBRARY));return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
