/* Independent NativeLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_NATIVE_LOADER_V9_H
#define BIRNAM_NATIVE_LOADER_V9_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *roots,const GcSettingsV1 *settings,Oop behavior,Oop load_module);
} NativeLoaderApiV9;
typedef const NativeLoaderApiV9 *(*NativeLoaderProviderV9)(void);
typedef int (*NativeLoaderDependencyV9)(const NativeLoaderApiV9 *);
_Static_assert(sizeof(NativeLoaderApiV9)==24 && offsetof(NativeLoaderApiV9,install)==16,"NativeLoader descriptor layout");
#endif
