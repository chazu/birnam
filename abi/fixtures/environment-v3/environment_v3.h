/* Independent Environment installation descriptor; include IdApi first. */
#ifndef BIRNAM_ENVIRONMENT_V3_H
#define BIRNAM_ENVIRONMENT_V3_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,Oop behavior,Oop get_env);
} EnvironmentApiV3;
typedef const EnvironmentApiV3 *(*EnvironmentProviderV3)(void);
typedef int (*EnvironmentDependencyV3)(const EnvironmentApiV3 *);
_Static_assert(sizeof(EnvironmentApiV3)==24 && offsetof(EnvironmentApiV3,install)==16,"Environment descriptor layout");
#endif
