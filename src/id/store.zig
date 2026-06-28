//! src/id/store.zig — rung 2: the cell / relational store, a module over libid.
//!
//! See `docs/33-rung-2-cells.md`. This is the FIRST superimposition over the
//! libid floor: object state becomes **cells** (get/assert over a historied
//! triple store), and the store is itself a libid object the siphon loads
//! through the id API (the `counter` module is the loading template).
//!
//! The bootstrap is the **raw-append → seed schema → flip** fixpoint — libid's
//! own `bind` trick one rung up. The store has one irreducible primitive,
//! `rawAssert` (append a triple, consult nothing); the schema is seeded *as
//! data* while the rule-engine is off, **including the self-referential knot**
//! `[cardinality cardinality one]`; then `flip` makes the store schema-aware.
//! From there every `assert` consults `currentValue(a, :cardinality)` — a fact
//! the store already holds. The relational model now models itself. Only
//! `rawAssert` and `flip` are Zig-hardcoded; the schema is data.
//!
//! libid (rung 1) is UNTOUCHED by this module: `bind`/`send`/`allocate`/
//! `lookup`/`addMethod` are not modified. Everything here is built through the
//! `IdApi`.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

/// Whether a fact asserts a value or retracts one (Datomic-style op-tagging).
const Op = enum(u8) { assert, retract };

/// A triple plus its op and the transaction it landed in. The log is append-only
/// for history-on attributes (assert + retract entries fold on read); for
/// history-off attributes the log holds only currently-live asserted facts (a
/// retract physically removes, a `:one` re-assert replaces in place — no growth).
const Fact = struct { e: Oop, a: Oop, v: Oop, tx: u64, op: Op = .assert };

/// The store's private backing — held in the Store libid object's payload (the
/// bytes at `object[-1]+0`, i.e. at the object pointer itself). Consistent with
/// libid keeping its method-dict backing in Zig arrays: reflectivity lives in
/// the API (any fact, incl. schema, is queryable), not in making each triple a
/// separately-dispatched object.
const StoreState = struct {
    facts: std.ArrayList(Fact) = .{},
    raw: bool = true,
    tx: u64 = 0,
};

// -- Well-known objects + selectors (set by `bringUp`) ----------------------

pub var store_vt: Oop = null; // the Store vtable (a proto delegating to Object)
pub var store_instance: Oop = null; // the one canonical seeded store
pub var cell_vt: Oop = null; // the Cell vtable (a proto delegating to Object)

pub var s_rawAssert: Oop = null;
pub var s_flip: Oop = null;
pub var s_assert: Oop = null;
pub var s_currentValue: Oop = null;
pub var s_count: Oop = null;
pub var s_history_read: Oop = null; // the `history:attr:` read selector
pub var s_cellFor: Oop = null; // `[store cellFor: e attr: a]`
pub var s_value: Oop = null; // `[cell value]`
pub var s_set: Oop = null; // `[cell set: v]`
pub var s_addMethodOn: Oop = null; // `[store addMethod: sel on: vt with: m]`
pub var s_retract: Oop = null; // `[store retract: e attr: a value: v]`
pub var s_new: Oop = null; // `[store new]`
pub var s_attrCount: Oop = null; // `[store attrCount: e]`
pub var s_attrAt: Oop = null; // `[store attrAt: e index: i]`

/// The api table, stashed so methods (which receive only self+args) can drive
/// the store via real dispatch. Set by `bringUp`.
var g_api: *idapi.IdApi = undefined;

// Schema symbols (the vocabulary the store describes itself with).
pub var s_cardinality: Oop = null;
pub var s_one: Oop = null;
pub var s_many: Oop = null;
pub var s_name: Oop = null;
pub var s_history: Oop = null; // the history-policy ATTRIBUTE
pub var s_true: Oop = null;
pub var s_false: Oop = null;

// -- Payload access + the append-only log -----------------------------------

fn stateOf(self: Oop) *StoreState {
    return @ptrCast(@alignCast(self.?));
}

fn appendOp(st: *StoreState, e: Oop, a: Oop, v: Oop, op: Op) void {
    st.facts.append(ca, .{ .e = e, .a = a, .v = v, .tx = st.tx, .op = op }) catch @panic("store: oom");
    st.tx += 1;
}
fn appendFact(st: *StoreState, e: Oop, a: Oop, v: Oop) void {
    appendOp(st, e, a, v, .assert);
}

