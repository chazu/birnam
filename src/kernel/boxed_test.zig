const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const boxed = @import("boxed.zig");
const bootstrap = @import("bootstrap.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const kernel_eids = @import("kernel_eids.zig");
const arenas = @import("arenas.zig");

// -- Phase 15.1b: boxes arena routing ----------------------------------------

test "mkFloat/mkFfiPtr route to the boxes arena when the registry is live" {
    if (arenas.ready()) arenas.deinit(); // start clean
    try arenas.init(testing.allocator);
    defer arenas.deinit(); // frees the boxes below → leak-clean

    // The passed allocator is a fallback; with the registry up it is ignored
    // and the box lands in the boxes arena.
    const f = try boxed.mkFloat(testing.failing_allocator, 2.5);
    const p = try boxed.mkFfiPtr(testing.failing_allocator, 0x1000);

    const boxes = arenas.arena(.boxes).?;
    try testing.expectEqual(@as(usize, 2), boxes.liveCount());
    // A box Ref points at the payload (slot 0); the allocation base — what the
    // arena records — is one Ref-word earlier (the vt slot at -1). The 15.2
    // mark walk will do this same payload→base mapping.
    const ref_sz = @sizeOf(ref_mod.Ref);
    const f_base = @intFromPtr(ref_mod.untagPointer(f)) - ref_sz;
    const p_base = @intFromPtr(ref_mod.untagPointer(p)) - ref_sz;
    try testing.expect(boxes.contains(f_base));
    try testing.expect(boxes.contains(p_base));
    // Values still read back correctly through the arena-backed payload.
    try testing.expectEqual(@as(f64, 2.5), boxed.untagFloat(f));
}

// -- Float -------------------------------------------------------------------

test "mkFloat round-trips f64 through Ref" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const values = [_]f64{ 0.0, 1.0, -1.0, 3.14159265358979, 1.0e308, -1.0e-300 };
    for (values) |v| {
        const r = try boxed.mkFloat(a, v);
        try testing.expect(boxed.isFloat(r));
        try testing.expect(!boxed.isFfiPtr(r));
        try testing.expectEqual(v, boxed.untagFloat(r));
    }
}

test "Float Ref discriminated from raw pointer and SmallInt" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const f = try boxed.mkFloat(a, 2.5);
    try testing.expect(ref_mod.isPointer(f)); // boxed = pointer-tagged
    try testing.expect(boxed.isFloat(f));

    const i = ref_mod.mkSmallInt(42).?;
    try testing.expect(!boxed.isFloat(i));

    const e = ref_mod.mkEid(99);
    try testing.expect(!boxed.isFloat(e));
}

test "Float NaN round-trips" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const r = try boxed.mkFloat(a, std.math.nan(f64));
    try testing.expect(boxed.isFloat(r));
    try testing.expect(std.math.isNan(boxed.untagFloat(r)));
}

// -- FFI pointer -------------------------------------------------------------

test "mkFfiPtr round-trips usize address" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const addrs = [_]usize{ 0x1000, 0xDEADBEEF, 0x7FFF_FFFF_FFFF };
    for (addrs) |addr| {
        const r = try boxed.mkFfiPtr(a, addr);
        try testing.expect(boxed.isFfiPtr(r));
        try testing.expect(!boxed.isFloat(r));
        try testing.expectEqual(addr, boxed.untagFfiPtr(r));
    }
}

test "FfiPtr distinct from Float boxed type" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fp = try boxed.mkFloat(a, 1.0);
    const pp = try boxed.mkFfiPtr(a, 0x1234);
    try testing.expect(boxed.isFloat(fp));
    try testing.expect(!boxed.isFfiPtr(fp));
    try testing.expect(!boxed.isFloat(pp));
    try testing.expect(boxed.isFfiPtr(pp));
}

// -- Bootstrap wires kernel vtables -----------------------------------------

test "bootstrap allocates FLOAT_VT and FFI_PTR_VT with proper delegation" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    var table = object_table.ObjectTable.init(a);
    var interner = interner_mod.Interner.init(a);
    const k = try bootstrap.boot(a, &store, &table, &interner);

    try testing.expect(ref_mod.isPointer(k.float_vt_ref));
    try testing.expect(ref_mod.isPointer(k.ffi_ptr_vt_ref));

    const ovt = ref_mod.mkEid(kernel_eids.OBJECT_VT);
    try testing.expectEqual(ovt, store.currentValue(kernel_eids.FLOAT_VT, kernel_eids.ATTR_DELEGATE).?);
    try testing.expectEqual(ovt, store.currentValue(kernel_eids.FFI_PTR_VT, kernel_eids.ATTR_DELEGATE).?);

    const vtv = ref_mod.mkEid(kernel_eids.VTABLE_VT);
    try testing.expectEqual(vtv, store.currentValue(kernel_eids.FLOAT_VT, kernel_eids.ATTR_VTABLE).?);
    try testing.expectEqual(vtv, store.currentValue(kernel_eids.FFI_PTR_VT, kernel_eids.ATTR_VTABLE).?);
}
