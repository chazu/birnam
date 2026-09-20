/* Design 74I: observations through the frozen IdApi, with an ordinary floor. */
#include "idapi_v1.h"
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>

static IdApi *api;
static Method original_lookup;
static size_t first_hook_calls, second_hook_calls, lookup_calls, collections;
static Oop watched;
static int mutate_during_gc;

static void require(int condition, const char *message) {
    if (!condition) {
        fprintf(stderr, "deopt-public-api: %s\n", message);
        exit(1);
    }
}

static Oop answer_true(Oop self, const Oop *args, size_t count) {
    (void)self; (void)args; (void)count;
    return api->true_obj;
}

static Oop answer_false(Oop self, const Oop *args, size_t count) {
    (void)self; (void)args; (void)count;
    return api->false_obj;
}

static void first_hook(Oop vtable, Oop selector) {
    (void)vtable; (void)selector;
    ++first_hook_calls;
}

static void second_hook(Oop vtable, Oop selector) {
    (void)vtable; (void)selector;
    ++second_hook_calls;
}

static Oop forwarding_lookup(Oop self, const Oop *args, size_t count) {
    ++lookup_calls;
    /* This saved permanent floor method is a fixture forwarding target, not
       a source of canonical-method authority for a prospective optimizer. */
    return original_lookup(self, args, count);
}

static void mutate_root(MarkFn mark) {
    mark(watched);
    if (mutate_during_gc) {
        /* alloc returns the native client's declared payload, not a Frame. */
        *(Oop *)watched = api->false_obj;
    }
}

static void collection_observer(MarkFn mark) {
    (void)mark;
    ++collections;
}

static void check_dispatch(void) {
    Oop selector = api->intern("boundaryAnswer");
    Oop parent_a = api->proto(api->object_vt);
    Oop parent_b = api->proto(api->object_vt);
    Oop child = api->proto(parent_a);
    Oop receiver = api->alloc(child, 0);
    api->method(parent_a, selector, answer_true);
    api->method(parent_b, selector, answer_false);
    api->set_method_overwrite_hook(first_hook);

    require(api->send(receiver, selector, NULL, 0) == api->true_obj, "initial inherited answer");
    api->set_parent(child, parent_b);
    require(api->send(receiver, selector, NULL, 0) == api->false_obj, "parent change must affect send");
    require(first_hook_calls == 0, "parent change unexpectedly notified overwrite hook");
    require(api->vtable_of(receiver) == child, "parent change changed receiver vtable");
    puts("{\"case\":\"parent_change\",\"answer_changed\":true,\"same_vtable\":true,\"hook_calls\":0}");

    api->method(child, selector, answer_true);
    require(api->send(receiver, selector, NULL, 0) == api->true_obj, "new shadow must affect send");
    require(first_hook_calls == 0, "new shadow unexpectedly notified overwrite hook");
    puts("{\"case\":\"shadow_addition\",\"answer_changed\":true,\"hook_calls\":0}");

    require(api->vtable_selector_at(child, 0) == selector, "shadow selector inventory");
    Oop inventory_end = api->vtable_selector_at(child, 1);
    api->method(child, selector, answer_false);
    require(api->send(receiver, selector, NULL, 0) == api->false_obj, "replacement must affect send");
    require(api->vtable_of(receiver) == child, "replacement changed receiver vtable");
    require(api->vtable_selector_at(child, 0) == selector &&
            api->vtable_selector_at(child, 1) == inventory_end, "replacement changed selector inventory");
    require(first_hook_calls == 1, "replacement should notify the installed hook once");
    puts("{\"case\":\"method_replacement\",\"answer_changed\":true,\"same_vtable\":true,\"same_selectors\":true,\"hook_calls\":1}");

    api->set_method_overwrite_hook(second_hook);
    api->method(child, selector, answer_true);
    require(first_hook_calls == 1 && second_hook_calls == 1, "hook setter must replace the prior observer");
    require(api->send(receiver, selector, NULL, 0) == api->true_obj, "second hook replacement answer");
    puts("{\"case\":\"observer_replaced\",\"old_observer_additional_calls\":0,\"new_observer_calls\":1}");
    api->set_method_overwrite_hook(NULL);

    Oop lookup = api->intern("lookup");
    /* Fresh id_init is known to be pristine for this diagnostic only. */
    original_lookup = (Method)api->send(api->vtable_vt, lookup, &lookup, 1);
    require(original_lookup != NULL, "initial floor lookup address");
    api->method(api->vtable_vt, lookup, forwarding_lookup);
    lookup_calls = 0;
    require(api->send(receiver, selector, NULL, 0) == api->true_obj, "ordinary custom lookup answer");
    size_t ordinary_calls = lookup_calls;
    require(ordinary_calls == 1, "ordinary send must invoke custom lookup once");
    lookup_calls = 0;
    require(api->send(child, lookup, &selector, 1) != NULL, "reflective guard answer");
    require(api->send(receiver, selector, NULL, 0) == api->true_obj, "guarded send answer");
    require(lookup_calls == ordinary_calls + 1, "reflective guard must add a lookup effect");
    puts("{\"case\":\"reflective_guard\",\"same_answer\":true,\"ordinary_effects\":1,\"guard_then_send_effects\":2}");
    lookup_calls = 0;
    require(api->send(api->vtable_vt, lookup, &lookup, 1) != NULL, "metatable lookup answer");
    require(lookup_calls == 1, "metatable lookup must still invoke the replacement");
    puts("{\"case\":\"metatable_lookup\",\"replacement_effects\":1}");
    api->method(api->vtable_vt, lookup, original_lookup);
}

