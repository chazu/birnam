/* Independent NativeLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_NATIVE_LOADER_V5_H
#define BIRNAM_NATIVE_LOADER_V5_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV1 *vtables,Oop behavior,Oop load_module);
} NativeLoaderApiV5;
typedef const NativeLoaderApiV5 *(*NativeLoaderProviderV5)(void);
typedef int (*NativeLoaderDependencyV5)(const NativeLoaderApiV5 *);
_Static_assert(sizeof(NativeLoaderApiV5)==24 && offsetof(NativeLoaderApiV5,install)==16,"NativeLoader descriptor layout");
#endif
