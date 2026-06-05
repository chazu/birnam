//! Phase 15.2/15.3/15.5 — heap collector tests (boxes + closures).

const std = @import("std");
const testing = std.testing;
const gc = @import("gc.zig");
const boxed = @import("boxed.zig");
const arenas = @import("arenas.zig");
const vm = @import("vm.zig");
const fs = @import("fact_store.zig");
const ot = @import("object_table.zig");
const ke = @import("kernel_eids.zig");
const ref = @import("ref.zig");

const FAIL = testing.failing_allocator; // boxes/closures route to arenas; param unused

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

    _ = try store.assert(100, ke.ATTR_AST_VALUE, kept, ke.M_USER);

    const st = gc.collectBoxes(&store, &table, a);
    try testing.expect(!st.skipped);
    try testing.expectEqual(@as(usize, 1), st.boxes_freed);
    try testing.expectEqual(@as(usize, 1), st.boxes_kept);
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
    const pool = try boxed.mkRrb(a, &.{in_pool}); // RRB nodes on the test arena
    _ = try store.assert(200, ke.ATTR_CONSTANT_POOL, pool, ke.M_USER);

    const st = gc.collectBoxes(&store, &table, a);
    try testing.expectEqual(@as(usize, 1), st.boxes_freed); // loose
    try testing.expectEqual(@as(f64, 2.5), boxed.untagFloat(in_pool));
    _ = loose;
}

test "gc: sweeps orphan closures, keeps table-referenced; live captures pin boxes" {
    try freshArenas();
    defer arenas.deinit();
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    // Live closure: referenced from a table slot, captures a float box.
    const cap_box = try boxed.mkFloat(FAIL, 4.5);
    const live = try vm.allocBytecodeBlock(FAIL, 500, ref.NIL, 0, 1);
    live.captures_tail[0] = cap_box;
    const live_eid = table.alloc();
    try table.put(live_eid, live.ref);

    // Orphan closure: referenced by nothing, captures its own box.
    const dead_box = try boxed.mkFloat(FAIL, 8.0);
    const dead = try vm.allocBytecodeBlock(FAIL, 501, ref.NIL, 0, 1);
    dead.captures_tail[0] = dead_box;

    try testing.expectEqual(@as(usize, 2), arenas.arena(.boxes).?.liveCount());
    try testing.expectEqual(@as(usize, 2), arenas.arena(.captures).?.liveCount());

    const st = gc.collectBoxes(&store, &table, a);
    // Dead closure + its box reclaimed; live closure + its captured box kept.
    try testing.expectEqual(@as(usize, 1), st.closures_freed);
    try testing.expectEqual(@as(usize, 1), st.closures_kept);
    try testing.expectEqual(@as(usize, 1), st.boxes_freed); // dead_box
    try testing.expectEqual(@as(usize, 1), st.boxes_kept); // cap_box via live closure
    try testing.expectEqual(@as(f64, 4.5), boxed.untagFloat(cap_box)); // still valid
}

test "gc: no-op when boxes arena registry is absent" {
    if (arenas.ready()) arenas.deinit();
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);
    const st = gc.collectBoxes(&store, &table, a);
    try testing.expect(st.skipped);
    try testing.expectEqual(@as(usize, 0), st.boxes_freed);
}
