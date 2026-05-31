//! Sparse object table.
//!
//! Maps EID → materialized `Ref` (a raw pointer). Entries exist only for
//! materialized objects. Absence of an EID in the table means the entity
//! is cold — its content lives only in the fact store. Existence of the
//! EID itself is determined by querying the fact store, NOT by this table.
//!
//! Decisions (VISION.md Phase 2 open issues, resolved):
//! - Sparse design, no sentinel for cold entities. `get` returns ?Ref;
//!   null = "not materialized, consult fact store."
//! - `alloc` mints EIDs only. Does NOT assert facts. Higher-level
//!   `assertNewEntity` helpers (Phase 3) combine alloc + initial-fact-assert.
//! - Table is a single mutable wrapper around a persistent HAMT root.
//!   The HAMT gives us structural sharing for snapshots; the wrapper
//!   keeps the runtime API in the single-cell-of-state shape the kernel
//!   wants.

const std = @import("std");
const hamt = @import("hamt.zig");
const ref_mod = @import("ref.zig");
const kernel_eids = @import("kernel_eids.zig");

const Ref = ref_mod.Ref;

/// HAMT keyed by EID, valued by the materialized Ref (raw pointer).
pub const Map = hamt.Hamt(u64, Ref, hamt.U64Ctx);

pub const ObjectTable = struct {
    allocator: std.mem.Allocator,
    /// Latest persistent HAMT root. Mutated through reassignment.
    map: Map,
    /// Monotonic EID counter. Starts at USER_EID_START; bootstrap is
    /// responsible for `put`-ing kernel EIDs (0..63) before any `alloc`.
    /// Atomic per Phase 11.7.iv (ADR-11 #7 forward-compat). No-op on
    /// single-thread; ready for non-transactor EID needs (reify-stack
    /// transients) under the Datomic shape.
    next_eid: std.atomic.Value(u64),
    /// Phase 8: per-EID hotness counter. Bumped on cold-EID dispatch.
    /// Transient — not snapshot-persisted. Saturates at u32 max.
    hotness: std.AutoHashMap(u64, u32),

    pub fn init(allocator: std.mem.Allocator) ObjectTable {
        return .{
            .allocator = allocator,
            .map = Map.empty(allocator, .{}),
            .next_eid = std.atomic.Value(u64).init(kernel_eids.USER_EID_START),
            .hotness = std.AutoHashMap(u64, u32).init(allocator),
        };
    }

    pub fn deinit(self: *ObjectTable) void {
        self.hotness.deinit();
    }

    /// Bump hotness counter for `eid`. Saturates at u32 max.
    pub fn bumpHotness(self: *ObjectTable, eid: u64) void {
        const gop = self.hotness.getOrPut(eid) catch return;
        if (!gop.found_existing) {
            gop.value_ptr.* = 1;
        } else if (gop.value_ptr.* < std.math.maxInt(u32)) {
            gop.value_ptr.* += 1;
        }
    }

    pub fn hotnessOf(self: *const ObjectTable, eid: u64) u32 {
        return self.hotness.get(eid) orelse 0;
    }

    pub fn resetHotness(self: *ObjectTable, eid: u64) void {
        _ = self.hotness.remove(eid);
    }

    /// Mint a new EID. Does not touch the table — minted EIDs may be cold
    /// (fact-only) at allocation time. Callers wanting a materialized
    /// object should `put` immediately after.
    pub fn alloc(self: *ObjectTable) u64 {
        return self.next_eid.fetchAdd(1, .monotonic);
    }

    /// Returns the materialized `Ref` for an EID, or null if the entity
    /// is cold (or never existed — caller distinguishes via fact-store query).
    pub fn get(self: *const ObjectTable, eid: u64) ?Ref {
        return self.map.get(eid);
    }

    /// Register a materialized pointer for an EID. Used by both bootstrap
    /// (pre-materializing kernel vtables) and runtime promotion.
    pub fn put(self: *ObjectTable, eid: u64, r: Ref) !void {
        self.map = try self.map.assoc(eid, r);
    }

    /// Remove the materialized entry for an EID. The entity itself is
    /// unchanged in the fact store; demotion just means "no longer hot."
    pub fn clear(self: *ObjectTable, eid: u64) !void {
        self.map = try self.map.dissoc(eid);
    }

    /// Number of currently-materialized entities.
    pub fn materializedCount(self: *const ObjectTable) usize {
        return self.map.count();
    }
};
