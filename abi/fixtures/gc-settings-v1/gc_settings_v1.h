#ifndef BIRNAM_GC_SETTINGS_V1_H
#define BIRNAM_GC_SETTINGS_V1_H
#include <stddef.h>
#include <stdint.h>
typedef struct { uint32_t version; size_t byte_size; void *context; void (*admit)(void *); } GcSettingsFloorV1;
typedef struct { uint32_t version; size_t byte_size; void *context; void (*set_auto)(void *,int); void (*set_precise)(void *,int); } GcSettingsV1;
typedef struct {
    uint32_t version; size_t byte_size;
    GcSettingsV1 *(*create)(const GcSettingsFloorV1 *);
    void (*destroy)(GcSettingsV1 *);
    void (*set_auto)(GcSettingsV1 *,int);
    void (*set_precise)(GcSettingsV1 *,int);
    int (*automatic_due)(const GcSettingsV1 *,int,size_t);
    int (*precise)(const GcSettingsV1 *);
} GcSettingsFactoryV1;
typedef const GcSettingsFactoryV1 *(*GcSettingsFactoryProviderV1)(void);
typedef int (*GcSettingsDependencyV1)(const GcSettingsFactoryV1 *);
typedef GcSettingsV1 *(*GcSettingsProviderV1)(void);
static inline void gc_settings_auto_v1(const GcSettingsV1 *selected,int enabled) { GcSettingsV1 api=*selected;api.set_auto(api.context,enabled); }
static inline void gc_settings_precise_v1(const GcSettingsV1 *selected,int enabled) { GcSettingsV1 api=*selected;api.set_precise(api.context,enabled); }
_Static_assert(sizeof(GcSettingsFloorV1)==32 && offsetof(GcSettingsFloorV1,admit)==24,"GC settings floor v1");
_Static_assert(sizeof(GcSettingsV1)==40 && offsetof(GcSettingsV1,set_precise)==32,"GC settings v1");
_Static_assert(sizeof(GcSettingsFactoryV1)==64 && offsetof(GcSettingsFactoryV1,precise)==56,"GC settings factory v1");
#endif
