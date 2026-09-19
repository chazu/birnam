/* Independent Environment installation descriptor; include IdApi first. */
#ifndef BIRNAM_ENVIRONMENT_V2_H
#define BIRNAM_ENVIRONMENT_V2_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,Oop behavior,Oop get_env);
} EnvironmentApiV2;
typedef const EnvironmentApiV2 *(*EnvironmentProviderV2)(void);
typedef int (*EnvironmentDependencyV2)(const EnvironmentApiV2 *);
_Static_assert(sizeof(EnvironmentApiV2)==24 && offsetof(EnvironmentApiV2,install)==16,"Environment descriptor layout");
#endif