static void check_collection(void) {
    Oop vtable = api->proto(api->object_vt);
    const uint8_t kinds[] = {IDAPI_V1_SLOT_OOP};
    api->set_layout(vtable, kinds, 1, IDAPI_V1_SLOT_RAW);
    watched = api->alloc(vtable, sizeof(Oop));
    *(Oop *)watched = api->true_obj;
    fixture_register_root(mutate_root);
    fixture_register_root(collection_observer);
    /* Run before the dispatch cases so their temporary objects cannot make
       conservative native roots affect this equal-count counterexample. */
    api->gc_collect();
    api->gc_collect();
    size_t before_count = api->gc_live_count();
    size_t before_collections = collections;
    mutate_during_gc = 1;
    api->gc_collect();
    require(*(Oop *)watched == api->false_obj, "root callback must mutate the payload");
    require(api->vtable_of(watched) == vtable, "collection changed watched vtable");
    size_t after_count = api->gc_live_count();
    if (after_count != before_count) {
        fprintf(stderr, "live counts before=%zu after=%zu\n", before_count, after_count);
    }
    require(after_count == before_count, "live count did not stay equal in this fixture");
    require(collections == before_collections + 1, "public root provider must observe collection");
    puts("{\"case\":\"gc_observation\",\"payload_changed\":true,\"same_vtable\":true,\"same_live_count\":true,\"provider_notifications\":1}");
}

int main(int argc, char **argv) {
    require(argc == 2, "expected ordinary floor dylib path");
    void *floor = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (floor == NULL) {
        fprintf(stderr, "%s\n", dlerror());
        return 1;
    }
    IdInitFn initialize = (IdInitFn)dlsym(floor, "id_init");
    require(initialize != NULL, "id_init missing");
    api = initialize();
    require(api != NULL, "id_init returned null");
    uintptr_t stack_anchor = 0;
    api->gc_set_stack_bottom((uintptr_t)&stack_anchor + sizeof(stack_anchor));
    fixture_set_auto(0);
    fixture_set_precise(1);
    const char *unavailable[] = {
        "birnam_dev_frame_oop_at_v1", "birnam_dev_frame_oop_at_put_v1",
        "birnam_dev_frame_slot_get_v1", "birnam_dev_frame_slot_put_v1",
        "birnam_frame_slot_get_v1", "birnam_frame_slot_put_v1",
    };
    for (size_t i = 0; i < sizeof(unavailable) / sizeof(unavailable[0]); ++i) {
        require(api->dlsym_fn(floor, unavailable[i]) == NULL, unavailable[i]);
    }
    puts("{\"case\":\"ordinary_floor\",\"unavailable_guard_helpers\":6}");
    check_collection();
    check_dispatch();
    /* id_init and its registered callbacks have process lifetime. */
    return 0;
}
