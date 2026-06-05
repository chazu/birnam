//! Phase 15.1 — segmented arena (recording allocator) tests.

const std = @import("std");
const testing = std.testing;
const arena_mod = @import("arena.zig");
const Arena = arena_mod.Arena;

test "arena: records live allocations; payload is byte-identical" {
    var ar = Arena.init(testing.allocator, .strings);
    defer ar.deinit();
    const a = ar.allocator();

    const buf = try a.alloc(u8, 16);
    @memset(buf, 0xab);
    try testing.expectEqual(@as(usize, 1), ar.liveCount());
    try testing.expect(ar.contains(@intFromPtr(buf.ptr)));
    // Payload usable exactly like a normal allocation.
    try testing.expectEqual(@as(u8, 0xab), buf[7]);

    a.free(buf);
    try testing.expectEqual(@as(usize, 0), ar.liveCount());
    try testing.expect(!ar.contains(@intFromPtr(buf.ptr)));
}

test "arena: sweep frees unmarked, keeps marked, clears marks" {
    var ar = Arena.init(testing.allocator, .boxes);
    defer ar.deinit();
    const a = ar.allocator();

    const keep = try a.alloc(u8, 8);
    const drop = try a.alloc(u8, 8);
    try testing.expectEqual(@as(usize, 2), ar.liveCount());

    // Mark only `keep`.
    try testing.expect(ar.mark(@intFromPtr(keep.ptr)));
    try testing.expect(!ar.mark(0xdead)); // unowned address → no-op

    const freed = ar.sweep();
    try testing.expectEqual(@as(usize, 1), freed); // `drop` collected
    try testing.expectEqual(@as(usize, 1), ar.liveCount());
    try testing.expect(ar.contains(@intFromPtr(keep.ptr)));
    try testing.expect(!ar.contains(@intFromPtr(drop.ptr)));

    // Marks cleared by sweep → a second sweep with no marks collects `keep`.
    const freed2 = ar.sweep();
    try testing.expectEqual(@as(usize, 1), freed2);
    try testing.expectEqual(@as(usize, 0), ar.liveCount());
}

test "arena: contains discriminates between two arenas (type-by-segment)" {
    var strs = Arena.init(testing.allocator, .strings);
    defer strs.deinit();
    var sigs = Arena.init(testing.allocator, .sig);
    defer sigs.deinit();

    const s = try strs.allocator().alloc(u8, 4);
    const g = try sigs.allocator().alloc(u8, 4);

    try testing.expect(strs.contains(@intFromPtr(s.ptr)));
    try testing.expect(!strs.contains(@intFromPtr(g.ptr)));
    try testing.expect(sigs.contains(@intFromPtr(g.ptr)));
    try testing.expect(!sigs.contains(@intFromPtr(s.ptr)));
}

test "arena: deinit frees outstanding live allocations (leak-clean)" {
    // No explicit frees; deinit must reclaim everything or testing.allocator
    // flags a leak.
    var ar = Arena.init(testing.allocator, .bytecode);
    const a = ar.allocator();
    _ = try a.alloc(u8, 32);
    _ = try a.alloc(u8, 64);
    _ = try a.alloc(u64, 10);
    try testing.expectEqual(@as(usize, 3), ar.liveCount());
    ar.deinit();
}

test "arena: realloc updates the record, preserving liveness" {
    var ar = Arena.init(testing.allocator, .captures);
    defer ar.deinit();
    const a = ar.allocator();

    var buf = try a.alloc(u8, 8);
    try testing.expectEqual(@as(usize, 1), ar.liveCount());
    buf = try a.realloc(buf, 4096); // likely relocates
    // Still exactly one tracked allocation, at the (possibly new) address.
    try testing.expectEqual(@as(usize, 1), ar.liveCount());
    try testing.expect(ar.contains(@intFromPtr(buf.ptr)));

    // Sweep with no marks frees it — proves the post-realloc record is the one
    // the collector will see.
    try testing.expectEqual(@as(usize, 1), ar.sweep());
    try testing.expectEqual(@as(usize, 0), ar.liveCount());
}
