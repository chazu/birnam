/* String callbacks use owner messages; weak observations retain identities only. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop names[15], identities[4];
static size_t watched, alive, finalized, violations;
enum { AT, PUT, COLLECT, FROM, IDENTITY, ALLOCATIONS, ALLOCATION, WATCH,
       REMAINING, REPLACEMENT, ALLOCATING, FINALIZE, FINALIZED, VIOLATIONS,
       COUNTED };
#define CHECK(v) do { Oop checked = (v); if (id_fault(checked)) return checked; } while (0)
static Oop send(Oop r, size_t s, const Oop *a, size_t n) {
    return id.message(id.root, r, names[s], a, n);
}
static Oop get(Oop c, size_t i) { Oop k = id_size(i); return send(c, AT, &k, 1); }
static Oop increment(Oop c, size_t i) {
    Oop pair[] = {id_size(i), id_size(id_unbox(get(c, i)) + 1)};
    return send(c, PUT, pair, 2);
}
/* [GC, answer] */
static Oop replacement(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root; (void)r; (void)a; (void)n;
    CHECK(send(get(c, 0), COLLECT, 0, 0)); return get(c, 1);
}
/* [GC, original allocation alias, mutable byte Sequence, calls, StringVt] */
static Oop allocating(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root;
    if (r != get(c, 4)) return id.message(id.root, r, get(c, 1), a, n);
    CHECK(increment(c, 3));
    Oop pair[] = {id_size(0), id_size('z')};
    CHECK(send(get(c, 2), PUT, pair, 2));
    CHECK(send(get(c, 0), COLLECT, 0, 0));
    return id.message(id.root, r, get(c, 1), a, n);
}
/* [GC, String, existing byte provider, original finalization alias] */
static Oop finalize(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root;
    if (n != 1) return id.invalid;
    finalized++;
    Oop provider = get(c, 2);
    if (!id_fault(send(get(c, 1), FROM, &provider, 1))) violations++;
    if (send(get(c, 0), COLLECT, 0, 0) != id_size(0)) violations++;
    /* Forward without constructing an argument container during finalization. */
    Oop result = id.message(id.root, r, get(c, 3), a, n);
    if (id_fault(result)) violations++;
    return result;
}
static Oop final_count(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root; (void)c; (void)r; (void)a;
    return n ? id.invalid : id_size(finalized);
}
static Oop violation_count(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root; (void)c; (void)r; (void)a;
    return n ? id.invalid : id_size(violations);
}
static Oop watch(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root; (void)c; (void)r;
    if (n > 4) return id.invalid;
    watched = n;
    for (size_t i = 0; i < n; i++) {
        identities[i] = send(id.root, IDENTITY, &a[i], 1); CHECK(identities[i]);
    }
    return 0;
}
static Oop allocation(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root; (void)c; (void)r;
    if (n != 3) return id.invalid;
    for (size_t i = 0; i < watched; i++) if (a[2] == identities[i]) alive++;
    return 0;
}
/* Context is the allocation visitor. No observed Oop is used after reclamation. */
static Oop remaining(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root; (void)r; (void)a;
    if (n) return id.invalid;
    alive = 0; CHECK(send(id.root, ALLOCATIONS, &c, 1)); return id_size(alive);
}
/* [original selector alias, calls] */
static Oop counted(Oop root, Oop c, Oop r, const Oop *a, size_t n) {
    (void)root; CHECK(increment(c, 1));
    return id.message(id.root, r, get(c, 0), a, n);
}
static Oop initialize(const IdProtocol *p, Oop image, const Oop *symbols,
                      size_t n, const Oop *a, size_t count) {
    (void)a; if (count || n != 15) return p->invalid;
    id = *p; for (size_t i = 0; i < n; i++) names[i] = symbols[i]; return image;
}
static const IdLiteral literals[] = {
    LIT("at:"), LIT("at:put:"), LIT("collectPrecisely"), LIT("fromBytes:"),
    LIT("identityOf:"), LIT("allocationsDo:"), LIT("allocation:bytes:identity:"),
    LIT("watch"), LIT("remaining"), LIT("replacement"), LIT("allocating"),
    LIT("finalize"), LIT("finalized"), LIT("violations"), LIT("counted")
};
static const IdMethodSpec methods[] = {
    {ALLOCATION, allocation}, {WATCH, watch}, {REMAINING, remaining},
    {REPLACEMENT, replacement}, {ALLOCATING, allocating}, {FINALIZE, finalize},
    {FINALIZED, final_count}, {VIOLATIONS, violation_count}, {COUNTED, counted}
};
const IdModule birnam_native_module_v2 = {2, sizeof(IdModule), literals, 15, methods, 9, initialize};
