/* Independent NativeLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_NATIVE_LOADER_V1_H
#define BIRNAM_NATIVE_LOADER_V1_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,Oop behavior,Oop load_module);
} NativeLoaderApiV1;
typedef const NativeLoaderApiV1 *(*NativeLoaderProviderV1)(void);
typedef int (*NativeLoaderDependencyV1)(const NativeLoaderApiV1 *);
_Static_assert(sizeof(NativeLoaderApiV1)==24 && offsetof(NativeLoaderApiV1,install)==16,"NativeLoader descriptor layout");
#endif
