#include "idapi_v10.h"
#include "namespace-support.h"
#include "object-model-support.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>

typedef Oop (*New)(Oop, Oop, const Oop *, size_t);
typedef Oop (*Admit)(const uint8_t *, size_t);
typedef Oop (*OwnedNew)(Oop, Oop, const Oop *, size_t, Oop);
typedef struct { uint64_t version, size; Admit admit; New construct; } V1;
typedef struct { uint64_t version, size; OwnedNew construct; } V2;
typedef Oop (*Attempt)(Oop, const void *, uint32_t, Oop, Oop, const Oop *, size_t, Oop, uint32_t, uint32_t *);
static IdApi *api;
static char events[64];
static size_t event_count;
static Oop vt, code, owner, captures[2], answer;
static int mode;
static Oop admit(const uint8_t *bytes, size_t count) { (void)bytes; (void)count; assert(0); return NULL; }
static void record(char event) { assert(event_count + 1 < sizeof(events)); events[event_count++] = event; events[event_count] = 0; }
static Oop construct(Oop table, Oop object, const Oop *values, size_t count) {
    record('C'); assert(table == vt && object == code && count == 2);
    assert(values[0] == captures[0] && values[1] == captures[1]); return answer;
}
static Oop construct_owned(Oop table, Oop object, const Oop *values, size_t count, Oop actual_owner) {
    assert(actual_owner == owner); return construct(table, object, values, count);
}
static Oop import(const char *name) {
    record(strstr(name, "Vt") ? 'T' : 'I');
    if (mode == 1 && !strstr(name, "Vt")) return NULL;
    return api->import_fn(name);
}
static Oop vtable(Oop value) { record('V'); return mode == 2 ? api->nil_obj : api->vtable_of(value); }
static size_t size(Oop value) { record('S'); return mode == 3 ? 0 : api->object_payload_size(value); }
static void *open_library(const char *path, int flags) { record('L'); return api->dlopen_fn(path, flags); }
static void *symbol(void *handle, const char *name) { record('D'); return api->dlsym_fn(handle, name); }

int main(int argc, char **argv) {
    assert(argc == 4);
    void *floor = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL); assert(floor);
    IdInitFn initialize = (IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init")); assert(initialize);
    api = initialize();
    fixture_mount_object_model(api, argv[1]);
    api->gc_set_auto(0);
    void *tcc = dlopen(argv[2], RTLD_NOW | RTLD_LOCAL); assert(tcc);
    ModuleInitFn tcc_init = (ModuleInitFn)dlsym(tcc, "module_init"); assert(tcc_init); fixture_attach_integer(api, tcc); tcc_init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports());
    void *codegen = dlopen(argv[3], RTLD_NOW | RTLD_LOCAL); assert(codegen);
    ModuleInitFn init = (ModuleInitFn)dlsym(codegen, "module_init"); assert(init);
    IdApi copied = *api; fixture_attach_integer(api, codegen); init(&copied,NULL,fixture_namespace_presence(),fixture_namespace_imports());
    Attempt attempt = (Attempt)dlsym(codegen, "birnam_portable_block_or_inline_v1"); assert(attempt);
    New baseline = (New)dlsym(codegen, "birnam_portable_block_new_v1"); assert(baseline);
    OwnedNew owned_baseline = (OwnedNew)dlsym(codegen, "birnam_portable_materialized_block_new_v2"); assert(owned_baseline);
    Oop v1vt = api->proto(api->object_vt), v2vt = api->proto(api->object_vt);
    api->set_layout(v1vt, NULL, 0, IDAPI_V10_SLOT_RAW); api->set_layout(v2vt, NULL, 0, IDAPI_V10_SLOT_RAW);
    V1 *v1 = api->alloc(v1vt, sizeof(V1)); *v1 = (V1){1, sizeof(V1), admit, construct};
    V2 *v2 = api->alloc(v2vt, sizeof(V2)); *v2 = (V2){2, sizeof(V2), construct_owned};
    api->export_obj("_BirnamPortableCodeApiV1", v1); api->export_obj("_BirnamPortableCodeApiV1Vt", v1vt);
    api->export_obj("_BirnamPortableCodeApiV2", v2); api->export_obj("_BirnamPortableCodeApiV2Vt", v2vt);
    vt = api->proto(api->object_vt); code = api->alloc(api->object_vt, 0);
    owner = api->alloc(api->object_vt, 0); captures[0] = captures[1] = api->true_obj;
    copied.import_fn = import; copied.vtable_of = vtable; copied.object_payload_size = size;
    copied.dlopen_fn = open_library; copied.dlsym_fn = symbol;
    for (unsigned owned = 0; owned <= 1; ++owned) {
        for (mode = 0; mode <= 4; ++mode) {
            v1->version = mode == 4 ? 999 : 1; v2->version = mode == 4 ? 999 : 2;
            for (unsigned nil_answer = 0; nil_answer <= 1; ++nil_answer) {
                answer = nil_answer ? api->nil_obj : code;
                event_count = 0; events[0] = 0;
                Oop expected = owned ? owned_baseline(vt, code, captures, 2, owner) : baseline(vt, code, captures, 2);
                char expected_events[64]; strcpy(expected_events, events);
                assert(strcmp(expected_events, mode == 1 ? "IT" : mode == 2 ? "ITV" : mode ? "ITVS" : "ITVSC") == 0);
                event_count = 0; events[0] = 0;
                uint32_t inlined = 999;
                Oop actual = attempt(api->nil_obj, NULL, 0, vt, code, captures, 2, owner, owned, &inlined);
                assert(actual == expected && inlined == 0);
                if (strcmp(events, expected_events)) fprintf(stderr, "owned=%u mode=%d expected=%s actual=%s\n", owned, mode, expected_events, events);
                assert(strcmp(events, expected_events) == 0);
            }
        }
    }
    puts("closure-api-context: V1/V2 refusal preserves exact capability callbacks, constructor operands and answers");
}
