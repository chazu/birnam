//! Proof for the id-native collection library: Range + the persistent Vector
//! trie, driven through the object model. Brings up the prelude too (Vector
//! `printString` sends `printString` to each element → SmallInt's prelude impl).

const std = @import("std");
const id = @import("id.zig");
const prelude = @import("prelude.zig");
const collection = @import("collection.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

var gapi: *idapi.IdApi = undefined;

fn setup() *idapi.IdApi {
    const api = id.id_init();
    gapi = api;
    prelude.bringUp(api);
    collection.bringUp(api);
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
fn ival(o: Oop) i64 {
    return gapi.smallIntValue(o);
}
fn strEq(o: Oop, want: []const u8) bool {
    var len: usize = 0;
    const p = gapi.stringBytes(o, &len);
    return std.mem.eql(u8, p[0..len], want);
}

// Stand-in blocks: plain objects whose value:/value:value: run a Zig predicate.
fn evenPred(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return if (@mod(gapi.smallIntValue(a[0]), 2) == 0) gapi.trueObj else gapi.falseObj;
}
fn doubleBlk(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return gapi.mkSmallInt(gapi.smallIntValue(a[0]) * 2);
}
fn falseBlk(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return gapi.falseObj;
}
fn sumBlk(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return gapi.mkSmallInt(gapi.smallIntValue(a[0]) + gapi.smallIntValue(a[1]));
}
fn block1(api: *idapi.IdApi, impl: idapi.Method) Oop {
    const vt = api.proto(api.object_vt);
    api.method(vt, api.intern("value:"), impl);
    return api.alloc(vt, 0);
}
fn block2(api: *idapi.IdApi, impl: idapi.Method) Oop {
    const vt = api.proto(api.object_vt);
    api.method(vt, api.intern("value:value:"), impl);
    return api.alloc(vt, 0);
}

test "collection: Range size / includes: / asVector" {
    const api = setup();
    const r = s1(api.mkSmallInt(2), "to:", api.mkSmallInt(10));
    try std.testing.expectEqual(@as(i64, 9), ival(s0(r, "size")));
    try std.testing.expect(s1(r, "includes:", api.mkSmallInt(5)) == api.trueObj);
    try std.testing.expect(s1(r, "includes:", api.mkSmallInt(11)) == api.falseObj);

    const v = s0(r, "asVector");
    try std.testing.expectEqual(@as(i64, 9), ival(s0(v, "size")));
    try std.testing.expectEqual(@as(i64, 2), ival(s1(v, "at:", api.mkSmallInt(0))));
    try std.testing.expectEqual(@as(i64, 10), ival(s1(v, "at:", api.mkSmallInt(8))));
}

test "collection: empty range is empty; start>end" {
    const api = setup();
    const r = s1(api.mkSmallInt(2), "to:", api.mkSmallInt(1));
    try std.testing.expectEqual(@as(i64, 0), ival(s0(r, "size")));
    try std.testing.expect(s0(s0(r, "asVector"), "isEmpty") == api.trueObj);
}

test "collection: select: / collect: / detect: over a Range" {
    const api = setup();
    const r = s1(api.mkSmallInt(1), "to:", api.mkSmallInt(10));
    const evens = s1(r, "select:", block1(api, &evenPred));
    try std.testing.expectEqual(@as(i64, 5), ival(s0(evens, "size")));
    try std.testing.expect(strEq(s0(evens, "printString"), "#[2 4 6 8 10]"));

    const doubled = s1(r, "collect:", block1(api, &doubleBlk));
    try std.testing.expectEqual(@as(i64, 20), ival(s1(doubled, "at:", api.mkSmallInt(9))));

    // detect: answers the first match…
    const firstEven = s1(r, "detect:", block1(api, &evenPred));
    try std.testing.expectEqual(@as(i64, 2), ival(firstEven));
    // …or nil when none — and nil knows it is nil.
    const none = s1(r, "detect:", block1(api, &falseBlk));
    try std.testing.expect(none == api.nilObj);
    try std.testing.expect(s0(none, "isNil") == api.trueObj);
}

test "collection: isNil / notNil split between nil and everything else" {
    const api = setup();
    try std.testing.expect(s0(api.nilObj, "isNil") == api.trueObj);
    try std.testing.expect(s0(api.nilObj, "notNil") == api.falseObj);
    try std.testing.expect(s0(api.mkSmallInt(0), "isNil") == api.falseObj);
    try std.testing.expect(s0(api.mkSmallInt(0), "notNil") == api.trueObj);
}

test "collection: inject:into: folds" {
    const api = setup();
    const r = s1(api.mkSmallInt(1), "to:", api.mkSmallInt(5));
    const sum = s2(r, "inject:into:", api.mkSmallInt(0), block2(api, &sumBlk));
    try std.testing.expectEqual(@as(i64, 15), ival(sum)); // 1+2+3+4+5
}

test "collection: Vector add: is persistent (structural sharing, old unchanged)" {
    const api = setup();
    const empty = s0(s1(api.mkSmallInt(1), "to:", api.mkSmallInt(0)), "asVector"); // size 0
    try std.testing.expectEqual(@as(i64, 0), ival(s0(empty, "size")));
    const v1 = s1(empty, "add:", api.mkSmallInt(7));
    const v2 = s1(v1, "add:", api.mkSmallInt(8));
    // The originals are unchanged — add: returns a new vector each time.
    try std.testing.expectEqual(@as(i64, 0), ival(s0(empty, "size")));
    try std.testing.expectEqual(@as(i64, 1), ival(s0(v1, "size")));
    try std.testing.expectEqual(@as(i64, 2), ival(s0(v2, "size")));
    try std.testing.expectEqual(@as(i64, 7), ival(s1(v2, "at:", api.mkSmallInt(0))));
    try std.testing.expectEqual(@as(i64, 8), ival(s1(v2, "at:", api.mkSmallInt(1))));
}

test "collection: the trie overflows the tail correctly (>32 elements)" {
    const api = setup();
    // 1..100 forces several tail-flushes + a root-overflow, exercising pushTail
    // and newPath — not just the tail fast-path.
    const v = s0(s1(api.mkSmallInt(1), "to:", api.mkSmallInt(100)), "asVector");
    try std.testing.expectEqual(@as(i64, 100), ival(s0(v, "size")));
    var i: i64 = 0;
    while (i < 100) : (i += 1) {
        try std.testing.expectEqual(i + 1, ival(s1(v, "at:", api.mkSmallInt(i))));
    }
}

test "collection: String concatenation builds a fresh String" {
    const api = setup();
    const hello = api.mkString("hello, ", 7);
    const world = api.mkString("world", 5);
    try std.testing.expect(strEq(s1(hello, ",", world), "hello, world"));
}

test "collection: Vector , Vector concatenates" {
    const api = setup();
    const a = s0(s1(api.mkSmallInt(1), "to:", api.mkSmallInt(3)), "asVector"); // #[1 2 3]
    const b = s0(s1(api.mkSmallInt(4), "to:", api.mkSmallInt(5)), "asVector"); // #[4 5]
    const c = s1(a, ",", b);
    try std.testing.expectEqual(@as(i64, 5), ival(s0(c, "size")));
    try std.testing.expect(strEq(s0(c, "printString"), "#[1 2 3 4 5]"));
}
