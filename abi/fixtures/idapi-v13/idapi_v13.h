#ifndef BIRNAM_IDAPI_V13_H
#define BIRNAM_IDAPI_V13_H

#include <stddef.h>
#include <stdint.h>
#include "../namespace-presence-v1/namespace_presence_v1.h"
#include "../namespace-import-v1/namespace_import_v1.h"
#include "../namespace-export-v1/namespace_export_v1.h"
#include "../vtable-enumeration-v2/vtable_enumeration_v2.h"

typedef void *Oop;
typedef Oop (*Method)(Oop self, const Oop *args, size_t count);
typedef void (*MarkFn)(Oop value);
typedef void (*RootProvider)(MarkFn mark);
typedef void (*TraceHook)(Oop self, MarkFn mark);
typedef void (*Finalizer)(Oop self);
typedef void (*MethodOverwriteHook)(Oop vtable, Oop selector);

typedef struct IdApi IdApi;
typedef IdApi *(*IdInitFn)(void);
typedef void *(*ModuleInitFn)(IdApi *api, void *prior_state, const NamespacePresenceV1 *presence, const NamespaceImportV1 *imports, const NamespaceExportV1 *exports, const VtableEnumerationV2 *vtables);

enum {
    IDAPI_V13_SLOT_RAW = 0,
    IDAPI_V13_SLOT_OOP = 1,
    IDAPI_V13_SLOT_CODE = 2,
    IDAPI_V13_SLOT_UNKNOWN = 255,
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
    void (*gc_register_root_provider)(RootProvider provider);
    size_t (*gc_collect)(void);
    size_t (*gc_live_count)(void);
    void (*set_trace_hook)(Oop vtable, TraceHook hook);
    void (*gc_set_auto)(int enabled);
    void (*set_finalizer)(Oop object, Finalizer finalizer);
    void (*set_layout)(Oop vtable, const uint8_t *kinds, size_t count, uint8_t rest);
    void (*gc_set_precise)(int enabled);
    size_t (*object_payload_size)(Oop object);
    uint8_t (*slot_kind)(Oop vtable, size_t word);
    void (*set_method_overwrite_hook)(MethodOverwriteHook hook);
};

_Static_assert(sizeof(void *) == 8, "IdApi v13 requires 64-bit pointers");
_Static_assert(sizeof(IdApi) == 216, "IdApi v13 table size changed");
_Static_assert(_Alignof(IdApi) == 8, "IdApi v13 table alignment changed");
_Static_assert(offsetof(IdApi, dlopen_fn) == 0, "IdApi v13 first field changed");
_Static_assert(offsetof(IdApi, send) == 72, "IdApi v13 send offset changed");
_Static_assert(offsetof(IdApi, set_layout) == 176, "IdApi v13 layout offset changed");
_Static_assert(offsetof(IdApi, set_method_overwrite_hook) == 208, "IdApi v13 final field changed");

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
