//! Proof for F7 — the error model. Brings up the prelude (for zeroDivide via the
//! guarded `/`) and the error module (the type + the object_vt DNU swap).

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
fn s2(rcv: Oop, sel: [*:0]const u8, a: Oop, b: Oop) Oop {
    return gapi.send(rcv, gapi.intern(sel), &[_]Oop{ a, b }, 2);
}
fn isErr(o: Oop) bool {
    return s0(o, "isError") == gapi.trueObj;
}
fn strEq(o: Oop, want: []const u8) bool {
    var len: usize = 0;
    const p = gapi.stringBytes(o, &len);
    return std.mem.eql(u8, p[0..len], want);
}

// A 0-arg block whose `value` sends an unknown selector to nil → a DNU error.
fn blkNilSend(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return gapi.send(gapi.nilObj, gapi.intern("frobnicate"), &[_]Oop{}, 0);
}
// A 0-arg block whose `value` is a plain value (no error).
fn blkOk(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return gapi.mkSmallInt(99);
}
// A handler block: value: → a sentinel SmallInt, ignoring the error.
fn handler7(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return gapi.mkSmallInt(7);
}
fn block(api: *idapi.IdApi, sel: [*:0]const u8, impl: idapi.Method) Oop {
    const vt = api.proto(api.object_vt);
    api.method(vt, api.intern(sel), impl);
    return api.alloc(vt, 0);
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

test "F7: on:do: catches a matching kind, passes a value through" {
    const api = setup();
    // Protected block errors → handler runs, returns 7.
    const caught = s2(block(api, "value", &blkNilSend), "on:do:", api.intern("doesNotUnderstand"), block(api, "value:", &handler7));
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(caught));
    // No error → the block's value flows through untouched.
    const passed = s2(block(api, "value", &blkOk), "on:do:", api.intern("doesNotUnderstand"), block(api, "value:", &handler7));
    try std.testing.expectEqual(@as(i64, 99), api.smallIntValue(passed));
}

test "F7: on:do: leaves an unmatched kind propagating; #error is the wildcard" {
    const api = setup();
    // Wrong kind → not caught; the original error passes through.
    const uncaught = s2(block(api, "value", &blkNilSend), "on:do:", api.intern("someOtherKind"), block(api, "value:", &handler7));
    try std.testing.expect(isErr(uncaught));
    // #error wildcard catches anything.
    const wild = s2(block(api, "value", &blkNilSend), "on:do:", api.intern("error"), block(api, "value:", &handler7));
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(wild));
}

test "F7: ifError: / ifNotError: dispatch on whether the receiver is an error" {
    const api = setup();
    const e = api.send(api.nilObj, api.intern("foo"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(s1(e, "ifError:", block(api, "value:", &handler7))));
    try std.testing.expect(s1(e, "ifNotError:", block(api, "value:", &handler7)) == e);
    // On a plain value the roles swap.
    const v = api.mkSmallInt(5);
    try std.testing.expect(s1(v, "ifError:", block(api, "value:", &handler7)) == v);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(s1(v, "ifNotError:", block(api, "value:", &handler7))));
}

test "F7: raising from a message string" {
    const api = setup();
    const e = s1(api.mkString("boom", 4), "error:", api.intern("myKind"));
    try std.testing.expect(isErr(e));
    try std.testing.expect(s0(e, "kind") == api.intern("myKind"));
    try std.testing.expect(strEq(s0(e, "message"), "boom"));
    // bare `error` uses the #error kind.
    try std.testing.expect(s0(api.mkString("oops", 4), "error") != api.nilObj);
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

test "F7: printString renders kind and message" {
    const api = setup();
    const e = api.send(api.nilObj, api.intern("frob"), &[_]Oop{}, 0);
    try std.testing.expect(strEq(s0(e, "printString"), "error(#doesNotUnderstand: does not understand #frob)"));
}
