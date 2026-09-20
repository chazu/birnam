/* Independent NativeLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_NATIVE_LOADER_V4_H
#define BIRNAM_NATIVE_LOADER_V4_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,Oop behavior,Oop load_module);
} NativeLoaderApiV4;
typedef const NativeLoaderApiV4 *(*NativeLoaderProviderV4)(void);
typedef int (*NativeLoaderDependencyV4)(const NativeLoaderApiV4 *);
_Static_assert(sizeof(NativeLoaderApiV4)==24 && offsetof(NativeLoaderApiV4,install)==16,"NativeLoader descriptor layout");
#endif
