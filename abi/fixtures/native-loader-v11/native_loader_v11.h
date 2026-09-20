/* Independent NativeLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_NATIVE_LOADER_V11_H
#define BIRNAM_NATIVE_LOADER_V11_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *roots,const GcSettingsV1 *settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,Oop behavior,Oop load_module);
} NativeLoaderApiV11;
typedef const NativeLoaderApiV11 *(*NativeLoaderProviderV11)(void);
typedef int (*NativeLoaderDependencyV11)(const NativeLoaderApiV11 *);
_Static_assert(sizeof(NativeLoaderApiV11)==24 && offsetof(NativeLoaderApiV11,install)==16,"NativeLoader descriptor layout");
#endif
