/* Native test callbacks use only Oop contexts and current Id messages. */
#include "../oop-id-v2/id.h"

static IdProtocol id;
static Oop names[23];
enum {
    AT, PUT, SEQ, INVOKE, GC, NEXT, GLOBAL_PUT, REPLACE, RECORD,
    DENY, FIXED_COUNT, COLLECT, REENTER, IMAGE_FAIL, REGISTRY_FAIL,
    LEGACY, COUNT, FIXED, REMOVE_MAIN, COLLECT_ONCE, COLLECT_EVERY, OBSERVED, LOOKUP_FORWARD
};
#define CHECK(v) do { Oop checked = (v); if (id_fault(checked)) return checked; } while (0)

static Oop send(Oop receiver, size_t selector, const Oop *args, size_t count) {
    return id.message(id.root, receiver, names[selector], args, count);
}
static Oop get(Oop context, size_t index) {
    Oop key = id_size(index);
    return send(context, AT, &key, 1);
}
static Oop put(Oop context, size_t index, Oop value) {
    Oop args[] = {id_size(index), value};
    return send(context, PUT, args, 2);
}
static size_t increment(Oop context, size_t index) {
    size_t count = id_unbox(get(context, index)) + 1;
    put(context, index, id_size(count));
    return count;
}
/* Explicitly enter the captured implementation through existing reflection. */
static Oop invoke(Oop original, Oop receiver, const Oop *args, size_t count) {
    Oop values = send(id.root, SEQ, args, count);
    CHECK(values);
    Oop invocation[] = {receiver, values};
    return send(original, INVOKE, invocation, 2);
}
/* [fault-or-value, calls] */
static Oop deny(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root; (void)receiver; (void)args; (void)count;
    increment(context, 1);
    return id_failure(get(context, 0));
}
static Oop fixed_count(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root; (void)receiver; (void)args; (void)count;
    increment(context, 1);
    return get(context, 0);
}
/* [original, GC, calls, fault-or-null, Globals-or-null, name, nil] */
static Oop collecting(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root;
    size_t calls = increment(context, 2);
    if (calls <= 8 || !(calls & (calls - 1))) {
        Oop result = send(get(context, 1), GC, 0, 0);
        CHECK(result);
    }
    Oop fault = get(context, 3);
    if (fault) return id_failure(fault);
    return invoke(get(context, 0), receiver, args, count);
}
static Oop remove_main(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    Oop values[] = {get(context, 5), get(context, 6)};
    Oop result = send(get(context, 4), GLOBAL_PUT, values, 2);
    CHECK(result);
    return collecting(root, context, receiver, args, count);
}
/* [original, GC, calls, failure-or-null] */
static Oop observed(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root;
    increment(context, 2);
    Oop failure = get(context, 3);
    if (failure) return id_failure(failure);
    Oop result = send(get(context, 1), GC, 0, 0);
    CHECK(result);
    return invoke(get(context, 0), receiver, args, count);
}
/* [original, GC, collections] */
static Oop collect_every(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root;
    increment(context, 2);
    Oop result = send(get(context, 1), GC, 0, 0);
    CHECK(result);
    return invoke(get(context, 0), receiver, args, count);
}
/* [original, GC, collections] */
static Oop collect_once(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root;
    if (get(context, 2) == id_size(0)) {
        CHECK(put(context, 2, id_size(1)));
        Oop result = send(get(context, 1), GC, 0, 0);
        CHECK(result);
    }
    return invoke(get(context, 0), receiver, args, count);
}
/* [original, unit, other-name, armed] */
static Oop reenter(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root;
    if (get(context, 3) == id_size(1)) {
        CHECK(put(context, 3, id_size(0)));
        Oop other = get(context, 2);
        Oop result = send(get(context, 1), NEXT, &other, 1);
        CHECK(result);
    }
    return invoke(get(context, 0), receiver, args, count);
}
/* [original, GC, error-or-fault, mark, calls, protected-result] */
static Oop image_fail(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root;
    Oop result = invoke(get(context, 0), receiver, args, count);
    CHECK(result);
    CHECK(put(context, 5, result));
    increment(context, 4);
    result = send(get(context, 1), GC, 0, 0);
    CHECK(result);
    result = get(context, 2);
    return get(context, 3) == id_size(1) ? id_failure(result) : result;
}
/* [original, GC, Globals, registry-name, other-owner, changed-name,
    changed-record, error, fault, mode, calls, protected-result] */
