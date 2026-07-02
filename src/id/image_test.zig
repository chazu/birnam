//! Tests for the image format (docs/40, slice 3). Step 1: the segment skeleton —
//! the header round-trips, an empty image is valid, and the load path rejects
//! garbage / truncation / a foreign format. Fact/heap round-trips arrive with
//! their steps.

const std = @import("std");
const image = @import("image.zig");

test "abiHash is deterministic and non-trivial" {
    try std.testing.expectEqual(image.abiHash(), image.abiHash());
    try std.testing.expect(image.abiHash() != 0);
}

test "a header-only image is valid; load walks its empty record stream" {
    // The format floor, exercised without a running system: writeHeaderOnly emits
    // just the header, and load (finding no records) accepts it. The full
    // store-integrated round-trip is the birnam-level test in S3.5.
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const path = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(path);
    const file = try std.fs.path.join(std.testing.allocator, &.{ path, "empty.image" });
    defer std.testing.allocator.free(file);

    try std.testing.expect(image.writeHeaderOnly(file));
    try std.testing.expect(image.validate(file));
}

test "load rejects a missing file, garbage, and a bad magic" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const dir = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(dir);

    const missing = try std.fs.path.join(std.testing.allocator, &.{ dir, "nope.image" });
    defer std.testing.allocator.free(missing);
    try std.testing.expect(!image.validate(missing));

    const garbage = try std.fs.path.join(std.testing.allocator, &.{ dir, "garbage.image" });
    defer std.testing.allocator.free(garbage);
    {
        const f = try std.fs.cwd().createFile(garbage, .{});
        defer f.close();
        try f.writeAll("not an image at all");
    }
    try std.testing.expect(!image.validate(garbage));

    // Right magic length, wrong bytes → rejected.
    const badmagic = try std.fs.path.join(std.testing.allocator, &.{ dir, "badmagic.image" });
    defer std.testing.allocator.free(badmagic);
    {
        const f = try std.fs.cwd().createFile(badmagic, .{});
        defer f.close();
        try f.writeAll("XXXXXXXX" ++ &[_]u8{0} ** 20);
    }
    try std.testing.expect(!image.validate(badmagic));
}

test "load rejects a truncated header" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const dir = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(dir);
    const trunc = try std.fs.path.join(std.testing.allocator, &.{ dir, "trunc.image" });
    defer std.testing.allocator.free(trunc);
    {
        const f = try std.fs.cwd().createFile(trunc, .{});
        defer f.close();
        try f.writeAll(image.MAGIC); // magic only — header incomplete
    }
    try std.testing.expect(!image.validate(trunc));
}
