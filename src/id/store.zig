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

var method_mutation_observer: Oop = null;
const MethodMutationAuthorizer = *const fn () callconv(.c) Oop;
var method_mutation_authorizer: ?MethodMutationAuthorizer = null;

/// Register the one mechanism-level observer of completed supported MOP writes.
/// This stays private to the Zig modules rather than widening IdApi; raw floor
/// mutation deliberately bypasses it and is discovered by lazy reconciliation.
pub fn setMethodMutationObserver(observer: Oop) void {
    method_mutation_observer = observer;
}

fn mSetMethodMutationObserver(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    if (method_mutation_observer != null and method_mutation_observer != args[0])
        return mopError("methodMutationObserverAlreadySet");
    setMethodMutationObserver(args[0]);
    return self;
}

fn mSetMethodMutationAuthorizer(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const candidate: MethodMutationAuthorizer = @ptrFromInt(@as(usize, @intCast(g_api.smallIntValue(args[0]))));
    if (method_mutation_authorizer != null and method_mutation_authorizer.? != candidate)
        return mopError("methodMutationAuthorizerAlreadySet");
    method_mutation_authorizer = candidate;
    return self;
}

/// Whether a fact asserts a value or retracts one (Datomic-style op-tagging).
const Op = enum(u8) { assert, retract };

/// A triple plus its op and the transaction it landed in. The log is append-only
/// for history-on attributes (assert + retract entries fold on read); for
/// history-off attributes the log holds only currently-live asserted facts (a
/// retract physically removes, a `:one` re-assert replaces in place — no growth).
const Fact = struct {
    e: Oop,
    a: Oop,
    v: Oop,
    tx: u64,
    // Derived access-path metadata: neither field is serialized or authoritative.
    // Both are rebuilt from the canonical log whenever fact positions shift.
    ea_hash: u64,
    op: Op = .assert,
    pair_first: bool = false,
};

/// The store's private backing — held in the Store libid object's payload (the
/// bytes at `object[-1]+0`, i.e. at the object pointer itself). Consistent with
/// libid keeping its method-dict backing in Zig arrays: reflectivity lives in
/// the API (any fact, incl. schema, is queryable), not in making each triple a
/// separately-dispatched object.
const StoreState = struct {
    facts: std.ArrayList(Fact) = .{},
    raw: bool = true,
    tx: u64 = 0,
    // The (e,a) postings index — an ACCESS PATH, never a second source of truth
    // (docs/42 §D rebuilt-cache discipline). Maps each distinct (e,a) pair to the
    // positions of its facts in the canonical log, in log order, so every
    // per-pair read walks its own few facts instead of the whole log. Holds
    // positions only (no Oops beyond the keys, which live facts already hold),
    // is maintained write-through at the three mutation chokepoints (`appendOp`,
    // `removeLive`, the image-load appends; `replaceInPlace` is position-stable),
    // and is rebuilt wholesale from the log whenever positions shift. Never
    // serialized: an image load re-derives it.
    ea: EaMap = .{},
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
pub var s_removeMethodOn: Oop = null; // `[store removeMethod: sel on: vt]`
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
// The mirror value for a NATIVELY-bound selector (docs/42 §D1): a GC-safe,
// eq-safe symbol marker, never a raw fn pointer. User methods mirror their real
// object; native primitives mirror this. Lets reflection see "vt responds to sel"
// (presence) without storing an un-traceable fn pointer as a fact value.
var s_primitive: Oop = null;

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
    if (a == b) return true; // interned symbols, same object, same eid
    if (a == null or b == null) return false; // one side is internal absence
    const av = g_api.vtableOf(a);
    const bv = g_api.vtableOf(b);
    // Integers are boxed (no immediates): equal-valued integers are DISTINCT
    // objects, so an identity-only `eq` would see duplicate facts/values as
    // distinct (breaking set semantics + aggregates). Match value types by value.
    if (av == g_api.smallint_vt and bv == g_api.smallint_vt)
        return g_api.smallIntValue(a) == g_api.smallIntValue(b);
    if (av == g_api.string_vt and bv == g_api.string_vt) {
        var la: usize = 0;
        var lb: usize = 0;
        const pa = g_api.stringBytes(a, &la);
        const pb = g_api.stringBytes(b, &lb);
        return std.mem.eql(u8, pa[0..la], pb[0..lb]);
    }
    // Floats by VALUE (IEEE ==), mirroring the prelude's per-type `=` — asserting
    // an equal-valued Float fact twice is ONE fact (set semantics), and retract
    // finds the fact a fresh equal-valued box names.
    if (av == g_api.float_vt and bv == g_api.float_vt)
        return g_api.floatValue(a) == g_api.floatValue(b);
    return false;
}

