//! Fact store: five persistent indexes + tx engine.
//!
//! Single-writer image — no locks. Every assert/retract bumps the tx
//! counter (monotonic, starting at 1; tx 0 is reserved for "before time").
//!
//! Indexes (locked Phase 3 design):
//!   EAVT  HAMT<e,     HAMT<a, ValueCell>>           — entity walks
//!   AEVT  HAMT<a,     HAMT<e, ValueCell>>           — schema walks
//!   AVET  HAMT<a,     HAMT<v, HAMT<e, PresenceCell>>>  — sort/uniqueness
//!   VAET  HAMT<v,     HAMT<a, HAMT<e, PresenceCell>>>  — reverse refs
//!   MEAT  HAMT<m,     HAMT<e, HAMT<a, ValueCell>>>  — module ownership
//!
//! Cardinality: single-valued only in this phase (asserting a new value
//! implicitly retracts any prior one). Multi-cardinality is Phase 9.5.
//!
//! No-NULL discipline: `assert` panics in Debug if `v == NIL`. Absence of
//! a fact is the only encoding for "no value." See VISION.md §No-NULL.

const std = @import("std");
const builtin = @import("builtin");
const hamt = @import("hamt.zig");
const rrb = @import("rrb.zig");
const ref_mod = @import("ref.zig");
const fact = @import("fact.zig");
const boxed = @import("boxed.zig");

pub const Ref = ref_mod.Ref;
pub const Op = fact.Op;
pub const ValueCell = fact.ValueCell;
pub const PresenceCell = fact.PresenceCell;
pub const ValueHist = fact.ValueHist;
pub const PresenceHist = fact.PresenceHist;
pub const ValueHistVec = fact.ValueHistVec;
pub const PresenceHistVec = fact.PresenceHistVec;

// -- Index type aliases -------------------------------------------------------

const InnerValueMap = hamt.Hamt(u64, ValueCell, hamt.U64Ctx);
const EAVTish = hamt.Hamt(u64, InnerValueMap, hamt.U64Ctx);

const InnerPresenceMap = hamt.Hamt(u64, PresenceCell, hamt.U64Ctx);
const MidPresenceMap = hamt.Hamt(u64, InnerPresenceMap, hamt.U64Ctx);
const AVETish = hamt.Hamt(u64, MidPresenceMap, hamt.U64Ctx);

const MEATish = hamt.Hamt(u64, EAVTish, hamt.U64Ctx);

// -- Epoch tightening --------------------------------------------------------
// Deep-clone the live index roots into a fresh arena so a compaction's
// path-copy garbage can be freed (see `tightenLiveEpoch`). Each layer clones
// its values into the same destination arena: value/presence cells clone their
// RRB history vector, and the nested index HAMTs recurse.

fn cloneValueCell(a: std.mem.Allocator, c: ValueCell) anyerror!ValueCell {
    return .{ .current_v = c.current_v, .history = try c.history.cloneInto(a) };
}
fn clonePresenceCell(a: std.mem.Allocator, c: PresenceCell) anyerror!PresenceCell {
    return .{ .present = c.present, .history = try c.history.cloneInto(a) };
}
fn cloneInnerValueMap(a: std.mem.Allocator, m: InnerValueMap) anyerror!InnerValueMap {
    return m.cloneInto(a, cloneValueCell);
}
fn cloneInnerPresenceMap(a: std.mem.Allocator, m: InnerPresenceMap) anyerror!InnerPresenceMap {
    return m.cloneInto(a, clonePresenceCell);
}
fn cloneMidPresenceMap(a: std.mem.Allocator, m: MidPresenceMap) anyerror!MidPresenceMap {
    return m.cloneInto(a, cloneInnerPresenceMap);
}
fn cloneEAVTish(a: std.mem.Allocator, m: EAVTish) anyerror!EAVTish {
    return m.cloneInto(a, cloneInnerValueMap);
}

// -- Cardinality schema (Phase 9.5.2) ----------------------------------------

pub const Cardinality = enum { one, many };

pub const CardinalitySchema = struct {
    /// EID of the `:cardinality` attribute (kernel_eids.ATTR_CARDINALITY).
    attr_eid: u64,
    /// Ref value that marks an attribute as `:many`. Comparison is by
    /// Ref-equality; anything else (including absence) → `:one`.
    many_value: Ref,
};

// -- Per-attribute storage tier (docs/19a-code-fact-tiers-plan.md) ------------

/// How cheaply an attribute's facts are stored. Code attrs (AST/bytecode
/// metadata) drop their per-fact history (Lever 1) and the never-read VAET
/// reverse-ref index (Lever 2). A future `skip_avet` (§2.4) is deliberately
/// omitted — it needs a `:many`/retract redesign, not just a flag.
pub const AttrTier = struct {
    no_history: bool = false,
    skip_vaet: bool = false,
};

// -- Index epoch registry (Phase 15.R R1b) -----------------------------------

/// The five index root pointers captured by value — a zero-copy snapshot of
/// the whole store (structural sharing). Held by a `RetainedEpoch` so a
/// swapped-out epoch stays queryable as a history archive (R2), and by a
/// `Snapshot` view.
pub const IndexRoots = struct {
    eavt: EAVTish,
    aevt: EAVTish,
    avet: AVETish,
    vaet: AVETish,
    meat: MEATish,
};

/// An index epoch swapped out by `compact()` but kept alive because a live
/// snapshot pins it (refcount > 0) or the retention horizon needs the history
/// it holds. Freed wholesale once nothing references it. The arena is the same
/// plain `ArenaAllocator` the live epoch used while writable.
pub const RetainedEpoch = struct {
    /// The epoch's arena while HOT (in RAM). `null` when spilled COLD to disk
    /// (R4) — `roots` are then invalid until `faultInEpoch` rebuilds them.
    arena: ?*std.heap.ArenaAllocator,
    /// Generation number (monotonic; the writable epoch's gen at swap-out).
    gen: u64,
    /// Lowest basis-T this epoch can serve a read for (basis-T when it became
    /// the writable epoch). Reads with `basis_t >= min_basis` are valid.
    min_basis: u64,
    /// Highest basis-T committed while this epoch was writable.
    max_basis: u64,
    /// Number of live snapshots pinning this epoch.
    refcount: u32,
    /// The index roots at swap-out — this epoch as a queryable history archive
    /// (R2). Valid only while HOT (`arena != null`).
    roots: IndexRoots,
    /// Phase 15.R R4 — segment file path when this epoch is spilled COLD; null
    /// when hot. Faulted back into a fresh arena on demand.
    cold_path: ?[]const u8 = null,
};

// -- Retention policy (Phase 15.R R2) ----------------------------------------

/// How far back history is retained. The floor it implies feeds
/// `oldestRetainedBasisT()`, which gates epoch freeing in `compact()`.
pub const RetentionPolicy = union(enum) {
    /// Keep only "now" (default). Compact collapses + frees old epochs (15.I).
    current_only,
    /// Keep all history forever (never free an epoch on horizon grounds).
    keep_all,
    /// Keep history with basis-T >= this value.
    keep_since_basis: u64,
    /// Keep the last N basis-Ts of history (floor tracks the live basis).
    keep_last_n: u64,
};

// -- Snapshot (Phase 15.R R1c) -----------------------------------------------

/// An immutable, first-class db-value: the store as of `basis_t`. `view` is a
/// read-only `FactStore` holding the five index roots captured by value, so
/// every existing point-read / iterator / datalog path works against it
/// unchanged (Decision B — full datalog-over-snapshot). Holding a Snapshot
/// pins its index `epoch_gen` against `compact()`; `deinit` releases the pin.
pub const Snapshot = struct {
    view: FactStore,
    basis_t: u64,
    epoch_gen: u64,
    /// Set when this is an `asOf:<past>` snapshot — reads are history-walked to
    /// this basis. Null for a "now" snapshot (read the frozen current roots).
    as_of_basis: ?u64 = null,
    /// The live store this was captured from — for unpinning on release.
    store: *FactStore,

    /// Exact point read honoring `as_of_basis`. For a "now" snapshot this is
    /// the frozen current value; for an `asOf:` snapshot it walks history.
    pub fn get(self: *const Snapshot, e: u64, a: u64) ?Ref {
        if (self.as_of_basis) |b| return self.view.currentValueAsOfBasis(e, a, b);
        return self.view.currentValue(e, a);
    }

    /// Unpin the epoch (making it collectible at the next compact if nothing
    /// else holds it) and free the snapshot. `allocator` must be the one that
    /// allocated it (the live store's allocator).
    pub fn deinit(self: *Snapshot, allocator: std.mem.Allocator) void {
        self.store.unpinEpoch(self.epoch_gen);
        allocator.destroy(self);
    }
};

// -- FactStore ---------------------------------------------------------------

