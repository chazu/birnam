const std = @import("std");
const ref = @import("ref.zig");
const testing = std.testing;

// -- Tag predicate disjointness ----------------------------------------------

test "tag predicates: pointer" {
    const r: ref.Ref = 0; // raw pointer bit pattern (addr 0)
    try testing.expect(ref.isPointer(r));
    try testing.expect(!ref.isSmallInt(r));
    try testing.expect(!ref.isEid(r));
    try testing.expect(!ref.isKeyword(r));
    try testing.expectEqual(ref.Tag.pointer, ref.tagOf(r));
}

test "tag predicates: smallint" {
    const r = ref.mkSmallInt(42).?;
    try testing.expect(!ref.isPointer(r));
    try testing.expect(ref.isSmallInt(r));
    try testing.expect(!ref.isEid(r));
    try testing.expect(!ref.isKeyword(r));
    try testing.expectEqual(ref.Tag.smallint, ref.tagOf(r));
}

test "tag predicates: eid" {
    const r = ref.mkEid(100);
    try testing.expect(!ref.isPointer(r));
    try testing.expect(!ref.isSmallInt(r));
    try testing.expect(ref.isEid(r));
    try testing.expect(!ref.isKeyword(r));
    try testing.expectEqual(ref.Tag.eid, ref.tagOf(r));
}

test "tag predicates: keyword" {
    const r = ref.mkKeyword(7);
    try testing.expect(!ref.isPointer(r));
    try testing.expect(!ref.isSmallInt(r));
    try testing.expect(!ref.isEid(r));
    try testing.expect(ref.isKeyword(r));
    try testing.expectEqual(ref.Tag.keyword, ref.tagOf(r));
}

// -- NIL ---------------------------------------------------------------------

test "NIL is mkEid(0)" {
    try testing.expectEqual(ref.mkEid(0), ref.NIL);
    try testing.expect(ref.isNil(ref.NIL));
    try testing.expect(ref.isEid(ref.NIL));
    try testing.expectEqual(@as(u64, 0), ref.untagEid(ref.NIL));
}

test "NIL is not all-zero" {
    try testing.expect(ref.NIL != 0);
}

test "all-zero Ref is a null pointer, not NIL" {
    const z: ref.Ref = 0;
    try testing.expect(ref.isPointer(z));
    try testing.expect(!ref.isNil(z));
}

// -- SmallInt round-trip + overflow ------------------------------------------

test "smallint round-trip: positive" {
    const cases = [_]i64{ 0, 1, 7, 42, 1000, 1 << 30, 1 << 50 };
    for (cases) |n| {
        const r = ref.mkSmallInt(n).?;
        try testing.expectEqual(n, ref.untagSmallInt(r));
    }
}

test "smallint round-trip: negative" {
    const cases = [_]i64{ -1, -2, -42, -1000, -(1 << 30), -(1 << 50) };
    for (cases) |n| {
        const r = ref.mkSmallInt(n).?;
        try testing.expectEqual(n, ref.untagSmallInt(r));
    }
}

test "smallint extremes fit" {
    const lo = ref.mkSmallInt(ref.SMALLINT_MIN).?;
    try testing.expectEqual(ref.SMALLINT_MIN, ref.untagSmallInt(lo));
    const hi = ref.mkSmallInt(ref.SMALLINT_MAX).?;
    try testing.expectEqual(ref.SMALLINT_MAX, ref.untagSmallInt(hi));
}

test "smallint overflow returns null" {
    try testing.expectEqual(@as(?ref.Ref, null), ref.mkSmallInt(ref.SMALLINT_MAX + 1));
    try testing.expectEqual(@as(?ref.Ref, null), ref.mkSmallInt(ref.SMALLINT_MIN - 1));
    try testing.expectEqual(@as(?ref.Ref, null), ref.mkSmallInt(std.math.maxInt(i64)));
    try testing.expectEqual(@as(?ref.Ref, null), ref.mkSmallInt(std.math.minInt(i64)));
}

// -- EID round-trip ----------------------------------------------------------

test "eid round-trip" {
    const cases = [_]u64{ 0, 1, 64, 100, 1 << 30, ref.EID_MAX };
    for (cases) |e| {
        const r = ref.mkEid(e);
        try testing.expectEqual(e, ref.untagEid(r));
        try testing.expect(ref.isEid(r));
    }
}

// -- Keyword round-trip ------------------------------------------------------

test "keyword round-trip" {
    const cases = [_]u64{ 0, 1, 42, 1 << 50, ref.KEYWORD_MAX };
    for (cases) |k| {
        const r = ref.mkKeyword(k);
        try testing.expectEqual(k, ref.untagKeyword(r));
        try testing.expect(ref.isKeyword(r));
    }
}

// -- Pointer encoding --------------------------------------------------------

test "pointer round-trip on aligned address" {
    var buf: [16]u8 align(8) = undefined;
    const r = ref.mkPointer(@ptrCast(&buf));
    try testing.expect(ref.isPointer(r));
    const got: *anyopaque = ref.untagPointer(r);
    try testing.expectEqual(@intFromPtr(&buf), @intFromPtr(got));
}