// -- The (e,a) postings index -------------------------------------------------
//
// Hash keys the way `eq` compares (the single identity chokepoint above): value
// types by value, everything else by address — safe because the collector is
// non-moving, and the index never crosses an image boundary (it is rebuilt from
// the loaded log, so per-process addresses are fine where a serialized structure
// could not use them).

fn mix64(x0: u64) u64 {
    var x = x0; // splitmix64 finalizer — full avalanche for address/int keys
    x ^= x >> 30;
    x *%= 0xbf58476d1ce4e5b9;
    x ^= x >> 27;
    x *%= 0x94d049bb133111eb;
    x ^= x >> 31;
    return x;
}

/// Hash consistent with `eq`: eq(a,b) implies valHash(a) == valHash(b).
fn valHash(o: Oop) u64 {
    if (o == null) return mix64(0);
    const vt = g_api.vtableOf(o);
    if (vt == g_api.smallint_vt) return mix64(@bitCast(g_api.smallIntValue(o)));
    if (vt == g_api.string_vt) {
        var len: usize = 0;
        const p = g_api.stringBytes(o, &len);
        return std.hash.Wyhash.hash(0, p[0..len]);
    }
    if (vt == g_api.float_vt) {
        // Value bits; -0.0 normalizes to +0.0 (IEEE == calls them equal).
        const f = g_api.floatValue(o);
        const bits: u64 = if (f == 0) 0 else @bitCast(f);
        return mix64(bits);
    }
    return mix64(@intFromPtr(o.?));
}

const EaKey = struct { e: Oop, a: Oop, hash: u64 };

fn eaKey(e: Oop, a: Oop) EaKey {
    return .{ .e = e, .a = a, .hash = mix64(valHash(e) *% 0x9e3779b97f4a7c15 +% valHash(a)) };
}

const EaCtx = struct {
    pub fn hash(_: EaCtx, k: EaKey) u64 {
        return k.hash;
    }
    pub fn eql(_: EaCtx, x: EaKey, y: EaKey) bool {
        return x.hash == y.hash and eq(x.e, y.e) and eq(x.a, y.a);
    }
};

const Postings = std.ArrayList(u32);
const EaMap = std.HashMapUnmanaged(EaKey, Postings, EaCtx, std.hash_map.default_max_load_percentage);

/// Add a fact position to the postings cache and report whether this fact is the
/// pair's first physical appearance in the canonical log.
fn indexAppend(st: *StoreState, key: EaKey, pos: usize) bool {
    const gop = st.ea.getOrPut(ca, key) catch @panic("store: oom");
    if (!gop.found_existing) gop.value_ptr.* = .{};
    gop.value_ptr.append(ca, @intCast(pos)) catch @panic("store: oom");
    return !gop.found_existing;
}

fn clearIndex(st: *StoreState) void {
    var it = st.ea.valueIterator();
    while (it.next()) |p| p.deinit(ca);
    st.ea.clearRetainingCapacity();
}

/// Re-derive the index from the log — the recovery path for any mutation that
/// shifts fact positions (`removeLive`). O(F), which is what the shifting
/// mutation itself already costs.
fn rebuildIndex(st: *StoreState) void {
    clearIndex(st);
    for (st.facts.items, 0..) |*f, i|
        f.pair_first = indexAppend(st, .{ .e = f.e, .a = f.a, .hash = f.ea_hash }, i);
}

/// The log positions of (e,a)'s facts, in log order — empty if the pair has none.
/// Every per-pair read walks this instead of the whole log; within-pair visit
/// order is identical to the old full scans, so all fold semantics (newest
/// decides, op-awareness, tx windows) are unchanged.
fn postingsOf(st: *StoreState, e: Oop, a: Oop) []const u32 {
    const p = st.ea.getPtr(eaKey(e, a)) orelse return &.{};
    return p.items;
}