pub const FactStore = struct {
    allocator: std.mem.Allocator,

    /// Phase 15.I — index epoch arena. When non-null, all FIVE persistent
    /// index structures (eavt/aevt/avet/vaet/meat, incl. their nested maps +
    /// `ValueHistVec`/`PresenceHistVec` history) allocate from this arena
    /// instead of `allocator`. `compact()` swaps to a fresh arena and frees
    /// the old one WHOLESALE — reclaiming both per-mutation path-copy garbage
    /// and the old current tree in one shot (the new tree shares no node with
    /// the old, so the old epoch is entirely dead). Opt-in (live entry points
    /// call `enableIndexArena`); unit tests leave it null → index on
    /// `allocator`, freed by the test's own arena teardown as before.
    ///
    /// A plain `ArenaAllocator` (wholesale free), NOT the recording `arena.zig`
    /// (that one is per-cell mark-sweep for the value heap). Heap-boxed so the
    /// pointer survives `FactStore` value copies and can be swapped in place.
    index_arena: ?*std.heap.ArenaAllocator = null,

    /// Phase 15.I/15.R gate. While false (current-only, the default and only
    /// mode today), `compact` frees the old index epoch unconditionally — no
    /// retained snapshot can reference it. When 15.R lands retained snapshots
    /// this flips true and the free becomes gated on the oldest retained
    /// basis-T; until that machinery exists, `true` conservatively SKIPS the
    /// free (leak, never corrupt — honors the never-free-live tenet).
    retain_history: bool = false,

    /// Phase 15.R R1b — generation of the current (writable) index epoch.
    /// Bumped each `compact()`. The writable epoch's arena is `index_arena`.
    index_epoch_gen: u64 = 0,
    /// Lowest basis-T the current writable epoch can serve (basis-T when it
    /// became writable; 1 at init, the compact basis-T thereafter).
    index_epoch_min_basis: u64 = 1,
    /// Live snapshots pinning the current writable epoch. Carried into a
    /// `RetainedEpoch` when this epoch is swapped out at compact.
    index_epoch_refcount: u32 = 0,
    /// Epochs swapped out by `compact()` but still pinned by a live snapshot
    /// (or the retention horizon). Freed when their refcount hits 0 and they
    /// fall below `oldestRetainedBasisT()`. Empty in the no-snapshot path, so
    /// behavior reduces exactly to 15.I (free-on-compact).
    retained_epochs: std.ArrayList(RetainedEpoch) = .{},

    /// Phase 15.R R2 — retention horizon policy. `current_only` (default) keeps
    /// only "now": compact collapses + frees old epochs (inert retention, the
    /// 15.I behavior). Other policies set a basis-T floor below which history is
    /// collectible; epochs covering the floor are retained (as queryable history
    /// archives) instead of freed. Derived floor folds into
    /// `oldestRetainedBasisT()`.
    retention_policy: RetentionPolicy = .current_only,
    /// Monotonic tx counter; starts at 1. Atomic per Phase 11.7.iv
    /// (ADR-11 #7 forward-compat). Single-writer transactor under the
    /// Datomic shape; atomic now keeps non-transactor read paths safe
    /// without a barrier change later. RESETS to 1 on `compact()` (the
    /// per-epoch internal tx coordinate).
    next_tx: std.atomic.Value(u64),

    /// Phase 15.R — separate monotonic basis-T counter (Decision A, locked
    /// 2026-06-06). Unlike `next_tx`, this **never resets on compact**. It is
    /// the durable user-facing time coordinate for `asOf:`, snapshot identity,
    /// and epoch `min_basis_t`. Bumped once per commit (assert/retract). Starts
    /// at 1; basis-T 0 means "before time". Not persisted across image save/load
    /// (snapshots are in-memory pins that don't cross a save) — re-asserts on
    /// load bump it from 1, preserving in-session monotonicity.
    basis_t: std.atomic.Value(u64),
    eavt: EAVTish,
    aevt: EAVTish, // same shape as EAVT, keys are (a, e)
    avet: AVETish,
    vaet: AVETish, // same shape as AVET, keys are (v, a, e)
    meat: MEATish,

    /// Phase 14 `--strip=history`: once frozen, the store is read-only —
    /// `assert`/`retract` reject with `error.ReadOnly`. Set by `freeze()`
    /// after an AOT `loadFrozen` (+ FFI resolve) completes. A frozen
    /// end-user app reads its baked facts and never mutates them.
    read_only: bool = false,

    /// Optional hook invoked after every assert/retract with the attribute
    /// EID. Kernel sets this to `mcache.flushSel` to keep dispatch-cache
    /// coherent. Decoupled (FactStore doesn't import the kernel).
    on_attr_mutate: ?*const fn (a: u64) void = null,

    /// Phase 8: optional entity-level hook fired alongside `on_attr_mutate`
    /// with both `(e, a)`. Promotion module installs this to demote any
    /// materialized entity whose facts change.
    on_entity_mutate: ?*const fn (e: u64, a: u64) callconv(.c) void = null,

    /// Phase 9.5.2: cardinality schema injection. When non-null, `assert`
    /// and `currentValue` branch on the cardinality of the attribute being
    /// touched. Cardinality is stored as a fact `[a :cardinality :many]`.
    /// Absence (or `:one`) keeps Phase 0-9 single-value semantics; `:many`
    /// flips to multi-datom storage with RRB-Ref read materialization
    /// (ADR-9.5-D2). The schema is fact-shaped — bootstrap injects the
    /// well-known EIDs after interning so this module stays kernel-decoupled.
    cardinality_schema: ?CardinalitySchema = null,

    /// Per-attribute storage-tier *policy* (W6 cut #5 / Q3): maps an attribute
    /// EID to its full `AttrTier` (which indexes / history it gets). Injected by
    /// bootstrap so this module stays kernel-decoupled, exactly like
    /// `cardinality_schema`. This is the general form — "tier is swappable policy
    /// over the one store" (`docs/10a` north star). MUST be a pure function of
    /// the EID — the replay vehicle (`replayEventsInto`) copies it and relies on
    /// that purity. When set, it supersedes `code_attr_pred`.
    tier_policy: ?*const fn (u64) AttrTier = null,

    /// Convenience predicate: "is this a code attribute?" → the fixed code tier
    /// (`{no_history, skip_vaet}`, docs/19a). Subsumed by `tier_policy`; kept as
    /// the common binary case (used by tests). A pure function of the EID, same
    /// replay-purity contract as `tier_policy`.
    code_attr_pred: ?*const fn (u64) bool = null,

    /// Phase 7.3 reification: lazy fact-EID side index. Populated on first
    /// `:as` resolution. Key = hashTriple(e, a, v).
    fact_eids: std.AutoHashMap(u64, u64),
    /// Reverse mapping populated alongside fact_eids: fact_eid -> module.
    fact_modules: std.AutoHashMap(u64, u64),
    /// Optional object table for minting fresh fact-EIDs during lazy
    /// reification. If null, ensureFactEid returns null (caller can pre-
    /// register manually for tests).
    eid_alloc_table: ?*@import("object_table.zig").ObjectTable = null,

    pub fn init(allocator: std.mem.Allocator) FactStore {
        return .{
            .allocator = allocator,
            .next_tx = std.atomic.Value(u64).init(1),
            .basis_t = std.atomic.Value(u64).init(1),
            .eavt = EAVTish.empty(allocator, .{}),
            .aevt = EAVTish.empty(allocator, .{}),
            .avet = AVETish.empty(allocator, .{}),
            .vaet = AVETish.empty(allocator, .{}),
            .meat = MEATish.empty(allocator, .{}),
            .on_attr_mutate = null,
            .on_entity_mutate = null,
            .fact_eids = std.AutoHashMap(u64, u64).init(allocator),
            .fact_modules = std.AutoHashMap(u64, u64).init(allocator),
        };
    }

    /// The allocator the five persistent indexes (and their nested maps +
    /// history vecs) allocate from: the index epoch arena when enabled, else
    /// `allocator`. Query results and transient collectors deliberately stay
    /// on `allocator` (they outlive / are independent of an index epoch).
    pub fn indexAlloc(self: *const FactStore) std.mem.Allocator {
        return if (self.index_arena) |ia| ia.allocator() else self.allocator;
    }

    /// Phase 15.I — route index allocation to a dedicated epoch arena. MUST be
    /// called on an empty store (right after `init`, before any assert) so the
    /// five roots are rebuilt on the arena and every later node lands there.
    /// Live entry points (shell/aot) call this; tests opt in explicitly.
    pub fn enableIndexArena(self: *FactStore) !void {
        if (self.index_arena != null) return;
        const ia = try self.allocator.create(std.heap.ArenaAllocator);
        ia.* = std.heap.ArenaAllocator.init(self.allocator);
        self.index_arena = ia;
        const a = ia.allocator();
        // Rebuild the (empty) roots so their STORED allocator is the arena —
        // assoc/dissoc path-copy via the stored allocator, not the call param.
        self.eavt = EAVTish.empty(a, .{});
        self.aevt = EAVTish.empty(a, .{});
        self.avet = AVETish.empty(a, .{});
        self.vaet = AVETish.empty(a, .{});
        self.meat = MEATish.empty(a, .{});
    }

    /// Release the index epoch arena (if any). The live store never calls this
    /// (process exit reclaims); tests that `enableIndexArena` call it to stay
    /// leak-clean. Index nodes on `allocator` (arena disabled) are not owned
    /// here — the caller's allocator frees them.
    pub fn deinitIndexArena(self: *FactStore) void {
        if (self.index_arena) |ia| {
            ia.deinit();
            self.allocator.destroy(ia);
            self.index_arena = null;
        }
        // 15.R: free any still-retained epochs (hot arena or cold segment).
        for (self.retained_epochs.items) |*re| self.freeRetainedEpoch(re);
        self.retained_epochs.deinit(self.allocator);
        self.retained_epochs = .{};
    }

    // -- Index epoch management (Phase 15.R R1b) -----------------------------

    /// The smallest basis-T that must be retained. Any epoch whose entire
    /// basis range falls below this is collectible. Default (no retention,
    /// no live snapshots) = `currentBasisT() + 1` → keep only "now", so a
    /// swapped-out historical epoch is freed immediately (identical to 15.I).
    /// Snapshots (R1c) and the retention horizon (R2) lower this floor.
    pub fn oldestRetainedBasisT(self: *const FactStore) u64 {
        if (self.retain_history) return 1; // legacy "retain all" gate
        var oldest = self.currentBasisT() +| 1;
        // Live snapshots pin epochs directly via refcount; the retention
        // horizon floor is folded in here. The AOT `--strip=history` floor
        // (= now) is already the default.
        if (self.retentionFloor()) |floor| oldest = @min(oldest, floor);
        return oldest;
    }

    /// The basis-T floor implied by the retention policy, or null for
    /// current-only. `keep_last_n` is computed against the live basis so the
    /// window slides forward as the store advances.
    pub fn retentionFloor(self: *const FactStore) ?u64 {
        return switch (self.retention_policy) {
            .current_only => null,
            .keep_all => 1,
            .keep_since_basis => |b| b,
            .keep_last_n => |n| blk: {
                const cur = self.currentBasisT();
                break :blk if (cur >= n) cur - n + 1 else 1;
            },
        };
    }

    /// Set the retention policy (R2). Opt into time-travel beyond "now".
    pub fn setRetention(self: *FactStore, policy: RetentionPolicy) void {
        self.retention_policy = policy;
    }

    /// Pin the current writable epoch (a snapshot captured it). Returns the
    /// generation so the holder can `unpinEpoch` exactly that epoch later.
    pub fn pinCurrentEpoch(self: *FactStore) u64 {
        self.index_epoch_refcount += 1;
        return self.index_epoch_gen;
    }

    /// Pin epoch `gen` — the current writable epoch or a retained archive.
    pub fn pinEpoch(self: *FactStore, gen: u64) void {
        if (gen == self.index_epoch_gen) {
            self.index_epoch_refcount += 1;
            return;
        }
        for (self.retained_epochs.items) |*re| {
            if (re.gen == gen) {
                re.refcount += 1;
                return;
            }
        }
    }

    /// Release a pin on epoch `gen`. If that epoch is a swapped-out retained
    /// epoch whose refcount reaches 0 and which falls below the retention
    /// floor, free it wholesale and drop it from the list. A pin on the
    /// still-current epoch just decrements its counter.
    pub fn unpinEpoch(self: *FactStore, gen: u64) void {
        if (gen == self.index_epoch_gen) {
            if (self.index_epoch_refcount > 0) self.index_epoch_refcount -= 1;
            return;
        }
        var i: usize = 0;
        while (i < self.retained_epochs.items.len) : (i += 1) {
            const re = &self.retained_epochs.items[i];
            if (re.gen == gen) {
                if (re.refcount > 0) re.refcount -= 1;
                if (re.refcount == 0 and re.max_basis < self.oldestRetainedBasisT()) {
                    self.freeRetainedEpoch(re);
                    _ = self.retained_epochs.swapRemove(i);
                }
                return;
            }
        }
    }

    /// Free a retained epoch's backing — the hot arena, or the cold segment
    /// file + its path string. Does not touch the list slot.
    fn freeRetainedEpoch(self: *FactStore, re: *RetainedEpoch) void {
        if (re.arena) |ar| {
            ar.deinit();
            self.allocator.destroy(ar);
            re.arena = null;
        }
        if (re.cold_path) |p| {
            std.fs.cwd().deleteFile(p) catch {};
            self.allocator.free(p);
            re.cold_path = null;
        }
    }

    // -- Cold-history tier (Phase 15.R R4) -----------------------------------

    /// One recorded history event. The full ordered log of an epoch — replaying
    /// it through `applyAssert`/`applyRetract` in `tx` order rebuilds the epoch
    /// index byte-faithfully (same algorithm that built it).
    pub const EpochEvent = struct { m: u64, e: u64, a: u64, v: Ref, tx: u64, basis: u64, op: Op };

    /// Collect the full ordered event log of an epoch's roots (walk MEAT — it
    /// carries the module column and the same per-cell history as EAVT). Sorted
    /// by `tx`. Caller frees the returned slice with `alloc`.
    fn collectEpochEvents(roots: IndexRoots, alloc: std.mem.Allocator) ![]EpochEvent {
        var events = std.ArrayList(EpochEvent){};
        errdefer events.deinit(alloc);
        var mit = roots.meat.iter();
        while (mit.next()) |m_kv| {
            const m = m_kv.key;
            var eit = m_kv.value.iter();
            while (eit.next()) |e_kv| {
                const e = e_kv.key;
                var ait = e_kv.value.iter();
                while (ait.next()) |a_kv| {
                    const a = a_kv.key;
                    const cell = a_kv.value;
                    const n = cell.history.count();
                    var i: usize = 0;
                    while (i < n) : (i += 1) {
                        const h = cell.history.nth(i);
                        try events.append(alloc, .{ .m = m, .e = e, .a = a, .v = h.v, .tx = h.tx, .basis = h.basis, .op = h.op });
                    }
                }
            }
        }
        const slice = try events.toOwnedSlice(alloc);
        std.mem.sort(EpochEvent, slice, {}, struct {
            fn lt(_: void, x: EpochEvent, y: EpochEvent) bool {
                return x.tx < y.tx;
            }
        }.lt);
        return slice;
    }

    /// Serialize an event log to a flat byte buffer (segment payload). Caller
    /// frees. Format: u64 count, then per event m,e,a,v,tx,basis (u64 LE) + op
    /// (u8).
    fn serializeEvents(events: []const EpochEvent, alloc: std.mem.Allocator) ![]u8 {
        var buf = std.ArrayList(u8){};
        errdefer buf.deinit(alloc);
        var hdr: [8]u8 = undefined;
        std.mem.writeInt(u64, &hdr, events.len, .little);
        try buf.appendSlice(alloc, &hdr);
        for (events) |ev| {
            inline for (.{ ev.m, ev.e, ev.a, @as(u64, ev.v), ev.tx, ev.basis }) |word| {
                var w: [8]u8 = undefined;
                std.mem.writeInt(u64, &w, word, .little);
                try buf.appendSlice(alloc, &w);
            }
            try buf.append(alloc, @intFromEnum(ev.op));
        }
        return buf.toOwnedSlice(alloc);
    }

    fn deserializeEvents(bytes: []const u8, alloc: std.mem.Allocator) ![]EpochEvent {
        if (bytes.len < 8) return error.CorruptSegment;
        const count = std.mem.readInt(u64, bytes[0..8], .little);
        const rec = 6 * 8 + 1;
        if (bytes.len < 8 + count * rec) return error.CorruptSegment;
        const out = try alloc.alloc(EpochEvent, count);
        errdefer alloc.free(out);
        var off: usize = 8;
        var i: usize = 0;
        while (i < count) : (i += 1) {
            const m = std.mem.readInt(u64, bytes[off..][0..8], .little);
            const e = std.mem.readInt(u64, bytes[off + 8 ..][0..8], .little);
            const a = std.mem.readInt(u64, bytes[off + 16 ..][0..8], .little);
            const v = std.mem.readInt(u64, bytes[off + 24 ..][0..8], .little);
            const tx = std.mem.readInt(u64, bytes[off + 32 ..][0..8], .little);
            const basis = std.mem.readInt(u64, bytes[off + 40 ..][0..8], .little);
            const op: Op = @enumFromInt(bytes[off + 48]);
            out[i] = .{ .m = m, .e = e, .a = a, .v = v, .tx = tx, .basis = basis, .op = op };
            off += rec;
        }
        return out;
    }

    /// Replay an event log into a FRESH index epoch arena, returning the arena
    /// and rebuilt roots. Uses a throwaway FactStore as the replay vehicle so
    /// `applyAssert`/`applyRetract` (the live index algorithm) reconstruct the
    /// epoch faithfully. Events must be tx-sorted.
    /// Replay an event log into fresh roots on `alloc` (the live index
    /// algorithm via a throwaway FactStore vehicle). Events must be tx-sorted.
    fn replayEventsInto(self: *FactStore, events: []const EpochEvent, alloc: std.mem.Allocator) !IndexRoots {
        var tmp = FactStore.init(alloc);
        tmp.cardinality_schema = self.cardinality_schema; // faithful :many handling
        tmp.tier_policy = self.tier_policy; // §0.1: same tier per fact on replay
        tmp.code_attr_pred = self.code_attr_pred; // (convenience fallback, idem)
        for (events) |ev| {
            switch (ev.op) {
                .assert => try tmp.applyAssert(ev.e, ev.a, ev.v, ev.m, ev.tx, ev.basis),
                .retract => _ = try tmp.applyRetract(ev.e, ev.a, ev.v, ev.m, ev.tx, ev.basis),
            }
        }
        return .{ .eavt = tmp.eavt, .aevt = tmp.aevt, .avet = tmp.avet, .vaet = tmp.vaet, .meat = tmp.meat };
    }

    fn faultInEvents(self: *FactStore, events: []const EpochEvent) !struct { arena: *std.heap.ArenaAllocator, roots: IndexRoots } {
        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        errdefer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        errdefer arena.deinit();
        const roots = try self.replayEventsInto(events, arena.allocator());
        return .{ .arena = arena, .roots = roots };
    }

    /// The current writable epoch's roots as an `IndexRoots`.
    fn currentRoots(self: *const FactStore) IndexRoots {
        return .{ .eavt = self.eavt, .aevt = self.aevt, .avet = self.avet, .vaet = self.vaet, .meat = self.meat };
    }

    /// Spill a retained epoch COLD: serialize its event log to a segment file
    /// under `dir`, free the in-RAM arena, and mark it cold. Bounds resident
    /// set by working set (VISION §Memory ¶cold-history). No-op if already cold
    /// or not found. The mmap RO fast path (vs replay) is Phase 12.5.
    pub fn spillEpoch(self: *FactStore, gen: u64, dir: []const u8) !void {
        for (self.retained_epochs.items) |*re| {
            if (re.gen != gen) continue;
            if (re.arena == null) return; // already cold
            const events = try collectEpochEvents(re.roots, self.allocator);
            defer self.allocator.free(events);
            const bytes = try serializeEvents(events, self.allocator);
            defer self.allocator.free(bytes);
            const path = try std.fmt.allocPrint(self.allocator, "{s}/birnam-epoch-{d}.seg", .{ dir, gen });
            errdefer self.allocator.free(path);
            try std.fs.cwd().writeFile(.{ .sub_path = path, .data = bytes });
            re.arena.?.deinit();
            self.allocator.destroy(re.arena.?);
            re.arena = null;
            re.roots = undefined; // invalid until fault-in
            re.cold_path = path;
            return;
        }
    }

    /// Fault a cold epoch back into RAM: read its segment, replay into a fresh
    /// arena, restore `roots`. No-op if hot or not found. Deletes the segment
    /// (it will be re-spilled if evicted again).
    pub fn faultInEpoch(self: *FactStore, gen: u64) !void {
        for (self.retained_epochs.items) |*re| {
            if (re.gen != gen) continue;
            const path = re.cold_path orelse return; // already hot
            const bytes = try std.fs.cwd().readFileAlloc(self.allocator, path, 1 << 34);
            defer self.allocator.free(bytes);
            const events = try deserializeEvents(bytes, self.allocator);
            defer self.allocator.free(events);
            const res = try self.faultInEvents(events);
            re.arena = res.arena;
            re.roots = res.roots;
            std.fs.cwd().deleteFile(path) catch {};
            self.allocator.free(path);
            re.cold_path = null;
            return;
        }
    }

    /// Phase 15.R R4 — auto-eviction cadence. Keep the `keep_hot_n` newest
    /// retained epochs (by gen) in RAM; spill the rest to `dir` IF they are hot
    /// and unpinned. A PINNED epoch is never spilled — a live snapshot's view
    /// holds its roots by value, so freeing the arena would dangle it; pinned
    /// epochs stay hot regardless of age. No-op under current-only retention
    /// (no retained epochs). Returns the number spilled. Call at a quiescent
    /// point (the shell line boundary, like `gc.maybeCollect`).
    pub fn maybeSpillEpochs(self: *FactStore, dir: []const u8, keep_hot_n: usize, scratch: std.mem.Allocator) !usize {
        const n = self.retained_epochs.items.len;
        if (n <= keep_hot_n) return 0;
        // cutoff = the keep_hot_n-th highest gen; epochs with gen < cutoff are
        // spill candidates (gens are unique → exactly keep_hot_n stay hot).
        var gens = try scratch.alloc(u64, n);
        defer scratch.free(gens);
        for (self.retained_epochs.items, 0..) |re, i| gens[i] = re.gen;
        std.mem.sort(u64, gens, {}, comptime std.sort.desc(u64));
        const cutoff = gens[keep_hot_n - 1];
        var spilled: usize = 0;
        for (self.retained_epochs.items) |re| {
            if (re.gen < cutoff and re.arena != null and re.refcount == 0) {
                try self.spillEpoch(re.gen, dir);
                spilled += 1;
            }
        }
        return spilled;
    }

    // -- Excision (Phase 15.R R3) --------------------------------------------

    /// True if any index epoch (current or retained) is pinned by a live
    /// snapshot. Excise refuses while this holds (Decision C): a snapshot's
    /// view captured roots BY VALUE, so we cannot reach in to scrub them —
    /// the user must drop those snapshots first.
    pub fn hasLiveSnapshots(self: *const FactStore) bool {
        if (self.index_epoch_refcount > 0) return true;
        for (self.retained_epochs.items) |re| {
            if (re.refcount > 0) return true;
        }
        return false;
    }

    /// Phase 15.R R3 — hard-delete every fact (current AND history) for entity
    /// `e`, from the live index AND every retained history archive (GDPR-shaped
    /// "forget this entity"). Distinct from `retract` (logical, keeps history)
    /// and `prune` (collapses below the horizon). Refuses with
    /// `error.SnapshotPins` if a live snapshot could still expose `e`
    /// (Decision C). Surgical: touches only `e`'s index paths, leaving every
    /// other entity's current values and history untouched. `scratch` holds
    /// transient (a, v) collections.
    pub fn exciseEntity(self: *FactStore, e: u64, scratch: std.mem.Allocator) !void {
        if (self.read_only) return error.ReadOnly;
        if (self.hasLiveSnapshots()) return error.SnapshotPins;

        // Collect e's current attrs first (for the post-removal mcache flush).
        var attrs = std.ArrayList(u64){};
        defer attrs.deinit(scratch);
        if (self.eavt.get(e)) |inner| {
            var ait = inner.iter();
            while (ait.next()) |kv| try attrs.append(scratch, kv.key);
        }

        // Live (current) roots.
        var cur = IndexRoots{ .eavt = self.eavt, .aevt = self.aevt, .avet = self.avet, .vaet = self.vaet, .meat = self.meat };
        _ = try exciseEntityFromRoots(&cur, e, scratch);
        self.eavt = cur.eavt;
        self.aevt = cur.aevt;
        self.avet = cur.avet;
        self.vaet = cur.vaet;
        self.meat = cur.meat;

        // Every retained history archive too (none are pinned — checked above).
        // Cold archives must be faulted in first so the scrub reaches them.
        for (self.retained_epochs.items) |*re| {
            if (re.cold_path != null) try self.faultInEpoch(re.gen);
            _ = try exciseEntityFromRoots(&re.roots, e, scratch);
        }

        // Flush dispatch caches keyed by e's (now-gone) attributes, and demote
        // any materialized struct for e.
        if (self.on_attr_mutate) |cb| for (attrs.items) |a| cb(a);
        if (self.on_entity_mutate) |cb| cb(e, 0);
    }

    /// Phase 15.R R3 — hard-delete every history event whose basis-T falls in
    /// `[from_basis, to_basis]` (inclusive), from the live index AND every
    /// retained archive. Rebuilds each affected index by replaying its event
    /// log with the in-range events removed — so current values correctly
    /// revert to whatever survived (an in-range assert removed reverts to the
    /// prior out-of-range value). Refuses while a live snapshot pins an epoch
    /// (Decision C). `scratch` holds the transient event log.
    pub fn exciseTxRange(self: *FactStore, from_basis: u64, to_basis: u64, scratch: std.mem.Allocator) !void {
        if (self.read_only) return error.ReadOnly;
        if (self.hasLiveSnapshots()) return error.SnapshotPins;
        if (from_basis > to_basis) return;

        var flush_attrs = std.AutoHashMap(u64, void).init(scratch);
        defer flush_attrs.deinit();

        // Live (current) epoch.
        try self.rewriteCurrentExcludingRange(from_basis, to_basis, scratch, &flush_attrs);
        // Retained archives (fault cold ones in first).
        for (self.retained_epochs.items) |*re| {
            if (re.cold_path != null) try self.faultInEpoch(re.gen);
            try self.rewriteArchiveExcludingRange(re, from_basis, to_basis, scratch, &flush_attrs);
        }

        if (self.on_attr_mutate) |cb| {
            var it = flush_attrs.keyIterator();
            while (it.next()) |a| cb(a.*);
        }
    }

    /// Collect an epoch's events minus those in [from,to]; record dropped attrs.
    fn filteredEvents(roots: IndexRoots, from: u64, to: u64, scratch: std.mem.Allocator, flush_attrs: *std.AutoHashMap(u64, void)) ![]EpochEvent {
        const all = try collectEpochEvents(roots, scratch);
        defer scratch.free(all);
        var kept = std.ArrayList(EpochEvent){};
        errdefer kept.deinit(scratch);
        for (all) |ev| {
            if (ev.basis >= from and ev.basis <= to) {
                try flush_attrs.put(ev.a, {});
            } else {
                try kept.append(scratch, ev);
            }
        }
        return kept.toOwnedSlice(scratch);
    }

    fn rewriteCurrentExcludingRange(self: *FactStore, from: u64, to: u64, scratch: std.mem.Allocator, flush_attrs: *std.AutoHashMap(u64, void)) !void {
        const kept = try filteredEvents(self.currentRoots(), from, to, scratch, flush_attrs);
        defer scratch.free(kept);
        if (self.index_arena) |_| {
            const res = try self.faultInEvents(kept); // new arena + roots
            const old = self.index_arena.?;
            self.index_arena = res.arena;
            self.eavt = res.roots.eavt;
            self.aevt = res.roots.aevt;
            self.avet = res.roots.avet;
            self.vaet = res.roots.vaet;
            self.meat = res.roots.meat;
            old.deinit();
            self.allocator.destroy(old);
        } else {
            const roots = try self.replayEventsInto(kept, self.allocator);
            self.eavt = roots.eavt;
            self.aevt = roots.aevt;
            self.avet = roots.avet;
            self.vaet = roots.vaet;
            self.meat = roots.meat;
        }
    }

    fn rewriteArchiveExcludingRange(self: *FactStore, re: *RetainedEpoch, from: u64, to: u64, scratch: std.mem.Allocator, flush_attrs: *std.AutoHashMap(u64, void)) !void {
        const kept = try filteredEvents(re.roots, from, to, scratch, flush_attrs);
        defer scratch.free(kept);
        const res = try self.faultInEvents(kept); // new arena + roots
        const old = re.arena.?;
        re.arena = res.arena;
        re.roots = res.roots;
        old.deinit();
        self.allocator.destroy(old);
    }

    // -- Snapshots (Phase 15.R R1c) ------------------------------------------

    /// Capture an immutable zero-copy snapshot of the store as of NOW. The
    /// returned `Snapshot` owns a read-only `FactStore` view that holds the
    /// five index roots BY VALUE — structural sharing means later live
    /// mutations (path-copy) leave the snapshot's roots seeing the old state.
    /// The snapshot PINS its index epoch (refcount), so `compact()` keeps that
    /// epoch alive instead of freeing it — this is the reachability question
    /// for the fact layer, over retained roots not nodes (VISION §Memory).
    /// Caller owns the returned pointer; `snap.deinit(allocator)` unpins +
    /// frees. (R1d hands ownership to a SNAPSHOT_VT object the value-heap GC
    /// sweeps, calling deinit on collection.)
    ///
    /// `snapshotNow` is the frozen-roots path (state as-of the capture basis,
    /// readable forever even after the live store prunes). Arbitrary
    /// `asOf: <past basis>` reconstruction is a follow-on (needs a snapshot
    /// taken at that basis, or history-walk-backed reads — see R2).
    pub fn snapshotNow(self: *FactStore) !*Snapshot {
        return self.buildSnapshot(.{
            .eavt = self.eavt,
            .aevt = self.aevt,
            .avet = self.avet,
            .vaet = self.vaet,
            .meat = self.meat,
        }, self.currentBasisT(), self.index_epoch_gen, null);
    }

    /// Phase 15.R R2 — capture a snapshot as of a PAST basis-T. Routes to the
    /// epoch whose basis range covers `basis` (the current writable epoch, or a
    /// retained history archive), pins it, and returns a view whose reads are
    /// `asOf:`-walked to `basis`. Requires that history reached `basis` is still
    /// retained (the retention policy governs that) — otherwise
    /// `error.HistoryNotRetained`. `basis >= now` degrades to `snapshotNow`.
    pub fn snapshotAsOf(self: *FactStore, basis: u64) !*Snapshot {
        if (basis >= self.currentBasisT()) return self.snapshotNow();
        // Current writable epoch holds history back to its min_basis.
        if (basis >= self.index_epoch_min_basis) {
            return self.buildSnapshot(.{
                .eavt = self.eavt,
                .aevt = self.aevt,
                .avet = self.avet,
                .vaet = self.vaet,
                .meat = self.meat,
            }, basis, self.index_epoch_gen, basis);
        }
        // Otherwise find a retained archive epoch covering `basis`, faulting it
        // back from disk first if it has been spilled cold (R4).
        for (self.retained_epochs.items) |*re| {
            if (basis >= re.min_basis and basis <= re.max_basis) {
                if (re.cold_path != null) try self.faultInEpoch(re.gen);
                return self.buildSnapshot(re.roots, basis, re.gen, basis);
            }
        }
        return error.HistoryNotRetained;
    }

    /// Build a Snapshot over `roots`, pinning epoch `gen`. `as_of` null = read
    /// current values of those roots (a "now" snapshot of that epoch); non-null
    /// = `asOf:`-walk reads to that basis.
    fn buildSnapshot(self: *FactStore, roots: IndexRoots, basis: u64, gen: u64, as_of: ?u64) !*Snapshot {
        const snap = try self.allocator.create(Snapshot);
        errdefer self.allocator.destroy(snap);
        var v = self.*; // copies scalar config (allocator, schema, etc.)
        v.eavt = roots.eavt;
        v.aevt = roots.aevt;
        v.avet = roots.avet;
        v.vaet = roots.vaet;
        v.meat = roots.meat;
        v.read_only = true;
        v.eid_alloc_table = null; // no reification mutation through a snapshot
        v.on_attr_mutate = null;
        v.on_entity_mutate = null;
        v.retained_epochs = .{}; // view owns no epochs (don't alias live's list)
        // Independent, lazily-allocated reification maps so a snapshot read
        // never aliases (or is invalidated by) the live store's maps.
        v.fact_eids = std.AutoHashMap(u64, u64).init(self.allocator);
        v.fact_modules = std.AutoHashMap(u64, u64).init(self.allocator);
        self.pinEpoch(gen);
        snap.* = .{
            .view = v,
            .basis_t = basis,
            .epoch_gen = gen,
            .as_of_basis = as_of,
            .store = self,
        };
        return snap;
    }

    // -- Phase 7.3 reification helpers ---------------------------------------

    fn hashTriple(e: u64, a: u64, v: Ref) u64 {
        var h: u64 = 0x9E3779B97F4A7C15;
        h = (h ^ e) *% 0xBF58476D1CE4E5B9;
        h = (h ^ a) *% 0xBF58476D1CE4E5B9;
        h = (h ^ v) *% 0xBF58476D1CE4E5B9;
        return h;
    }

    /// Look up existing fact-EID for (e, a, v), or null if not yet reified.
    pub fn lookupFactEid(self: *const FactStore, e: u64, a: u64, v: Ref) ?u64 {
        return self.fact_eids.get(hashTriple(e, a, v));
    }

    /// Register a freshly minted fact-EID with its module. Caller (query
    /// engine) allocates the EID via object_table.alloc.
    pub fn registerFactEid(
        self: *FactStore,
        e: u64,
        a: u64,
        v: Ref,
        fact_eid: u64,
        module_eid: u64,
    ) !void {
        try self.fact_eids.put(hashTriple(e, a, v), fact_eid);
        try self.fact_modules.put(fact_eid, module_eid);
    }

    /// Reverse lookup: find the module that asserted a fact, by walking
    /// MEAT. Used by lazy reification to populate fact_modules cache.
    /// O(modules × facts_per_module) — Phase 7.3 spike accepts the slow
    /// path; the proper 7.3.7 reification work eliminates it.
    pub fn findModuleOfFact(self: *FactStore, e: u64, a: u64, v: Ref) ?u64 {
        var m_iter = self.meat.iter();
        while (m_iter.next()) |entry| {
            const m = entry.key;
            const e_map = entry.value;
            const inner = e_map.get(e) orelse continue;
            const cell = inner.get(a) orelse continue;
            if (cell.current_v) |cv| {
                if (cv == v) return m;
            }
        }
        return null;
    }

    pub fn moduleOfFactEid(self: *const FactStore, fact_eid: u64) ?u64 {
        return self.fact_modules.get(fact_eid);
    }

    pub fn setEidAllocTable(self: *FactStore, t: *@import("object_table.zig").ObjectTable) void {
        self.eid_alloc_table = t;
    }

    /// Look up an existing fact-EID; mint one (and record its module) if
    /// absent. Returns null only if no eid_alloc_table is configured.
    pub fn ensureFactEid(self: *FactStore, e: u64, a: u64, v: Ref) ?u64 {
        if (self.lookupFactEid(e, a, v)) |existing| return existing;
        const table = self.eid_alloc_table orelse return null;
        const new_eid = table.alloc();
        const m = self.findModuleOfFact(e, a, v) orelse 0;
        self.registerFactEid(e, a, v, new_eid, m) catch return null;
        return new_eid;
    }

    // -- Mutations -----------------------------------------------------------

    /// Frozen bulk insert (Phase 14 `--strip=history`). Builds the five
    /// indexes directly — single tx (=1), no history churn, no implicit-
    /// retract, no mutate hooks. Safe only over a current-values-only image
    /// replay: one fact per (e,a) for `:one` attrs (so there's nothing to
    /// implicit-retract), values accrete for `:many`. `loadFrozen` is its
    /// only caller; it never references `assert`, so the tx-writer path is
    /// skipped during a frozen load.
    pub fn frozenInsert(self: *FactStore, e: u64, a: u64, v: Ref, m: u64) !void {
        const tier = self.attrTier(a); // carry the same tier as the live path (§2.3)
        const nh = tier.no_history;
        self.eavt = try updateValueLeaf(self.indexAlloc(), self.eavt, e, a, v, .{ .v = v, .tx = 1, .op = .assert }, nh);
        self.aevt = try updateValueLeaf(self.indexAlloc(), self.aevt, a, e, v, .{ .v = v, .tx = 1, .op = .assert }, nh);
        self.meat = try updateMeat(self.indexAlloc(), self.meat, m, e, a, v, .{ .v = v, .tx = 1, .op = .assert }, nh);
        self.avet = try updatePresenceLeaf(self.indexAlloc(), self.avet, a, v, e, true, .{ .tx = 1, .op = .assert }, nh);
        if (!tier.skip_vaet)
            self.vaet = try updatePresenceLeaf(self.indexAlloc(), self.vaet, v, a, e, true, .{ .tx = 1, .op = .assert }, nh);
    }

    /// Phase 15.R — compact's collapse insert. Like `frozenInsert` but writes
    /// an explicit `tx` (sequential per-epoch coordinate) and `basis` (the
    /// single collapse basis, so `asOf:` reads of the collapsed epoch are
    /// correct), fires `on_attr_mutate` (mcache flush) but never
    /// `on_entity_mutate` (no demote churn during a rebuild). No implicit-
    /// retract: the collapse source is current-only (one value per (e,a) for
    /// `:one`; `:many` accretes as present). Does not touch the tx/basis
    /// counters — `compact` manages those.
    fn compactInsert(self: *FactStore, e: u64, a: u64, v: Ref, m: u64, tx: u64, basis: u64) !void {
        const ia = self.indexAlloc();
        const tier = self.attrTier(a); // rebuild must agree with the live path, else
        const nh = tier.no_history; //    compact re-materializes the skipped VAET (§2.5 #4)
        self.eavt = try updateValueLeaf(ia, self.eavt, e, a, v, .{ .v = v, .tx = tx, .op = .assert, .basis = basis }, nh);
        self.aevt = try updateValueLeaf(ia, self.aevt, a, e, v, .{ .v = v, .tx = tx, .op = .assert, .basis = basis }, nh);
        self.meat = try updateMeat(ia, self.meat, m, e, a, v, .{ .v = v, .tx = tx, .op = .assert, .basis = basis }, nh);
        self.avet = try updatePresenceLeaf(ia, self.avet, a, v, e, true, .{ .tx = tx, .op = .assert, .basis = basis }, nh);
        if (!tier.skip_vaet)
            self.vaet = try updatePresenceLeaf(ia, self.vaet, v, a, e, true, .{ .tx = tx, .op = .assert, .basis = basis }, nh);
        if (self.on_attr_mutate) |cb| cb(a);
    }

    /// Lock the store read-only (Phase 14 `--strip=history`). After this,
    /// `assert`/`retract` reject. Idempotent.
    pub fn freeze(self: *FactStore) void {
        self.read_only = true;
    }

    /// Core index maintenance for an assert at explicit (tx, basis), no
    /// counter bump / no read_only check / no hooks. Shared by `assert` (live)
    /// and `replayEvent` (R4 fault-in) so a faulted-in epoch is rebuilt by the
    /// EXACT same algorithm that built it — faithful by construction.
    fn applyAssert(self: *FactStore, e: u64, a: u64, v: Ref, m: u64, tx: u64, bt: u64) !void {
        const tier = self.attrTier(a); // Levers 1+2: per-attr no_history / skip_vaet
        const nh = tier.no_history;
        const prev_v = self.rawCurrentValue(e, a);
        const is_many = self.cardinalityOfAttr(a) == .many;
        self.eavt = try updateValueLeaf(self.indexAlloc(), self.eavt, e, a, v, .{ .v = v, .tx = tx, .op = .assert, .basis = bt }, nh);
        self.aevt = try updateValueLeaf(self.indexAlloc(), self.aevt, a, e, v, .{ .v = v, .tx = tx, .op = .assert, .basis = bt }, nh);
        self.meat = try updateMeat(self.indexAlloc(), self.meat, m, e, a, v, .{ .v = v, .tx = tx, .op = .assert, .basis = bt }, nh);
        if (!is_many) {
            if (prev_v) |pv| {
                if (pv != v) {
                    self.avet = try updatePresenceLeaf(self.indexAlloc(), self.avet, a, pv, e, false, .{ .tx = tx, .op = .retract, .basis = bt }, nh);
                    if (!tier.skip_vaet)
                        self.vaet = try updatePresenceLeaf(self.indexAlloc(), self.vaet, pv, a, e, false, .{ .tx = tx, .op = .retract, .basis = bt }, nh);
                }
            }
        }
        self.avet = try updatePresenceLeaf(self.indexAlloc(), self.avet, a, v, e, true, .{ .tx = tx, .op = .assert, .basis = bt }, nh);
        if (!tier.skip_vaet)
            self.vaet = try updatePresenceLeaf(self.indexAlloc(), self.vaet, v, a, e, true, .{ .tx = tx, .op = .assert, .basis = bt }, nh);
    }

    /// Core index maintenance for a retract at explicit (tx, basis). Returns
    /// whether it applied (false = no current (e,a,v) to retract). Shared by
    /// `retract` and `replayEvent`.
    fn applyRetract(self: *FactStore, e: u64, a: u64, v: Ref, m: u64, tx: u64, bt: u64) !bool {
        if (!self.hasFact(e, a, v)) return false; // AVET-gated — retained for code attrs
        const tier = self.attrTier(a);
        const nh = tier.no_history;
        self.eavt = try updateValueLeaf(self.indexAlloc(), self.eavt, e, a, null, .{ .v = v, .tx = tx, .op = .retract, .basis = bt }, nh);
        self.aevt = try updateValueLeaf(self.indexAlloc(), self.aevt, a, e, null, .{ .v = v, .tx = tx, .op = .retract, .basis = bt }, nh);
        self.meat = try updateMeat(self.indexAlloc(), self.meat, m, e, a, null, .{ .v = v, .tx = tx, .op = .retract, .basis = bt }, nh);
        self.avet = try updatePresenceLeaf(self.indexAlloc(), self.avet, a, v, e, false, .{ .tx = tx, .op = .retract, .basis = bt }, nh);
        if (!tier.skip_vaet)
            self.vaet = try updatePresenceLeaf(self.indexAlloc(), self.vaet, v, a, e, false, .{ .tx = tx, .op = .retract, .basis = bt }, nh);
        return true;
    }

    pub fn assert(self: *FactStore, e: u64, a: u64, v: Ref, m: u64) !u64 {
        if (self.read_only) return error.ReadOnly;
        if (builtin.mode == .Debug) {
            if (ref_mod.isNil(v)) @panic("FactStore.assert: NIL value forbidden (Date discipline)");
        }
        const tx = self.next_tx.fetchAdd(1, .monotonic);
        const bt = self.basis_t.fetchAdd(1, .monotonic); // 15.R: never resets on compact
        try self.applyAssert(e, a, v, m, tx, bt);
        if (self.on_attr_mutate) |cb| cb(a);
        if (self.on_entity_mutate) |cb| cb(e, a);
        return tx;
    }

    pub fn retract(self: *FactStore, e: u64, a: u64, v: Ref, m: u64) !u64 {
        if (self.read_only) return error.ReadOnly;
        const tx = self.next_tx.fetchAdd(1, .monotonic);
        const bt = self.basis_t.fetchAdd(1, .monotonic); // 15.R: never resets on compact
        if (!try self.applyRetract(e, a, v, m, tx, bt)) return tx;
        if (self.on_attr_mutate) |cb| cb(a);
        if (self.on_entity_mutate) |cb| cb(e, a);
        return tx;
    }

    // -- Cardinality (Phase 9.5.2) -------------------------------------------

    pub fn setCardinalitySchema(self: *FactStore, schema: CardinalitySchema) void {
        self.cardinality_schema = schema;
    }

    /// Inject the full per-attribute storage-tier policy (W6 cut #5 / Q3). The
    /// general form: any attribute → any `AttrTier`. Supersedes `code_attr_pred`.
    pub fn setTierPolicy(self: *FactStore, policy: *const fn (u64) AttrTier) void {
        self.tier_policy = policy;
    }

    /// Inject the "is this a code attribute?" predicate (the binary convenience
    /// case). Subsumed by `setTierPolicy`; retained for callers/tests that only
    /// need the fixed code tier.
    pub fn setCodeAttrPred(self: *FactStore, pred: *const fn (u64) bool) void {
        self.code_attr_pred = pred;
    }

    /// Storage tier for attribute `a`. The injected `tier_policy` decides it
    /// (defaulting, via the `code_attr_pred` convenience, to: code attrs drop
    /// history (Lever 1) and the VAET index (Lever 2)). Pure function of the EID
    /// — never reads store state, so the replay vehicle reproduces the same
    /// decision per fact (§0.1).
    pub fn attrTier(self: *const FactStore, a: u64) AttrTier {
        if (self.tier_policy) |policy| return policy(a);
        if (self.code_attr_pred) |pred| {
            if (pred(a)) return .{ .no_history = true, .skip_vaet = true };
        }
        return .{};
    }

    /// Lookup raw EAVT current_v without consulting cardinality. Used by
    /// internal code paths (assert/retract implicit-retract logic;
    /// `cardinalityOfAttr` itself) to break recursion.
    pub fn rawCurrentValue(self: *const FactStore, e: u64, a: u64) ?Ref {
        const inner = self.eavt.get(e) orelse return null;
        const cell = inner.get(a) orelse return null;
        return cell.current_v;
    }

    /// Cardinality of attribute `a`. Defaults to `.one` when no schema is
    /// injected or no `:cardinality` fact has been asserted on `a`.
    pub fn cardinalityOfAttr(self: *const FactStore, a: u64) Cardinality {
        const schema = self.cardinality_schema orelse return .one;
        // Base case: the cardinality attribute itself is single-valued.
        if (a == schema.attr_eid) return .one;
        const v = self.rawCurrentValue(a, schema.attr_eid) orelse return .one;
        return if (v == schema.many_value) .many else .one;
    }

    // -- Point queries -------------------------------------------------------

    /// Current value of (e, a). Polymorphic per ADR-9.5-D2:
    ///   - `:one` (default): single value Ref, or null when absent.
    ///   - `:many`: an RRB-Ref containing every currently-asserted value;
    ///     empty RRB-Ref when all values have been retracted; null only
    ///     when the attribute has never been touched on `e`.
    pub fn currentValue(self: *const FactStore, e: u64, a: u64) ?Ref {
        if (self.cardinalityOfAttr(a) == .many) {
            return self.materializeManyCurrent(e, a) catch null;
        }
        return self.rawCurrentValue(e, a);
    }

    /// Walk AVET[a] for every value `v` where `(a, v, e).present == true`,
    /// collect into an RRB-Ref. Returns null if `e` has never touched `a`
    /// (no presence cell ever existed for any v at this (a, e)).
    fn materializeManyCurrent(self: *const FactStore, e: u64, a: u64) !?Ref {
        const mid = self.avet.get(a) orelse return null;
        var any_cell_seen = false;
        var collected = std.ArrayList(Ref){};
        defer collected.deinit(self.allocator);
        var it = mid.iter();
        while (it.next()) |entry| {
            const v: Ref = entry.key;
            const leaf = entry.value;
            const cell = leaf.get(e) orelse continue;
            any_cell_seen = true;
            if (cell.present) try collected.append(self.allocator, v);
        }
        if (!any_cell_seen) return null;
        return try boxed.mkRrb(self.allocator, collected.items);
    }

    /// Iterator over the currently-asserted values of (e, a). Works for
    /// any cardinality — `:one` yields 0 or 1 value, `:many` yields N.
    pub const CurrentValuesIter = struct {
        outer: ?MidPresenceMap.Iterator,
        e: u64,

        pub fn next(self: *CurrentValuesIter) ?Ref {
            const it = &(self.outer orelse return null);
            while (it.next()) |entry| {
                const cell = entry.value.get(self.e) orelse continue;
                if (cell.present) return entry.key;
            }
            return null;
        }
    };

    pub fn currentValuesIter(self: *const FactStore, e: u64, a: u64) CurrentValuesIter {
        const mid = self.avet.get(a) orelse return .{ .outer = null, .e = e };
        return .{ .outer = mid.iter(), .e = e };
    }

    /// Returns the tx of the most recent mutation (assert or retract) on
    /// (e, a), or null if the cell has no history.
    pub fn lastTouchTx(self: *const FactStore, e: u64, a: u64) ?u64 {
        const inner = self.eavt.get(e) orelse return null;
        const cell = inner.get(a) orelse return null;
        const n = cell.history.count();
        if (n == 0) return null;
        return cell.history.nth(n - 1).tx;
    }

    /// Value of (e, a) as of the end of `as_of_tx` (i.e. counting only history
    /// entries with `tx <= as_of_tx`). Returns null if absent at that time.
    pub fn currentValueAsOf(self: *const FactStore, e: u64, a: u64, as_of_tx: u64) ?Ref {
        const inner = self.eavt.get(e) orelse return null;
        const cell = inner.get(a) orelse return null;

        // Walk history backward; return latest entry with tx <= as_of_tx.
        var i: usize = cell.history.count();
        while (i > 0) {
            i -= 1;
            const ent = cell.history.nth(i);
            if (ent.tx <= as_of_tx) {
                return if (ent.op == .assert) ent.v else null;
            }
        }
        return null;
    }

    /// Phase 15.R — exact value of (e, a) as of basis-T `basis` (the durable,
    /// non-resetting coordinate). Walks the cell history by `basis`. Returns
    /// null if absent at that basis. Reaches only as far back as history is
    /// retained (the retention horizon governs how far that is); entries
    /// written before basis tracking carry basis 0 and read as "from the
    /// beginning of time".
    pub fn currentValueAsOfBasis(self: *const FactStore, e: u64, a: u64, basis: u64) ?Ref {
        const inner = self.eavt.get(e) orelse return null;
        const cell = inner.get(a) orelse return null;
        var i: usize = cell.history.count();
        while (i > 0) {
            i -= 1;
            const ent = cell.history.nth(i);
            if (ent.basis <= basis) {
                return if (ent.op == .assert) ent.v else null;
            }
        }
        return null;
    }

    /// Phase 16 Tier E — every asserted value of (e, a) over time, oldest →
    /// newest, as an RRB vector Ref. Retracts are skipped (only the values a
    /// cell ever held). Empty vector when the cell has no history. Backs
    /// `[cell history]`; reaches only as far as the retention horizon keeps.
    pub fn valueHistory(self: *const FactStore, e: u64, a: u64) !Ref {
        const inner = self.eavt.get(e) orelse return boxed.mkRrb(self.allocator, &.{});
        const cell = inner.get(a) orelse return boxed.mkRrb(self.allocator, &.{});
        var collected = std.ArrayList(Ref){};
        defer collected.deinit(self.allocator);
        var i: usize = 0;
        const n = cell.history.count();
        while (i < n) : (i += 1) {
            const ent = cell.history.nth(i);
            if (ent.op == .assert) try collected.append(self.allocator, ent.v);
        }
        return boxed.mkRrb(self.allocator, collected.items);
    }

    // -- Existence / presence ------------------------------------------------

    /// True if entity `e` currently has any asserted attribute.
    pub fn entityExists(self: *const FactStore, e: u64) bool {
        const inner = self.eavt.get(e) orelse return false;
        var it = inner.iter();
        while (it.next()) |kv| {
            if (kv.value.current_v != null) return true;
        }
        return false;
    }

    /// True if (e, a, v) is currently asserted.
    pub fn hasFact(self: *const FactStore, e: u64, a: u64, v: Ref) bool {
        const inner = self.avet.get(a) orelse return false;
        const mid = inner.get(v) orelse return false;
        const cell = mid.get(e) orelse return false;
        return cell.present;
    }

    // -- Iterators -----------------------------------------------------------

    /// All current (a, v) pairs for an entity.
    pub const EntityAttrIter = struct {
        inner_it: ?InnerValueMap.Iterator,

        pub fn next(self: *EntityAttrIter) ?struct { a: u64, v: Ref } {
            const it = &(self.inner_it orelse return null);
            while (it.next()) |kv| {
                if (kv.value.current_v) |v| return .{ .a = kv.key, .v = v };
            }
            return null;
        }
    };

    pub fn attributesOf(self: *const FactStore, e: u64) EntityAttrIter {
        const inner = self.eavt.get(e) orelse return .{ .inner_it = null };
        return .{ .inner_it = inner.iter() };
    }

    /// All current (e, v) pairs with attribute `a`.
    pub const AttrEntityIter = struct {
        inner_it: ?InnerValueMap.Iterator,

        pub fn next(self: *AttrEntityIter) ?struct { e: u64, v: Ref } {
            const it = &(self.inner_it orelse return null);
            while (it.next()) |kv| {
                if (kv.value.current_v) |v| return .{ .e = kv.key, .v = v };
            }
            return null;
        }
    };

    pub fn entitiesWith(self: *const FactStore, a: u64) AttrEntityIter {
        const inner = self.aevt.get(a) orelse return .{ .inner_it = null };
        return .{ .inner_it = inner.iter() };
    }

    /// Entities with a specific (a, v) — AVET-backed equality lookup.
    pub const AttrValueEntityIter = struct {
        inner_it: ?InnerPresenceMap.Iterator,

        pub fn next(self: *AttrValueEntityIter) ?u64 {
            const it = &(self.inner_it orelse return null);
            while (it.next()) |kv| {
                if (kv.value.present) return kv.key;
            }
            return null;
        }
    };

    pub fn entitiesMatching(self: *const FactStore, a: u64, v: Ref) AttrValueEntityIter {
        const mid = self.avet.get(a) orelse return .{ .inner_it = null };
        const leaf = mid.get(v) orelse return .{ .inner_it = null };
        return .{ .inner_it = leaf.iter() };
    }

    /// All (a, e) pairs whose value is `v` — VAET reverse-ref walk.
    pub const ValueRefIter = struct {
        /// Pre-flattened list of present (a, e) pairs. Built eagerly because
        /// the two-level VAET walk doesn't fit the simple Iterator shape.
        items: std.ArrayList(AE),
        idx: usize,
        owner: std.mem.Allocator,

        pub const AE = struct { a: u64, e: u64 };

        pub fn next(self: *ValueRefIter) ?AE {
            if (self.idx >= self.items.items.len) return null;
            const v = self.items.items[self.idx];
            self.idx += 1;
            return v;
        }
        pub fn deinit(self: *ValueRefIter) void {
            self.items.deinit(self.owner);
        }
    };

    pub fn pointingAt(self: *const FactStore, v: Ref, scratch: std.mem.Allocator) !ValueRefIter {
        var out = std.ArrayList(ValueRefIter.AE){};
        const mid = self.vaet.get(v) orelse return .{ .items = out, .idx = 0, .owner = scratch };
        var a_it = mid.iter();
        while (a_it.next()) |a_kv| {
            var e_it = a_kv.value.iter();
            while (e_it.next()) |e_kv| {
                if (e_kv.value.present) try out.append(scratch, .{ .a = a_kv.key, .e = e_kv.key });
            }
        }
        return .{ .items = out, .idx = 0, .owner = scratch };
    }

    /// All currently-asserted (e, a, v) triples owned by module `m`.
    pub const ModuleFactIter = struct {
        items: std.ArrayList(EAV),
        idx: usize,
        owner: std.mem.Allocator,

        pub const EAV = struct { e: u64, a: u64, v: Ref };

        pub fn next(self: *ModuleFactIter) ?EAV {
            if (self.idx >= self.items.items.len) return null;
            const r = self.items.items[self.idx];
            self.idx += 1;
            return r;
        }
        pub fn deinit(self: *ModuleFactIter) void {
            self.items.deinit(self.owner);
        }
    };

    pub fn factsOfModule(self: *const FactStore, m: u64, scratch: std.mem.Allocator) !ModuleFactIter {
        var out = std.ArrayList(ModuleFactIter.EAV){};
        const e_map = self.meat.get(m) orelse return .{ .items = out, .idx = 0, .owner = scratch };
        var e_it = e_map.iter();
        while (e_it.next()) |e_kv| {
            var a_it = e_kv.value.iter();
            while (a_it.next()) |a_kv| {
                if (a_kv.value.current_v) |v| try out.append(scratch, .{ .e = e_kv.key, .a = a_kv.key, .v = v });
            }
        }
        return .{ .items = out, .idx = 0, .owner = scratch };
    }

    // -- Misc accessors ------------------------------------------------------

    pub fn currentTx(self: *const FactStore) u64 {
        return self.next_tx.load(.monotonic) - 1;
    }

    /// Phase 15.R — current basis-T (the durable, non-resetting time
    /// coordinate). Equals the basis-T of the most recent commit.
    pub fn currentBasisT(self: *const FactStore) u64 {
        return self.basis_t.load(.monotonic) - 1;
    }

    /// Flatten every current (module, entity, attribute, value) fact in
    /// `meat`. Phase 12.1 image writer's enumeration source — `meat` keys
    /// by module first, so a single nested walk yields the module column
    /// for free. Current values only (no history).
    pub const AllFactsIter = struct {
        m_it: MEATish.Iterator,
        e_it: ?EAVTish.Iterator = null,
        a_it: ?InnerValueMap.Iterator = null,
        cur_m: u64 = 0,
        cur_e: u64 = 0,

        pub const MEAV = struct { m: u64, e: u64, a: u64, v: Ref };

        pub fn next(self: *AllFactsIter) ?MEAV {
            while (true) {
                if (self.a_it) |*ait| {
                    while (ait.next()) |a_kv| {
                        if (a_kv.value.current_v) |v| {
                            return .{ .m = self.cur_m, .e = self.cur_e, .a = a_kv.key, .v = v };
                        }
                    }
                    self.a_it = null;
                }
                if (self.e_it) |*eit| {
                    if (eit.next()) |e_kv| {
                        self.cur_e = e_kv.key;
                        self.a_it = e_kv.value.iter();
                        continue;
                    }
                    self.e_it = null;
                }
                const m_kv = self.m_it.next() orelse return null;
                self.cur_m = m_kv.key;
                self.e_it = m_kv.value.iter();
            }
        }
    };

    pub fn meatIter(self: *const FactStore) AllFactsIter {
        return .{ .m_it = self.meat.iter() };
    }

    pub const CompactStats = struct { kept: usize, dropped: usize };

    /// Phase 13.2 — rewrite the indexes keeping only facts for which
    /// `keep(ctx, m, e, a, v)` returns true. Snapshots current facts,
    /// resets all five indexes to empty, and re-asserts the kept set at a
    /// fresh tx sequence. History collapses to current values (the kept
    /// facts), and the old persistent-index roots become unreferenced (GC).
    /// Callers that GC materialized structs should clear the object-table
    /// entries for dropped entities separately. `scratch` holds the
    /// transient snapshot; the store keeps using `self.allocator`.
    pub fn compact(
        self: *FactStore,
        scratch: std.mem.Allocator,
        ctx: anytype,
        comptime keep: fn (@TypeOf(ctx), m: u64, e: u64, a: u64, v: Ref) bool,
    ) !CompactStats {
        const Snap = struct { m: u64, e: u64, a: u64, v: Ref };
        var facts = std.ArrayList(Snap){};
        defer facts.deinit(scratch);
        var it = self.meatIter();
        while (it.next()) |f| try facts.append(scratch, .{ .m = f.m, .e = f.e, .a = f.a, .v = f.v });
        // `facts` holds (m,e,a,v) by value — v is a Ref (u64). No pointer into
        // the old index escapes, so the old epoch is safe to free below.

        // Phase 15.I/R1b — swap to a fresh index epoch. Re-asserting kept facts
        // rebuilds the index entirely in the new epoch (shares no node with the
        // old). The OLD epoch is then either freed wholesale (nothing pins it —
        // the 15.I current-only path) or moved to `retained_epochs` (a live
        // snapshot pins it, or the retention horizon needs its history — 15.R).
        // Skipped when the arena is disabled (tests → index on `self.allocator`)
        // or when `retain_history` is set (legacy 15.I no-swap leak-in-place).
        const basis_at_compact = self.currentBasisT();
        var old_meta: ?RetainedEpoch = null;
        if (self.index_arena != null and !self.retain_history) {
            const fresh = try self.allocator.create(std.heap.ArenaAllocator);
            fresh.* = std.heap.ArenaAllocator.init(self.allocator);
            old_meta = .{
                .arena = self.index_arena.?,
                .gen = self.index_epoch_gen,
                .min_basis = self.index_epoch_min_basis,
                .max_basis = basis_at_compact,
                .refcount = self.index_epoch_refcount,
                // Capture the OLD roots NOW (before the reset below) so a
                // retained epoch stays queryable as a history archive.
                .roots = .{ .eavt = self.eavt, .aevt = self.aevt, .avet = self.avet, .vaet = self.vaet, .meat = self.meat },
            };
            self.index_arena = fresh; // indexAlloc() now hands out the new epoch
            self.index_epoch_gen += 1;
            self.index_epoch_refcount = 0;
            self.index_epoch_min_basis = basis_at_compact; // collapse point
        }

        // Reset indexes + reification side tables. next_tx restarts at 1.
        const ia = self.indexAlloc();
        self.eavt = EAVTish.empty(ia, .{});
        self.aevt = EAVTish.empty(ia, .{});
        self.avet = AVETish.empty(ia, .{});
        self.vaet = AVETish.empty(ia, .{});
        self.meat = MEATish.empty(ia, .{});
        self.fact_eids.clearRetainingCapacity();
        self.fact_modules.clearRetainingCapacity();

        // Rebuild via `compactInsert`, NOT `assert`: it (a) writes every base
        // fact at the single collapse basis `basis_at_compact` (so `asOf:` any
        // basis >= the collapse point sees the whole collapsed state — a
        // monotonically-increasing per-fact basis would make mid-range asOf
        // queries spuriously absent), and (b) leaves the basis_t counter
        // untouched (a rebuild is not a semantic mutation; next_tx is the
        // per-epoch coordinate and uses a sequential tx here). on_entity_mutate
        // (demote) stays suppressed; on_attr_mutate (mcache flush) fires.
        var stats = CompactStats{ .kept = 0, .dropped = 0 };
        var tx: u64 = 1;
        for (facts.items) |f| {
            if (keep(ctx, f.m, f.e, f.a, f.v)) {
                try self.compactInsert(f.e, f.a, f.v, f.m, tx, basis_at_compact);
                tx += 1;
                stats.kept += 1;
            } else {
                stats.dropped += 1;
            }
        }
        self.next_tx.store(tx, .monotonic);

        // Tighten the freshly-rebuilt epoch. `compactInsert` path-copies all
        // five HAMTs per fact, so this epoch is mostly abandoned spine garbage
        // (measured ~90%); the abandoned nodes only release at the *next*
        // compaction, which makes its own equal pile. Deep-clone the live roots
        // into a fresh arena and free the garbagey one, collapsing the
        // steady-state index footprint to live size. Safe here: the epoch was
        // just built (refcount 0, no snapshot pins it). Best-effort — a failed
        // tighten (OOM) leaves a valid, if larger, epoch, so it must not fail
        // the compaction.
        if (self.index_arena != null and !self.retain_history) {
            self.tightenLiveEpoch() catch {};
        }

        // Old epoch: free wholesale if nothing references it, else retain.
        // The new index is fully built and references only new-epoch nodes,
        // so the old epoch is internally dead — only an external pin (live
        // snapshot refcount) or the retention horizon can keep it alive.
        if (old_meta) |old| {
            const pinned = old.refcount > 0;
            const below_horizon = old.max_basis < self.oldestRetainedBasisT();
            if (!pinned and below_horizon) {
                old.arena.?.deinit(); // a freshly swapped-out epoch is always hot
                self.allocator.destroy(old.arena.?);
            } else {
                try self.retained_epochs.append(self.allocator, old);
            }
        }
        return stats;
    }

    /// Collapse the live index epoch to its live structure by deep-cloning the
    /// five roots into a fresh arena and freeing the current
    /// (path-copy-garbagey) one. Only valid on the just-rebuilt live epoch —
    /// refcount 0, no snapshot pins it — so the freed arena has no external
    /// reader. The clone copies only nodes reachable from the live roots; the
    /// abandoned spine garbage is unreachable and left behind in the freed
    /// arena. The cloned roots carry the same epoch identity (gen / min_basis /
    /// refcount are unchanged — same data, same basis range, just tighter), so
    /// the epoch registry and any later snapshot stay correct. On clone failure
    /// the store is left untouched (roots still point at the old arena) and the
    /// partial fresh arena is dropped — the caller treats it as best-effort.
    fn tightenLiveEpoch(self: *FactStore) !void {
        const garbagey = self.index_arena orelse return;
        const fresh = try self.allocator.create(std.heap.ArenaAllocator);
        errdefer self.allocator.destroy(fresh);
        fresh.* = std.heap.ArenaAllocator.init(self.allocator);
        errdefer fresh.deinit();
        const fa = fresh.allocator();
        const new_eavt = try self.eavt.cloneInto(fa, cloneInnerValueMap);
        const new_aevt = try self.aevt.cloneInto(fa, cloneInnerValueMap);
        const new_avet = try self.avet.cloneInto(fa, cloneMidPresenceMap);
        const new_vaet = try self.vaet.cloneInto(fa, cloneMidPresenceMap);
        const new_meat = try self.meat.cloneInto(fa, cloneEAVTish);
        // All clones succeeded — publish the tight roots and free the garbage.
        self.eavt = new_eavt;
        self.aevt = new_aevt;
        self.avet = new_avet;
        self.vaet = new_vaet;
        self.meat = new_meat;
        self.index_arena = fresh;
        garbagey.deinit();
        self.allocator.destroy(garbagey);
    }
};

