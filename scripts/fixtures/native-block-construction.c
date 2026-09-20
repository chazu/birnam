#include "idapi_v10.h"
#include "namespace-support.h"
#include "object-model-support.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef Oop (*Admit)(const uint8_t *, size_t);
typedef Oop (*NewBlock)(Oop, Oop, const Oop *, size_t);
typedef Oop (*NewOwnedBlock)(Oop, Oop, const Oop *, size_t, Oop);
typedef struct { uint64_t version, size; Admit admit; NewBlock construct; } PortableV1;
typedef struct { uint64_t version, size; NewOwnedBlock construct; } PortableV2;
typedef unsigned (*Query)(const void *, Oop, const uintptr_t *, Oop, Oop, size_t, Oop, const uint8_t *, size_t, Oop);
typedef unsigned (*Admission)(const void *, Oop, Oop, size_t, Oop, const uint8_t *, size_t, unsigned);

static IdApi *api;
static unsigned allocations;
static Oop forbidden_import(void *context,const char *name) { (void)context;(void)name;assert(!"Block admission invoked import");return NULL; }
static NamespaceNamesApiV1 original_names, selected_names;
static unsigned name_calls;
static const char *counted_name(void *context,Oop value) {
    assert(context==&selected_names);name_calls++;
    return original_names.name_of(original_names.context,value);
}
static Admission admission;
static Oop probe, probe_selector;
static struct {
    const void *constructor;
    Oop vt, code, owner;
    size_t captures, length;
    const uint8_t *program;
    unsigned materialized;
} request;
static Oop run_probe(Oop self, const Oop *args, size_t count) {
    (void)self; (void)args; assert(count == 0);
    return admission(request.constructor, request.vt, request.code, request.captures,
                     request.owner, request.program, request.length, request.materialized)
        ? api->true_obj : api->false_obj;
}
static unsigned floor_admits(const void *constructor, Oop vt, Oop code, size_t captures,
                             Oop owner, const uint8_t *program, size_t length, unsigned materialized) {
    request.constructor = constructor; request.vt = vt; request.code = code;
    request.captures = captures; request.owner = owner; request.program = program;
    request.length = length; request.materialized = materialized;
    return api->send(probe, probe_selector, NULL, 0) == api->true_obj;
}
static Oop counted_alloc(Oop vt, size_t bytes) { ++allocations; return api->alloc(vt, bytes); }
static Oop value(Oop self, const Oop *args, size_t count) { (void)self; return count ? args[0] : api->nil_obj; }
static Method original_accessor;
static unsigned accessor_calls;
static Oop counted_accessor(Oop self, const Oop *args, size_t count) {
    ++accessor_calls;
    return original_accessor(self, args, count);
}

