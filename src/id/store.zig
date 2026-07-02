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

// The Globals scope (docs/36): birnam's program namespace, a thin facade over
// the well-known store instance keyed on one reserved entity. Cells for free:
// globals get fact storage + opt-in history, queryable, with no new store core.
pub var globals_vt: Oop = null;
pub var globals_obj: Oop = null; // exported as "Globals"
var g_globals_entity: Oop = null; // the reserved entity all globals hang off of

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
var s_value3: Oop = null; // "value:value:value:" — the liveTriplesDo: block call
pub var s_true: Oop = null;
pub var s_false: Oop = null;

// -- Payload access + the append-only log -----------------------------------

fn stateOf(self: Oop) *StoreState {
    return @ptrCast(@alignCast(self.?));
}

/// The store's identity chokepoint (F6 / docs/34): "are these the same id/value?"
/// Pointer identity today — and the single seam where an `identityOf:` /
/// candidate-key policy will later slot in. ALL entity/attr/value matching routes
/// through here, so the policy lands in one place, not a scatter of inline `==`.
/// (Absence checks — `x == null` — stay literal: absence is not a value, F1.)
inline fn eq(a: Oop, b: Oop) bool {
    return a == b;
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
        if (eq(f.e, e) and eq(f.a, a) and eq(f.v, v) and f.op == .assert) {
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
        if (eq(f.e, e) and eq(f.a, a) and eq(f.v, v)) return f.op == .assert;
    }
    return false;
}

/// Newest-first scan for the value of (e, a) AS OF a basis watermark: only facts
/// with `tx < asof` are visible (`asof == null` = all = the current value; `asof`
/// is a `[store basis]` value = the next-tx watermark, so the snapshot excludes
/// anything committed at/after it). The newest visible matching fact decides; a
/// retract entry means the value is absent.
fn currentValueRawAsOf(st: *StoreState, e: Oop, a: Oop, asof: ?u64) Oop {
    var i = st.facts.items.len;
    while (i > 0) {
        i -= 1;
        const f = st.facts.items[i];
        if (asof) |t| if (f.tx >= t) continue; // asof is a watermark: visible iff tx < asof
        if (eq(f.e, e) and eq(f.a, a)) return if (f.op == .retract) null else f.v;
    }
    return null;
}

