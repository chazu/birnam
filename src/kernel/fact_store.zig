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

// -- FactStore ---------------------------------------------------------------

pub const FactStore = struct {
    allocator: std.mem.Allocator,
    next_tx: u64, // monotonic; starts at 1
    eavt: EAVTish,
    aevt: EAVTish, // same shape as EAVT, keys are (a, e)
    avet: AVETish,
    vaet: AVETish, // same shape as AVET, keys are (v, a, e)
    meat: MEATish,

    /// Optional hook invoked after every assert/retract with the attribute
    /// EID. Kernel sets this to `mcache.flushSel` to keep dispatch-cache
    /// coherent. Decoupled (FactStore doesn't import the kernel).
    on_attr_mutate: ?*const fn (a: u64) void = null,

    /// Phase 8: optional entity-level hook fired alongside `on_attr_mutate`
    /// with both `(e, a)`. Promotion module installs this to demote any
    /// materialized entity whose facts change.
    on_entity_mutate: ?*const fn (e: u64, a: u64) callconv(.c) void = null,

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
            .next_tx = 1,
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

    pub fn assert(self: *FactStore, e: u64, a: u64, v: Ref, m: u64) !u64 {
        if (builtin.mode == .Debug) {
            if (ref_mod.isNil(v)) @panic("FactStore.assert: NIL value forbidden (Date discipline)");
        }
        const tx = self.next_tx;
        self.next_tx += 1;

        // Snapshot the previous value before any writes.
        const prev_v = self.currentValue(e, a);

        // EAVT / AEVT: append (v, tx, assert) to the value-cell at (e,a) / (a,e).
        self.eavt = try updateValueLeaf(self.allocator, self.eavt, e, a, v, .{ .v = v, .tx = tx, .op = .assert });
        self.aevt = try updateValueLeaf(self.allocator, self.aevt, a, e, v, .{ .v = v, .tx = tx, .op = .assert });

        // MEAT: same pattern but three-level (m, e, a).
        self.meat = try updateMeat(self.allocator, self.meat, m, e, a, v, .{ .v = v, .tx = tx, .op = .assert });

        // AVET / VAET: presence-cell maintenance.
        if (prev_v) |pv| {
            if (pv != v) {
                self.avet = try updatePresenceLeaf(self.allocator, self.avet, a, pv, e, false, .{ .tx = tx, .op = .retract });
                self.vaet = try updatePresenceLeaf(self.allocator, self.vaet, pv, a, e, false, .{ .tx = tx, .op = .retract });
            }
        }
        self.avet = try updatePresenceLeaf(self.allocator, self.avet, a, v, e, true, .{ .tx = tx, .op = .assert });
        self.vaet = try updatePresenceLeaf(self.allocator, self.vaet, v, a, e, true, .{ .tx = tx, .op = .assert });

        if (self.on_attr_mutate) |cb| cb(a);
        if (self.on_entity_mutate) |cb| cb(e, a);

        return tx;
    }

    pub fn retract(self: *FactStore, e: u64, a: u64, v: Ref, m: u64) !u64 {
        const tx = self.next_tx;
        self.next_tx += 1;

        // If (e,a) doesn't currently hold `v`, this is a no-op (logically: nothing to retract).
        // We still record the retract in history if (e,a,v) was ever asserted, but for the
        // spike: only act if current value matches.
        const prev_v = self.currentValue(e, a);
        if (prev_v == null or prev_v.? != v) return tx;

        self.eavt = try updateValueLeaf(self.allocator, self.eavt, e, a, null, .{ .v = v, .tx = tx, .op = .retract });
        self.aevt = try updateValueLeaf(self.allocator, self.aevt, a, e, null, .{ .v = v, .tx = tx, .op = .retract });
        self.meat = try updateMeat(self.allocator, self.meat, m, e, a, null, .{ .v = v, .tx = tx, .op = .retract });
        self.avet = try updatePresenceLeaf(self.allocator, self.avet, a, v, e, false, .{ .tx = tx, .op = .retract });
        self.vaet = try updatePresenceLeaf(self.allocator, self.vaet, v, a, e, false, .{ .tx = tx, .op = .retract });

        if (self.on_attr_mutate) |cb| cb(a);
        if (self.on_entity_mutate) |cb| cb(e, a);

        return tx;
    }

    // -- Point queries -------------------------------------------------------

    /// Current value of (e, a), or null if absent (never asserted or retracted).
    pub fn currentValue(self: *const FactStore, e: u64, a: u64) ?Ref {
        const inner = self.eavt.get(e) orelse return null;
        const cell = inner.get(a) orelse return null;
        return cell.current_v;
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
        return self.next_tx - 1;
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