/// Physically drop currently-live asserted facts matching (e, a, v) — the
/// history-off retention path (no retract entry lingers, no growth).
fn removeLive(st: *StoreState, e: Oop, a: Oop, v: Oop) void {
    var i: usize = 0;
    while (i < st.facts.items.len) {
        const f = st.facts.items[i];
        if (f.e == e and f.a == a and f.v == v and f.op == .assert) {
            _ = st.facts.orderedRemove(i);
        } else i += 1;
    }
}

/// Is value `v` currently a live member of (e, a)? The newest matching (e,a,v)
/// fact decides (assert → present, retract → absent). Op-aware.
fn isMember(st: *StoreState, e: Oop, a: Oop, v: Oop) bool {
    var i = st.facts.items.len;
    while (i > 0) {
        i -= 1;
        const f = st.facts.items[i];
        if (f.e == e and f.a == a and f.v == v) return f.op == .assert;
    }
    return false;
}

/// Newest-first scan for the current value of (e, a); null if none/retracted.
/// The newest matching fact decides: a retract entry means the value is absent.
fn currentValueRaw(st: *StoreState, e: Oop, a: Oop) Oop {
    var i = st.facts.items.len;
    while (i > 0) {
        i -= 1;
        const f = st.facts.items[i];
        if (f.e == e and f.a == a) return if (f.op == .retract) null else f.v;
    }
    return null;
}

/// The cardinality of attribute `a`, read from the store's OWN schema cells.
/// Defaults to `:one` when unseeded. This is the self-consultation: reading the
/// cardinality of `:cardinality` itself terminates on the seeded knot, not in a
/// regress (`docs/33` §bootstrap).
fn cardinalityOf(st: *StoreState, a: Oop) Oop {
    const c = currentValueRaw(st, a, s_cardinality);
    return if (c == null) s_one else c;
}

/// Whether attribute `a` retains a timeline — read from the store's own schema.
/// **Default OFF** (`docs/33` D6, the 4 GB lesson): a no-history `:one` attr
/// keeps exactly one slot, so a hot-updated value never balloons the log.
fn historyOf(st: *StoreState, a: Oop) Oop {
    const h = currentValueRaw(st, a, s_history);
    return if (h == null) s_false else h;
}

/// Number of facts the log currently holds for (e, a) — the timeline length.
fn historyLen(st: *StoreState, e: Oop, a: Oop) usize {
    var c: usize = 0;
    for (st.facts.items) |f| {
        if (f.e == e and f.a == a) c += 1;
    }
    return c;
}

/// Overwrite the value of the newest (e, a) fact in place (no growth); append if
/// none exists. The no-history retention path for `:one`.
fn replaceInPlace(st: *StoreState, e: Oop, a: Oop, v: Oop) void {
    var i = st.facts.items.len;
    while (i > 0) {
        i -= 1;
        if (st.facts.items[i].e == e and st.facts.items[i].a == a) {
            st.facts.items[i].v = v;
            st.facts.items[i].tx = st.tx;
            st.tx += 1;
            return;
        }
    }
    appendFact(st, e, a, v);
}

/// Distinct currently-present values of (e, a) — the set, for `:many` counting.
/// Op-aware: a value counts iff its NEWEST (e,a,v) fact is an assert, so a
/// retracted-then-not-reasserted member drops out of the set.
fn valueCount(st: *StoreState, e: Oop, a: Oop) usize {
    var seen: std.ArrayList(Oop) = .{};
    defer seen.deinit(ca);
    var present: usize = 0;
    var i = st.facts.items.len;
    while (i > 0) {
        i -= 1;
        const f = st.facts.items[i];
        if (f.e != e or f.a != a) continue;
        var dup = false;
        for (seen.items) |s| {
            if (s == f.v) {
                dup = true;
                break;
            }
        }
        if (dup) continue; // an older entry for this value — newest already decided
        seen.append(ca, f.v) catch @panic("store: oom");
        if (f.op == .assert) present += 1;
    }
    return present;
}

// -- The methods (installed on store_vt via api.method) ----------------------

