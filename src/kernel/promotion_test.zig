const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const object = @import("object.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const bootstrap = @import("bootstrap.zig");
const promotion = @import("promotion.zig");
const kernel_eids = @import("kernel_eids.zig");

const Ref = ref_mod.Ref;

const A_X: u64 = 250; // user-range attribute EIDs
const A_Y: u64 = 251;
const A_Z: u64 = 252;

fn boot(
    a: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
) !void {
    store.* = fact_store.FactStore.init(a);
    table.* = object_table.ObjectTable.init(a);
    interner.* = interner_mod.Interner.init(a);
    _ = try bootstrap.boot(a, store, table, interner);
}

// -- Hotness counter ---------------------------------------------------------

test "hotness: bump saturates and resets on demote" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);

    const E: u64 = 1000;
    table.bumpHotness(E);
    table.bumpHotness(E);
    table.bumpHotness(E);
    try testing.expectEqual(@as(u32, 3), table.hotnessOf(E));

    table.resetHotness(E);
    try testing.expectEqual(@as(u32, 0), table.hotnessOf(E));
}

// -- Manual promote + demote round-trip --------------------------------------

test "promote: materializes struct with vt at -1, sorted slots" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);

    // Build a user entity with three attributes. Use OBJECT_VT as a stand-in
    // vtable (bootstrap already registered it).
    const E: u64 = 5000;
    const vt_ref = ref_mod.mkEid(kernel_eids.OBJECT_VT);
    // Assert in non-sorted order to verify slot layout sorts.
    _ = try store.assert(E, A_Z, ref_mod.mkSmallInt(30).?, kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(E, A_X, ref_mod.mkSmallInt(10).?, kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(E, A_Y, ref_mod.mkSmallInt(20).?, kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(E, kernel_eids.ATTR_VTABLE, vt_ref, kernel_eids.M_BOOTSTRAP);

    try promotion.promote(E);
    try testing.expect(promotion.isMaterialized(E));

    const ptr_ref = table.get(E).?;
    try testing.expect(ref_mod.isPointer(ptr_ref));
    const obj = object.fromRef(ptr_ref);

    // -1 slot is the vtable Ref.
    try testing.expectEqual(vt_ref, object.vtableOf(obj));

    // Slots in attribute-EID ascending order: A_X(250)=10, A_Y(251)=20, A_Z(252)=30.
    try testing.expectEqual(@as(i64, 10), ref_mod.untagSmallInt(object.slotAt(obj, 0)));
    try testing.expectEqual(@as(i64, 20), ref_mod.untagSmallInt(object.slotAt(obj, 1)));
    try testing.expectEqual(@as(i64, 30), ref_mod.untagSmallInt(object.slotAt(obj, 2)));
}

test "demote: frees struct and clears object_table entry" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);

    const E: u64 = 5001;
    _ = try store.assert(E, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.OBJECT_VT), kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(E, A_X, ref_mod.mkSmallInt(42).?, kernel_eids.M_BOOTSTRAP);

    try promotion.promote(E);
    try testing.expect(promotion.isMaterialized(E));

    promotion.demote(E);
    try testing.expect(!promotion.isMaterialized(E));
    try testing.expectEqual(@as(?Ref, null), table.get(E));
}

// -- Shape-mutation auto-demote ---------------------------------------------

test "shape change: asserting a new attribute auto-demotes" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);

    const E: u64 = 5002;
    _ = try store.assert(E, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.OBJECT_VT), kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(E, A_X, ref_mod.mkSmallInt(1).?, kernel_eids.M_BOOTSTRAP);

    try promotion.promote(E);
    try testing.expect(promotion.isMaterialized(E));

    // A new attribute changes the struct shape → demote (fix A only patches
    // value writes to attrs already in the layout).
    _ = try store.assert(E, A_Y, ref_mod.mkSmallInt(2).?, kernel_eids.M_BOOTSTRAP);
    try testing.expect(!promotion.isMaterialized(E));
}

// -- Same-shape value write: patch in place, stay materialized (fix A) -------

