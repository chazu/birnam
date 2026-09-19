#include "idapi_v7.h"
#include "../../../scripts/fixtures/namespace-support.h"
#include "object-model-support.h"
#include "trace-support.h"

#include <dlfcn.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static IdApi *api;
static Oop provider_root;
static unsigned provider_calls;
static unsigned trace_calls;
static unsigned finalizer_calls;
static Method original_protocol[4];
static unsigned protocol_calls[4];

#define PROTOCOL_WRAPPER(N) \
    static Oop protocol_##N(Oop self, const Oop *args, size_t count) { \
        ++protocol_calls[N]; \
        return original_protocol[N](self, args, count); \
    }
PROTOCOL_WRAPPER(0)
PROTOCOL_WRAPPER(1)
PROTOCOL_WRAPPER(2)
PROTOCOL_WRAPPER(3)

static void fail(const char *message) {
    fprintf(stderr, "idapi-v7-client: %s\n", message);
    exit(1);
}

static Oop answer_42(Oop self, const Oop *args, size_t count) {
    (void)self;
    if (args != NULL || count != 0) fail("zero-argument Method buffer drifted");
    return fixture_integer_make(42);
}

static void root_provider(MarkFn mark) {
    provider_calls += 1;
    mark(provider_root);
}

static void trace_hook(Oop self, MarkFn mark) {
    (void)self;
    trace_calls += 1;
    mark(api->nil_obj);
}

static void finalizer(Oop self) {
    (void)self;
    finalizer_calls += 1;
}