/// `[store rawAssert: e attr: a value: v]` — the one irreducible primitive:
/// append a triple, consult nothing. The bootstrap floor.
fn mRawAssert(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    appendFact(stateOf(self), args[0], args[1], args[2]);
    return self;
}

/// `[store flip]` — leave raw mode; become schema-aware. The single switch.
fn mFlip(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    stateOf(self).raw = false;
    return self;
}

/// `[store assert: e attr: a value: v]` — schema-aware. Consults the store's own
/// `currentValue(a, :cardinality)` to place the value: `:one` supersedes
/// (idempotent if unchanged); `:many` accumulates as a set. In raw mode this is
/// just an append (the bootstrap uses `rawAssert` directly, but assert stays
/// safe pre-flip).
fn mAssert(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const st = stateOf(self);
    const e = args[0];
    const a = args[1];
    const v = args[2];
    if (st.raw) {
        appendFact(st, e, a, v);
        return self;
    }
    const card = cardinalityOf(st, a); // the self-consultation
    if (card == s_one) {
        if (currentValueRaw(st, e, a) == v) return self; // idempotent: no value change
        if (historyOf(st, a) == s_true)
            appendFact(st, e, a, v) // opt-in: keep the timeline
        else
            replaceInPlace(st, e, a, v); // default: one slot, no growth (D6)
    } else {
        // :many — set semantics; accumulate distinct members (op-aware: a
        // previously-retracted value can be re-added).
        if (isMember(st, e, a, v)) return self; // already in the set
        appendFact(st, e, a, v);
    }
    return self;
}

/// `[store retract: e attr: a value: v]` — remove v from (e, a). History-on
/// attributes append a retract entry (the add/remove timeline is kept); history-
/// off attributes physically drop the live fact (no growth). Unblocks `:many`
/// history and gives `:one` removal.
fn mRetract(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const st = stateOf(self);
    const e = args[0];
    const a = args[1];
    const v = args[2];
    if (!isMember(st, e, a, v)) return self; // retracting an absent value is a no-op
    if (!st.raw and historyOf(st, a) == s_true)
        appendOp(st, e, a, v, .retract) // keep the timeline
    else
        removeLive(st, e, a, v); // default: drop the live fact, no growth
    return self;
}

/// `[store currentValue: e attr: a]` — the latest value of (e, a), or nil.
fn mCurrentValue(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return currentValueRaw(stateOf(self), args[0], args[1]);
}

/// `[store count: e attr: a]` — how many values (e, a) currently holds, pointer-
/// encoded (libid is pre-SmallInt). `:one` → 0 or 1; `:many` → set size.
fn mCount(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const st = stateOf(self);
    const e = args[0];
    const a = args[1];
    const c: usize = if (cardinalityOf(st, a) == s_one)
        (if (currentValueRaw(st, e, a) == null) @as(usize, 0) else 1)
    else
        valueCount(st, e, a);
    return @ptrFromInt(c);
}

/// `[store history: e attr: a]` — the timeline length for (e, a), pointer-
/// encoded. With history off (default) a `:one` attr stays at 1 however many
/// times it is asserted; opted in (`[a :history true]`) it grows. The observable
/// proof that history is opt-in (D6).
fn mHistory(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return @ptrFromInt(historyLen(stateOf(self), args[0], args[1]));
}

// -- Query surface: entity introspection (docs/33 §2b, open item) ------------
//
// `attrCount:` / `attrAt:index:` enumerate the attributes an entity currently
// holds (first-appearance order, present-filtered). With no collection type or
// blocks yet (those arrive with peg/compiler), this count+indexed-accessor pair
// is the poor-man's iteration. It is GENERAL: over an app entity it lists its
// attributes; over a tracked vtable it lists that type's mirrored selectors —
// one mechanism, both reflective questions.

fn collectAttrs(st: *StoreState, e: Oop, out: *std.ArrayList(Oop)) void {
    for (st.facts.items) |f| {
        if (f.e != e) continue;
        var dup = false;
        for (out.items) |s| {
            if (s == f.a) {
                dup = true;
                break;
            }
        }
        if (!dup) out.append(ca, f.a) catch @panic("store: oom");
    }
    // Keep only attributes that currently hold a value (drop fully-retracted).
    var i: usize = 0;
    while (i < out.items.len) {
        const a = out.items[i];
        const present = if (cardinalityOf(st, a) == s_one)
            currentValueRaw(st, e, a) != null
        else
            valueCount(st, e, a) > 0;
        if (!present) _ = out.orderedRemove(i) else i += 1;
    }
}

