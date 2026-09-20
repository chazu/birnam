#ifndef BIRNAM_RANGE_V5_H
#define BIRNAM_RANGE_V5_H
/* Independent Range construction ABI; include IdApi first. */
typedef struct {
    uint32_t version;
    size_t byte_size;
    Oop (*prepare)(IdApi *,const NamespaceImportV1 *,const RootRegistryV1 *,Oop,int);
    void (*finish_prototype)(IdApi *);
    void (*set_layout)(IdApi *);
    void (*install)(IdApi *,Oop,Oop,Oop,Method);
    void (*publish)(IdApi *,const NamespaceExportV1 *);
} RangeApiV5;
typedef const RangeApiV5 *(*RangeProviderV5)(void);
typedef int (*RangeDependencyV5)(const RangeApiV5 *);
_Static_assert(sizeof(RangeApiV5)==56 && offsetof(RangeApiV5,publish)==48,"Range v5 descriptor shape");
#endif