int main(int argc, char **argv) {
    assert(argc == 7 || (argc == 8 && strcmp(argv[7], "without-prelude") == 0));
    void *floor = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    assert(floor);
    IdInitFn initialize = (IdInitFn)(fixture_prepare_namespace(floor,argv[1]),dlsym(floor,"id_init"));
    assert(initialize);
    api = initialize();
    original_names=*fixture_namespace_names();
    selected_names=original_names;
    fixture_names=&selected_names; /* Independently selected copied capability. */
    fixture_mount_object_model(api, argv[1]);
    admission = (Admission)dlsym(floor, "birnam_id_warm_block_construction_v1");
    assert(admission);
    api->gc_set_auto(0);
    probe_selector = api->intern("probeConstruction");
    Oop probe_vt = api->proto(api->object_vt);
    api->method(probe_vt, probe_selector, run_probe);
    probe = api->alloc(probe_vt, 0);
    IdApi module_api = *api;
    NamespaceImportV1 module_imports=*fixture_namespace_imports(),original_imports=module_imports;
    void *portable = NULL;
    for (int i = 2; i <= 5; ++i) {
        if (argc == 8 && i == 2) continue;
        void *module = api->dlopen_fn(argv[i], RTLD_NOW | RTLD_LOCAL);
        assert(module);
        ModuleInitFn init = (ModuleInitFn)api->dlsym_fn(module, "module_init");
        assert(init);
        fixture_attach_integer(api, module);
        init(i == 5 ? &module_api : api,NULL,fixture_namespace_presence(),i == 5 ? &module_imports : fixture_namespace_imports());
        if (i == 5) portable = module;
    }
    Query query = (Query)api->dlsym_fn(portable, "birnam_native_block_construction_v4");
    assert(query);
    PortableV1 *v1 = fixture_namespace_import("_BirnamPortableCodeApiV1");
    PortableV2 *v2 = fixture_namespace_import("_BirnamPortableCodeApiV2");
    assert(v1 && v1->version == 1 && v1->size == sizeof(*v1));
    assert(v2 && v2->version == 2 && v2->size == sizeof(*v2));
    uintptr_t context[] = {
        (uintptr_t)api->alloc, (uintptr_t)fixture_namespace_imports()->import, (uintptr_t)api->object_payload_size,
        (uintptr_t)api->vtable_of, (uintptr_t)api->send, (uintptr_t)api->vtable_selector_at,
        (uintptr_t)api->symbol_name, (uintptr_t)api->intern, (uintptr_t)api->symbol_vt,
        (uintptr_t)fixture_namespace_names()->name_of, (uintptr_t)fixture_namespace_names()->context, (uintptr_t)api->vtable_vt,
        (uintptr_t)fixture_namespace_imports()->context,
    };
    FILE *file = fopen(argv[6], "rb");
    assert(file && fseek(file, 0, SEEK_END) == 0);
    long length = ftell(file);
    assert(length > 0 && fseek(file, 0, SEEK_SET) == 0);
    uint8_t *bytes = malloc((size_t)length);
    assert(bytes && fread(bytes, 1, (size_t)length, file) == (size_t)length);
    fclose(file);
    Oop code = v1->admit(bytes, (size_t)length);
    Oop equal_code = v1->admit(bytes, (size_t)length);
    assert(code && equal_code && code != equal_code);
    if (argc == 8) {
        Oop empty_vt = api->proto(api->object_vt);
        api->method(empty_vt, api->intern("value:"), value);
        size_t live = api->gc_live_count();
        assert(floor_admits((void *)v1->construct, empty_vt, code, 2, NULL, bytes, (size_t)length, 0) == 0);
        assert(api->gc_live_count() == live);
        free(bytes);
        puts("native-block-construction: absent Block family declines without allocation");
        return 0;
    }
    Oop family = fixture_namespace_import("BlockVt");
    Oop vt = api->proto(family);
    Oop selector = api->intern("value:");
    api->method(vt, selector, value);
    Oop capture = api->alloc(api->object_vt, sizeof(Oop));
    *(Oop *)capture = api->true_obj;
    Oop captures[] = { capture, capture };
#define ASK(fn, table, object, count, owner) query((const void *)(fn), api->nil_obj, context, table, object, count, owner, bytes, (size_t)length, vtable_owner(api, table))
    size_t before = api->gc_live_count();
    assert(ASK(v1->construct, vt, code, 2, NULL) == 0); /* cold setup */
    assert(vtable_owner(api, vt) == NULL && api->gc_live_count() == before);
    Oop first = v1->construct(vt, code, captures, 2);
    assert(api->vtable_of(first) == vt && api->object_payload_size(first) == 3 * sizeof(Oop));
    assert(((Oop *)first)[0] == code && ((Oop *)first)[1] == capture && ((Oop *)first)[2] == capture);
    Oop carrier = vtable_owner(api, vt);
    assert(carrier);
    before = api->gc_live_count();
    assert(ASK(v1->construct, vt, code, 2, NULL) == 1);
    assert(ASK(v1->construct, vt, equal_code, 2, NULL) == 1); /* payload uses supplied identity */
    assert(ASK(v2->construct, vt, code, 2, NULL) == 2);
    assert(admission((void *)v1->construct, vt, code, 2, NULL, bytes, (size_t)length, 0) == 0); /* no active entry */
    assert(floor_admits((void *)v1->construct, vt, code, 2, NULL, bytes, (size_t)length, 0) == 1);
    assert(floor_admits((void *)v2->construct, vt, code, 2, NULL, bytes, (size_t)length, 1) == 1);
    assert(floor_admits((void *)v1->construct, vt, code, 2, NULL, bytes, (size_t)length, 1) == 0);
    assert(floor_admits((void *)value, vt, code, 2, NULL, bytes, (size_t)length, 0) == 0);
    assert(api->gc_live_count() == before);
    assert(ASK(value, vt, code, 2, NULL) == 0);
    assert(ASK(v1->construct, vt, code, 1, NULL) == 0);
    assert(ASK(v1->construct, vt, code, 2, capture) == 0);
    assert(ASK(v2->construct, vt, code, 2, capture) == 0);
    bytes[0] ^= 1;
    assert(ASK(v1->construct, vt, code, 2, NULL) == 0);
    bytes[0] ^= 1;
    for (unsigned i = 0; i < sizeof(context)/sizeof(context[0]); ++i) {
        uintptr_t saved = context[i]; context[i] = 0;
        assert(ASK(v1->construct, vt, code, 2, NULL) == 0);
        context[i] = saved;
    }
    /* Both live operands are part of pure admission. Never invoke a changed
       capability even when it forwards to the original implementation. */
    selected_names.name_of=counted_name;
    assert(ASK(v1->construct, vt, code, 2, NULL)==0);
    assert(floor_admits((void *)v1->construct, vt, code, 2, NULL, bytes, (size_t)length, 0)==0);
    assert(name_calls==0);
    selected_names=original_names;selected_names.context=&selected_names;
    assert(ASK(v1->construct, vt, code, 2, NULL)==0);
    assert(floor_admits((void *)v1->construct, vt, code, 2, NULL, bytes, (size_t)length, 0)==0);
    selected_names.name_of=counted_name;
    assert(ASK(v1->construct, vt, code, 2, NULL)==0 && name_calls==0);
    selected_names=original_names;
    assert(ASK(v1->construct, vt, code, 2, NULL)==1);
    assert(floor_admits((void *)v1->construct, vt, code, 2, NULL, bytes, (size_t)length, 0)==1);
    module_imports.import=forbidden_import;
    assert(ASK(v1->construct, vt, code, 2, NULL)==0);
    assert(floor_admits((void *)v1->construct, vt, code, 2, NULL, bytes, (size_t)length, 0)==0);
    module_imports=original_imports;module_imports.context=&module_imports;
    assert(ASK(v1->construct, vt, code, 2, NULL)==0);
    assert(floor_admits((void *)v1->construct, vt, code, 2, NULL, bytes, (size_t)length, 0)==0);
    module_imports.import=forbidden_import;
    assert(ASK(v1->construct, vt, code, 2, NULL)==0);
    module_imports=original_imports;
    assert(ASK(v1->construct, vt, code, 2, NULL)==1);
    assert(floor_admits((void *)v1->construct, vt, code, 2, NULL, bytes, (size_t)length, 0)==1);
    module_api.alloc = counted_alloc;
    assert(ASK(v1->construct, vt, code, 2, NULL) == 0);
    assert(floor_admits((void *)v1->construct, vt, code, 2, NULL, bytes, (size_t)length, 0) == 0);
    assert(allocations == 0);
    Oop second = v1->construct(vt, equal_code, captures, 2);
    assert(allocations == 1 && second != first && ((Oop *)second)[0] == equal_code);
    module_api.alloc = api->alloc;
    assert(ASK(v1->construct, vt, code, 2, NULL) == 1);
    assert(floor_admits((void *)v1->construct, vt, code, 2, NULL, bytes, (size_t)length, 0) == 1);
    /* Admission is pure and declines a changed owner/parent/lookup protocol;
       ordinary construction must still execute that replacement. */
    const char *accessors[] = {"ownerOf:", "parentOf:", "lookup"};
    Oop lookup = api->intern("lookup");
    for (unsigned i = 0; i < 3; ++i) {
        Oop accessor = api->intern(accessors[i]);
        Oop raw = api->send(api->vtable_vt, lookup, &accessor, 1);
        memcpy(&original_accessor, &raw, sizeof(original_accessor));
        api->method(api->vtable_vt, accessor, counted_accessor);
        accessor_calls = 0;
        assert(floor_admits((void *)v1->construct, vt, code, 2, NULL, bytes, (size_t)length, 0) == 0);
        /* The outer probe send itself uses lookup when lookup is replaced. */
        unsigned before_construct = accessor_calls;
        if (i != 2) assert(before_construct == 0);
        Oop ordinary = v1->construct(vt, code, captures, 2);
        assert(ordinary && accessor_calls > before_construct);
        api->method(api->vtable_vt, accessor, original_accessor);
        assert(floor_admits((void *)v1->construct, vt, code, 2, NULL, bytes, (size_t)length, 0) == 1);
    }
    *(Oop *)capture = api->false_obj;
    assert(*(Oop *)((Oop *)first)[1] == api->false_obj && ((Oop *)second)[2] == capture);
    set_owner(api, vt, api->alloc(api->object_vt, 0));
    assert(ASK(v1->construct, vt, code, 2, NULL) == 0); /* no invalid payload read */
    set_owner(api, vt, carrier);
    set_parent(api, vt, api->object_vt);
    assert(floor_admits((void *)v1->construct, vt, code, 2, NULL, bytes, (size_t)length, 0) == 0);
    set_parent(api, vt, family);
    assert(ASK(v1->construct, vt, code, 2, NULL) == 1);
    Oop owned_vt = api->proto(family);
    api->method(owned_vt, selector, value);
    Oop owned = v2->construct(owned_vt, code, captures, 2, capture);
    assert(api->vtable_of(owned) == owned_vt);
    assert(ASK(v2->construct, owned_vt, code, 2, capture) == 2);
    assert(ASK(v2->construct, owned_vt, code, 2, NULL) == 0);
    assert(ASK(v1->construct, owned_vt, code, 2, NULL) == 0);
    free(bytes);
    puts("native-block-construction: cold refusal, warm identity, shared captures, owner and API changes passed");
    return 0;
}
