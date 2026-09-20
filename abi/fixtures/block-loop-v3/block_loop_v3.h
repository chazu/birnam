/* Independent Block Loop installation descriptor; include IdApi first. */
#ifndef BIRNAM_BLOCK_LOOP_V3_H
#define BIRNAM_BLOCK_LOOP_V3_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,Oop behavior,Oop while_true,Oop while_false);
} BlockLoopApiV3;
typedef const BlockLoopApiV3 *(*BlockLoopProviderV3)(void);
typedef int (*BlockLoopDependencyV3)(const BlockLoopApiV3 *);
_Static_assert(sizeof(BlockLoopApiV3)==24 && offsetof(BlockLoopApiV3,install)==16,"Block Loop descriptor layout");
#endif