static void *required_symbol(void *handle, const char *name) {
    void *symbol = dlsym(handle, name);
    if (symbol == NULL) fail(name);
    return symbol;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: %s <libbirnam_id.dylib> <fixture-module.dylib>\n", argv[0]);
        return 2;
    }

    void *floor = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (floor == NULL) fail(dlerror());
    fixture_prepare_namespace(floor, argv[1]);
    void *raw_init = required_symbol(floor, "id_init");
    IdInitFn id_init = NULL;
    memcpy(&id_init, &raw_init, sizeof(id_init));
    api = id_init();
    if (api == NULL) fail("id_init returned null");
    fixture_mount_trace(api, argv[1]);
    size_t stack_anchor = 0;
    api->gc_set_stack_bottom((size_t)&stack_anchor);
    Oop private_selector = api->intern("ensurePrivateVtableFor:");
    Oop lookup = api->intern("lookup");
    if (api->send(api->vtable_vt, lookup, &private_selector, 1) != NULL)
        fail("private-behavior implementation unexpectedly present in bare Id");
    fixture_mount_object_model(api, argv[1]);
    if (api->send(api->vtable_vt, lookup, &private_selector, 1) == NULL)
        fail("object-model did not publish its service");

    if (api->object_vt == NULL || api->vtable_vt == NULL || api->symbol_vt == NULL)
        fail("canonical vtables are missing");
    if (api->nil_obj == NULL || api->true_obj == NULL || api->false_obj == NULL ||
        api->nil_obj == api->true_obj || api->true_obj == api->false_obj)
        fail("canonical singletons drifted");

    Oop selector = api->intern("fixtureAnswer");
    Oop vtable = api->proto(api->object_vt);
    api->method(vtable, selector, answer_42);
    Oop object = api->alloc(vtable, 0);
    Oop answer = api->send(object, selector, NULL, 0);
    if (!fixture_is_integer(answer) || fixture_integer_value(answer) != 42)
        fail("zero-argument send failed");

    api->export_obj("IdApiV7FixtureObject", object);
    if (!api->is_exported("IdApiV7FixtureObject") ||
        api->import_fn("IdApiV7FixtureObject") != object)
        fail("environment round-trip failed");
    const char *exported_name = api->exported_name_of(object);
    if (exported_name == NULL || strcmp(exported_name, "IdApiV7FixtureObject") != 0)
        fail("reverse export lookup failed");

    Oop string = fixture_string_make((const uint8_t *)"v7-bytes", 8);
    size_t string_length = 0;
    const uint8_t *string_data = fixture_string_bytes(string, &string_length);
    if (string_length != 8 || memcmp(string_data, "v7-bytes", 8) != 0)
        fail("String ABI failed");
    Oop floating = fixture_float_make(1.25);
    if (!fixture_is_float(floating) || fixture_float_value(floating) != 1.25)
        fail("Float ABI failed");
    Oop word = fixture_word_make(UINT64_C(0xfedcba9876543210));
    if (!fixture_is_word(word) || fixture_word_value(word) != UINT64_C(0xfedcba9876543210) ||
        api->vtable_of(word) != fixture_word->behavior)
        fail("late Word ABI failed");

    set_parent(api, vtable, api->object_vt);
    set_owner(api, vtable, object);
    if (vtable_owner(api, vtable) != object || vtable_parent(api, vtable) != api->object_vt)
        fail("vtable ownership/parent ABI failed");
    if (api->vtable_selector_at(vtable, 0) != selector)
        fail("direct selector enumeration failed");
    Oop child = api->alloc(vtable, 0);
    Oop private_vtable = api->send(api->vtable_vt, private_selector, &child, 1);
    if (private_vtable == vtable || vtable_owner(api, private_vtable) != child ||
        vtable_parent(api, private_vtable) != vtable)
        fail("private vtable ABI failed");
    if (api->send(api->vtable_vt, private_selector, &child, 1) != private_vtable ||
        api->vtable_of(object) != vtable)
        fail("private behavior reuse or sibling isolation changed");
    if (api->send(api->vtable_vt, private_selector, &vtable, 1) != vtable)
        fail("vtable self case changed");
    if (vtable_owner(api, NULL) || vtable_parent(api, NULL) ||
        vtable_owner(api, (Oop)(uintptr_t)0x40) || vtable_parent(api, object))
        fail("default queries lost exact-registry candidate handling");

    const char *protocol_names[] = {"ownerOf:", "ownerOf:put:", "parentOf:", "parentOf:put:"};
    Method wrappers[] = {protocol_0, protocol_1, protocol_2, protocol_3};
    Oop protocols[4];
    for (unsigned i = 0; i < 4; ++i) {
        protocols[i] = api->intern(protocol_names[i]);
        Oop implementation = api->send(api->vtable_vt, lookup, &protocols[i], 1);
        if (!implementation) fail("default protocol missing");
        memcpy(&original_protocol[i], &implementation, sizeof(Method));
        api->method(api->vtable_vt, protocols[i], wrappers[i]);
    }
    set_owner(api, vtable, object);
    set_parent(api, vtable, api->object_vt);
    if (vtable_owner(api, vtable) != object || vtable_parent(api, vtable) != api->object_vt)
        fail("replaced protocols changed forwarded values");
    for (unsigned i = 0; i < 4; ++i)
        if (protocol_calls[i] != 1) fail("protocol replacement bypassed");
    Oop fresh = api->alloc(vtable, 0);
    Oop fresh_private = api->send(api->vtable_vt, private_selector, &fresh, 1);
    if (fresh_private == vtable || protocol_calls[0] != 2 || protocol_calls[1] != 2)
        fail("object-model bypassed replaced owner protocols");
    for (unsigned i = 0; i < 4; ++i)
        api->method(api->vtable_vt, protocols[i], original_protocol[i]);

    /* A cached inherited lookup must observe a parent write immediately. */
    Oop inherited = api->proto(vtable);
    Oop peer = api->alloc(inherited, 0);
    if (fixture_integer_value(api->send(peer, selector, NULL, 0)) != 42)
        fail("inherited lookup failed before parent mutation");
    set_parent(api, inherited, api->object_vt);
    if (api->send(inherited, lookup, &selector, 1) != NULL)
        fail("parent mutation retained stale inherited lookup");
    set_parent(api, inherited, vtable);
    if (fixture_integer_value(api->send(peer, selector, NULL, 0)) != 42)
        fail("restored parent was not visible");
    if (api->vtable_count() < 2 || api->vtable_at(api->vtable_count()) != NULL)
        fail("vtable registry ABI failed");

    api->set_layout(vtable, NULL, 0, IDAPI_V7_SLOT_RAW);
    if (api->slot_kind(vtable, 0) != IDAPI_V7_SLOT_RAW ||
        api->object_payload_size(object) != 0)
        fail("layout/image introspection ABI failed");
    api->set_trace_hook(vtable, trace_hook);
    Oop finalizable = api->alloc(vtable, sizeof(uint64_t));
    api->set_finalizer(finalizable, finalizer);
    (void)finalizable;

    api->gc_set_auto(0);
    api->gc_set_precise(1);
    provider_root = object;
    api->gc_register_root_provider(root_provider);
    (void)api->gc_collect();
    if (provider_calls == 0 || trace_calls == 0)
        fail("collector callbacks were not invoked");

    void *module = api->dlopen_fn(argv[2], RTLD_NOW | RTLD_LOCAL);
    if (module == NULL) fail("IdApi dlopen failed");
    void *raw_module_init = api->dlsym_fn(module, "module_init");
    void *raw_module_calls = api->dlsym_fn(module, "fixture_module_calls");
    if (raw_module_init == NULL || raw_module_calls == NULL)
        fail("IdApi dlsym failed");
    ModuleInitFn module_init = NULL;
    unsigned (*module_calls)(void *) = NULL;
    memcpy(&module_init, &raw_module_init, sizeof(module_init));
    memcpy(&module_calls, &raw_module_calls, sizeof(module_calls));
    void *state = module_init(api, NULL);
    if (state == NULL || module_init(api, state) != state || module_calls(state) != 2)
        fail("module state threading failed");

    printf("idapi-v7-client: ok fields=34 module_calls=2 provider_calls=%u trace_calls=%u finalizers=%u\n",
           provider_calls, trace_calls, finalizer_calls);
    return 0;
}
