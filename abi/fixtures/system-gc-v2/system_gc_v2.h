/* Independent System GC control installation descriptor; include IdApi first. */
#ifndef BIRNAM_SYSTEM_GC_V2_H
#define BIRNAM_SYSTEM_GC_V2_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const GcSettingsV1 *settings, Oop behavior, Oop gc, Oop live_objects, Oop auto_gc, Oop precise_gc);
} SystemGcApiV2;
typedef const SystemGcApiV2 *(*SystemGcProviderV2)(void);
typedef int (*SystemGcDependencyV2)(const SystemGcApiV2 *);
_Static_assert(sizeof(SystemGcApiV2)==24 && offsetof(SystemGcApiV2,install)==16,"System GC installation v2");
#endif
