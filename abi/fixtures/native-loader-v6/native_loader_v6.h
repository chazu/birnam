/* Independent NativeLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_NATIVE_LOADER_V6_H
#define BIRNAM_NATIVE_LOADER_V6_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,Oop behavior,Oop load_module);
} NativeLoaderApiV6;
typedef const NativeLoaderApiV6 *(*NativeLoaderProviderV6)(void);
typedef int (*NativeLoaderDependencyV6)(const NativeLoaderApiV6 *);
_Static_assert(sizeof(NativeLoaderApiV6)==24 && offsetof(NativeLoaderApiV6,install)==16,"NativeLoader descriptor layout");
#endif
