#ifndef BIRNAM_IDAPI_V19_H
#define BIRNAM_IDAPI_V19_H

#include <stddef.h>
#include <stdint.h>
#include "../namespace-presence-v1/namespace_presence_v1.h"
#include "../namespace-import-v1/namespace_import_v1.h"
#include "../namespace-export-v1/namespace_export_v1.h"
#include "../vtable-enumeration-v2/vtable_enumeration_v2.h"
#include "../method-observation-v1/method_observation_v1.h"
#include "../root-registry-v1/root_registry_v1.h"
#include "../gc-settings-v1/gc_settings_v1.h"
#include "../payload-layout-v1/payload_layout_v1.h"
#include "../trace-hooks-v1/trace_hooks_v1.h"
#include "../finalizers-v1/finalizers_v1.h"

typedef void *Oop;
typedef Oop (*Method)(Oop self, const Oop *args, size_t count);
typedef void (*MarkFn)(Oop value);
typedef void (*RootProvider)(MarkFn mark);
typedef void (*TraceHook)(Oop self, MarkFn mark);
typedef void (*Finalizer)(Oop self);

typedef struct IdApi IdApi;
typedef IdApi *(*IdInitFn)(void);
typedef void *(*ModuleInitFn)(IdApi *api, void *prior_state, const NamespacePresenceV1 *presence, const NamespaceImportV1 *imports, const NamespaceExportV1 *exports, const VtableEnumerationV2 *vtables, const MethodObservationV1 *observation, const RootRegistryV1 *roots,const GcSettingsV1 *settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,const FinalizersV1 *finalizer_cap);

enum {
    IDAPI_V19_SLOT_RAW = 0,
    IDAPI_V19_SLOT_OOP = 1,
    IDAPI_V19_SLOT_CODE = 2,
    IDAPI_V19_SLOT_UNKNOWN = 255,
};

struct IdApi {
    void *(*dlopen_fn)(const char *path, int mode);
    void *(*dlsym_fn)(void *handle, const char *name);
    Oop object_vt;
    Oop vtable_vt;
    Oop symbol_vt;
    Oop (*intern)(const char *string);
    Oop (*proto)(Oop base);
    void (*method)(Oop type, Oop selector, Method implementation);
    Oop (*alloc)(Oop type, size_t payload_bytes);
    Oop (*send)(Oop receiver, Oop selector, const Oop *args, size_t count);
    Oop nil_obj;
    Oop true_obj;
    Oop false_obj;
    const char *(*symbol_name)(Oop symbol);
    Oop (*vtable_of)(Oop object);
    void (*gc_set_stack_bottom)(size_t address);
    size_t (*gc_collect)(void);
    size_t (*gc_live_count)(void);
    size_t (*object_payload_size)(Oop object);
};

_Static_assert(sizeof(void *) == 8, "IdApi v19 requires 64-bit pointers");
_Static_assert(sizeof(IdApi) == 152, "IdApi v19 table size changed");
_Static_assert(_Alignof(IdApi) == 8, "IdApi v19 table alignment changed");
_Static_assert(offsetof(IdApi, dlopen_fn) == 0, "IdApi v19 first field changed");
_Static_assert(offsetof(IdApi, send) == 72, "IdApi v19 send offset changed");
_Static_assert(offsetof(IdApi, object_payload_size) == 144, "IdApi v19 final field changed");

/* Message adapters only; the table contains no owner/parent entries. */
static inline Oop vtable_owner(IdApi *api, Oop candidate) {
    return api->send(api->vtable_vt, api->intern("ownerOf:"), &candidate, 1);
}
static inline Oop vtable_parent(IdApi *api, Oop candidate) {
    return api->send(api->vtable_vt, api->intern("parentOf:"), &candidate, 1);
}
static inline void set_owner(IdApi *api, Oop vt, Oop owner) {
    Oop args[] = {vt, owner};
    api->send(api->vtable_vt, api->intern("ownerOf:put:"), args, 2);
}
static inline void set_parent(IdApi *api, Oop vt, Oop parent) {
    Oop args[] = {vt, parent};
    api->send(api->vtable_vt, api->intern("parentOf:put:"), args, 2);
}

#endif
