//! Proof for F7 — the error model CORE. Brings up the prelude (for zeroDivide via
//! the guarded `/`) and the error module (the type + the object_vt DNU swap).
//!
//! This suite covers only what error.zig still owns: the representation, the
//! `doesNotUnderstand:` swap, `isError`, and the `kind`/`message` accessors. The
//! error *protocol* (`on:do:`/`ensure:`/`ifError:`/`ifNotError:`/`printString`,
//! `String` raising) is now self-hosted birnam in `stdlib.birnam` (docs/44a) and is
//! exercised at the full-stack level by `examples/errors.birnam` — it cannot be
//! tested here because this suite brings up the bare kernel without stdlib.

const std = @import("std");
const id = @import("id.zig");
const prelude = @import("prelude.zig");
const errmod = @import("error.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

var gapi: *idapi.IdApi = undefined;

fn setup() *idapi.IdApi {
    const api = id.id_init();
    gapi = api;
    prelude.bringUp(api);
    errmod.bringUp(api); // exports "Error", swaps object_vt's DNU
    return api;
}

fn s0(rcv: Oop, sel: [*:0]const u8) Oop {
    return gapi.send(rcv, gapi.intern(sel), &[_]Oop{}, 0);
}
fn s1(rcv: Oop, sel: [*:0]const u8, a: Oop) Oop {
    return gapi.send(rcv, gapi.intern(sel), &[_]Oop{a}, 1);
}
fn isErr(o: Oop) bool {
    return s0(o, "isError") == gapi.trueObj;
}
fn strEq(o: Oop, want: []const u8) bool {
    var len: usize = 0;
    const p = gapi.stringBytes(o, &len);
    return std.mem.eql(u8, p[0..len], want);
}

test "F7: a missed send to nil yields a #doesNotUnderstand error, not a panic" {
    const api = setup();
    const e = api.send(api.nilObj, api.intern("frobnicate"), &[_]Oop{}, 0);
    try std.testing.expect(isErr(e));
    try std.testing.expect(s0(e, "kind") == api.intern("doesNotUnderstand"));
    try std.testing.expect(strEq(s0(e, "message"), "does not understand #frobnicate"));
}

test "F7: errors propagate through a chain of sends (DNU → self)" {
    const api = setup();
    const e = api.send(api.nilObj, api.intern("foo"), &[_]Oop{}, 0);
    // Sending anything to the error keeps answering the SAME error.
    const e2 = s1(s0(e, "bar"), "baz:", api.mkSmallInt(3));
    try std.testing.expect(e2 == e);
}

test "F7: isError partitions errors from everything else" {
    const api = setup();
    try std.testing.expect(!isErr(api.nilObj));
    try std.testing.expect(!isErr(api.mkSmallInt(0)));
    try std.testing.expect(!isErr(api.trueObj));
    try std.testing.expect(!isErr(api.mkString("x", 1)));
    const e = api.send(api.nilObj, api.intern("foo"), &[_]Oop{}, 0);
    try std.testing.expect(isErr(e));
}

test "F7: division by zero is a #zeroDivide error (no trap)" {
    const api = setup();
    const e = s1(api.mkSmallInt(10), "/", api.mkSmallInt(0));
    try std.testing.expect(isErr(e));
    try std.testing.expect(s0(e, "kind") == api.intern("zeroDivide"));
    const m = s1(api.mkSmallInt(10), "mod:", api.mkSmallInt(0));
    try std.testing.expect(isErr(m));
    try std.testing.expect(s0(m, "kind") == api.intern("zeroDivide"));
    // A normal division still works.
    try std.testing.expectEqual(@as(i64, 25), api.smallIntValue(s1(api.mkSmallInt(100), "/", api.mkSmallInt(4))));
}
