const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const strings = @import("strings.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const kernel_eids = @import("kernel_eids.zig");
const siphon = @import("siphon.zig");

const Ref = ref_mod.Ref;

// -- String encoding ---------------------------------------------------------

test "mkString round-trips bytes" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const r = try strings.mkString(arena.allocator(), "hello");
    try testing.expect(strings.isString(r));
    try testing.expectEqualSlices(u8, "hello", strings.bytes(r));
}

test "string tag disjoint from other tags" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const r = try strings.mkString(arena.allocator(), "x");
    try testing.expect(strings.isString(r));
    try testing.expect(!ref_mod.isPointer(r));
    try testing.expect(!ref_mod.isEid(r));
    try testing.expect(!ref_mod.isSmallInt(r));
    try testing.expect(!ref_mod.isKeyword(r));
}

test "tagOf recognizes string" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const r = try strings.mkString(arena.allocator(), "abc");
    try testing.expectEqual(ref_mod.Tag.string, ref_mod.tagOf(r));
}

test "strings.equals byte-compares distinct allocations" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = try strings.mkString(arena.allocator(), "foo");
    const b = try strings.mkString(arena.allocator(), "foo");
    const c = try strings.mkString(arena.allocator(), "bar");
    try testing.expect(strings.equals(a, b)); // different Refs, same bytes
    try testing.expect(!strings.equals(a, c));
}

test "empty string" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const r = try strings.mkString(arena.allocator(), "");
    try testing.expect(strings.isString(r));
    try testing.expectEqual(@as(usize, 0), strings.bytes(r).len);
}

// -- Interner ----------------------------------------------------------------

fn setup(arena: *std.heap.ArenaAllocator) !struct {
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
} {
    const allocator = arena.allocator();
    const store = try allocator.create(fact_store.FactStore);
    store.* = fact_store.FactStore.init(allocator);
    const table = try allocator.create(object_table.ObjectTable);
    table.* = object_table.ObjectTable.init(allocator);
    const interner = try allocator.create(interner_mod.Interner);
    interner.* = interner_mod.Interner.init(allocator);
    _ = try siphon.boot(allocator, store, table, interner);
    return .{ .store = store, .table = table, .interner = interner };
}

test "intern is idempotent" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);
    const e1 = try s.interner.intern(s.store, s.table, "alpha");
    const e2 = try s.interner.intern(s.store, s.table, "alpha");
    try testing.expectEqual(e1, e2);
}

test "intern distinguishes" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);
    const a = try s.interner.intern(s.store, s.table, "alpha");
    const b = try s.interner.intern(s.store, s.table, "beta");
    try testing.expect(a != b);
}

test "kernel selectors carry the right names after bootstrap" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);

    try testing.expectEqual(kernel_eids.SEL_LOOKUP, try s.interner.intern(s.store, s.table, "lookup:"));
    try testing.expectEqual(kernel_eids.SEL_INIT, try s.interner.intern(s.store, s.table, "init"));
    try testing.expectEqual(kernel_eids.SEL_VTABLE, try s.interner.intern(s.store, s.table, "vtable"));
    try testing.expectEqual(kernel_eids.SEL_METHOD_AT_PUT_WITH, try s.interner.intern(s.store, s.table, "methodAt:put:with:"));
    try testing.expectEqual(kernel_eids.SEL_DELEGATED, try s.interner.intern(s.store, s.table, "delegated"));
    try testing.expectEqual(kernel_eids.SEL_ALLOCATE, try s.interner.intern(s.store, s.table, "allocate"));
}

test "reverse lookup: nameOf returns the seeded name" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);
    const got = s.interner.nameOf(kernel_eids.SEL_LOOKUP).?;
    try testing.expectEqualSlices(u8, "lookup:", got);
}

test "intern asserts :name fact in the store" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);

    const eid = try s.interner.intern(s.store, s.table, "newSelector");
    const name_ref = s.store.currentValue(eid, kernel_eids.ATTR_NAME).?;
    try testing.expect(strings.isString(name_ref));
    try testing.expectEqualSlices(u8, "newSelector", strings.bytes(name_ref));
}

test "new user-interned selectors land in the user EID range" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);

    const e = try s.interner.intern(s.store, s.table, "userFoo");
    try testing.expect(e >= kernel_eids.USER_EID_START);
}

test "1000 distinct strings intern to 1000 distinct EIDs" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);

    var buf: [32]u8 = undefined;
    var seen = std.AutoHashMap(u64, void).init(std.testing.allocator);
    defer seen.deinit();

    var i: usize = 0;
    while (i < 1000) : (i += 1) {
        const name = try std.fmt.bufPrint(&buf, "sel_{d}", .{i});
        const eid = try s.interner.intern(s.store, s.table, name);
        try testing.expect(!seen.contains(eid));
        try seen.put(eid, {});
    }
    try testing.expectEqual(@as(usize, 1000), seen.count());
}
