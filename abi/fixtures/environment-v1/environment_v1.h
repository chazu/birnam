/* Independent Environment installation descriptor; include IdApi first. */
#ifndef BIRNAM_ENVIRONMENT_V1_H
#define BIRNAM_ENVIRONMENT_V1_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,Oop behavior,Oop get_env);
} EnvironmentApiV1;
typedef const EnvironmentApiV1 *(*EnvironmentProviderV1)(void);
typedef int (*EnvironmentDependencyV1)(const EnvironmentApiV1 *);
_Static_assert(sizeof(EnvironmentApiV1)==24 && offsetof(EnvironmentApiV1,install)==16,"Environment descriptor layout");
#endif