fn appendOp(st: *StoreState, e: Oop, a: Oop, v: Oop, op: Op) void {
    const key = eaKey(e, a);
    st.facts.append(ca, .{ .e = e, .a = a, .v = v, .tx = st.tx, .ea_hash = key.hash, .op = op }) catch @panic("store: oom");
    st.facts.items[st.facts.items.len - 1].pair_first = indexAppend(st, key, st.facts.items.len - 1);
    st.tx += 1;
}
fn appendFact(st: *StoreState, e: Oop, a: Oop, v: Oop) void {
    appendOp(st, e, a, v, .assert);
}

/// Physically drop currently-live asserted facts matching (e, a, v) — the
/// history-off retention path (no retract entry lingers, no growth). Removal
/// shifts every later fact's position, so the index is rebuilt after — O(F),
/// the same order the removal's own shifting already costs.
fn removeLive(st: *StoreState, e: Oop, a: Oop, v: Oop) void {
    var matches: std.ArrayList(u32) = .{};
    defer matches.deinit(ca);
    for (postingsOf(st, e, a)) |i| {
        const f = st.facts.items[i];
        if (eq(f.v, v) and f.op == .assert) matches.append(ca, i) catch @panic("store: oom");
    }
    if (matches.items.len == 0) return;
    var i = matches.items.len; // remove highest-first so earlier positions stay valid
    while (i > 0) {
        i -= 1;
        _ = st.facts.orderedRemove(matches.items[i]);
    }
    rebuildIndex(st);
}

/// Is value `v` currently a live member of (e, a)? The newest matching (e,a,v)
/// fact decides (assert → present, retract → absent). Op-aware.
fn isMember(st: *StoreState, e: Oop, a: Oop, v: Oop) bool {
    const posts = postingsOf(st, e, a);
    var i = posts.len;
    while (i > 0) {
        i -= 1;
        const f = st.facts.items[posts[i]];
        if (eq(f.v, v)) return f.op == .assert;
    }
    return false;
}

