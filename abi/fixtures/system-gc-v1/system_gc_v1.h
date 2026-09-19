/* Independent System GC control installation descriptor; include IdApi first. */
#ifndef BIRNAM_SYSTEM_GC_V1_H
#define BIRNAM_SYSTEM_GC_V1_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution, Oop behavior, Oop gc, Oop live_objects, Oop auto_gc, Oop precise_gc);
} SystemGcApiV1;
typedef const SystemGcApiV1 *(*SystemGcProviderV1)(void);
typedef int (*SystemGcDependencyV1)(const SystemGcApiV1 *);
_Static_assert(sizeof(SystemGcApiV1)==24 && offsetof(SystemGcApiV1,install)==16,"String collection boundary shape");
#endif
