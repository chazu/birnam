#ifndef BIRNAM_RANGE_V3_H
#define BIRNAM_RANGE_V3_H
/* Independent Range construction ABI; include IdApi first. */
typedef struct {
    uint32_t version;
    size_t byte_size;
    Oop (*prepare)(IdApi *,const NamespaceImportV1 *,Oop,int);
    void (*finish_prototype)(IdApi *);
    void (*set_layout)(IdApi *);
    void (*install)(IdApi *,Oop,Oop,Oop,Method);
    void (*publish)(IdApi *);
} RangeApiV3;
typedef const RangeApiV3 *(*RangeProviderV3)(void);
typedef int (*RangeDependencyV3)(const RangeApiV3 *);
_Static_assert(sizeof(RangeApiV3)==56 && offsetof(RangeApiV3,publish)==48,"Range v3 descriptor shape");
#endif