/// `[store attrCount: e]` — how many distinct attributes `e` currently holds.
fn mAttrCount(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var out: std.ArrayList(Oop) = .{};
    defer out.deinit(ca);
    collectAttrs(stateOf(self), args[0], &out);
    return @ptrFromInt(out.items.len);
}

/// `[store attrAt: e index: i]` — the i-th attribute of `e` (pointer-encoded
/// index), or nil if out of range. Pair with `attrCount:` to iterate.
fn mAttrAt(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const idx: usize = @intFromPtr(args[1]);
    var out: std.ArrayList(Oop) = .{};
    defer out.deinit(ca);
    collectAttrs(stateOf(self), args[0], &out);
    return if (idx >= out.items.len) null else out.items[idx];
}

// -- Cells: object state as a handle (docs/33 §2a.3, D5 stratification) -------
//
// A Cell is a separate libid object addressing one (store, e, a). The stateful
// protocol (value/set:/history) lives on the CELL — a mirror — never on the base
// entity, so an entity's own message protocol stays clean (Bracha–Ungar
// stratification). A cell drives the store through real dispatch (`g_api.send`),
// so it composes with cardinality + opt-in history exactly like a direct assert.

const CellState = struct { store: Oop, e: Oop, a: Oop };

fn cellStateOf(self: Oop) *CellState {
    return @ptrCast(@alignCast(self.?));
}

/// `[store cellFor: e attr: a]` — mint a cell handle onto (this store, e, a).
fn mCellFor(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const cell = g_api.alloc(cell_vt, @sizeOf(CellState));
    cellStateOf(cell).* = .{ .store = self, .e = args[0], .a = args[1] };
    return cell;
}

/// `[cell value]` — the cell's current value (`currentValue(e, a)` on its store).
fn mCellValue(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    const c = cellStateOf(self);
    return g_api.send(c.store, s_currentValue, &[_]Oop{ c.e, c.a }, 2);
}

/// `[cell set: v]` — assert v at (e, a) (schema-aware: cardinality + history).
fn mCellSet(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const c = cellStateOf(self);
    _ = g_api.send(c.store, s_assert, &[_]Oop{ c.e, c.a, args[0] }, 3);
    return self;
}

/// `[cell history]` — the cell's timeline length (pointer-encoded).
fn mCellHistory(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    const c = cellStateOf(self);
    return g_api.send(c.store, s_history_read, &[_]Oop{ c.e, c.a }, 2);
}

// -- The dispatch mirror (docs/33 §2b.1, D3/D4) ------------------------------
//
// `[store addMethod: sel on: vt with: m]` is the OPT-IN tracked install: it
// writes the vtable dict (the authoritative, fast path) AND projects the fact
// `[vt sel m]` — one write, two representations (the JIT shape: the fact is a
// projection of the dict, not a second authority). So a method install becomes
// queryable/historied: `currentValue(vt, sel)` is the bound method;
// `[sel :history true]` makes method changes a timeline (hot-reload visibility).
//
// Untracked types install via plain `[vt addMethod: sel with: m]` (libid, D4) —
// invisible to the store, the default escape hatch. The root metavtable is
// NEVER patched; methods stay bare fn pointers (no per-method objects, D3).

fn mTrackedAddMethod(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const sel = args[0];
    const vt = args[1];
    const m = args[2];
    g_api.method(vt, sel, @ptrCast(@alignCast(m.?))); // 1. authoritative dict
    _ = g_api.send(self, s_assert, &[_]Oop{ vt, sel, m }, 3); // 2. derived fact
    return self;
}

/// `[store new]` — mint a fresh, independently-seeded store (a sibling, same
/// vtable). The siphon mints one well-known instance; this makes the store
/// first-class mintable (docs/33 D8). Prototype-style: an instance makes more of
/// its own kind, no class involved.
fn mNew(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = args;
    _ = n;
    return freshStore();
}

// -- Bringup + the schema seed ----------------------------------------------

