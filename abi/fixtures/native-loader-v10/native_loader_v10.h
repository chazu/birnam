/* Independent NativeLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_NATIVE_LOADER_V10_H
#define BIRNAM_NATIVE_LOADER_V10_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *roots,const GcSettingsV1 *settings,const PayloadLayoutV1 *layouts,Oop behavior,Oop load_module);
} NativeLoaderApiV10;
typedef const NativeLoaderApiV10 *(*NativeLoaderProviderV10)(void);
typedef int (*NativeLoaderDependencyV10)(const NativeLoaderApiV10 *);
_Static_assert(sizeof(NativeLoaderApiV10)==24 && offsetof(NativeLoaderApiV10,install)==16,"NativeLoader descriptor layout");
#endif
