#ifndef BIRNAM_RANGE_V2_H
#define BIRNAM_RANGE_V2_H
/* Independent Range construction ABI; include IdApi first. */
typedef struct {
    uint32_t version;
    size_t byte_size;
    Oop (*prepare)(IdApi *,Oop,int);
    void (*finish_prototype)(IdApi *);
    void (*set_layout)(IdApi *);
    void (*install)(IdApi *,Oop,Oop,Oop,Method);
    void (*publish)(IdApi *);
} RangeApiV2;
typedef const RangeApiV2 *(*RangeProviderV2)(void);
typedef int (*RangeDependencyV2)(const RangeApiV2 *);
_Static_assert(sizeof(RangeApiV2)==56 && offsetof(RangeApiV2,publish)==48,"Range v2 descriptor shape");
#endif
