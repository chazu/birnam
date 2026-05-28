const std = @import("std");
const hamt = @import("hamt.zig");
const testing = std.testing;

const U64Map = hamt.Hamt(u64, u64, hamt.U64Ctx);
const StrMap = hamt.Hamt([]const u8, u64, hamt.StrCtx);

// -- Basic operations ----------------------------------------------------------

test "empty map has count 0 and no entries" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const m = U64Map.empty(arena.allocator(), .{});
    try testing.expectEqual(@as(usize, 0), m.count());
    try testing.expectEqual(@as(?u64, null), m.get(42));
}

test "single insert + lookup" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m = U64Map.empty(arena.allocator(), .{});
    m = try m.assoc(42, 100);
    try testing.expectEqual(@as(usize, 1), m.count());
    try testing.expectEqual(@as(?u64, 100), m.get(42));
    try testing.expectEqual(@as(?u64, null), m.get(43));
}

test "update existing key keeps count, replaces value" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m = U64Map.empty(arena.allocator(), .{});
    m = try m.assoc(1, 100);
    m = try m.assoc(1, 200);
    try testing.expectEqual(@as(usize, 1), m.count());
    try testing.expectEqual(@as(?u64, 200), m.get(1));
}

test "dissoc removes key, decrements count" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m = U64Map.empty(arena.allocator(), .{});
    m = try m.assoc(1, 10);
    m = try m.assoc(2, 20);
    m = try m.dissoc(1);
    try testing.expectEqual(@as(usize, 1), m.count());
    try testing.expectEqual(@as(?u64, null), m.get(1));
    try testing.expectEqual(@as(?u64, 20), m.get(2));
}

test "dissoc missing key is a no-op" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m = U64Map.empty(arena.allocator(), .{});
    m = try m.assoc(1, 10);
    m = try m.dissoc(99);
    try testing.expectEqual(@as(usize, 1), m.count());
    try testing.expectEqual(@as(?u64, 10), m.get(1));
}

test "dissoc emptying makes count 0" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m = U64Map.empty(arena.allocator(), .{});
    m = try m.assoc(7, 70);
    m = try m.dissoc(7);
    try testing.expectEqual(@as(usize, 0), m.count());
    try testing.expectEqual(@as(?u64, null), m.get(7));
}

// -- Larger scale --------------------------------------------------------------

test "10k inserts + lookups round-trip" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m = U64Map.empty(arena.allocator(), .{});
    const N: u64 = 10_000;
    var i: u64 = 0;
    while (i < N) : (i += 1) {
        m = try m.assoc(i, i * 3);
    }
    try testing.expectEqual(@as(usize, N), m.count());
    i = 0;
    while (i < N) : (i += 1) {
        try testing.expectEqual(@as(?u64, i * 3), m.get(i));
    }
    try testing.expectEqual(@as(?u64, null), m.get(N + 1));
}

test "10k inserts then 10k dissocs" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m = U64Map.empty(arena.allocator(), .{});
    const N: u64 = 10_000;
    var i: u64 = 0;
    while (i < N) : (i += 1) {
        m = try m.assoc(i, i);
    }
    i = 0;
    while (i < N) : (i += 1) {
        m = try m.dissoc(i);
    }
    try testing.expectEqual(@as(usize, 0), m.count());
    try testing.expectEqual(@as(?u64, null), m.get(5000));
}

test "interleaved insert / dissoc" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m = U64Map.empty(arena.allocator(), .{});
    const N: u64 = 1000;
    var i: u64 = 0;
    while (i < N) : (i += 1) {
        m = try m.assoc(i, i);
    }
    // Remove evens
    i = 0;
    while (i < N) : (i += 2) {
        m = try m.dissoc(i);
    }
    try testing.expectEqual(@as(usize, N / 2), m.count());
    i = 0;
    while (i < N) : (i += 1) {
        const want: ?u64 = if (i % 2 == 0) null else i;
        try testing.expectEqual(want, m.get(i));
    }
}

// -- Persistence / structural sharing ------------------------------------------

test "assoc returns new root; original is unchanged" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m1 = U64Map.empty(arena.allocator(), .{});
    m1 = try m1.assoc(1, 100);
    const m2 = try m1.assoc(2, 200);

    try testing.expectEqual(@as(usize, 1), m1.count());
    try testing.expectEqual(@as(?u64, null), m1.get(2));

    try testing.expectEqual(@as(usize, 2), m2.count());
    try testing.expectEqual(@as(?u64, 100), m2.get(1));
    try testing.expectEqual(@as(?u64, 200), m2.get(2));
}

test "dissoc returns new root; original is unchanged" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m1 = U64Map.empty(arena.allocator(), .{});
    m1 = try m1.assoc(1, 100);
    m1 = try m1.assoc(2, 200);
    const m2 = try m1.dissoc(1);

    try testing.expectEqual(@as(usize, 2), m1.count());
    try testing.expectEqual(@as(?u64, 100), m1.get(1));

    try testing.expectEqual(@as(usize, 1), m2.count());
    try testing.expectEqual(@as(?u64, null), m2.get(1));
    try testing.expectEqual(@as(?u64, 200), m2.get(2));
}

