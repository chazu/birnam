//! Phase 11.7.v — tx_engine commit tests.

const std = @import("std");
const testing = std.testing;
const fact_store_mod = @import("fact_store.zig");
const tx_engine = @import("tx_engine.zig");
const ref_mod = @import("ref.zig");

test "tx_engine: commit assert returns tx id" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    const tx = try tx_engine.commit(&store, tx_engine.opAssert(1000, 42, ref_mod.mkSmallInt(7).?, 1));
    try testing.expectEqual(@as(u64, 1), tx);

    const val = store.currentValue(1000, 42);
    try testing.expect(val != null);
    try testing.expectEqual(ref_mod.mkSmallInt(7).?, val.?);
}

test "tx_engine: commit assert + retract bumps tx counter monotonically" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    const v = ref_mod.mkSmallInt(99).?;
    const tx1 = try tx_engine.commit(&store, tx_engine.opAssert(2000, 50, v, 1));
    const tx2 = try tx_engine.commit(&store, tx_engine.opRetract(2000, 50, v, 1));

    try testing.expect(tx2 > tx1);
    try testing.expect(store.currentValue(2000, 50) == null);
}

test "tx_engine: parity with direct store.assert" {
    var arena_a = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena_a.deinit();
    var store_a = fact_store_mod.FactStore.init(arena_a.allocator());
    var arena_b = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena_b.deinit();
    var store_b = fact_store_mod.FactStore.init(arena_b.allocator());

    const v = ref_mod.mkSmallInt(123).?;
    _ = try store_a.assert(500, 30, v, 2);
    _ = try tx_engine.commit(&store_b, tx_engine.opAssert(500, 30, v, 2));

    try testing.expectEqual(store_a.currentValue(500, 30), store_b.currentValue(500, 30));
}
