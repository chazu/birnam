/* Independent late String collection installation descriptor; include IdApi first. */
#ifndef BIRNAM_STRING_COLLECTION_V1_H
#define BIRNAM_STRING_COLLECTION_V1_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(IdApi *registration,const IdApi *execution);
} StringCollectionApiV1;
typedef const StringCollectionApiV1 *(*StringCollectionProviderV1)(void);
typedef int (*StringCollectionDependencyV1)(const StringCollectionApiV1 *);
_Static_assert(sizeof(StringCollectionApiV1)==24 && offsetof(StringCollectionApiV1,install)==16,"String collection boundary shape");
#endif
