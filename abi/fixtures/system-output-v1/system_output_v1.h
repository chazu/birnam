/* Independent System output installation descriptor; include IdApi first. */
#ifndef BIRNAM_SYSTEM_OUTPUT_V1_H
#define BIRNAM_SYSTEM_OUTPUT_V1_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,Oop behavior,Oop print,Oop println);
} SystemOutputApiV1;
typedef const SystemOutputApiV1 *(*SystemOutputProviderV1)(void);
typedef int (*SystemOutputDependencyV1)(const SystemOutputApiV1 *);
_Static_assert(sizeof(SystemOutputApiV1)==24 && offsetof(SystemOutputApiV1,install)==16,"System output descriptor layout");
#endif
