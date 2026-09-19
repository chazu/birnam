#ifndef BIRNAM_COLLECTION_VALUE_V1_H
#define BIRNAM_COLLECTION_VALUE_V1_H
/* Independent C descriptor; include IdApi first. */
typedef struct {
    uint32_t version;
    size_t byte_size;
    int (*same_value)(const IdApi *,Oop,Oop);
    uint32_t (*key_hash)(const IdApi *,Oop);
} CollectionValueApiV1;
typedef const CollectionValueApiV1 *(*CollectionValueProviderV1)(void);
typedef int (*CollectionValueDependencyV1)(const CollectionValueApiV1 *);
_Static_assert(sizeof(CollectionValueApiV1)==32 && offsetof(CollectionValueApiV1,key_hash)==24,"collection value descriptor");
#endif
