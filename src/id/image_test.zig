//! Tests for the image (docs/40, slice 3). Two levels: FORMAT tests exercise the
//! header + record framing with no running system (via `writeHeaderOnly` /
//! `validate`); the ROUND-TRIP test brings up a real id stack in-process and
//! proves store-reachable state — leaf globals, a vector trie, and a HAMT —
//! survives `saveImage` → `loadImage` (a wholesale replace, so an intervening
//! mutation is undone by the load).

const std = @import("std");
const image = @import("image.zig");
const id = @import("id.zig");
const store = @import("store.zig");
const collection = @import("collection.zig");
const prelude = @import("prelude.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

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

// -- The round-trip: a real stack, in-process --------------------------------

var gapi: *idapi.IdApi = undefined;

fn setup() *idapi.IdApi {
    const api = id.id_init();
    gapi = api;
    prelude.bringUp(api); // exports SystemVt (image extends it)
    store.bringUp(api); // seeds the schema; exports Store + Globals
    collection.bringUp(api); // Vector + Map factories
    image.bringUp(api); // saveImage:/loadImage: + the fact collector
    return api;
}

fn send(rcv: Oop, sel: [*:0]const u8, args: []const Oop) Oop {
    return gapi.send(rcv, gapi.intern(sel), args.ptr, args.len);
}
fn gPut(globals: Oop, name: [*:0]const u8, val: Oop) void {
    _ = send(globals, "at:put:", &.{ gapi.intern(name), val });
}
fn gAt(globals: Oop, name: [*:0]const u8) Oop {
    return send(globals, "at:", &.{gapi.intern(name)});
}

fn tmpImagePath(a: std.mem.Allocator, tmp: *std.testing.TmpDir) ![]u8 {
    const dir = try tmp.dir.realpathAlloc(a, ".");
    defer a.free(dir);
    return std.fs.path.join(a, &.{ dir, "rt.image" });
}

test "round-trip: leaf + vector + map globals survive save/load" {
    const api = setup();
    const globals = api.import("Globals").?;

    // int, string, vector #[10 20 30], map {#k -> 7}
    gPut(globals, "gx", api.mkSmallInt(42));
    gPut(globals, "gs", api.mkString("hi", 2));

    const vfac = api.import("Vector").?;
    var v = send(vfac, "new", &.{});
    v = send(v, "add:", &.{api.mkSmallInt(10)});
    v = send(v, "add:", &.{api.mkSmallInt(20)});
    v = send(v, "add:", &.{api.mkSmallInt(30)});
    gPut(globals, "gv", v);

    const mfac = api.import("Map").?;
    var m = send(mfac, "new", &.{});
    m = send(m, "at:put:", &.{ api.intern("k"), api.mkSmallInt(7) });
    gPut(globals, "gm", m);

    // Save, then MUTATE gx — the load must undo it (wholesale replace).
    const a = std.testing.allocator;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const path = try tmpImagePath(a, &tmp);
    defer a.free(path);

    try std.testing.expect(image.saveImage(path));
    gPut(globals, "gx", api.mkSmallInt(999));
    try std.testing.expect(image.loadImage(path));

    // Leaves.
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(gAt(globals, "gx")));
    const gs = gAt(globals, "gs");
    var slen: usize = 0;
    const sp = api.stringBytes(gs, &slen);
    try std.testing.expectEqualStrings("hi", sp[0..slen]);

    // Vector trie: size + an element (0-indexed).
    const gv = gAt(globals, "gv");
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(send(gv, "size", &.{})));
    try std.testing.expectEqual(@as(i64, 20), api.smallIntValue(send(gv, "at:", &.{api.mkSmallInt(1)})));

    // HAMT: the symbol key resolves after re-interning (the keyHash fix).
    const gm = gAt(globals, "gm");
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(send(gm, "at:", &.{api.intern("k")})));
}