test "value write same shape: stays materialized, slot patched in place" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);

    const E: u64 = 5004;
    _ = try store.assert(E, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.OBJECT_VT), kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(E, A_X, ref_mod.mkSmallInt(1).?, kernel_eids.M_BOOTSTRAP); // slot 0 (A_X < A_Y)
    _ = try store.assert(E, A_Y, ref_mod.mkSmallInt(2).?, kernel_eids.M_BOOTSTRAP); // slot 1

    try promotion.promote(E);
    try testing.expect(promotion.isMaterialized(E));

    // Value-replace of an in-layout attr → NO demote, slot updated in place.
    _ = try store.assert(E, A_X, ref_mod.mkSmallInt(99).?, kernel_eids.M_BOOTSTRAP);
    try testing.expect(promotion.isMaterialized(E));

    const obj = object.fromRef(table.get(E).?);
    try testing.expectEqual(@as(i64, 99), ref_mod.untagSmallInt(object.slotAt(obj, 0))); // A_X patched
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(object.slotAt(obj, 1))); // A_Y intact

    // A retract shrinks the shape → demote (can't patch to absent).
    _ = try store.retract(E, A_Y, ref_mod.mkSmallInt(2).?, kernel_eids.M_BOOTSTRAP);
    try testing.expect(!promotion.isMaterialized(E));
}

// -- Shape-change demote preserves hotness (fix B) --------------------------

test "demote via shape change preserves hotness for immediate re-promote" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);

    const E: u64 = 5005;
    _ = try store.assert(E, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.OBJECT_VT), kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(E, A_X, ref_mod.mkSmallInt(1).?, kernel_eids.M_BOOTSTRAP);

    var i: u32 = 0;
    while (i < 7) : (i += 1) table.bumpHotness(E);
    try promotion.promote(E);
    try testing.expect(promotion.isMaterialized(E));

    // Shape change → demote. Fix B: hotness is NOT reset, so the entity is
    // still hot and re-promotes on the next dispatch instead of re-earning the
    // whole threshold.
    _ = try store.assert(E, A_Y, ref_mod.mkSmallInt(2).?, kernel_eids.M_BOOTSTRAP);
    try testing.expect(!promotion.isMaterialized(E));
    try testing.expectEqual(@as(u32, 7), table.hotnessOf(E));
}

// -- FFI pin ----------------------------------------------------------------

test "ffi_enter blocks demote until ffi_exit" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);

    const E: u64 = 5003;
    _ = try store.assert(E, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.OBJECT_VT), kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(E, A_X, ref_mod.mkSmallInt(1).?, kernel_eids.M_BOOTSTRAP);

    try promotion.promote(E);
    promotion.ffiEnter();

    // Mutation during FFI should NOT demote.
    _ = try store.assert(E, A_Y, ref_mod.mkSmallInt(2).?, kernel_eids.M_BOOTSTRAP);
    try testing.expect(promotion.isMaterialized(E));

    // Direct demote also no-ops during FFI.
    promotion.demote(E);
    try testing.expect(promotion.isMaterialized(E));

    promotion.ffiExit();
    promotion.demote(E);
    try testing.expect(!promotion.isMaterialized(E));
}

// -- Threshold auto-promote --------------------------------------------------

test "threshold: hotness >= cached threshold triggers auto-promote on cold dispatch" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);

    // Lower threshold for the test to keep the loop short. Reassert the
    // threshold fact; promotion's hook refreshes the cache.
    _ = try store.assert(
        kernel_eids.M_BOOTSTRAP,
        kernel_eids.ATTR_PROMOTE_THRESHOLD,
        ref_mod.mkSmallInt(5).?,
        kernel_eids.M_BOOTSTRAP,
    );
    try testing.expectEqual(@as(u32, 5), promotion.currentThreshold());

    const E: u64 = 5004;
    _ = try store.assert(E, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.OBJECT_VT), kernel_eids.M_BOOTSTRAP);

    // Manually bump hotness to threshold.
    var i: usize = 0;
    while (i < 5) : (i += 1) table.bumpHotness(E);
    try testing.expect(!promotion.isMaterialized(E));
    promotion.maybeAutoPromote(E);
    try testing.expect(promotion.isMaterialized(E));
}

// -- Threshold fact refresh -------------------------------------------------

test "threshold refresh: re-asserting the threshold fact updates the cached value" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);
    _ = &table;

    try testing.expectEqual(@as(u32, 100), promotion.currentThreshold());
    _ = try store.assert(
        kernel_eids.M_BOOTSTRAP,
        kernel_eids.ATTR_PROMOTE_THRESHOLD,
        ref_mod.mkSmallInt(42).?,
        kernel_eids.M_BOOTSTRAP,
    );
    try testing.expectEqual(@as(u32, 42), promotion.currentThreshold());
}
