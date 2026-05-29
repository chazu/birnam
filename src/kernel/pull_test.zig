const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const pull = @import("pull.zig");

const Ref = ref_mod.Ref;

const M: u64 = 1;
const E1: u64 = 100;
const E2: u64 = 101;
const E3: u64 = 102;
const A_NAME: u64 = 200;
const A_AGE: u64 = 201;
const A_PARENT: u64 = 202;
const A_NEXT: u64 = 203;

fn smallInt(n: i64) Ref {
    return ref_mod.mkSmallInt(n).?;
}

// -- Flat attribute pull -----------------------------------------------------

test "flat pull: name + age" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    _ = try store.assert(E1, A_NAME, smallInt(42), M);
    _ = try store.assert(E1, A_AGE, smallInt(30), M);

    const elements = try a.alloc(pull.PullElement, 2);
    elements[0] = .{ .attr = A_NAME };
    elements[1] = .{ .attr = A_AGE };
    const spec = pull.PullSpec{ .elements = elements };

    const result = try pull.executePull(a, &store, E1, spec);
    defer {
        result.deinit(a);
        a.destroy(result);
    }
    try testing.expectEqual(E1, result.entity);
    try testing.expectEqual(@as(usize, 2), result.entries.len);
    try testing.expectEqual(smallInt(42), result.entries[0].value.leaf);
    try testing.expectEqual(smallInt(30), result.entries[1].value.leaf);
}

test "pull missing attribute returns null_value (Date discipline)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    _ = try store.assert(E1, A_NAME, smallInt(7), M);

    const elements = try a.alloc(pull.PullElement, 2);
    elements[0] = .{ .attr = A_NAME };
    elements[1] = .{ .attr = A_AGE };
    const spec = pull.PullSpec{ .elements = elements };

    const result = try pull.executePull(a, &store, E1, spec);
    defer {
        result.deinit(a);
        a.destroy(result);
    }
    try testing.expectEqual(smallInt(7), result.entries[0].value.leaf);
    try testing.expectEqual(pull.PullValue.null_value, result.entries[1].value);
}

// -- Sub-pull ---------------------------------------------------------------

test "sub: pull a nested entity by attribute" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    _ = try store.assert(E1, A_NAME, smallInt(1), M);
    _ = try store.assert(E1, A_PARENT, ref_mod.mkEid(E2), M);
    _ = try store.assert(E2, A_NAME, smallInt(2), M);

    const sub_elements = try a.alloc(pull.PullElement, 1);
    sub_elements[0] = .{ .attr = A_NAME };

    const elements = try a.alloc(pull.PullElement, 2);
    elements[0] = .{ .attr = A_NAME };
    elements[1] = .{ .sub = .{ .attr = A_PARENT, .spec = sub_elements } };
    const spec = pull.PullSpec{ .elements = elements };

    const result = try pull.executePull(a, &store, E1, spec);
    defer {
        result.deinit(a);
        a.destroy(result);
    }
    try testing.expectEqual(smallInt(1), result.entries[0].value.leaf);
    const nested = result.entries[1].value.nested;
    try testing.expectEqual(E2, nested.entity);
    try testing.expectEqual(smallInt(2), nested.entries[0].value.leaf);
}

// -- Recurse ---------------------------------------------------------------

test "recurse: walk a linked chain" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    // E1 -> E2 -> E3 (terminal).
    _ = try store.assert(E1, A_NEXT, ref_mod.mkEid(E2), M);
    _ = try store.assert(E2, A_NEXT, ref_mod.mkEid(E3), M);
    // E3 has no :next.

    const elements = try a.alloc(pull.PullElement, 1);
    elements[0] = .{ .recurse = .{ .attr = A_NEXT, .depth = 10 } };
    const spec = pull.PullSpec{ .elements = elements };

    const result = try pull.executePull(a, &store, E1, spec);
    defer {
        result.deinit(a);
        a.destroy(result);
    }

    // E1's :next is E2 (nested PullResult)
    const r2 = result.entries[0].value.nested;
    try testing.expectEqual(E2, r2.entity);
    // E2's :next is E3 (nested)
    const r3 = r2.entries[0].value.nested;
    try testing.expectEqual(E3, r3.entity);
    // E3's :next is null
    try testing.expectEqual(pull.PullValue.null_value, r3.entries[0].value);
}

test "recurse: depth limit cuts the walk and emits depth_marker" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    _ = try store.assert(E1, A_NEXT, ref_mod.mkEid(E2), M);
    _ = try store.assert(E2, A_NEXT, ref_mod.mkEid(E3), M);

    const elements = try a.alloc(pull.PullElement, 1);
    elements[0] = .{ .recurse = .{ .attr = A_NEXT, .depth = 1 } };
    const spec = pull.PullSpec{ .elements = elements };

    const result = try pull.executePull(a, &store, E1, spec);
    defer {
        result.deinit(a);
        a.destroy(result);
    }
    const r2 = result.entries[0].value.nested;
    try testing.expectEqual(E2, r2.entity);
    // Depth budget exhausted at E2 → :next emits depth_marker.
    try testing.expectEqual(pull.PullValue.depth_marker, r2.entries[0].value);
}

test "recurse: cycle emits cycle_marker, does not loop" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    // E1 -> E2 -> E1 (cycle).
    _ = try store.assert(E1, A_NEXT, ref_mod.mkEid(E2), M);
    _ = try store.assert(E2, A_NEXT, ref_mod.mkEid(E1), M);

    const elements = try a.alloc(pull.PullElement, 1);
    elements[0] = .{ .recurse = .{ .attr = A_NEXT, .depth = 50 } };
    const spec = pull.PullSpec{ .elements = elements };

    const result = try pull.executePull(a, &store, E1, spec);
    defer {
        result.deinit(a);
        a.destroy(result);
    }
    const r2 = result.entries[0].value.nested;
    try testing.expectEqual(E2, r2.entity);
    // E2's :next refers back to E1 (already visited) → cycle_marker.
    try testing.expectEqual(pull.PullValue{ .cycle_marker = E1 }, r2.entries[0].value);
}
