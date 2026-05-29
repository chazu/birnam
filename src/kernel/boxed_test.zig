const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const boxed = @import("boxed.zig");
const bootstrap = @import("bootstrap.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const kernel_eids = @import("kernel_eids.zig");

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
