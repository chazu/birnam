const std = @import("std");
const ot = @import("object_table.zig");
const ref = @import("ref.zig");
const kernel_eids = @import("kernel_eids.zig");
const testing = std.testing;

// -- alloc -------------------------------------------------------------------

test "alloc starts at USER_EID_START" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var t = ot.ObjectTable.init(arena.allocator());
    try testing.expectEqual(kernel_eids.USER_EID_START, t.alloc());
}

test "alloc is monotonic" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var t = ot.ObjectTable.init(arena.allocator());
    const a = t.alloc();
    const b = t.alloc();
    const c = t.alloc();
    try testing.expect(a < b);
    try testing.expect(b < c);
    try testing.expectEqual(@as(u64, 1), b - a);
    try testing.expectEqual(@as(u64, 1), c - b);
}

test "alloc does not populate the table" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var t = ot.ObjectTable.init(arena.allocator());
    const eid = t.alloc();
    try testing.expectEqual(@as(?ref.Ref, null), t.get(eid));
    try testing.expectEqual(@as(usize, 0), t.materializedCount());
}

test "1000 allocations distinct + monotonic" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var t = ot.ObjectTable.init(arena.allocator());
    var prev: u64 = 0;
    var i: usize = 0;
    while (i < 1000) : (i += 1) {
        const e = t.alloc();
        if (i > 0) try testing.expect(e > prev);
        prev = e;
    }
}

// -- put / get / clear -------------------------------------------------------

test "put + get round-trip" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var t = ot.ObjectTable.init(arena.allocator());
    const eid = t.alloc();
    var buf: [8]u8 align(8) = undefined;
    const r = ref.mkPointer(@ptrCast(&buf));
    try t.put(eid, r);
    try testing.expectEqual(r, t.get(eid).?);
    try testing.expectEqual(@as(usize, 1), t.materializedCount());
}

test "put overwrites" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var t = ot.ObjectTable.init(arena.allocator());
    const eid = t.alloc();
    var buf1: [8]u8 align(8) = undefined;
    var buf2: [8]u8 align(8) = undefined;
    const r1 = ref.mkPointer(@ptrCast(&buf1));
    const r2 = ref.mkPointer(@ptrCast(&buf2));
    try t.put(eid, r1);
    try t.put(eid, r2);
    try testing.expectEqual(r2, t.get(eid).?);
    try testing.expectEqual(@as(usize, 1), t.materializedCount());
}

test "clear removes entry" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var t = ot.ObjectTable.init(arena.allocator());
    const eid = t.alloc();
    var buf: [8]u8 align(8) = undefined;
    try t.put(eid, ref.mkPointer(@ptrCast(&buf)));
    try t.clear(eid);
    try testing.expectEqual(@as(?ref.Ref, null), t.get(eid));
    try testing.expectEqual(@as(usize, 0), t.materializedCount());
}

test "clear of absent EID is a no-op" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var t = ot.ObjectTable.init(arena.allocator());
    const eid = t.alloc();
    try t.clear(eid); // never been put
    try testing.expectEqual(@as(?ref.Ref, null), t.get(eid));
    try testing.expectEqual(@as(usize, 0), t.materializedCount());
}

// -- Kernel EID range --------------------------------------------------------

test "bootstrap may put kernel EIDs without colliding with alloc" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var t = ot.ObjectTable.init(arena.allocator());
    var vt_buf: [16]u8 align(8) = undefined;
    const vt_ref = ref.mkPointer(@ptrCast(&vt_buf));
    try t.put(kernel_eids.VTABLE_VT, vt_ref);
    try t.put(kernel_eids.OBJECT_VT, vt_ref); // same Ref ok for test
    try testing.expectEqual(vt_ref, t.get(kernel_eids.VTABLE_VT).?);
    try testing.expectEqual(vt_ref, t.get(kernel_eids.OBJECT_VT).?);

    // user alloc still begins at USER_EID_START — unaffected by kernel puts
    try testing.expectEqual(kernel_eids.USER_EID_START, t.alloc());
}

// -- Scale -------------------------------------------------------------------

test "10k materialized entries" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var t = ot.ObjectTable.init(arena.allocator());

    // Use a single dummy aligned buffer; we're testing the table, not the pointer targets.
    var dummy: [8]u8 align(8) = undefined;
    const r = ref.mkPointer(@ptrCast(&dummy));

    var eids: [10_000]u64 = undefined;
    var i: usize = 0;
    while (i < 10_000) : (i += 1) {
        eids[i] = t.alloc();
        try t.put(eids[i], r);
    }
    try testing.expectEqual(@as(usize, 10_000), t.materializedCount());
    // Sample lookups
    try testing.expectEqual(r, t.get(eids[0]).?);
    try testing.expectEqual(r, t.get(eids[5000]).?);
    try testing.expectEqual(r, t.get(eids[9999]).?);
}
