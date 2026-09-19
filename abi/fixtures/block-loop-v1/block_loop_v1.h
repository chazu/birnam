/* Independent Block Loop installation descriptor; include IdApi first. */
#ifndef BIRNAM_BLOCK_LOOP_V1_H
#define BIRNAM_BLOCK_LOOP_V1_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,Oop behavior,Oop while_true,Oop while_false);
} BlockLoopApiV1;
typedef const BlockLoopApiV1 *(*BlockLoopProviderV1)(void);
typedef int (*BlockLoopDependencyV1)(const BlockLoopApiV1 *);
_Static_assert(sizeof(BlockLoopApiV1)==24 && offsetof(BlockLoopApiV1,install)==16,"Block Loop descriptor layout");
#endif
