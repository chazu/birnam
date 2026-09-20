/* Independent NativeLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_NATIVE_LOADER_V7_H
#define BIRNAM_NATIVE_LOADER_V7_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,Oop behavior,Oop load_module);
} NativeLoaderApiV7;
typedef const NativeLoaderApiV7 *(*NativeLoaderProviderV7)(void);
typedef int (*NativeLoaderDependencyV7)(const NativeLoaderApiV7 *);
_Static_assert(sizeof(NativeLoaderApiV7)==24 && offsetof(NativeLoaderApiV7,install)==16,"NativeLoader descriptor layout");
#endif