/// Newest-first scan for the value of (e, a) AS OF a basis watermark: only facts
/// with `tx < asof` are visible (`asof == null` = all = the current value; `asof`
/// is a `[store basis]` value = the next-tx watermark, so the snapshot excludes
/// anything committed at/after it). The newest visible matching fact decides; a
/// retract entry means the value is absent.
fn currentValueRawAsOf(st: *StoreState, e: Oop, a: Oop, asof: ?u64) Oop {
    const posts = postingsOf(st, e, a);
    var i = posts.len;
    while (i > 0) {
        i -= 1;
        const f = st.facts.items[posts[i]];
        if (asof) |t| if (f.tx >= t) continue; // asof is a watermark: visible iff tx < asof
        return if (f.op == .retract) null else f.v;
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
    return cardinalityOfAsOf(st, a, null);
}

/// The cardinality of `a` AS OF a basis watermark (docs/42 §D4): schema is data,
/// so it time-travels like any other cell. An `asOf:` query must interpret an old
/// snapshot with the schema *as it was then*, not today's — read the
/// `:cardinality` fact under the same watermark. (Schema set once at seed, tx 0–2,
/// so any realistic basis sees it; a later cardinality change is honoured per-basis.)
fn cardinalityOfAsOf(st: *StoreState, a: Oop, asof: ?u64) Oop {
    const c = currentValueRawAsOf(st, a, s_cardinality, asof);
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
    return postingsOf(st, e, a).len;
}

/// Overwrite the value of the newest (e, a) fact in place (no growth); append if
/// none exists. The no-history retention path for `:one`. Position-stable: the
/// newest (e,a) fact is the pair's last posting, and it stays newest (its tx
/// bumps to the write time), so the index needs no maintenance here.
fn replaceInPlace(st: *StoreState, e: Oop, a: Oop, v: Oop) void {
    const posts = postingsOf(st, e, a);
    if (posts.len > 0) {
        const i = posts[posts.len - 1];
        st.facts.items[i].v = v;
        st.facts.items[i].tx = st.tx;
        st.tx += 1;
        return;
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
    const posts = postingsOf(st, e, a);
    var i = posts.len;
    while (i > 0) {
        i -= 1;
        const f = st.facts.items[posts[i]];
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
    retractHeld(st, e, a, v);
    return self;
}

fn retractHeld(st: *StoreState, e: Oop, a: Oop, v: Oop) void {
    if (!st.raw and eq(historyOf(st, a), s_true))
        appendOp(st, e, a, v, .retract) // keep the timeline
    else
        removeLive(st, e, a, v); // default: drop the live fact, no growth
}

/// `[store currentValue: e attr: a]` — the latest value of (e, a), or nil.
/// Normalises the store's internal absence (Zig null) to the nil singleton, so
/// birnam code never sees a raw null — no object understands anything, so a send
/// to it PANICKED the VM (`[db currentValue: #absent attr: #x]` was a crash). Like
/// `[Globals at:]`, this single-value read can't tell "absent" from "bound to
/// nil"; the distinction rides `currentValue:attr:ifAbsent:` / `hasValue:attr:`
/// (§G6, the no-NULL lock: absence rides ifAbsent:, nil is a value).
fn mCurrentValue(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const v = currentValueRaw(stateOf(self), args[0], args[1]);
    return if (v == null) g_api.nilObj else v;
}

/// `[store currentValue: e attr: a ifAbsent: aBlock]` — the value of (e, a), EVEN
/// IF that value is nil; only genuine absence (no fact / retracted) evaluates
/// `aBlock` for the answer. The general-store counterpart of `[Globals
/// at:ifAbsent:]` (§G6): the store keeps absence and bound-nil distinct, the
/// block is the out-of-band channel that reads the difference.
fn mCurrentValueIfAbsent(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const v = currentValueRaw(stateOf(self), args[0], args[1]);
    if (v != null) return v; // present — the value (possibly nil)
    return g_api.send(args[2], s_value, &[_]Oop{}, 0); // absent — the block decides
}

/// `[store hasValue: e attr: a]` — true iff (e, a) currently holds a value,
/// INCLUDING a stored nil (§G6). The block-free presence test — `[Globals
/// includes:]` generalised to any entity — for callers (birnam or Zig) that need
/// the absent/bound-nil distinction without constructing a block.
fn mHasValue(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return if (currentValueRaw(stateOf(self), args[0], args[1]) == null) g_api.falseObj else g_api.trueObj;
}

/// `[store rawCurrentValue: e attr: a]` — the RAW tier (cf. `rawAssert:…`): the
/// latest value of (e, a), or the store's internal absence — Zig null, NOT the
/// nil singleton. For Zig callers in other dylibs (they can't reach
/// `currentValueRaw` directly) on paths where absent-vs-bound-nil must ride ONE
/// send: codegen's `birnam_global` resolution reads this per global reference (a
/// bound nil must WIN over the exported env; a `hasValue:` pre-check would double
/// a hot path). ⚠ Zig-facing: birnam code sending anything to the returned
/// absence PANICS the VM — birnam reads ride `currentValue:attr:` (nil-normalised)
/// / `currentValue:attr:ifAbsent:` / `hasValue:attr:` instead.
fn mRawCurrentValue(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return currentValueRaw(stateOf(self), args[0], args[1]);
}

/// Does (e, a) have any fact at/after `basis`? (An in-place overwrite of a
/// history-off `:one` fact bumps its tx to the write time, so a value that WAS held
/// before `basis` can present as a single fact with `tx >= basis`.)
fn hasFactAtOrAfter(st: *StoreState, e: Oop, a: Oop, basis: u64) bool {
    for (postingsOf(st, e, a)) |i| if (st.facts.items[i].tx >= basis) return true;
    return false;
}

/// A `#historyNotRetained` error value (F7 error model, imported at call time), or
/// nil if the error module isn't loaded — the honest signal that an `asOf:` read
/// cannot be answered because the attribute keeps no timeline.
fn historyNotRetainedError() Oop {
    const ef = g_api.import("Error");
    if (ef == null) return g_api.nilObj;
    return g_api.send(ef, g_api.intern("kind:"), &[_]Oop{g_api.intern("historyNotRetained")}, 1);
}

/// `[store valueOf: e attr: a asOf: basis]` — the value of (e, a) AS OF a basis
/// watermark, made HONEST for history-off attributes (docs/42 §D3). A history-off
/// `:one` attribute keeps no timeline: an overwrite discards the old value and bumps
/// the fact's tx, so a naive asOf read would report the attribute ABSENT at an
/// earlier basis when it in fact held a (now-lost) value. So: return the value if it
/// is reliably visible at `basis`; else, if history is off AND a later fact exists
/// for (e, a) (the ambiguous, possibly-overwritten case), return a
/// `#historyNotRetained` error rather than a silently-wrong nil; a genuine absence
/// (no fact, or a history-on timeline that shows none) still reads nil.
fn mValueOfAsOf(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const st = stateOf(self);
    const e = args[0];
    const a = args[1];
    const basis: u64 = @intCast(g_api.smallIntValue(args[2]));
    const v = currentValueRawAsOf(st, e, a, basis);
    if (v != null) return v; // reliably visible at basis
    if (eq(historyOf(st, a), s_false) and hasFactAtOrAfter(st, e, a, basis))
        return historyNotRetainedError(); // can't be sure — honest, not silently nil
    return g_api.nilObj; // genuinely absent
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
/// current). Cardinality is read AS OF `hi` (docs/42 §D4: schema is data and
/// time-travels — an old snapshot is read with the schema as it was then); the
/// deciding fact is the newest with `tx < hi`, and a triple is emitted only if that
/// fact is an assert AND its `tx >= lo`.
fn enumerateTriples(st: *StoreState, blk: Oop, lo: ?u64, hi: ?u64) void {
    // Distinct (e,a) pairs in first-appearance order, among facts within `hi` —
    // the same scan-order contract as ever. `pair_first` is rebuilt together
    // with the postings cache, so enumeration need not allocate and re-hash a
    // temporary seen-pair set on every query.
    var pairs: std.ArrayList(EaKey) = .{};
    defer pairs.deinit(ca);
    for (st.facts.items) |f| {
        if (visibleHi(f.tx, hi) and f.pair_first)
            pairs.append(ca, .{ .e = f.e, .a = f.a, .hash = f.ea_hash }) catch @panic("store: oom");
    }

    for (pairs.items) |p| {
        const posts = postingsOf(st, p.e, p.a);
        if (eq(cardinalityOfAsOf(st, p.a, hi), s_one)) {
            // The deciding fact: newest matching with tx < hi.
            var i = posts.len;
            while (i > 0) {
                i -= 1;
                const f = st.facts.items[posts[i]];
                if (!visibleHi(f.tx, hi)) continue;
                if (f.op == .assert and inLo(f.tx, lo)) emitTriple(blk, p.e, p.a, f.v);
                break; // newest matching fact decides
            }
        } else {
            // :many — each distinct value whose newest fact (within hi) is an assert.
            var seen: std.ArrayList(Oop) = .{};
            defer seen.deinit(ca);
            var i = posts.len;
            while (i > 0) {
                i -= 1;
                const f = st.facts.items[posts[i]];
                if (!visibleHi(f.tx, hi)) continue;
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
    if (method_mutation_authorizer) |authorize| {
        const allowed = authorize();
        if (allowed != g_api.trueObj) return allowed;
    }
    // Preserve history in the TRACKED path before entering id's allocation-free
    // raw-mutation hook. The hook then sees absence and is a no-op.
    const st = stateOf(self);
    if (currentValueRaw(st, vt, sel)) |old| retractHeld(st, vt, sel, old);
    g_api.method(vt, sel, @ptrCast(@alignCast(m.?))); // 1. authoritative dict
    _ = g_api.send(self, s_assert, &[_]Oop{ vt, sel, m }, 3); // 2. derived fact
    if (method_mutation_observer != null)
        _ = g_api.send(method_mutation_observer, g_api.intern("managedMethodChanged:on:"), &[_]Oop{ sel, vt }, 2);
    return self;
}

fn mopError(kind: [*:0]const u8) Oop {
    const ef = g_api.import("Error") orelse return g_api.nilObj;
    return g_api.send(ef, g_api.intern("kind:"), &[_]Oop{g_api.intern(kind)}, 1);
}

/// `[Store removeMethod: sel on: vt]` — the supported exact-removal MOP.
/// Only a direct binding may be removed, and candidate/lifecycle callers must name
/// an already-owned stable vtable. The raw floor method returns the old pointer;
/// this public operation deliberately returns Store/error instead.
fn mTrackedRemoveMethod(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const sel = args[0];
    const vt = args[1];
    if (method_mutation_authorizer) |authorize| {
        const allowed = authorize();
        if (allowed != g_api.trueObj) return allowed;
    }
    if (g_api.vtableOwner(vt) == null) return mopError("unstableMethodTarget");
    var direct = false;
    var i: usize = 0;
    while (g_api.vtableSelectorAt(vt, i)) |bound| : (i += 1) {
        if (bound == sel) {
            direct = true;
            break;
        }
    }
    if (!direct) return mopError("missingDirectMethod");
    const st = stateOf(self);
    if (currentValueRaw(st, vt, sel)) |old| retractHeld(st, vt, sel, old);
    const removed = g_api.send(vt, g_api.intern("removeMethod:"), &[_]Oop{sel}, 1);
    std.debug.assert(removed != null);
    if (method_mutation_observer != null)
        _ = g_api.send(method_mutation_observer, g_api.intern("managedMethodChanged:on:"), &[_]Oop{ sel, vt }, 2);
    return self;
}

/// Is `vt` a real type in the object hierarchy (its chain reaches `object_vt`), as
/// opposed to an internal dict — the intern table / export env are `Vtable` structs
/// too, registered in the floor's registry, but delegate to nothing and hold data
/// keys, not a dispatch protocol. Backfilling those would assert garbage facts.
fn isTypeVtable(vt: Oop) bool {
    var cur = vt;
    while (cur != null) {
        if (cur == g_api.object_vt) return true;
        cur = g_api.vtableParent(cur);
    }
    return false;
}

fn isRegisteredTypeVtable(o: Oop) bool {
    var i: usize = 0;
    while (i < g_api.vtableCount()) : (i += 1) {
        const vt = g_api.vtableAt(i);
        if (vt == o) return isTypeVtable(vt);
    }
    return false;
}

/// `[Store backfillMirror]` — make the MOP mirror TOTAL (docs/42 §D1). For every
/// selector bound on a real type vtable that has NO mirror fact yet, assert a
/// `#primitive` marker, so reflection sees the native protocol (Integer's `+`,
/// String's `,`, …) alongside user methods. Idempotent (assert-if-absent) and
/// re-run on every full boot AND image load, so the total mirror survives a reload
/// with no lost functionality. Object-valued facts (user methods already mirrored)
/// are left untouched; a later user override of a native selector replaces its
/// marker in place through the ordinary assert path.
fn mBackfillMirror(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    const st = stateOf(self);
    const nvt = g_api.vtableCount();
    var i: usize = 0;
    while (i < nvt) : (i += 1) {
        const vt = g_api.vtableAt(i);
        if (!isTypeVtable(vt)) continue;
        var j: usize = 0;
        while (g_api.vtableSelectorAt(vt, j)) |sel| : (j += 1) {
            if (currentValueRaw(st, vt, sel) == null)
                appendFact(st, vt, sel, s_primitive); // presence, GC/eq-safe marker
        }
    }
    return self;
}

/// Drive `backfillMirror` on the well-known store — for init sites that hold only
/// the api (the CLI's `loadStack`, the test harness, image load).
pub fn backfillMirror(api: *idapi.IdApi) void {
    _ = api.send(store_instance, api.intern("backfillMirror"), &[_]Oop{}, 0);
}

/// The method-overwrite hook (docs/42 §D1, the consistency law). When any path
/// changes/removes an existing (vt,sel) in the dict, any mirror fact for it is now
/// stale — retract it so `currentValue(vt,sel)` never returns a binding no longer
/// present. This includes `#primitive`: a replacement/removal invalidates presence
/// just as surely as it invalidates an object-valued method. Pure: touches only the
/// fact log (no send/alloc), safe mid-mutation and reentrantly under Store's MOP.
fn mirrorOverwriteHook(vt: Oop, sel: Oop) callconv(.c) void {
    if (store_instance == null) return;
    const st = stateOf(store_instance);
    const cur = currentValueRawAsOf(st, vt, sel, null);
    if (cur == null) return; // not mirrored — absent stays absent (honest)
    removeLive(st, vt, sel, cur); // stale object binding → retract (→ absent)
}

// -- The Globals scope (docs/36) --------------------------------------------
//
// A namespace is just an object you send lookup messages to. `Globals` forwards
// to the well-known store on the reserved `g_globals_entity`, so a global binding
// IS a cell: `at:put:` asserts, `at:` reads currentValue, both inheriting the
// store's opt-in history + queryability. No new store core.

/// `[Globals at: aSymbol]` — the current value bound to `aSymbol`, or nil.
/// Normalises the store's internal absence (Zig null) to the nil singleton, so
/// birnam code never sees a raw null. A single-value accessor CAN'T distinguish
/// "absent" from "bound to nil" — both answer nil; use `at:ifAbsent:` (§G6, the
/// no-NULL lock: absence rides `ifAbsent:`, nil is a value) when the difference
/// matters.
fn mGlobalsAt(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    // The raw in-file read: the `currentValue:attr:` SURFACE now normalises
    // absence to nil itself, so the accessors that need the raw distinction
    // read the state directly.
    const cur = currentValueRaw(stateOf(store_instance), g_globals_entity, args[0]);
    return if (cur == null) g_api.nilObj else cur;
}

/// `[Globals at: aSymbol ifAbsent: aBlock]` — the value bound to `aSymbol`,
/// EVEN IF that value is nil; only when `aSymbol` has no binding at all is
/// `aBlock` evaluated (`value`) for the answer. This is the one accessor that
/// tells "unbound" from "bound to nil" apart — the store already keeps them
/// distinct (internal absence is Zig null, a bound nil is the nil singleton), so
/// no Binding object is needed; the distinction just needs an out-of-band channel,
/// which the block is (§G6 / docs/36 §3, the no-NULL stdlib lock).
fn mGlobalsAtIfAbsent(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const cur = currentValueRaw(stateOf(store_instance), g_globals_entity, args[0]);
    if (cur != null) return cur; // present — return the bound value (possibly nil)
    return g_api.send(args[1], s_value, &[_]Oop{}, 0); // absent — evaluate the block
}

/// `[Globals at: aSymbol put: aValue]` — bind (assert) and return the value.
fn mGlobalsAtPut(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    _ = g_api.send(store_instance, s_assert, &[_]Oop{ g_globals_entity, args[0], args[1] }, 3);
    return args[1];
}

/// `[Globals includes: aSymbol]` — true iff `aSymbol` has a binding, INCLUDING a
/// binding to nil (§G6). The store keeps internal absence (Zig null) distinct from
/// a bound nil (the nil singleton), so this reads presence honestly: `false` only
/// when genuinely unbound, `true` for a global explicitly set to nil.
fn mGlobalsIncludes(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const cur = currentValueRaw(stateOf(store_instance), g_globals_entity, args[0]);
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

// -- Image serialization (docs/40 slice 3) ----------------------------------
//
// The store owns its facts, so it yields/accepts them RAW; the `image` module
// owns the codec (leaf + symbol + heap-ref) and the container. Save is an
// internal iterator — `imageFactsInto:` sends `e:a:v:tx:op:` per fact to an
// image-side collector (no block-arg cap, no per-fact allocation; the shape of
// the existing `liveTriplesDo:`). Load is a begin/add/end protocol that rebuilds
// the fact stream verbatim — the timeline (per-fact tx + op) and the `tx`/`raw`
// counters included. It REPLACES the freshly-seeded fact list wholesale: the
// saved stream already carries the deterministically-seeded schema (docs/40 §3b).

var s_eavto: Oop = null; // the collector selector, "e:a:v:tx:op:"

/// `[Store imageFactsInto: collector]` — send every raw fact to `collector` as
/// `[collector e: e a: a v: v tx: <SmallInt> op: <SmallInt>]`. Answers self.
fn mImageFactsInto(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const st = stateOf(self);
    const collector = args[0];
    for (st.facts.items) |f| {
        // The MOP mirror is a rebuilt cache, not independently serialized (docs/42
        // §D1). Skip the entire vtable-entity projection, including retired raw
        // trampoline pointers retained by selector history: those are not Oops and
        // must never reach the heap codec. Code replay rebuilds user bindings and
        // `backfillMirror` rebuilds primitive presence markers.
        if (isRegisteredTypeVtable(f.e)) continue;
        _ = g_api.send(collector, s_eavto, &[_]Oop{
            f.e,                              f.a,                                  f.v,
            g_api.mkSmallInt(@intCast(f.tx)), g_api.mkSmallInt(@intFromEnum(f.op)),
        }, 5);
    }
    return self;
}

/// `[Store imageLoadBegin]` — drop the current fact stream ahead of a wholesale
/// replace (docs/40 §3b). Answers self.
fn mImageLoadBegin(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    const st = stateOf(self);
    st.facts.clearRetainingCapacity();
    clearIndex(st); // the index is a cache of the log — dropped and re-derived
    return self;
}

/// `[Store imageAddFact: e attr: a value: v tx: t op: o]` — append one raw fact,
/// its tx + op preserved (`t`/`o` are SmallInts). Answers self.
fn mImageAddFact(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const st = stateOf(self);
    const key = eaKey(args[0], args[1]);
    st.facts.append(ca, .{
        .e = args[0],
        .a = args[1],
        .v = args[2],
        .tx = @intCast(g_api.smallIntValue(args[3])),
        .ea_hash = key.hash,
        .op = @enumFromInt(@as(u8, @intCast(g_api.smallIntValue(args[4])))),
    }) catch @panic("store: oom");
    st.facts.items[st.facts.items.len - 1].pair_first = indexAppend(st, key, st.facts.items.len - 1);
    return self;
}

/// `[Store imageLoadEnd: tx raw: rawFlag]` — restore the counters after the last
/// fact is added (`tx` a SmallInt, `rawFlag` a Boolean). Answers self.
fn mImageLoadEnd(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const st = stateOf(self);
    st.tx = @intCast(g_api.smallIntValue(args[0]));
    st.raw = args[1] == g_api.trueObj;
    return self;
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
    method_mutation_observer = null;
    method_mutation_authorizer = null;
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
    s_removeMethodOn = api.intern("removeMethod:on:");
    s_retract = api.intern("retract:attr:value:");
    s_new = api.intern("new");
    s_attrCount = api.intern("attrCount:");
    s_attrAt = api.intern("attrAt:index:");
    s_value3 = api.intern("value:value:value:");
    s_primitive = api.intern("primitive");

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
    api.method(store_vt, api.intern("currentValue:attr:ifAbsent:"), &mCurrentValueIfAbsent);
    api.method(store_vt, api.intern("hasValue:attr:"), &mHasValue);
    api.method(store_vt, api.intern("rawCurrentValue:attr:"), &mRawCurrentValue);
    api.method(store_vt, api.intern("valueOf:attr:asOf:"), &mValueOfAsOf);
    api.method(store_vt, s_count, &mCount);
    api.method(store_vt, s_history_read, &mHistory);
    api.method(store_vt, s_cellFor, &mCellFor);
    api.method(store_vt, s_addMethodOn, &mTrackedAddMethod);
    api.method(store_vt, s_removeMethodOn, &mTrackedRemoveMethod);
    api.method(store_vt, api.intern("setMethodMutationObserver:"), &mSetMethodMutationObserver);
    api.method(store_vt, api.intern("setMethodMutationAuthorizer:"), &mSetMethodMutationAuthorizer);
    api.method(store_vt, s_retract, &mRetract);
    api.method(store_vt, s_new, &mNew);
    api.method(store_vt, s_attrCount, &mAttrCount);
    api.method(store_vt, s_attrAt, &mAttrAt);
    api.method(store_vt, api.intern("backfillMirror"), &mBackfillMirror);
    api.method(store_vt, api.intern("liveTriplesDo:"), &mLiveTriplesDo);
    api.method(store_vt, api.intern("liveTriplesAsOf:do:"), &mLiveTriplesAsOfDo);
    api.method(store_vt, api.intern("liveTriplesSince:do:"), &mLiveTriplesSinceDo);
    api.method(store_vt, api.intern("liveTriplesBetween:and:do:"), &mLiveTriplesBetweenAndDo);
    api.method(store_vt, api.intern("basis"), &mBasis);
    api.method(store_vt, api.intern("cardinality:"), &mCardinality);
    s_eavto = api.intern("e:a:v:tx:op:");
    api.method(store_vt, api.intern("imageFactsInto:"), &mImageFactsInto);
    api.method(store_vt, api.intern("imageLoadBegin"), &mImageLoadBegin);
    api.method(store_vt, api.intern("imageAddFact:attr:value:tx:op:"), &mImageAddFact);
    api.method(store_vt, api.intern("imageLoadEnd:raw:"), &mImageLoadEnd);
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
    api.method(globals_vt, api.intern("at:ifAbsent:"), &mGlobalsAtIfAbsent);
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

    // Interweave the mirror with the object model (docs/42 §D1): from now on, any
    // rebind of a mirrored (vt,sel) retracts the stale fact — the mirror can no
    // longer hold a wrong value. Registered AFTER the store's own methods install
    // (those ran hook-free above), and idempotent across a reload.
    api.setMethodOverwriteHook(&mirrorOverwriteHook);
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
