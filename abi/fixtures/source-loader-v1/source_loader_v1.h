/* Independent SourceLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_SOURCE_LOADER_V1_H
#define BIRNAM_SOURCE_LOADER_V1_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,Oop behavior,Oop load);
} SourceLoaderApiV1;
typedef const SourceLoaderApiV1 *(*SourceLoaderProviderV1)(void);
typedef int (*SourceLoaderDependencyV1)(const SourceLoaderApiV1 *);
_Static_assert(sizeof(SourceLoaderApiV1)==24 && offsetof(SourceLoaderApiV1,install)==16,"SourceLoader descriptor layout");
#endif
