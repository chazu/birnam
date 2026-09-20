#include "idapi_v13.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>
#ifdef NATIVE_LOADER_NO_INIT
int not_a_module(void) { return 1; }
#else
#ifndef EXPECT_NATIVE_LOADER_LIBRARY
#define EXPECT_NATIVE_LOADER_LIBRARY "libbirnam_native_loader.dylib"
#endif
static IdApi *first_api;
static unsigned calls;
static Oop count_loads(Oop self,const Oop *args,size_t count) {
    (void)self;assert(!args && !count);char value[64];snprintf(value,sizeof(value),"native-load-%u",calls);
    return first_api->intern(value);
}
/* The direct System loader must not perform host dependency admission. */
int birnam_native_loader_dependency_v6(const void *provider) {
    (void)provider;assert(!"direct loader attached dependencies");return 0;
}
void *module_init(IdApi *api,void *state,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables) { (void)vtables;
    (void)exports;
    (void)presence;
    assert(api && !state);if(first_api) assert(api==first_api);else first_api=api;
    Oop type=namespace_import_v1(imports,"SystemVt"),selector=api->intern("loadModule:");
    Method method=(Method)api->send(type,api->intern("lookup"),&selector,1);Dl_info owner;
    assert(method && dladdr((const void *)method,&owner));const char *base=strrchr(owner.dli_fname,'/');
    assert(base && !strcmp(base+1,EXPECT_NATIVE_LOADER_LIBRARY));
    api->gc_collect();calls++;api->method(type,api->intern("nativeLoaderCount"),count_loads);
    return (void *)(uintptr_t)0x1234;
}
int birnam_symbol_borrow_v1(void) { return 1; }
#endif
