//! Phase 15.2/15.3 — boxes mark-sweep collector tests.

const std = @import("std");
const testing = std.testing;
const gc = @import("gc.zig");
const boxed = @import("boxed.zig");
const arenas = @import("arenas.zig");
const fs = @import("fact_store.zig");
const ot = @import("object_table.zig");
const ke = @import("kernel_eids.zig");
const ref = @import("ref.zig");

const FAIL = testing.failing_allocator; // boxes route to the arena; param unused

fn freshArenas() !void {
    if (arenas.ready()) arenas.deinit();
    try arenas.init(testing.allocator);
}

test "gc: frees orphan boxes, keeps fact-referenced ones" {
    try freshArenas();
    defer arenas.deinit();
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    const kept = try boxed.mkFloat(FAIL, 1.5);
    const orphan = try boxed.mkFloat(FAIL, 9.0);
    try testing.expectEqual(@as(usize, 2), arenas.arena(.boxes).?.liveCount());

    // Only `kept` is referenced (a fact value); `orphan` by nothing.
    _ = try store.assert(100, ke.ATTR_AST_VALUE, kept, ke.M_USER);

    const st = gc.collectBoxes(&store, &table);
    try testing.expect(!st.skipped);
    try testing.expectEqual(@as(usize, 1), st.boxes_freed);
    try testing.expectEqual(@as(usize, 1), st.boxes_kept);
    try testing.expectEqual(@as(usize, 1), arenas.arena(.boxes).?.liveCount());

    // `kept` still reads back; `orphan` is freed — do not deref it.
    try testing.expectEqual(@as(f64, 1.5), boxed.untagFloat(kept));
    _ = orphan;
}

test "gc: keeps a box reachable only through a constant-pool RRB" {
    try freshArenas();
    defer arenas.deinit();
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    const in_pool = try boxed.mkFloat(FAIL, 2.5);
    const loose = try boxed.mkFloat(FAIL, 7.0);
    try testing.expectEqual(@as(usize, 2), arenas.arena(.boxes).?.liveCount());

    // `in_pool` lives only inside an RRB stored as a :constant-pool value.
    const pool = try boxed.mkRrb(a, &.{in_pool}); // RRB nodes on the test arena, not boxes
    _ = try store.assert(200, ke.ATTR_CONSTANT_POOL, pool, ke.M_USER);

    const st = gc.collectBoxes(&store, &table);
    try testing.expectEqual(@as(usize, 1), st.boxes_freed); // `loose`
    try testing.expectEqual(@as(f64, 2.5), boxed.untagFloat(in_pool)); // survived via RRB
    _ = loose;
}

test "gc: no-op when boxes arena registry is absent" {
    if (arenas.ready()) arenas.deinit();
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);
    const st = gc.collectBoxes(&store, &table);
    try testing.expect(st.skipped);
    try testing.expectEqual(@as(usize, 0), st.boxes_freed);
}
