/* Independent SourceLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_SOURCE_LOADER_V2_H
#define BIRNAM_SOURCE_LOADER_V2_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,Oop behavior,Oop load);
} SourceLoaderApiV2;
typedef const SourceLoaderApiV2 *(*SourceLoaderProviderV2)(void);
typedef int (*SourceLoaderDependencyV2)(const SourceLoaderApiV2 *);
_Static_assert(sizeof(SourceLoaderApiV2)==24 && offsetof(SourceLoaderApiV2,install)==16,"SourceLoader descriptor layout");
#endif