/// The current value of (e, a); null if none/retracted.
fn currentValueRaw(st: *StoreState, e: Oop, a: Oop) Oop {
    return currentValueRawAsOf(st, e, a, null);
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
        if (!eq(f.e, e) or !eq(f.a, a)) continue;
        var dup = false;
        for (seen.items) |s| {
            if (eq(s, f.v)) {
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
    if (eq(card, s_one)) {
        if (eq(currentValueRaw(st, e, a), v)) return self; // idempotent: no value change
        if (eq(historyOf(st, a), s_true))
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
    if (!st.raw and eq(historyOf(st, a), s_true))
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
    const c: usize = if (eq(cardinalityOf(st, a), s_one))
        (if (currentValueRaw(st, e, a) == null) @as(usize, 0) else 1)
    else
        valueCount(st, e, a);
    return g_api.mkSmallInt(@intCast(c));
}

/// `[store history: e attr: a]` — the timeline length for (e, a), pointer-
/// encoded. With history off (default) a `:one` attr stays at 1 however many
/// times it is asserted; opted in (`[a :history true]`) it grows. The observable
/// proof that history is opt-in (D6).
fn mHistory(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return g_api.mkSmallInt(@intCast(historyLen(stateOf(self), args[0], args[1])));
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
        if (!eq(f.e, e)) continue;
        var dup = false;
        for (out.items) |s| {
            if (eq(s, f.a)) {
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
        const present = if (eq(cardinalityOf(st, a), s_one))
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
    return g_api.mkSmallInt(@intCast(out.items.len));
}

/// `[store attrAt: e index: i]` — the i-th attribute of `e` (`i` a SmallInt), or
/// nil if out of range. Pair with `attrCount:` to iterate.
fn mAttrAt(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const idx: usize = @intCast(g_api.smallIntValue(args[1]));
    var out: std.ArrayList(Oop) = .{};
    defer out.deinit(ca);
    collectAttrs(stateOf(self), args[0], &out);
    return if (idx >= out.items.len) null else out.items[idx];
}

// -- The relational seam: enumerate LIVE triples (docs/33; the query side) ----
//
// `[store liveTriplesDo: aBlock]` sends `value:value:value:` to `aBlock` once per
// live (entity, attr, value) triple — the CURRENT state, post-retraction and
// cardinality-resolved (one current value for :one attrs; each distinct live
// member for :many). This is the single primitive the relational engine
// (a sibling module) scans to run datalog joins: it owns the store's liveness
// semantics here, so the engine stays representation-free. Schema facts are data
// like any other, so they enumerate too (a query simply won't match them unless
// it asks). Collection-free by design — the block does any accumulation.

const EA = struct { e: Oop, a: Oop };

fn pairSeen(pairs: []const EA, e: Oop, a: Oop) bool {
    for (pairs) |p| if (eq(p.e, e) and eq(p.a, a)) return true;
    return false;
}

// A transaction WINDOW [lo, hi): `hi` is the snapshot watermark (deciding fact must
// have tx < hi; null = current), `lo` is a lower bound for emission (the deciding
// fact must have tx >= lo; null = 0). asOf = [null, hi); since = [lo, null);
// between = [lo, hi). `visibleHi` gates which facts participate; `inLo` gates which
// resulting triples are emitted (so since/between report only what CHANGED in the
// window, while the value itself still folds over the whole prefix).
fn visibleHi(tx: u64, hi: ?u64) bool {
    return if (hi) |h| tx < h else true;
}
fn inLo(tx: u64, lo: ?u64) bool {
    return if (lo) |l| tx >= l else true;
}

/// Enumerate the live triples over a transaction window (both bounds null =
/// current). Cardinality is read CURRENT (schema is treated as stable; it does not
/// time-travel); the deciding fact is the newest with `tx < hi`, and a triple is
/// emitted only if that fact is an assert AND its `tx >= lo`.
fn enumerateTriples(st: *StoreState, blk: Oop, lo: ?u64, hi: ?u64) void {
    // Distinct (e,a) pairs in first-appearance order, among facts within `hi`.
    var pairs: std.ArrayList(EA) = .{};
    defer pairs.deinit(ca);
    for (st.facts.items) |f| {
        if (!visibleHi(f.tx, hi)) continue;
        if (!pairSeen(pairs.items, f.e, f.a))
            pairs.append(ca, .{ .e = f.e, .a = f.a }) catch @panic("store: oom");
    }

    for (pairs.items) |p| {
        if (eq(cardinalityOf(st, p.a), s_one)) {
            // The deciding fact: newest matching with tx < hi.
            var i = st.facts.items.len;
            while (i > 0) {
                i -= 1;
                const f = st.facts.items[i];
                if (!visibleHi(f.tx, hi)) continue;
                if (!eq(f.e, p.e) or !eq(f.a, p.a)) continue;
                if (f.op == .assert and inLo(f.tx, lo)) emitTriple(blk, p.e, p.a, f.v);
                break; // newest matching fact decides
            }
        } else {
            // :many — each distinct value whose newest fact (within hi) is an assert.
            var seen: std.ArrayList(Oop) = .{};
            defer seen.deinit(ca);
            var i = st.facts.items.len;
            while (i > 0) {
                i -= 1;
                const f = st.facts.items[i];
                if (!visibleHi(f.tx, hi)) continue;
                if (!eq(f.e, p.e) or !eq(f.a, p.a)) continue;
                var dup = false;
                for (seen.items) |s| if (eq(s, f.v)) {
                    dup = true;
                    break;
                };
                if (dup) continue; // newest entry for this value already decided
                seen.append(ca, f.v) catch @panic("store: oom");
                if (f.op == .assert and inLo(f.tx, lo)) emitTriple(blk, p.e, p.a, f.v);
            }
        }
    }
}

fn mLiveTriplesDo(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    enumerateTriples(stateOf(self), args[0], null, null);
    return self;
}

/// `[store liveTriplesAsOf: tx do: aBlock]` — the snapshot live as of transaction
/// `tx` (a `[store basis]` watermark): the world before anything committed at/after
/// `tx`. Window [null, tx).
fn mLiveTriplesAsOfDo(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const hi: u64 = @intCast(g_api.smallIntValue(args[0]));
    enumerateTriples(stateOf(self), args[1], null, hi);
    return self;
}

/// `[store liveTriplesSince: tx do: aBlock]` — only triples whose CURRENT value was
/// asserted at/after `tx`: what has changed since that basis. Window [tx, null).
fn mLiveTriplesSinceDo(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const lo: u64 = @intCast(g_api.smallIntValue(args[0]));
    enumerateTriples(stateOf(self), args[1], lo, null);
    return self;
}

/// `[store liveTriplesBetween: lo and: hi do: aBlock]` — the snapshot at `hi`
/// restricted to triples whose deciding fact landed at/after `lo`. Window [lo, hi).
fn mLiveTriplesBetweenAndDo(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const lo: u64 = @intCast(g_api.smallIntValue(args[0]));
    const hi: u64 = @intCast(g_api.smallIntValue(args[1]));
    enumerateTriples(stateOf(self), args[2], lo, hi);
    return self;
}

/// `[store cardinality: a]` — the cardinality symbol (`#one` / `#many`) governing
/// attribute `a`, read from the store's own schema. Lets the query side render a
/// `:many` attribute as a collection in pull.
fn mCardinality(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return cardinalityOf(stateOf(self), args[0]);
}

/// `[store basis]` — the current transaction counter (a SmallInt). Capture it,
/// mutate the store, then query `asOf:` that basis to see the earlier snapshot.
fn mBasis(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    return g_api.mkSmallInt(@intCast(stateOf(self).tx));
}

fn emitTriple(blk: Oop, e: Oop, a: Oop, v: Oop) void {
    _ = g_api.send(blk, s_value3, &[_]Oop{ e, a, v }, 3);
}

// -- GC trace hook (F3) ------------------------------------------------------
//
// A store keeps its facts in a `c_allocator` ArrayList — Oop-bearing memory the
// conservative collector cannot reach by tracing the store's (opaque) payload. A
// per-vtable trace hook (installed on `store_vt`) marks every live fact's e/a/v
// when the collector traces a reachable store, so entities/values held only by
// the store survive — for ANY store instance (the singleton `db`/`Globals` AND
// every `[Store new]`), since the hook fires per traced instance.
fn gcTraceHook(self: Oop, mark: idapi.MarkFn) callconv(.c) void {
    const st = stateOf(self);
    for (st.facts.items) |f| {
        mark(f.e);
        mark(f.a);
        mark(f.v);
    }
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

// -- The Globals scope (docs/36) --------------------------------------------
//
// A namespace is just an object you send lookup messages to. `Globals` forwards
// to the well-known store on the reserved `g_globals_entity`, so a global binding
// IS a cell: `at:put:` asserts, `at:` reads currentValue, both inheriting the
// store's opt-in history + queryability. No new store core.

/// `[Globals at: aSymbol]` — the current value bound to `aSymbol`, or nil.
/// Normalises the store's internal absence (Zig null) to the nil singleton, so
/// birnam code never sees a raw null.
fn mGlobalsAt(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const cur = g_api.send(store_instance, s_currentValue, &[_]Oop{ g_globals_entity, args[0] }, 2);
    return if (cur == null) g_api.nilObj else cur;
}

/// `[Globals at: aSymbol put: aValue]` — bind (assert) and return the value.
fn mGlobalsAtPut(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    _ = g_api.send(store_instance, s_assert, &[_]Oop{ g_globals_entity, args[0], args[1] }, 3);
    return args[1];
}

/// `[Globals includes: aSymbol]` — true if a non-nil binding exists. (v1 treats
/// absent and explicitly-nil the same; a real Binding object distinguishes them
/// in v2 — docs/36 §3.)
fn mGlobalsIncludes(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const cur = g_api.send(store_instance, s_currentValue, &[_]Oop{ g_globals_entity, args[0] }, 2);
    return if (cur != null) g_api.trueObj else g_api.falseObj;
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

// -- Image serialization (docs/40 slice 3, step 2) --------------------------
//
// The store owns its facts, so it owns their on-disk form: the `.facts` section
// the `image` module frames into an image file. Step 2 serializes every fact
// whose e/a/v are all SELF-DESCRIBING LEAF values — nil/true/false, smallint,
// float, symbol, string — verbatim, the timeline included (per-fact tx + op,
// plus the store's `tx` counter and `raw` flag). A fact with a COMPOUND heap
// value (a vector, map, or user object) cannot be represented until the heap
// section (step 3): it is SKIPPED and COUNTED, never silently corrupted
// (docs/40 §6 — the attic rule, counted-never-dropped). Load REPLACES the
// freshly-seeded fact list wholesale — the saved stream already carries the
// deterministically-seeded schema, so there is no double-seed.

const FieldTag = enum(u8) { nil = 0, true_ = 1, false_ = 2, int = 3, float = 4, symbol = 5, string = 6 };
const Bytes = std.ArrayListUnmanaged(u8);

var g_last_skipped: u32 = 0; // facts skipped by the most recent dump (compound values)

fn wU32(b: *Bytes, v: u32) void {
    var t: [4]u8 = undefined;
    std.mem.writeInt(u32, &t, v, .little);
    b.appendSlice(ca, &t) catch @panic("store: oom");
}
fn wU64(b: *Bytes, v: u64) void {
    var t: [8]u8 = undefined;
    std.mem.writeInt(u64, &t, v, .little);
    b.appendSlice(ca, &t) catch @panic("store: oom");
}
fn wBytes(b: *Bytes, s: []const u8) void {
    wU32(b, @intCast(s.len));
    b.appendSlice(ca, s) catch @panic("store: oom");
}

/// Encode one leaf Oop field into `b`; returns false (having written a partial,
/// to-be-discarded scratch) when the value is a compound heap object the step-2
/// codec can't represent — the caller then skips the whole fact.
fn encodeField(b: *Bytes, o: Oop) bool {
    if (o == g_api.nilObj) {
        b.append(ca, @intFromEnum(FieldTag.nil)) catch @panic("store: oom");
        return true;
    }
    if (o == g_api.trueObj) {
        b.append(ca, @intFromEnum(FieldTag.true_)) catch @panic("store: oom");
        return true;
    }
    if (o == g_api.falseObj) {
        b.append(ca, @intFromEnum(FieldTag.false_)) catch @panic("store: oom");
        return true;
    }
    if (g_api.isSmallInt(o) != 0) {
        b.append(ca, @intFromEnum(FieldTag.int)) catch @panic("store: oom");
        wU64(b, @bitCast(g_api.smallIntValue(o)));
        return true;
    }
    if (g_api.isFloat(o) != 0) {
        b.append(ca, @intFromEnum(FieldTag.float)) catch @panic("store: oom");
        wU64(b, @bitCast(g_api.floatValue(o)));
        return true;
    }
    const vt = g_api.vtableOf(o);
    if (vt == g_api.symbol_vt) {
        b.append(ca, @intFromEnum(FieldTag.symbol)) catch @panic("store: oom");
        wBytes(b, std.mem.span(g_api.symbolName(o)));
        return true;
    }
    if (vt == g_api.string_vt) {
        b.append(ca, @intFromEnum(FieldTag.string)) catch @panic("store: oom");
        var len: usize = 0;
        const p = g_api.stringBytes(o, &len);
        wBytes(b, p[0..len]);
        return true;
    }
    return false; // compound heap object → deferred to the heap section (step 3)
}

/// `[Store dumpImageSection]` — serialize this store's fact stream + counters to
/// a String (the image's `.facts` section). Section layout:
///   u64 tx · u8 raw · u32 kept · u32 skipped · kept×(u8 op · u64 tx · e · a · v)
/// where each e/a/v is a tagged leaf field. `[Store imageSkippedCount]` reports
/// how many facts the most recent dump deferred.
fn mDumpImageSection(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    const st = stateOf(self);
    var recs: Bytes = .{};
    defer recs.deinit(ca);
    var kept: u32 = 0;
    var skipped: u32 = 0;
    for (st.facts.items) |f| {
        var scratch: Bytes = .{};
        defer scratch.deinit(ca);
        scratch.append(ca, @intFromEnum(f.op)) catch @panic("store: oom");
        wU64(&scratch, f.tx);
        // Commit only if ALL THREE fields are leaf-encodable (partial scratch is
        // discarded otherwise — a fact is atomic).
        if (encodeField(&scratch, f.e) and encodeField(&scratch, f.a) and encodeField(&scratch, f.v)) {
            recs.appendSlice(ca, scratch.items) catch @panic("store: oom");
            kept += 1;
        } else skipped += 1;
    }
    var body: Bytes = .{};
    defer body.deinit(ca);
    wU64(&body, st.tx);
    body.append(ca, if (st.raw) 1 else 0) catch @panic("store: oom");
    wU32(&body, kept);
    wU32(&body, skipped);
    body.appendSlice(ca, recs.items) catch @panic("store: oom");
    g_last_skipped = skipped;
    return g_api.mkString(body.items.ptr, body.items.len);
}

/// `[Store imageSkippedCount]` — facts the most recent `dumpImageSection`
/// deferred to the heap section (compound values). A SmallInt.
fn mImageSkippedCount(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = args;
    _ = n;
    return g_api.mkSmallInt(@intCast(g_last_skipped));
}

const Rd = struct {
    b: []const u8,
    i: usize = 0,
    fn byte(r: *Rd) ?u8 {
        if (r.i + 1 > r.b.len) return null;
        const v = r.b[r.i];
        r.i += 1;
        return v;
    }
    fn u32v(r: *Rd) ?u32 {
        if (r.i + 4 > r.b.len) return null;
        const v = std.mem.readInt(u32, r.b[r.i..][0..4], .little);
        r.i += 4;
        return v;
    }
    fn u64v(r: *Rd) ?u64 {
        if (r.i + 8 > r.b.len) return null;
        const v = std.mem.readInt(u64, r.b[r.i..][0..8], .little);
        r.i += 8;
        return v;
    }
    fn bytesv(r: *Rd) ?[]const u8 {
        const len = r.u32v() orelse return null;
        if (r.i + len > r.b.len) return null;
        const s = r.b[r.i .. r.i + len];
        r.i += len;
        return s;
    }
};

/// Decode one leaf field, re-interning symbols by name (idempotent → the same
/// symbol identity this process already holds; the durable anchor). Null on a
/// malformed field.
fn decodeField(r: *Rd) ?Oop {
    const tag = r.byte() orelse return null;
    return switch (@as(FieldTag, @enumFromInt(tag))) {
        .nil => g_api.nilObj,
        .true_ => g_api.trueObj,
        .false_ => g_api.falseObj,
        .int => g_api.mkSmallInt(@bitCast(r.u64v() orelse return null)),
        .float => g_api.mkFloat(@bitCast(r.u64v() orelse return null)),
        .symbol => blk: {
            const s = r.bytesv() orelse return null;
            const z = ca.dupeZ(u8, s) catch @panic("store: oom");
            defer ca.free(z);
            break :blk g_api.intern(z.ptr);
        },
        .string => blk: {
            const s = r.bytesv() orelse return null;
            break :blk g_api.mkString(s.ptr, s.len);
        },
    };
}

/// `[Store loadImageSection: aString]` — replace this store's fact stream +
/// counters from a `.facts` section. Wholesale replacement (docs/40 §2): the
/// saved stream already carries the seeded schema. Answers the SmallInt count of
/// facts restored, or nil on a malformed section.
fn mLoadImageSection(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const st = stateOf(self);
    var len: usize = 0;
    const p = g_api.stringBytes(args[0], &len);
    var r = Rd{ .b = p[0..len] };
    const tx = r.u64v() orelse return g_api.nilObj;
    const raw_b = r.byte() orelse return g_api.nilObj;
    const kept = r.u32v() orelse return g_api.nilObj;
    _ = r.u32v() orelse return g_api.nilObj; // skipped — informational, not replayed
    st.facts.clearRetainingCapacity();
    var i: u32 = 0;
    while (i < kept) : (i += 1) {
        const op = r.byte() orelse return g_api.nilObj;
        const ftx = r.u64v() orelse return g_api.nilObj;
        const e = decodeField(&r) orelse return g_api.nilObj;
        const a = decodeField(&r) orelse return g_api.nilObj;
        const v = decodeField(&r) orelse return g_api.nilObj;
        st.facts.append(ca, .{ .e = e, .a = a, .v = v, .tx = ftx, .op = @enumFromInt(op) }) catch @panic("store: oom");
    }
    st.tx = tx;
    st.raw = raw_b != 0;
    return g_api.mkSmallInt(@intCast(kept));
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
    s_value3 = api.intern("value:value:value:");

    g_globals_entity = api.intern("__globals__");

    const reloading = api.isExported("Store") != 0;
    if (reloading) {
        store_vt = api.import("StoreVt");
        store_instance = api.import("Store");
        cell_vt = api.import("CellVt");
        globals_vt = api.import("GlobalsVt");
        globals_obj = api.import("Globals");
    } else {
        store_vt = api.proto(api.object_vt);
        cell_vt = api.proto(api.object_vt);
        globals_vt = api.proto(api.object_vt);
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
    api.method(store_vt, api.intern("liveTriplesDo:"), &mLiveTriplesDo);
    api.method(store_vt, api.intern("liveTriplesAsOf:do:"), &mLiveTriplesAsOfDo);
    api.method(store_vt, api.intern("liveTriplesSince:do:"), &mLiveTriplesSinceDo);
    api.method(store_vt, api.intern("liveTriplesBetween:and:do:"), &mLiveTriplesBetweenAndDo);
    api.method(store_vt, api.intern("basis"), &mBasis);
    api.method(store_vt, api.intern("cardinality:"), &mCardinality);
    api.method(store_vt, api.intern("dumpImageSection"), &mDumpImageSection);
    api.method(store_vt, api.intern("imageSkippedCount"), &mImageSkippedCount);
    api.method(store_vt, api.intern("loadImageSection:"), &mLoadImageSection);
    api.setTraceHook(store_vt, &gcTraceHook); // facts live in non-id memory (F3)
    // Exact payload layouts (F3): a store's payload is its Zig StoreState (list
    // pointers + counters — raw words; its Oops are the facts, reached by the
    // trace hook above); a cell is {store, e, a} — all Oops.
    api.setLayout(store_vt, null, 0, idapi.slot_raw);
    api.setLayout(cell_vt, null, 0, idapi.slot_oop);

    // Cell protocol (the `history` selector reuses the "history" symbol — one
    // identity, two roles: a cell message here, a schema attribute key above).
    api.method(cell_vt, s_value, &mCellValue);
    api.method(cell_vt, s_set, &mCellSet);
    api.method(cell_vt, s_history, &mCellHistory);

    api.method(globals_vt, api.intern("at:"), &mGlobalsAt);
    api.method(globals_vt, api.intern("at:put:"), &mGlobalsAtPut);
    api.method(globals_vt, api.intern("includes:"), &mGlobalsIncludes);

    if (!reloading) {
        store_instance = freshStore();
        globals_obj = api.alloc(globals_vt, 0);
        _ = api.exportObj("StoreVt", store_vt);
        _ = api.exportObj("Store", store_instance);
        _ = api.exportObj("CellVt", cell_vt);
        _ = api.exportObj("GlobalsVt", globals_vt);
        _ = api.exportObj("Globals", globals_obj);
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