test "1000 versions retained simultaneously" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var versions: [1000]U64Map = undefined;
    versions[0] = U64Map.empty(arena.allocator(), .{});
    var i: u64 = 1;
    while (i < 1000) : (i += 1) {
        versions[i] = try versions[i - 1].assoc(i, i * 7);
    }
    // Each version should reflect the cumulative state at its point in time.
    i = 1;
    while (i < 1000) : (i += 1) {
        try testing.expectEqual(@as(usize, i), versions[i].count());
        // version i should have keys 1..i
        try testing.expectEqual(@as(?u64, 7), versions[i].get(1));
        try testing.expectEqual(@as(?u64, i * 7), versions[i].get(i));
        // and should NOT have key i+1 (if applicable)
        if (i + 1 < 1000) {
            try testing.expectEqual(@as(?u64, null), versions[i].get(i + 1));
        }
    }
}

// -- Collision handling --------------------------------------------------------

// Force collisions by using a hash context that always returns 0.
const ZeroHashCtx = struct {
    pub fn hash(_: ZeroHashCtx, _: u64) u64 {
        return 0;
    }
    pub fn eql(_: ZeroHashCtx, a: u64, b: u64) bool {
        return a == b;
    }
};
const ZeroMap = hamt.Hamt(u64, u64, ZeroHashCtx);

test "collision bucket: many keys with identical hash" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m = ZeroMap.empty(arena.allocator(), .{});
    const N: u64 = 50;
    var i: u64 = 0;
    while (i < N) : (i += 1) {
        m = try m.assoc(i, i * 2);
    }
    try testing.expectEqual(@as(usize, N), m.count());
    i = 0;
    while (i < N) : (i += 1) {
        try testing.expectEqual(@as(?u64, i * 2), m.get(i));
    }
    // Remove some, check rest
    i = 0;
    while (i < N) : (i += 3) {
        m = try m.dissoc(i);
    }
    i = 0;
    while (i < N) : (i += 1) {
        const want: ?u64 = if (i % 3 == 0) null else i * 2;
        try testing.expectEqual(want, m.get(i));
    }
}

test "collision bucket: update preserves count" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m = ZeroMap.empty(arena.allocator(), .{});
    m = try m.assoc(1, 10);
    m = try m.assoc(2, 20);
    m = try m.assoc(1, 999);
    try testing.expectEqual(@as(usize, 2), m.count());
    try testing.expectEqual(@as(?u64, 999), m.get(1));
    try testing.expectEqual(@as(?u64, 20), m.get(2));
}

// -- String keys ---------------------------------------------------------------

test "string keys: basic ops" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m = StrMap.empty(arena.allocator(), .{});
    m = try m.assoc("foo", 1);
    m = try m.assoc("bar", 2);
    m = try m.assoc("baz", 3);
    try testing.expectEqual(@as(usize, 3), m.count());
    try testing.expectEqual(@as(?u64, 1), m.get("foo"));
    try testing.expectEqual(@as(?u64, 2), m.get("bar"));
    try testing.expectEqual(@as(?u64, 3), m.get("baz"));
    try testing.expectEqual(@as(?u64, null), m.get("qux"));
}

// -- Iterator ------------------------------------------------------------------

test "iterator visits all entries exactly once" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m = U64Map.empty(arena.allocator(), .{});
    const N: u64 = 200;
    var i: u64 = 0;
    while (i < N) : (i += 1) {
        m = try m.assoc(i, i * 11);
    }

    var seen = std.AutoHashMap(u64, void).init(std.testing.allocator);
    defer seen.deinit();

    var it = m.iter();
    var visited: usize = 0;
    while (it.next()) |kv| {
        try testing.expect(!seen.contains(kv.key));
        try seen.put(kv.key, {});
        try testing.expectEqual(kv.key * 11, kv.value);
        visited += 1;
    }
    try testing.expectEqual(N, @as(u64, @intCast(visited)));
}

test "iterator on empty" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const m = U64Map.empty(arena.allocator(), .{});
    var it = m.iter();
    try testing.expectEqual(@as(?U64Map.KV, null), it.next());
}

// -- Sanity at 100k ------------------------------------------------------------

test "100k insert + lookup sanity" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    var m = U64Map.empty(arena.allocator(), .{});
    const N: u64 = 100_000;
    var i: u64 = 0;
    while (i < N) : (i += 1) {
        m = try m.assoc(i, i ^ 0xDEADBEEF);
    }
    try testing.expectEqual(@as(usize, N), m.count());
    // Sample lookups
    try testing.expectEqual(@as(?u64, 0 ^ 0xDEADBEEF), m.get(0));
    try testing.expectEqual(@as(?u64, 50_000 ^ 0xDEADBEEF), m.get(50_000));
    try testing.expectEqual(@as(?u64, (N - 1) ^ 0xDEADBEEF), m.get(N - 1));
    try testing.expectEqual(@as(?u64, null), m.get(N));
}
