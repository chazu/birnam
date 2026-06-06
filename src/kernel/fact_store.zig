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

// -- Cardinality schema (Phase 9.5.2) ----------------------------------------

pub const Cardinality = enum { one, many };

pub const CardinalitySchema = struct {
    /// EID of the `:cardinality` attribute (kernel_eids.ATTR_CARDINALITY).
    attr_eid: u64,
    /// Ref value that marks an attribute as `:many`. Comparison is by
    /// Ref-equality; anything else (including absence) → `:one`.
    many_value: Ref,
};

// -- Index epoch registry (Phase 15.R R1b) -----------------------------------

/// An index epoch swapped out by `compact()` but kept alive because a live
/// snapshot pins it (refcount > 0) or the retention horizon needs the history
/// it holds. Freed wholesale once nothing references it. The arena is the same
/// plain `ArenaAllocator` the live epoch used while writable.
pub const RetainedEpoch = struct {
    arena: *std.heap.ArenaAllocator,
    /// Generation number (monotonic; the writable epoch's gen at swap-out).
    gen: u64,
    /// Lowest basis-T this epoch can serve a read for (basis-T when it became
    /// the writable epoch). Reads with `basis_t >= min_basis` are valid.
    min_basis: u64,
    /// Highest basis-T committed while this epoch was writable.
    max_basis: u64,
    /// Number of live snapshots pinning this epoch.
    refcount: u32,
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

    /// Phase 15.R R2 — retention horizon floor (basis-T). When set, history
    /// at or above this basis is kept by `compact()` and epochs covering it are
    /// not freed. `null` = current-only (the default); folded into
    /// `oldestRetainedBasisT()`.
    retention_floor: ?u64 = null,
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
        // 15.R: free any still-pinned retained epochs (test teardown; a live
        // process exit reclaims regardless).
        for (self.retained_epochs.items) |re| {
            re.arena.deinit();
            self.allocator.destroy(re.arena);
        }
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
        // Live snapshots pin epochs directly via refcount; the horizon (R2)
        // and any extra floor are folded in here when those land.
        if (self.retention_floor) |floor| oldest = @min(oldest, floor);
        return oldest;
    }

    /// Pin the current writable epoch (a snapshot captured it). Returns the
    /// generation so the holder can `unpinEpoch` exactly that epoch later.
    pub fn pinCurrentEpoch(self: *FactStore) u64 {
        self.index_epoch_refcount += 1;
        return self.index_epoch_gen;
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
                    re.arena.deinit();
                    self.allocator.destroy(re.arena);
                    _ = self.retained_epochs.swapRemove(i);
                }
                return;
            }
        }
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
        self.eavt = try updateValueLeaf(self.indexAlloc(), self.eavt, e, a, v, .{ .v = v, .tx = 1, .op = .assert });
        self.aevt = try updateValueLeaf(self.indexAlloc(), self.aevt, a, e, v, .{ .v = v, .tx = 1, .op = .assert });
        self.meat = try updateMeat(self.indexAlloc(), self.meat, m, e, a, v, .{ .v = v, .tx = 1, .op = .assert });
        self.avet = try updatePresenceLeaf(self.indexAlloc(), self.avet, a, v, e, true, .{ .tx = 1, .op = .assert });
        self.vaet = try updatePresenceLeaf(self.indexAlloc(), self.vaet, v, a, e, true, .{ .tx = 1, .op = .assert });
    }

    /// Lock the store read-only (Phase 14 `--strip=history`). After this,
    /// `assert`/`retract` reject. Idempotent.
    pub fn freeze(self: *FactStore) void {
        self.read_only = true;
    }

    pub fn assert(self: *FactStore, e: u64, a: u64, v: Ref, m: u64) !u64 {
        if (self.read_only) return error.ReadOnly;
        if (builtin.mode == .Debug) {
            if (ref_mod.isNil(v)) @panic("FactStore.assert: NIL value forbidden (Date discipline)");
        }
        const tx = self.next_tx.fetchAdd(1, .monotonic);
        _ = self.basis_t.fetchAdd(1, .monotonic); // 15.R: never resets on compact

        // Snapshot the previous value before any writes. Use raw lookup
        // so `:many` attrs (where currentValue materializes an RRB)
        // don't fool the implicit-retract logic below.
        const prev_v = self.rawCurrentValue(e, a);
        const is_many = self.cardinalityOfAttr(a) == .many;

        // EAVT / AEVT: append (v, tx, assert) to the value-cell at (e,a) / (a,e).
        // For `:many` attrs the cell's current_v is just "latest assert"
        // and not the canonical truth; AVET drives reads.
        self.eavt = try updateValueLeaf(self.indexAlloc(), self.eavt, e, a, v, .{ .v = v, .tx = tx, .op = .assert });
        self.aevt = try updateValueLeaf(self.indexAlloc(), self.aevt, a, e, v, .{ .v = v, .tx = tx, .op = .assert });

        // MEAT: same pattern but three-level (m, e, a).
        self.meat = try updateMeat(self.indexAlloc(), self.meat, m, e, a, v, .{ .v = v, .tx = tx, .op = .assert });

        // AVET / VAET: presence-cell maintenance.
        //   `:one` (default) — implicit-retract the previous distinct value.
        //   `:many` — never implicit-retract; multiple values coexist as present.
        if (!is_many) {
            if (prev_v) |pv| {
                if (pv != v) {
                    self.avet = try updatePresenceLeaf(self.indexAlloc(), self.avet, a, pv, e, false, .{ .tx = tx, .op = .retract });
                    self.vaet = try updatePresenceLeaf(self.indexAlloc(), self.vaet, pv, a, e, false, .{ .tx = tx, .op = .retract });
                }
            }
        }
        self.avet = try updatePresenceLeaf(self.indexAlloc(), self.avet, a, v, e, true, .{ .tx = tx, .op = .assert });
        self.vaet = try updatePresenceLeaf(self.indexAlloc(), self.vaet, v, a, e, true, .{ .tx = tx, .op = .assert });

        if (self.on_attr_mutate) |cb| cb(a);
        if (self.on_entity_mutate) |cb| cb(e, a);

        return tx;
    }

    pub fn retract(self: *FactStore, e: u64, a: u64, v: Ref, m: u64) !u64 {
        if (self.read_only) return error.ReadOnly;
        const tx = self.next_tx.fetchAdd(1, .monotonic);
        _ = self.basis_t.fetchAdd(1, .monotonic); // 15.R: never resets on compact

        // No-op if (e, a, v) isn't currently present (AVET is authoritative
        // for both `:one` and `:many`).
        if (!self.hasFact(e, a, v)) return tx;

        // EAVT/AEVT current_v: for `:one` this nulls the cache (no remaining
        // value at (e, a)). For `:many` it makes current_v stale relative to
        // AVET — that's fine; reads consult cardinality.
        self.eavt = try updateValueLeaf(self.indexAlloc(), self.eavt, e, a, null, .{ .v = v, .tx = tx, .op = .retract });
        self.aevt = try updateValueLeaf(self.indexAlloc(), self.aevt, a, e, null, .{ .v = v, .tx = tx, .op = .retract });
        self.meat = try updateMeat(self.indexAlloc(), self.meat, m, e, a, null, .{ .v = v, .tx = tx, .op = .retract });
        self.avet = try updatePresenceLeaf(self.indexAlloc(), self.avet, a, v, e, false, .{ .tx = tx, .op = .retract });
        self.vaet = try updatePresenceLeaf(self.indexAlloc(), self.vaet, v, a, e, false, .{ .tx = tx, .op = .retract });

        if (self.on_attr_mutate) |cb| cb(a);
        if (self.on_entity_mutate) |cb| cb(e, a);

        return tx;
    }

    // -- Cardinality (Phase 9.5.2) -------------------------------------------

    pub fn setCardinalitySchema(self: *FactStore, schema: CardinalitySchema) void {
        self.cardinality_schema = schema;
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
        self.next_tx.store(1, .monotonic);
        self.fact_eids.clearRetainingCapacity();
        self.fact_modules.clearRetainingCapacity();

        // Suppress entity-demotion during re-assert: kept entities keep
        // their materialized object-table slots (re-asserting the same
        // facts must not churn the cache). on_attr_mutate (mcache flush)
        // stays active so dispatch caches refresh.
        const saved_entity_hook = self.on_entity_mutate;
        self.on_entity_mutate = null;
        defer self.on_entity_mutate = saved_entity_hook;

        var stats = CompactStats{ .kept = 0, .dropped = 0 };
        for (facts.items) |f| {
            if (keep(ctx, f.m, f.e, f.a, f.v)) {
                _ = try self.assert(f.e, f.a, f.v, f.m);
                stats.kept += 1;
            } else {
                stats.dropped += 1;
            }
        }
        // compact is a rebuild, not a semantic mutation: the re-asserts above
        // bumped basis_t; restore it so the user-facing timeline is unchanged
        // (next_tx legitimately restarts at 1 — the per-epoch coordinate).
        self.basis_t.store(basis_at_compact + 1, .monotonic);

        // Old epoch: free wholesale if nothing references it, else retain.
        // The new index is fully built and references only new-epoch nodes,
        // so the old epoch is internally dead — only an external pin (live
        // snapshot refcount) or the retention horizon can keep it alive.
        if (old_meta) |old| {
            const pinned = old.refcount > 0;
            const below_horizon = old.max_basis < self.oldestRetainedBasisT();
            if (!pinned and below_horizon) {
                old.arena.deinit();
                self.allocator.destroy(old.arena);
            } else {
                try self.retained_epochs.append(self.allocator, old);
            }
        }
        return stats;
    }
};

