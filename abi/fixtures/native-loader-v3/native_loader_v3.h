/* Independent NativeLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_NATIVE_LOADER_V3_H
#define BIRNAM_NATIVE_LOADER_V3_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,Oop behavior,Oop load_module);
} NativeLoaderApiV3;
typedef const NativeLoaderApiV3 *(*NativeLoaderProviderV3)(void);
typedef int (*NativeLoaderDependencyV3)(const NativeLoaderApiV3 *);
_Static_assert(sizeof(NativeLoaderApiV3)==24 && offsetof(NativeLoaderApiV3,install)==16,"NativeLoader descriptor layout");
#endif
