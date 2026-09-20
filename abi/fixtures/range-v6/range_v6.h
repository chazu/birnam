#ifndef BIRNAM_RANGE_V6_H
#define BIRNAM_RANGE_V6_H
/* Independent Range construction ABI; include IdApi first. */
typedef struct {
    uint32_t version;
    size_t byte_size;
    Oop (*prepare)(IdApi *,const NamespaceImportV1 *,const RootRegistryV1 *,Oop,int);
    void (*finish_prototype)(IdApi *);
    void (*set_layout)(IdApi *,const PayloadLayoutV1 *);
    void (*install)(IdApi *,Oop,Oop,Oop,Method);
    void (*publish)(IdApi *,const NamespaceExportV1 *);
} RangeApiV6;
typedef const RangeApiV6 *(*RangeProviderV6)(void);
typedef int (*RangeDependencyV6)(const RangeApiV6 *);
_Static_assert(sizeof(RangeApiV6)==56 && offsetof(RangeApiV6,publish)==48,"Range v5 descriptor shape");
#endif
