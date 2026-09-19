/* Independent Block Loop installation descriptor; include IdApi first. */
#ifndef BIRNAM_BLOCK_LOOP_V2_H
#define BIRNAM_BLOCK_LOOP_V2_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,Oop behavior,Oop while_true,Oop while_false);
} BlockLoopApiV2;
typedef const BlockLoopApiV2 *(*BlockLoopProviderV2)(void);
typedef int (*BlockLoopDependencyV2)(const BlockLoopApiV2 *);
_Static_assert(sizeof(BlockLoopApiV2)==24 && offsetof(BlockLoopApiV2,install)==16,"Block Loop descriptor layout");
#endif
