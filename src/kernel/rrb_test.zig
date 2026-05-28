const std = @import("std");
const rrb = @import("rrb.zig");
const testing = std.testing;

const U64Vec = rrb.Rrb(u64);

// -- Basic ops -----------------------------------------------------------------

test "empty vector" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const v = U64Vec.empty(arena.allocator());
    try testing.expectEqual(@as(usize, 0), v.count());
}

test "push + nth" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var v = U64Vec.empty(arena.allocator());
    v = try v.push(10);
    v = try v.push(20);
    v = try v.push(30);
    try testing.expectEqual(@as(usize, 3), v.count());
    try testing.expectEqual(@as(u64, 10), v.nth(0));
    try testing.expectEqual(@as(u64, 20), v.nth(1));
    try testing.expectEqual(@as(u64, 30), v.nth(2));
}

test "set" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var v = U64Vec.empty(arena.allocator());
    var i: u64 = 0;
    while (i < 5) : (i += 1) v = try v.push(i);
    v = try v.set(2, 999);
    try testing.expectEqual(@as(u64, 0), v.nth(0));
    try testing.expectEqual(@as(u64, 1), v.nth(1));
    try testing.expectEqual(@as(u64, 999), v.nth(2));
    try testing.expectEqual(@as(u64, 3), v.nth(3));
    try testing.expectEqual(@as(u64, 4), v.nth(4));
}

test "pop" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var v = U64Vec.empty(arena.allocator());
    v = try v.push(10);
    v = try v.push(20);
    v = try v.push(30);
    v = try v.pop();
    try testing.expectEqual(@as(usize, 2), v.count());
    try testing.expectEqual(@as(u64, 10), v.nth(0));
    try testing.expectEqual(@as(u64, 20), v.nth(1));
}

test "push + pop empties" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var v = U64Vec.empty(arena.allocator());
    v = try v.push(1);
    v = try v.pop();
    try testing.expectEqual(@as(usize, 0), v.count());
}

// -- Larger sizes / tree growth -----------------------------------------------

test "push 1000 — index + lookup round-trip" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var v = U64Vec.empty(arena.allocator());
    const N: u64 = 1000;
    var i: u64 = 0;
    while (i < N) : (i += 1) v = try v.push(i * 3);
    try testing.expectEqual(@as(usize, N), v.count());
    i = 0;
    while (i < N) : (i += 1) try testing.expectEqual(@as(u64, i * 3), v.nth(i));
}

test "push then pop to empty" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var v = U64Vec.empty(arena.allocator());
    const N: u64 = 500;
    var i: u64 = 0;
    while (i < N) : (i += 1) v = try v.push(i);
    while (i > 0) : (i -= 1) v = try v.pop();
    try testing.expectEqual(@as(usize, 0), v.count());
}

test "set every element of 1000 round-trips correctly" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var v = U64Vec.empty(arena.allocator());
    const N: u64 = 1000;
    var i: u64 = 0;
    while (i < N) : (i += 1) v = try v.push(0);
    i = 0;
    while (i < N) : (i += 1) v = try v.set(i, i * 7);
    i = 0;
    while (i < N) : (i += 1) try testing.expectEqual(@as(u64, i * 7), v.nth(i));
}

// Crosses multiple tree levels: 32 (depth 1) -> 1024 (depth 2) -> 32768 (depth 3)
test "tree-level transitions: 1, 32, 33, 1024, 1025, 10000" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const Ns = [_]u64{ 1, 32, 33, 1024, 1025, 10_000 };
    for (Ns) |N| {
        var fresh = U64Vec.empty(arena.allocator());
        var i: u64 = 0;
        while (i < N) : (i += 1) fresh = try fresh.push(i);
        try testing.expectEqual(@as(usize, N), fresh.count());
        i = 0;
        while (i < N) : (i += 1) try testing.expectEqual(@as(u64, i), fresh.nth(i));
    }
}

// -- Persistence / structural sharing -----------------------------------------

test "push returns new root; original unchanged" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var v1 = U64Vec.empty(arena.allocator());
    v1 = try v1.push(10);
    const v2 = try v1.push(20);
    try testing.expectEqual(@as(usize, 1), v1.count());
    try testing.expectEqual(@as(u64, 10), v1.nth(0));
    try testing.expectEqual(@as(usize, 2), v2.count());
    try testing.expectEqual(@as(u64, 20), v2.nth(1));
}

test "set returns new root; original unchanged" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var v1 = U64Vec.empty(arena.allocator());
    v1 = try v1.push(1);
    v1 = try v1.push(2);
    v1 = try v1.push(3);
    const v2 = try v1.set(1, 999);
    try testing.expectEqual(@as(u64, 2), v1.nth(1));
    try testing.expectEqual(@as(u64, 999), v2.nth(1));
}

