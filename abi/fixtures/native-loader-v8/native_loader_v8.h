/* Independent NativeLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_NATIVE_LOADER_V8_H
#define BIRNAM_NATIVE_LOADER_V8_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *roots,Oop behavior,Oop load_module);
} NativeLoaderApiV8;
typedef const NativeLoaderApiV8 *(*NativeLoaderProviderV8)(void);
typedef int (*NativeLoaderDependencyV8)(const NativeLoaderApiV8 *);
_Static_assert(sizeof(NativeLoaderApiV8)==24 && offsetof(NativeLoaderApiV8,install)==16,"NativeLoader descriptor layout");
#endif
