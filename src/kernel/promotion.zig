//! Phase 8: promotion / demotion.
//!
//! Materializes hot EIDs into Piumarta-style `[vt_ref][slots...]` structs
//! cached in the object_table. Cold-EID dispatch checks the table first;
//! present = struct-backed, vtable read from `-1` (cheap). Absent =
//! fact-store lookup (slow, but correct).
//!
//! Decisions per Phase 8 design doc:
//!   D1 hybrid: kernel counts (object_table.bumpHotness), policy will act later.
//!       Phase 8 ships the auto-promote on threshold built-in.
//!   D2 inline hotness on object_table (AutoHashMap, not HAMT — mutable bump).
//!   D3 saturate + reset-on-demote.
//!   D4 global threshold fact, kernel-cached, re-read on mutate.
//!   D5 layout cache: per-EID Layout (attr-EID sorted). Sibling-shape sharing
//!       deferred — Phase 8 keeps it simple.
//!   D6 demote on any assert/retract that touches a promoted entity.
//!   D7 sorted attr-EID order (locked in VISION §Materialized Layout).
//!   D8 FFI pin: single global depth counter blocks demotion while > 0.

const std = @import("std");
const ref_mod = @import("ref.zig");
const object = @import("object.zig");
const fact_store_mod = @import("fact_store.zig");
const object_table_mod = @import("object_table.zig");
const kernel_eids = @import("kernel_eids.zig");

const Ref = ref_mod.Ref;

/// Per-EID materialization layout. Slot `i` of the struct holds the value
/// of attribute `attrs[i]`. Slots ordered by attribute-EID ascending.
pub const Layout = struct {
    /// Owned slice. Freed on demote.
    attrs: []u64,
};

// -- Globals ---------------------------------------------------------------

var g_allocator: ?std.mem.Allocator = null;
var g_store: ?*fact_store_mod.FactStore = null;
var g_table: ?*object_table_mod.ObjectTable = null;
var g_layouts: ?*std.AutoHashMap(u64, Layout) = null;
var g_threshold: u32 = 100;
var g_ffi_depth: u32 = 0;
/// Reentry guard — demotion fires from the fact-store mutate hook; we
/// must not re-enter via our own fact mutations (none today, but cheap
/// insurance for future evolution).
var g_in_hook: bool = false;

/// Install promotion machinery. Caller owns the heap-allocated layouts map.
/// After this call, hook fires on every fact_store assert/retract.
pub fn init(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
) !void {
    const layouts = try allocator.create(std.AutoHashMap(u64, Layout));
    layouts.* = std.AutoHashMap(u64, Layout).init(allocator);
    g_allocator = allocator;
    g_store = store;
    g_table = table;
    g_layouts = layouts;
    g_ffi_depth = 0;
    g_in_hook = false;
    // Read initial threshold from facts.
    refreshThreshold();
    // Install the entity-level mutate hook.
    store.on_entity_mutate = &onEntityMutate;
}

pub fn deinit() void {
    const layouts = g_layouts orelse return;
    const allocator = g_allocator.?;
    var it = layouts.iterator();
    while (it.next()) |entry| allocator.free(entry.value_ptr.attrs);
    layouts.deinit();
    allocator.destroy(layouts);
    g_layouts = null;
    g_allocator = null;
    g_store = null;
    g_table = null;
}

pub fn currentThreshold() u32 {
    return g_threshold;
}

/// Re-read `M_BOOTSTRAP :promote-threshold` from the fact store and update
/// the cached value. Called at init and whenever the threshold fact mutates.
fn refreshThreshold() void {
    const store = g_store orelse return;
    if (store.currentValue(kernel_eids.M_BOOTSTRAP, kernel_eids.ATTR_PROMOTE_THRESHOLD)) |v| {
        if (ref_mod.isSmallInt(v)) {
            const n = ref_mod.untagSmallInt(v);
            if (n > 0 and n <= std.math.maxInt(u32)) {
                g_threshold = @intCast(n);
            }
        }
    }
}

// -- FFI pin ---------------------------------------------------------------

pub fn ffiEnter() void {
    g_ffi_depth += 1;
}

pub fn ffiExit() void {
    if (g_ffi_depth > 0) g_ffi_depth -= 1;
}

pub fn ffiDepth() u32 {
    return g_ffi_depth;
}

// -- Promote ---------------------------------------------------------------

pub const PromoteError = error{
    NotInitialized,
    NoVtable,
};