test "pop returns new root; original unchanged" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var v1 = U64Vec.empty(arena.allocator());
    v1 = try v1.push(10);
    v1 = try v1.push(20);
    const v2 = try v1.pop();
    try testing.expectEqual(@as(usize, 2), v1.count());
    try testing.expectEqual(@as(u64, 20), v1.nth(1));
    try testing.expectEqual(@as(usize, 1), v2.count());
}

test "500 versions retained simultaneously" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var versions: [500]U64Vec = undefined;
    versions[0] = U64Vec.empty(arena.allocator());
    var i: u64 = 1;
    while (i < 500) : (i += 1) versions[i] = try versions[i - 1].push(i * 13);
    i = 1;
    while (i < 500) : (i += 1) {
        try testing.expectEqual(@as(usize, i), versions[i].count());
        var j: u64 = 1;
        while (j <= i) : (j += 1) {
            try testing.expectEqual(@as(u64, j * 13), versions[i].nth(j - 1));
        }
    }
}

// -- concat / split (naive impls) ---------------------------------------------

test "concat: two non-empty vectors" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var a = U64Vec.empty(arena.allocator());
    var b = U64Vec.empty(arena.allocator());
    for ([_]u64{ 1, 2, 3 }) |x| a = try a.push(x);
    for ([_]u64{ 4, 5, 6, 7 }) |x| b = try b.push(x);
    const c = try a.concat(b);
    try testing.expectEqual(@as(usize, 7), c.count());
    var i: u64 = 0;
    while (i < 7) : (i += 1) try testing.expectEqual(@as(u64, i + 1), c.nth(i));
}

test "concat: with empty" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var a = U64Vec.empty(arena.allocator());
    const b = U64Vec.empty(arena.allocator());
    a = try a.push(10);
    a = try a.push(20);
    const c1 = try a.concat(b);
    try testing.expectEqual(@as(usize, 2), c1.count());
    const c2 = try b.concat(a);
    try testing.expectEqual(@as(usize, 2), c2.count());
}

test "concat: 200 + 300 elements" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var a = U64Vec.empty(arena.allocator());
    var b = U64Vec.empty(arena.allocator());
    var i: u64 = 0;
    while (i < 200) : (i += 1) a = try a.push(i);
    while (i < 500) : (i += 1) b = try b.push(i);
    const c = try a.concat(b);
    try testing.expectEqual(@as(usize, 500), c.count());
    i = 0;
    while (i < 500) : (i += 1) try testing.expectEqual(@as(u64, i), c.nth(i));
}

test "splitAt: round-trip via concat" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var v = U64Vec.empty(arena.allocator());
    var i: u64 = 0;
    while (i < 100) : (i += 1) v = try v.push(i);
    const halves = try v.splitAt(37);
    const left = halves[0];
    const right = halves[1];
    try testing.expectEqual(@as(usize, 37), left.count());
    try testing.expectEqual(@as(usize, 63), right.count());
    const rejoined = try left.concat(right);
    try testing.expectEqual(@as(usize, 100), rejoined.count());
    i = 0;
    while (i < 100) : (i += 1) try testing.expectEqual(@as(u64, i), rejoined.nth(i));
}

test "splitAt: edges" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var v = U64Vec.empty(arena.allocator());
    var i: u64 = 0;
    while (i < 10) : (i += 1) v = try v.push(i);
    const at_zero = try v.splitAt(0);
    try testing.expectEqual(@as(usize, 0), at_zero[0].count());
    try testing.expectEqual(@as(usize, 10), at_zero[1].count());
    const at_end = try v.splitAt(10);
    try testing.expectEqual(@as(usize, 10), at_end[0].count());
    try testing.expectEqual(@as(usize, 0), at_end[1].count());
}

// -- Iterator ------------------------------------------------------------------

test "iterator visits in order" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var v = U64Vec.empty(arena.allocator());
    const N: u64 = 100;
    var i: u64 = 0;
    while (i < N) : (i += 1) v = try v.push(i * 5);
    var it = v.iter();
    var expected: u64 = 0;
    while (it.next()) |val| {
        try testing.expectEqual(expected * 5, val);
        expected += 1;
    }
    try testing.expectEqual(N, expected);
}

test "iterator on empty" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const v = U64Vec.empty(arena.allocator());
    var it = v.iter();
    try testing.expectEqual(@as(?u64, null), it.next());
}

// -- Sanity at 100k -----------------------------------------------------------

test "100k push + lookup sanity" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var v = U64Vec.empty(arena.allocator());
    const N: u64 = 100_000;
    var i: u64 = 0;
    while (i < N) : (i += 1) v = try v.push(i ^ 0xABCDEF12);
    try testing.expectEqual(@as(usize, N), v.count());
    try testing.expectEqual(@as(u64, 0 ^ 0xABCDEF12), v.nth(0));
    try testing.expectEqual(@as(u64, 50_000 ^ 0xABCDEF12), v.nth(50_000));
    try testing.expectEqual(@as(u64, (N - 1) ^ 0xABCDEF12), v.nth(N - 1));
}