// -- Internal helpers --------------------------------------------------------

/// Surgically remove every trace of entity `e` from one set of index roots
/// (Phase 15.R R3). Walks `e`'s full EAVT history to find every (a, v) pair it
/// ever held, then `dissoc`s `e` from the matching AVET/VAET leaves, AEVT
/// attr-maps, MEAT module-maps, and finally the EAVT entry itself. dissoc
/// path-copies via each map's stored allocator (the epoch arena), so the roots
/// are rewritten in place without touching any other entity. Returns whether
/// `e` was present. `scratch` holds transient collections.
fn exciseEntityFromRoots(roots: *IndexRoots, e: u64, scratch: std.mem.Allocator) !bool {
    const inner = roots.eavt.get(e) orelse return false;

    const AV = struct { a: u64, v: Ref };
    var pairs = std.ArrayList(AV){};
    defer pairs.deinit(scratch);
    var attrs = std.ArrayList(u64){};
    defer attrs.deinit(scratch);

    var ait = inner.iter();
    while (ait.next()) |kv| {
        try attrs.append(scratch, kv.key);
        const cell = kv.value;
        const n = cell.history.count();
        var i: usize = 0;
        while (i < n) : (i += 1) {
            try pairs.append(scratch, .{ .a = kv.key, .v = cell.history.nth(i).v });
        }
        // current_v may post-date the collected history in pathological cases;
        // include it defensively.
        if (cell.current_v) |cv| try pairs.append(scratch, .{ .a = kv.key, .v = cv });
    }

    // VAET[v][a] : dissoc e
    for (pairs.items) |p| {
        if (roots.vaet.get(p.v)) |mid| {
            if (mid.get(p.a)) |leaf| {
                const new_mid = try mid.assoc(p.a, try leaf.dissoc(e));
                roots.vaet = try roots.vaet.assoc(p.v, new_mid);
            }
        }
    }
    // AVET[a][v] : dissoc e
    for (pairs.items) |p| {
        if (roots.avet.get(p.a)) |mid| {
            if (mid.get(p.v)) |leaf| {
                const new_mid = try mid.assoc(p.v, try leaf.dissoc(e));
                roots.avet = try roots.avet.assoc(p.a, new_mid);
            }
        }
    }
    // AEVT[a] : dissoc e
    for (attrs.items) |a| {
        if (roots.aevt.get(a)) |amap| {
            roots.aevt = try roots.aevt.assoc(a, try amap.dissoc(e));
        }
    }
    // MEAT[m] : dissoc e (for every module that owned a fact of e)
    {
        var mods = std.ArrayList(u64){};
        defer mods.deinit(scratch);
        var mit = roots.meat.iter();
        while (mit.next()) |kv| {
            if (kv.value.get(e) != null) try mods.append(scratch, kv.key);
        }
        for (mods.items) |m| {
            if (roots.meat.get(m)) |emap| {
                roots.meat = try roots.meat.assoc(m, try emap.dissoc(e));
            }
        }
    }
    // EAVT : dissoc e wholesale
    roots.eavt = try roots.eavt.dissoc(e);
    return true;
}