/// Wire the store onto libid through the id API: intern the vocabulary, build
/// (or re-import) the Store vtable, (re)install its methods, and — on first
/// load — mint the one canonical instance, seed the schema, and export both.
///
/// Hot-reload safe: methods are re-installed every load (fresh fn pointers for
/// the freshly-loaded dylib), but the instance + its StoreState persist via the
/// id environment (`import`/`export`) — the store IS a libid object, so the id
/// namespace is its natural persistence, not the opaque module-state channel.
pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    s_cardinality = api.intern("cardinality");
    s_one = api.intern("one");
    s_many = api.intern("many");
    s_name = api.intern("name");
    s_history = api.intern("history");
    s_true = api.intern("true");
    s_false = api.intern("false");

    s_rawAssert = api.intern("rawAssert:attr:value:");
    s_flip = api.intern("flip");
    s_assert = api.intern("assert:attr:value:");
    s_currentValue = api.intern("currentValue:attr:");
    s_count = api.intern("count:attr:");
    s_history_read = api.intern("history:attr:");
    s_cellFor = api.intern("cellFor:attr:");
    s_value = api.intern("value");
    s_set = api.intern("set:");
    s_addMethodOn = api.intern("addMethod:on:with:");
    s_retract = api.intern("retract:attr:value:");
    s_new = api.intern("new");
    s_attrCount = api.intern("attrCount:");
    s_attrAt = api.intern("attrAt:index:");

    const reloading = api.isExported("Store") != 0;
    if (reloading) {
        store_vt = api.import("StoreVt");
        store_instance = api.import("Store");
        cell_vt = api.import("CellVt");
    } else {
        store_vt = api.proto(api.object_vt);
        cell_vt = api.proto(api.object_vt);
    }

    api.method(store_vt, s_rawAssert, &mRawAssert);
    api.method(store_vt, s_flip, &mFlip);
    api.method(store_vt, s_assert, &mAssert);
    api.method(store_vt, s_currentValue, &mCurrentValue);
    api.method(store_vt, s_count, &mCount);
    api.method(store_vt, s_history_read, &mHistory);
    api.method(store_vt, s_cellFor, &mCellFor);
    api.method(store_vt, s_addMethodOn, &mTrackedAddMethod);
    api.method(store_vt, s_retract, &mRetract);
    api.method(store_vt, s_new, &mNew);
    api.method(store_vt, s_attrCount, &mAttrCount);
    api.method(store_vt, s_attrAt, &mAttrAt);

    // Cell protocol (the `history` selector reuses the "history" symbol — one
    // identity, two roles: a cell message here, a schema attribute key above).
    api.method(cell_vt, s_value, &mCellValue);
    api.method(cell_vt, s_set, &mCellSet);
    api.method(cell_vt, s_history, &mCellHistory);

    if (!reloading) {
        store_instance = freshStore();
        _ = api.exportObj("StoreVt", store_vt);
        _ = api.exportObj("Store", store_instance);
        _ = api.exportObj("CellVt", cell_vt);
    }
}

/// Allocate a fresh store instance, initialize its state, and seed it. Used both
/// for the well-known instance (bringUp) and for `[store new]`.
fn freshStore() Oop {
    const s = g_api.alloc(store_vt, @sizeOf(StoreState));
    stateOf(s).* = .{};
    seedInto(s);
    return s;
}

/// Raw-assert the schema as data (rule-engine off), then flip → schema-aware.
/// The cardinality knot `[cardinality cardinality one]` unties the regress: it
/// lets `cardinalityOf(:cardinality)` terminate on a held fact. Only `rawAssert`
/// (here, `appendFact`) and the `flip` (here, `raw = false`) are hardcoded.
fn seedInto(s: Oop) void {
    const st = stateOf(s);
    appendFact(st, s_cardinality, s_cardinality, s_one); // ← the knot
    appendFact(st, s_name, s_cardinality, s_one);
    appendFact(st, s_history, s_cardinality, s_one);
    st.raw = false; // flip
}

/// The reloadable-module entrypoint (cf. `idapi.ModuleInitFn`). The opaque
/// `state` channel is unused: the store persists as a libid object via the id
/// environment, not as foreign state. Returns null (nothing to thread).
pub export fn module_init(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}
