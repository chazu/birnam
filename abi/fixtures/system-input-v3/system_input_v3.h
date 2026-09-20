/* Independent System input installation descriptor; include IdApi first. */
#ifndef BIRNAM_SYSTEM_INPUT_V3_H
#define BIRNAM_SYSTEM_INPUT_V3_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,Oop behavior,Oop read_line);
} SystemInputApiV3;
typedef const SystemInputApiV3 *(*SystemInputProviderV3)(void);
typedef int (*SystemInputDependencyV3)(const SystemInputApiV3 *);
_Static_assert(sizeof(SystemInputApiV3)==24 && offsetof(SystemInputApiV3,install)==16,"System input descriptor layout");
#endif