fn updateValueLeaf(
    allocator: std.mem.Allocator,
    outer: EAVTish,
    k_outer: u64,
    k_inner: u64,
    new_current: ?Ref,
    new_hist: ValueHist,
    no_history: bool,
) !EAVTish {
    const inner = outer.get(k_outer) orelse InnerValueMap.empty(allocator, .{});
    const empty_cell = ValueCell{ .current_v = null, .history = ValueHistVec.empty(allocator) };
    const cell = inner.get(k_inner) orelse empty_cell;
    // Lever 1: a no-history attr keeps current_v but never grows its history vec.
    const new_hist_vec = if (no_history) cell.history else try cell.history.push(new_hist);
    const new_cell = ValueCell{ .current_v = new_current, .history = new_hist_vec };
    const new_inner = try inner.assoc(k_inner, new_cell);
    return outer.assoc(k_outer, new_inner);
}

fn updateMeat(
    allocator: std.mem.Allocator,
    meat: MEATish,
    m: u64,
    e: u64,
    a: u64,
    new_current: ?Ref,
    new_hist: ValueHist,
    no_history: bool,
) !MEATish {
    const e_map = meat.get(m) orelse EAVTish.empty(allocator, .{});
    const new_e_map = try updateValueLeaf(allocator, e_map, e, a, new_current, new_hist, no_history);
    return meat.assoc(m, new_e_map);
}

fn updatePresenceLeaf(
    allocator: std.mem.Allocator,
    outer: AVETish,
    k_outer: u64,
    k_mid: u64,
    k_inner: u64,
    new_present: bool,
    new_hist: PresenceHist,
    no_history: bool,
) !AVETish {
    const mid = outer.get(k_outer) orelse MidPresenceMap.empty(allocator, .{});
    const leaf = mid.get(k_mid) orelse InnerPresenceMap.empty(allocator, .{});
    const empty_cell = PresenceCell{ .present = false, .history = PresenceHistVec.empty(allocator) };
    const cell = leaf.get(k_inner) orelse empty_cell;
    // Lever 1: the `present` flag always updates; only the history vec is gated.
    const new_hist_vec = if (no_history) cell.history else try cell.history.push(new_hist);
    const new_cell = PresenceCell{ .present = new_present, .history = new_hist_vec };
    const new_leaf = try leaf.assoc(k_inner, new_cell);
    const new_mid = try mid.assoc(k_mid, new_leaf);
    return outer.assoc(k_outer, new_mid);
}