static Oop registry_fail(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root;
    size_t calls = increment(context, 10), mode = id_unbox(get(context, 9));
    if (calls > 1) return invoke(get(context, 0), receiver, args, count);
    if (mode == 5) return get(context, 7);
    if (mode == 7) return receiver;
    Oop result = invoke(get(context, 0), receiver, args, count);
    CHECK(result);
    if (result != receiver) return result;
    CHECK(put(context, 11, result));
    Oop values[] = {get(context, 6), get(context, 5)};
    result = 0;
    if (mode == 2) result = send(receiver, REPLACE, values, 2);
    if (mode == 3 || mode == 6) result = send(receiver, RECORD, values, 2);
    if (mode == 4) {
        Oop pair[] = {get(context, 3), get(context, 4)};
        result = send(get(context, 2), GLOBAL_PUT, pair, 2);
    }
    CHECK(result);
    result = send(get(context, 1), GC, 0, 0);
    CHECK(result);
    return mode == 6 ? get(context, 11)
        : (mode == 1 ? id_failure(get(context, 8)) : get(context, 7));
}
/* [original, legacy, calls] */
static Oop legacy(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root; (void)args; (void)count;
    increment(context, 2);
    Oop value = get(context, 1);
    return invoke(get(context, 0), receiver, &value, 1);
}
/* [original, calls] */
static Oop counted(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root;
    increment(context, 1);
    return invoke(get(context, 0), receiver, args, count);
}
static Oop fixed(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root; (void)receiver; (void)args; (void)count;
    return context;
}
/* [original lookup alias, calls]. The alias is installed on the custom
   metavtable. Forward the existing argument slice without constructing a
   Sequence or sending to the Id receiver. */
static Oop lookup_forward(Oop root, Oop context, Oop receiver, const Oop *args, size_t count) {
    (void)root;
    if (count != 1) return id.invalid;
    increment(context, 1);
    Oop alias = get(context, 0);
    CHECK(alias);
    return id.message(id.root, receiver, alias, args, count);
}
static Oop initialize(const IdProtocol *protocol, Oop image, const Oop *symbols,
                      size_t symbol_count, const Oop *args, size_t count) {
    (void)args;
    if (count || symbol_count != 23) return protocol->invalid;
    id = *protocol;
    for (size_t i = 0; i < symbol_count; i++) names[i] = symbols[i];
    return image;
}
static const IdLiteral literals[] = {
    LIT("at:"), LIT("at:put:"), LIT("sequence"), LIT("invokeOn:arguments:"),
    LIT("collectPrecisely"), LIT("nextGenerationFor:"), LIT("at:put:"),
    LIT("replaceCompiledSource:named:"), LIT("recordCompiledSource:named:"),
    LIT("deny"), LIT("fixedCount"), LIT("collecting"), LIT("reenter"),
    LIT("imageFailure"), LIT("registryFailure"), LIT("legacySource"),
    LIT("counted"), LIT("fixed"), LIT("removeMain"), LIT("collectOnce"), LIT("collectEvery"), LIT("observed"), LIT("lookupForward")
};
static const IdMethodSpec methods[] = {
    {DENY, deny}, {FIXED_COUNT, fixed_count}, {COLLECT, collecting},
    {REENTER, reenter}, {IMAGE_FAIL, image_fail}, {REGISTRY_FAIL, registry_fail},
    {LEGACY, legacy}, {COUNT, counted}, {FIXED, fixed}, {REMOVE_MAIN, remove_main},
    {COLLECT_ONCE, collect_once}, {COLLECT_EVERY, collect_every}, {OBSERVED, observed}, {LOOKUP_FORWARD, lookup_forward}
};
const IdModule birnam_native_module_v2 = {
    2, sizeof(IdModule), literals, 23, methods, 14, initialize
};
