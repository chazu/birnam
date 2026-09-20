/* Independent NativeLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_NATIVE_LOADER_V12_H
#define BIRNAM_NATIVE_LOADER_V12_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *roots,const GcSettingsV1 *settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,const FinalizersV1 *finalizer_cap,Oop behavior,Oop load_module);
} NativeLoaderApiV12;
typedef const NativeLoaderApiV12 *(*NativeLoaderProviderV12)(void);
typedef int (*NativeLoaderDependencyV12)(const NativeLoaderApiV12 *);
_Static_assert(sizeof(NativeLoaderApiV12)==24 && offsetof(NativeLoaderApiV12,install)==16,"NativeLoader descriptor layout");
#endif
