//! Phase 12.1 slice 1 — image fact-layer round-trip.

const std = @import("std");
const testing = std.testing;
const image = @import("image.zig");
const fs = @import("fact_store.zig");
const ref = @import("ref.zig");
const boxed = @import("boxed.zig");

fn vi(n: i64) ref.Ref {
    return ref.mkSmallInt(n).?;
}

const M_USER: u64 = 2;
const E_A: u64 = 300;
const E_B: u64 = 301;
const A_REF: u64 = 400;
const A_NUM: u64 = 401;
const A_KW: u64 = 402;

test "image: immediate-valued facts round-trip with counters" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var src = fs.FactStore.init(a);
    _ = try src.assert(E_A, A_REF, ref.mkEid(E_B), M_USER);
    _ = try src.assert(E_A, A_NUM, vi(42), M_USER);
    _ = try src.assert(E_B, A_NUM, vi(-7), M_USER);
    _ = try src.assert(E_B, A_KW, ref.mkKeyword(99), M_USER);

    const SAVED_NEXT_EID: u64 = 512;
    const res = try image.write(a, &src, SAVED_NEXT_EID);

    try testing.expectEqual(@as(u64, 4), res.facts_written);
    try testing.expectEqual(@as(u64, 0), res.skipped_pointer_values);

    var dst = fs.FactStore.init(a);
    const loaded = try image.load(&dst, res.bytes);

    try testing.expectEqual(@as(u64, 4), loaded.facts_loaded);
    try testing.expectEqual(SAVED_NEXT_EID, loaded.next_eid);
    try testing.expectEqual(src.currentTx() + 1, loaded.next_tx);

    try testing.expectEqual(ref.mkEid(E_B), dst.currentValue(E_A, A_REF).?);
    try testing.expectEqual(vi(42), dst.currentValue(E_A, A_NUM).?);
    try testing.expectEqual(vi(-7), dst.currentValue(E_B, A_NUM).?);
    try testing.expectEqual(ref.mkKeyword(99), dst.currentValue(E_B, A_KW).?);

    // next_tx restored to the source high-water mark, not the replay count.
    try testing.expectEqual(src.currentTx(), dst.currentTx());
}

test "image: pointer-valued facts are counted as skipped, never dropped silently" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var src = fs.FactStore.init(a);
    _ = try src.assert(E_A, A_NUM, vi(1), M_USER);
    // A boxed float materializes as a heap pointer Ref — not serializable
    // in slice 1.
    const float_ref = try boxed.mkFloat(a, 3.14);
    try testing.expect(ref.isPointer(float_ref));
    _ = try src.assert(E_A, A_REF, float_ref, M_USER);

    const res = try image.write(a, &src, 256);
    try testing.expectEqual(@as(u64, 1), res.facts_written);
    try testing.expectEqual(@as(u64, 1), res.skipped_pointer_values);

    var dst = fs.FactStore.init(a);
    _ = try image.load(&dst, res.bytes);
    try testing.expectEqual(vi(1), dst.currentValue(E_A, A_NUM).?);
    // The pointer fact did not round-trip (deferred to a later slice).
    try testing.expect(dst.currentValue(E_A, A_REF) == null);
}

test "image: rejects bad magic and version" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var dst = fs.FactStore.init(arena.allocator());

    try testing.expectError(error.BadMagic, image.load(&dst, "not an image"));
    try testing.expectError(error.Truncated, image.load(&dst, "BRN"));
}
