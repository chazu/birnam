/* Independent NativeLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_NATIVE_LOADER_V2_H
#define BIRNAM_NATIVE_LOADER_V2_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,Oop behavior,Oop load_module);
} NativeLoaderApiV2;
typedef const NativeLoaderApiV2 *(*NativeLoaderProviderV2)(void);
typedef int (*NativeLoaderDependencyV2)(const NativeLoaderApiV2 *);
_Static_assert(sizeof(NativeLoaderApiV2)==24 && offsetof(NativeLoaderApiV2,install)==16,"NativeLoader descriptor layout");
#endif
