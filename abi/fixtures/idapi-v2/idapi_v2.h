#ifndef BIRNAM_IDAPI_V2_H
#define BIRNAM_IDAPI_V2_H

#include <stddef.h>
#include <stdint.h>

typedef void *Oop;
typedef Oop (*Method)(Oop self, const Oop *args, size_t count);
typedef void (*MarkFn)(Oop value);
typedef void (*RootProvider)(MarkFn mark);
typedef void (*TraceHook)(Oop self, MarkFn mark);
typedef void (*Finalizer)(Oop self);
typedef void (*MethodOverwriteHook)(Oop vtable, Oop selector);

typedef struct IdApi IdApi;
typedef IdApi *(*IdInitFn)(void);
typedef void *(*ModuleInitFn)(IdApi *api, void *prior_state);

enum {
    IDAPI_V2_SLOT_RAW = 0,
    IDAPI_V2_SLOT_OOP = 1,
    IDAPI_V2_SLOT_CODE = 2,
    IDAPI_V2_SLOT_UNKNOWN = 255,
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
    Oop (*import_fn)(const char *key);
    Oop (*export_obj)(const char *key, Oop value);
    int (*is_exported)(const char *key);
    Oop (*send)(Oop receiver, Oop selector, const Oop *args, size_t count);
    Oop smallint_vt;
    Oop nil_obj;
    Oop true_obj;
    Oop false_obj;
    Oop (*mk_smallint)(int64_t value);
    int64_t (*smallint_value)(Oop value);
    int (*is_smallint)(Oop value);
    Oop string_vt;
    Oop (*mk_string)(const uint8_t *bytes, size_t length);
    const uint8_t *(*string_bytes)(Oop string, size_t *length);
    const char *(*symbol_name)(Oop symbol);
    Oop (*vtable_of)(Oop object);
    void (*gc_set_stack_bottom)(size_t address);
    void (*gc_register_root_provider)(RootProvider provider);
    size_t (*gc_collect)(void);
    size_t (*gc_live_count)(void);
    void (*set_trace_hook)(Oop vtable, TraceHook hook);
    void (*gc_set_auto)(int enabled);
    Oop float_vt;
    Oop (*mk_float)(double value);
    double (*float_value)(Oop value);
    int (*is_float)(Oop value);
    void (*set_finalizer)(Oop object, Finalizer finalizer);
    void (*set_layout)(Oop vtable, const uint8_t *kinds, size_t count, uint8_t rest);
    void (*gc_set_precise)(int enabled);
    size_t (*object_payload_size)(Oop object);
    const char *(*exported_name_of)(Oop object);
    uint8_t (*slot_kind)(Oop vtable, size_t word);
    void (*set_owner)(Oop vtable, Oop owner);
    void (*set_parent)(Oop vtable, Oop parent);
    void (*set_method_overwrite_hook)(MethodOverwriteHook hook);
    size_t (*vtable_count)(void);
    Oop (*vtable_at)(size_t index);
    Oop (*vtable_selector_at)(Oop vtable, size_t index);
    Oop (*vtable_parent)(Oop vtable);
    Oop (*mk_word)(uint64_t value);
    uint64_t (*word_value)(Oop value);
    int (*is_word)(Oop value);
    Oop word_vt;
    Oop (*vtable_owner)(Oop vtable);
};

_Static_assert(sizeof(void *) == 8, "IdApi v2 requires 64-bit pointers");
_Static_assert(sizeof(IdApi) == 424, "IdApi v2 table size changed");
_Static_assert(_Alignof(IdApi) == 8, "IdApi v2 table alignment changed");
_Static_assert(offsetof(IdApi, dlopen_fn) == 0, "IdApi v2 first field changed");
_Static_assert(offsetof(IdApi, send) == 96, "IdApi v2 send offset changed");
_Static_assert(offsetof(IdApi, set_layout) == 288, "IdApi v2 layout offset changed");
_Static_assert(offsetof(IdApi, vtable_owner) == 416, "IdApi v2 final field changed");

#endif
