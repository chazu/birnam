#ifndef BIRNAM_RANGE_V1_H
#define BIRNAM_RANGE_V1_H
/* Independent Range representation/construction ABI; include IdApi first. */
typedef struct { int64_t start,end; } RangeBoundsV1;
typedef Oop (*RangePrepareV1)(IdApi *,Oop,int);
typedef void (*RangeStageV1)(IdApi *);
typedef void (*RangeInstallV1)(IdApi *,Oop,Oop,Oop,Method,Method);
typedef RangeBoundsV1 (*RangeReadV1)(Oop);
typedef struct {
    uint32_t version; size_t byte_size;
    RangePrepareV1 prepare;
    RangeStageV1 finish_prototype, set_layout;
    RangeInstallV1 install;
    RangeStageV1 publish;
    RangeReadV1 bounds;
} RangeApiV1;
typedef const RangeApiV1 *(*RangeProviderV1)(void);
typedef int (*RangeDependencyV1)(const RangeApiV1 *);
_Static_assert(sizeof(RangeApiV1)==64 && offsetof(RangeApiV1,bounds)==56,"Range descriptor shape");
_Static_assert(sizeof(RangeBoundsV1)==16 && offsetof(RangeBoundsV1,end)==8,"Range bounds shape");
#endif
