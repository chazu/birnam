//! Phase 12.1 slice 1 — image fact-layer round-trip.

const std = @import("std");
const testing = std.testing;
const image = @import("image.zig");
const fs = @import("fact_store.zig");
const ref = @import("ref.zig");
const boxed = @import("boxed.zig");
const strings = @import("strings.zig");
const kernel_eids = @import("kernel_eids.zig");

fn vi(n: i64) ref.Ref {
    return ref.mkSmallInt(n).?;
}

const M_USER: u64 = kernel_eids.M_USER;
const M_CODE: u64 = kernel_eids.M_LANG; // a code-module tag — must NOT persist
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

test "image: boxed float values round-trip (slice 3)" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var src = fs.FactStore.init(a);
    _ = try src.assert(E_A, A_REF, try boxed.mkFloat(a, 3.14159), M_USER);
    _ = try src.assert(E_B, A_REF, try boxed.mkFloat(a, -0.0), M_USER);

    const res = try image.write(a, &src, 256);
    try testing.expectEqual(@as(u64, 2), res.facts_written);
    try testing.expectEqual(@as(u64, 0), res.skipped_pointer_values);

    var dst = fs.FactStore.init(a);
    _ = try image.load(&dst, res.bytes);

    const got = dst.currentValue(E_A, A_REF).?;
    try testing.expect(boxed.isFloat(got));
    try testing.expectEqual(@as(f64, 3.14159), boxed.untagFloat(got));
    try testing.expectEqual(@as(f64, -0.0), boxed.untagFloat(dst.currentValue(E_B, A_REF).?));
}

test "image: RRB vector values round-trip incl mixed + nested (slice 3)" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var src = fs.FactStore.init(a);
    // Mixed-element vector: smallint, string, float — like a constant pool.
    const inner = try boxed.mkRrb(a, &.{
        vi(7),
        try strings.mkString(a, "pool-entry"),
        try boxed.mkFloat(a, 2.5),
    });
    // Nested: a vector containing the inner vector + an eid.
    const outer = try boxed.mkRrb(a, &.{ inner, ref.mkEid(E_B) });
    _ = try src.assert(E_A, A_REF, outer, M_USER);

    const res = try image.write(a, &src, 256);
    try testing.expectEqual(@as(u64, 1), res.facts_written);
    try testing.expectEqual(@as(u64, 0), res.skipped_pointer_values);

    var dst = fs.FactStore.init(a);
    _ = try image.load(&dst, res.bytes);

    const got_outer = dst.currentValue(E_A, A_REF).?;
    try testing.expect(boxed.isRrbRef(got_outer));
    const ro = boxed.untagRrbRef(got_outer);
    try testing.expectEqual(@as(usize, 2), ro.count());

    var oit = ro.iter();
    const got_inner = oit.next().?;
    try testing.expectEqual(ref.mkEid(E_B), oit.next().?);
    try testing.expect(boxed.isRrbRef(got_inner));
    const ri = boxed.untagRrbRef(got_inner);
    try testing.expectEqual(@as(usize, 3), ri.count());
    var iit = ri.iter();
    try testing.expectEqual(vi(7), iit.next().?);
    try testing.expectEqualStrings("pool-entry", strings.bytes(iit.next().?));
    try testing.expectEqual(@as(f64, 2.5), boxed.untagFloat(iit.next().?));
}

test "image: still-deferred pointer classes counted as skipped, never dropped" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var src = fs.FactStore.init(a);
    _ = try src.assert(E_A, A_NUM, vi(1), M_USER);
    // FFI pointer is a heap-pointer class not yet handled by the image.
    const ffi = try boxed.mkFfiPtr(a, 0xCAFE);
    try testing.expect(ref.isPointer(ffi));
    _ = try src.assert(E_A, A_REF, ffi, M_USER);

    const res = try image.write(a, &src, 256);
    try testing.expectEqual(@as(u64, 1), res.facts_written);
    try testing.expectEqual(@as(u64, 1), res.skipped_pointer_values);

    var dst = fs.FactStore.init(a);
    _ = try image.load(&dst, res.bytes);
    try testing.expectEqual(vi(1), dst.currentValue(E_A, A_NUM).?);
    try testing.expect(dst.currentValue(E_A, A_REF) == null);
}

test "image: string-valued facts round-trip by bytes (slice 2)" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var src = fs.FactStore.init(a);
    const s1 = try strings.mkString(a, "hello world");
    const s2 = try strings.mkString(a, ""); // empty string edge case
    _ = try src.assert(E_A, A_REF, s1, M_USER);
    _ = try src.assert(E_B, A_REF, s2, M_USER);
    _ = try src.assert(E_A, A_NUM, vi(5), M_USER); // mixed immediate + string

    const res = try image.write(a, &src, 256);
    try testing.expectEqual(@as(u64, 3), res.facts_written);
    try testing.expectEqual(@as(u64, 0), res.skipped_pointer_values);

    var dst = fs.FactStore.init(a);
    _ = try image.load(&dst, res.bytes);

    const got1 = dst.currentValue(E_A, A_REF).?;
    const got2 = dst.currentValue(E_B, A_REF).?;
    try testing.expect(strings.isString(got1));
    try testing.expectEqualStrings("hello world", strings.bytes(got1));
    try testing.expectEqualStrings("", strings.bytes(got2));
    try testing.expectEqual(vi(5), dst.currentValue(E_A, A_NUM).?);

    // Rebuilt string is a fresh allocation, not the original pointer.
    try testing.expect(got1 != s1);
}

test "image: persists M_USER facts only, skips code-module provenance" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var src = fs.FactStore.init(a);
    _ = try src.assert(E_A, A_NUM, vi(1), M_USER); // user data — persist
    _ = try src.assert(E_B, A_NUM, vi(2), M_CODE); // code fact — reconstructed on load, skip

    const res = try image.write(a, &src, 256);
    try testing.expectEqual(@as(u64, 1), res.facts_written);

    var dst = fs.FactStore.init(a);
    _ = try image.load(&dst, res.bytes);
    try testing.expectEqual(vi(1), dst.currentValue(E_A, A_NUM).?);
    // The code-module fact was not persisted (init rebuilds it).
    try testing.expect(dst.currentValue(E_B, A_NUM) == null);
}

test "provenance: module unload (retractAllOf) leaves M_USER data intact" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fs.FactStore.init(a);
    // A code-module install fact + a user-data fact on the same entity.
    _ = try store.assert(E_A, A_NUM, vi(100), M_CODE);
    _ = try store.assert(E_A, A_REF, ref.mkEid(E_B), M_USER);

    // Mirror Manager.retractAllOf(M_CODE): retract every fact owned by the
    // code module. M_USER facts live in a disjoint meat subtree, so they
    // are unreachable from this walk.
    var it = try store.factsOfModule(M_CODE, a);
    defer it.deinit();
    while (it.next()) |t| _ = try store.retract(t.e, t.a, t.v, M_CODE);

    // Code fact gone; user data survives the unload.
    try testing.expect(store.currentValue(E_A, A_NUM) == null);
    try testing.expectEqual(ref.mkEid(E_B), store.currentValue(E_A, A_REF).?);
}

test "image: rejects bad magic and version" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var dst = fs.FactStore.init(arena.allocator());

    try testing.expectError(error.BadMagic, image.load(&dst, "not an image"));
    try testing.expectError(error.Truncated, image.load(&dst, "BRN"));
}
