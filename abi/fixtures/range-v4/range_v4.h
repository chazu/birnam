#ifndef BIRNAM_RANGE_V4_H
#define BIRNAM_RANGE_V4_H
/* Independent Range construction ABI; include IdApi first. */
typedef struct {
    uint32_t version;
    size_t byte_size;
    Oop (*prepare)(IdApi *,const NamespaceImportV1 *,Oop,int);
    void (*finish_prototype)(IdApi *);
    void (*set_layout)(IdApi *);
    void (*install)(IdApi *,Oop,Oop,Oop,Method);
    void (*publish)(IdApi *,const NamespaceExportV1 *);
} RangeApiV4;
typedef const RangeApiV4 *(*RangeProviderV4)(void);
typedef int (*RangeDependencyV4)(const RangeApiV4 *);
_Static_assert(sizeof(RangeApiV4)==56 && offsetof(RangeApiV4,publish)==48,"Range v4 descriptor shape");
#endif
