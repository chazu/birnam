//! Phase 15.1b — global arena registry tests.

const std = @import("std");
const testing = std.testing;
const arenas = @import("arenas.zig");
const arena_mod = @import("arena.zig");

test "arenas: init populates the registry; deinit tears it down" {
    try testing.expect(!arenas.ready());
    try arenas.init(testing.allocator);
    defer arenas.deinit();
    try testing.expect(arenas.ready());

    // Every per-kind allocator works and records into its own arena.
    const s = try arenas.strings().alloc(u8, 8);
    _ = s;
    try testing.expectEqual(@as(usize, 1), arenas.arena(.strings).?.liveCount());
    try testing.expectEqual(@as(usize, 0), arenas.arena(.boxes).?.liveCount());
    // deinit (via defer) frees the outstanding `s` → leak-clean.
}

test "arenas: allocations land in the addressed arena (type-by-segment)" {
    try arenas.init(testing.allocator);
    defer arenas.deinit();

    const box = try arenas.boxes().alloc(u8, 16);
    const sig = try arenas.sig().alloc(u8, 16);

    try testing.expect(arenas.arena(.boxes).?.contains(@intFromPtr(box.ptr)));
    try testing.expect(!arenas.arena(.boxes).?.contains(@intFromPtr(sig.ptr)));
    try testing.expect(arenas.arena(.sig).?.contains(@intFromPtr(sig.ptr)));
}

test "arenas: re-init tears the prior registry down (no leak across boots)" {
    try arenas.init(testing.allocator);
    _ = try arenas.strings().alloc(u8, 32); // outstanding alloc on registry #1
    // A second init must free registry #1 (incl. the outstanding alloc) before
    // installing #2 — else testing.allocator flags a leak.
    try arenas.init(testing.allocator);
    defer arenas.deinit();
    try testing.expectEqual(@as(usize, 0), arenas.arena(.strings).?.liveCount());
}

test "arenas: arena() is null before init" {
    // Ordering: this runs in a fresh state only if no prior test left it set.
    // Guard explicitly by deinit if needed.
    if (arenas.ready()) arenas.deinit();
    try testing.expect(arenas.arena(.structs) == null);
}
