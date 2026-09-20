/* Independent SourceLoader installation descriptor; include IdApi first. */
#ifndef BIRNAM_SOURCE_LOADER_V3_H
#define BIRNAM_SOURCE_LOADER_V3_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,Oop behavior,Oop load);
} SourceLoaderApiV3;
typedef const SourceLoaderApiV3 *(*SourceLoaderProviderV3)(void);
typedef int (*SourceLoaderDependencyV3)(const SourceLoaderApiV3 *);
_Static_assert(sizeof(SourceLoaderApiV3)==24 && offsetof(SourceLoaderApiV3,install)==16,"SourceLoader descriptor layout");
#endif
