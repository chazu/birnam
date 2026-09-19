/* Independent System input installation descriptor; include IdApi first. */
#ifndef BIRNAM_SYSTEM_INPUT_V2_H
#define BIRNAM_SYSTEM_INPUT_V2_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,Oop behavior,Oop read_line);
} SystemInputApiV2;
typedef const SystemInputApiV2 *(*SystemInputProviderV2)(void);
typedef int (*SystemInputDependencyV2)(const SystemInputApiV2 *);
_Static_assert(sizeof(SystemInputApiV2)==24 && offsetof(SystemInputApiV2,install)==16,"System input descriptor layout");
#endif