/// Materialize `eid` as a `[vt_ref][slot_0..slot_{n-1}]` struct backed by a
/// fresh allocation. No-op if already materialized.
pub fn promote(eid: u64) anyerror!void {
    const allocator = g_allocator orelse return error.NotInitialized;
    const store = g_store orelse return error.NotInitialized;
    const table = g_table orelse return error.NotInitialized;
    const layouts = g_layouts orelse return error.NotInitialized;

    if (table.get(eid)) |_| return; // already materialized

    const vt_ref = store.currentValue(eid, kernel_eids.ATTR_VTABLE) orelse return error.NoVtable;
    if (!ref_mod.isEid(vt_ref)) return error.NoVtable;

    // Collect non-vtable (a, v) pairs into a sorted array.
    const Pair = struct { a: u64, v: Ref };
    var pairs = std.ArrayList(Pair){};
    defer pairs.deinit(allocator);
    var it = store.attributesOf(eid);
    while (it.next()) |entry| {
        if (entry.a == kernel_eids.ATTR_VTABLE) continue;
        try pairs.append(allocator, .{ .a = entry.a, .v = entry.v });
    }
    const cmp = struct {
        fn less(_: void, x: Pair, y: Pair) bool {
            return x.a < y.a;
        }
    }.less;
    std.mem.sort(Pair, pairs.items, {}, cmp);

    const n = pairs.items.len;
    const total_bytes = @sizeOf(Ref) * (1 + n);
    const buf = try allocator.alignedAlloc(u8, .of(Ref), total_bytes);
    @memset(buf, 0);
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = vt_ref;
    const obj_ptr: object.ObjPtr = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    var i: usize = 0;
    while (i < n) : (i += 1) {
        object.setSlot(obj_ptr, i, pairs.items[i].v);
    }

    // Save layout for size-aware free at demote time.
    const attrs_owned = try allocator.alloc(u64, n);
    for (pairs.items, 0..) |p, idx| attrs_owned[idx] = p.a;
    try layouts.put(eid, .{ .attrs = attrs_owned });

    try table.put(eid, ref_mod.mkPointer(@ptrCast(obj_ptr)));
}

// -- Demote ----------------------------------------------------------------

/// Free `eid`'s materialized struct and remove it from the object_table.
/// No-op if not materialized. Blocked when `g_ffi_depth > 0` to preserve
/// in-flight FFI pointers.
pub fn demote(eid: u64) void {
    if (g_ffi_depth > 0) return;
    const allocator = g_allocator orelse return;
    const table = g_table orelse return;
    const layouts = g_layouts orelse return;

    const ptr_ref = table.get(eid) orelse return;
    if (!ref_mod.isPointer(ptr_ref)) return;

    const layout = layouts.get(eid) orelse {
        // Layout missing — defensive. Clear table entry without freeing.
        table.clear(eid) catch {};
        table.resetHotness(eid);
        return;
    };

    const obj_addr: usize = @intFromPtr(ref_mod.untagPointer(ptr_ref));
    const base_addr = obj_addr - @sizeOf(Ref);
    const base_ptr: [*]align(@alignOf(Ref)) u8 = @ptrFromInt(base_addr);
    const total_bytes = @sizeOf(Ref) * (1 + layout.attrs.len);
    allocator.free(base_ptr[0..total_bytes]);
    allocator.free(layout.attrs);
    _ = layouts.remove(eid);

    table.clear(eid) catch {};
    table.resetHotness(eid);
}

pub fn isMaterialized(eid: u64) bool {
    const table = g_table orelse return false;
    return table.get(eid) != null;
}

// -- Auto-promote on threshold --------------------------------------------

/// Called from send.zig cold path after bumping hotness. Checks threshold;
/// if exceeded and not yet materialized and not in FFI, promotes.
pub fn maybeAutoPromote(eid: u64) void {
    if (g_ffi_depth > 0) return;
    const table = g_table orelse return;
    if (table.get(eid) != null) return;
    if (table.hotnessOf(eid) < g_threshold) return;
    promote(eid) catch return;
}

// -- Fact-store mutation hook ----------------------------------------------

/// Fired from fact_store.assert / retract with the (e, a) that mutated.
/// Demote if `e` is currently materialized, since any fact change may
/// invalidate the cached struct layout.
fn onEntityMutate(e: u64, a: u64) callconv(.c) void {
    if (g_in_hook) return;
    g_in_hook = true;
    defer g_in_hook = false;

    // Threshold-fact mutation: refresh cache, don't demote.
    if (e == kernel_eids.M_BOOTSTRAP and a == kernel_eids.ATTR_PROMOTE_THRESHOLD) {
        refreshThreshold();
        return;
    }

    if (isMaterialized(e)) demote(e);
}