// -- Internal helpers --------------------------------------------------------

fn updateValueLeaf(
    allocator: std.mem.Allocator,
    outer: EAVTish,
    k_outer: u64,
    k_inner: u64,
    new_current: ?Ref,
    new_hist: ValueHist,
) !EAVTish {
    const inner = outer.get(k_outer) orelse InnerValueMap.empty(allocator, .{});
    const empty_cell = ValueCell{ .current_v = null, .history = ValueHistVec.empty(allocator) };
    const cell = inner.get(k_inner) orelse empty_cell;
    const new_hist_vec = try cell.history.push(new_hist);
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
) !MEATish {
    const e_map = meat.get(m) orelse EAVTish.empty(allocator, .{});
    const new_e_map = try updateValueLeaf(allocator, e_map, e, a, new_current, new_hist);
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
) !AVETish {
    const mid = outer.get(k_outer) orelse MidPresenceMap.empty(allocator, .{});
    const leaf = mid.get(k_mid) orelse InnerPresenceMap.empty(allocator, .{});
    const empty_cell = PresenceCell{ .present = false, .history = PresenceHistVec.empty(allocator) };
    const cell = leaf.get(k_inner) orelse empty_cell;
    const new_hist_vec = try cell.history.push(new_hist);
    const new_cell = PresenceCell{ .present = new_present, .history = new_hist_vec };
    const new_leaf = try leaf.assoc(k_inner, new_cell);
    const new_mid = try mid.assoc(k_mid, new_leaf);
    return outer.assoc(k_outer, new_mid);
}
