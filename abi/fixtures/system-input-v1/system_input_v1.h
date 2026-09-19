/* Independent System input installation descriptor; include IdApi first. */
#ifndef BIRNAM_SYSTEM_INPUT_V1_H
#define BIRNAM_SYSTEM_INPUT_V1_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,Oop behavior,Oop read_line);
} SystemInputApiV1;
typedef const SystemInputApiV1 *(*SystemInputProviderV1)(void);
typedef int (*SystemInputDependencyV1)(const SystemInputApiV1 *);
_Static_assert(sizeof(SystemInputApiV1)==24 && offsetof(SystemInputApiV1,install)==16,"System input descriptor layout");
#endif
