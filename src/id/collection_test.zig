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
fn block(api: *idapi.IdApi, sel: [*:0]const u8, impl: idapi.Method) Oop {
    const vt = api.proto(api.object_vt);
    api.method(vt, api.intern(sel), impl);
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

// -- the HAMT-backed Map -----------------------------------------------------

fn mapNew(api: *idapi.IdApi) Oop {
    return s0(api.import("Map"), "new");
}
// A 0-arg block whose value is a fixed SmallInt — the at:ifAbsent: default.
fn seed42(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return gapi.mkSmallInt(42);
}

test "collection: Map at:put: / at: with SmallInt keys; at:put: is persistent" {
    const api = setup();
    const m0 = mapNew(api);
    try std.testing.expectEqual(@as(i64, 0), ival(s0(m0, "size")));
    const m1 = s2(m0, "at:put:", api.mkSmallInt(1), api.mkSmallInt(10));
    const m2 = s2(m1, "at:put:", api.mkSmallInt(2), api.mkSmallInt(20));
    // Originals are untouched (persistent).
    try std.testing.expectEqual(@as(i64, 0), ival(s0(m0, "size")));
    try std.testing.expectEqual(@as(i64, 1), ival(s0(m1, "size")));
    try std.testing.expectEqual(@as(i64, 2), ival(s0(m2, "size")));
    try std.testing.expectEqual(@as(i64, 10), ival(s1(m2, "at:", api.mkSmallInt(1))));
    try std.testing.expectEqual(@as(i64, 20), ival(s1(m2, "at:", api.mkSmallInt(2))));
    try std.testing.expect(s1(m2, "at:", api.mkSmallInt(9)) == api.nilObj); // absent → nil
}

test "collection: Map String keys compare by VALUE (equal literals dedupe)" {
    const api = setup();
    var m = mapNew(api);
    m = s2(m, "at:put:", api.mkString("hi", 2), api.mkSmallInt(1));
    // A DISTINCT String object with equal bytes updates the SAME entry.
    m = s2(m, "at:put:", api.mkString("hi", 2), api.mkSmallInt(2));
    try std.testing.expectEqual(@as(i64, 1), ival(s0(m, "size")));
    try std.testing.expectEqual(@as(i64, 2), ival(s1(m, "at:", api.mkString("hi", 2))));
    try std.testing.expect(s1(m, "includesKey:", api.mkString("hi", 2)) == api.trueObj);
    try std.testing.expect(s1(m, "includesKey:", api.mkString("bye", 3)) == api.falseObj);
}

test "collection: Map at:ifAbsent: runs the block only when absent" {
    const api = setup();
    var m = mapNew(api);
    m = s2(m, "at:put:", api.mkSmallInt(1), api.mkSmallInt(10));
    // present → the stored value, block ignored
    try std.testing.expectEqual(@as(i64, 10), ival(s2(m, "at:ifAbsent:", api.mkSmallInt(1), block(api, "value", &seed42))));
    // absent → the block's value
    try std.testing.expectEqual(@as(i64, 42), ival(s2(m, "at:ifAbsent:", api.mkSmallInt(9), block(api, "value", &seed42))));
}

test "collection: Map keys / values; the HAMT survives many keys (subnodes/collisions)" {
    const api = setup();
    var m = mapNew(api);
    // 200 entries forces the bitmap nodes to spill into subnodes at depth.
    var i: i64 = 0;
    while (i < 200) : (i += 1) m = s2(m, "at:put:", api.mkSmallInt(i), api.mkSmallInt(i * i));
    try std.testing.expectEqual(@as(i64, 200), ival(s0(m, "size")));
    i = 0;
    while (i < 200) : (i += 1) {
        try std.testing.expectEqual(i * i, ival(s1(m, "at:", api.mkSmallInt(i))));
    }
    // keys + values are vectors of the right length.
    try std.testing.expectEqual(@as(i64, 200), ival(s0(s0(m, "keys"), "size")));
    try std.testing.expectEqual(@as(i64, 200), ival(s0(s0(m, "values"), "size")));
}

var kv_key_sum: i64 = 0;
var kv_val_sum: i64 = 0;
fn kvAccum(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    kv_key_sum += gapi.smallIntValue(a[0]);
    kv_val_sum += gapi.smallIntValue(a[1]);
    return gapi.nilObj;
}

test "collection: Map keysAndValuesDo: visits every entry once" {
    const api = setup();
    var m = mapNew(api);
    m = s2(m, "at:put:", api.mkSmallInt(1), api.mkSmallInt(100));
    m = s2(m, "at:put:", api.mkSmallInt(2), api.mkSmallInt(200));
    m = s2(m, "at:put:", api.mkSmallInt(3), api.mkSmallInt(300));
    kv_key_sum = 0;
    kv_val_sum = 0;
    _ = s1(m, "keysAndValuesDo:", block2(api, &kvAccum));
    try std.testing.expectEqual(@as(i64, 6), kv_key_sum); // 1+2+3
    try std.testing.expectEqual(@as(i64, 600), kv_val_sum); // 100+200+300
}

test "collection: Map printString renders key: value pairs" {
    const api = setup();
    var m = mapNew(api);
    m = s2(m, "at:put:", api.mkString("a", 1), api.mkSmallInt(1));
    const s = s0(m, "printString");
    // Single entry → deterministic.
    try std.testing.expect(strEq(s, "{a: 1}"));
}

// Sort-key blocks for sortBy: (Zig stand-ins): identity (ascending) and negate
// (descending).
fn identityBlk(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return a[0];
}
fn negateBlk(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return gapi.mkSmallInt(-gapi.smallIntValue(a[0]));
}

fn vecNew(api: *idapi.IdApi) Oop {
    return s0(api.import("Vector"), "new");
}

test "collection: String asLowercase / replace:with: / words" {
    const api = setup();

    try std.testing.expect(strEq(s0(api.mkString("HeLLo", 5), "asLowercase"), "hello"));
    // replace: strips both punctuation marks; chained like word-frequency does.
    const cleaned = s2(api.mkString("a.b,c.", 6), "replace:with:", api.mkString(".", 1), api.mkString("", 0));
    try std.testing.expect(strEq(cleaned, "ab,c"));
    // empty needle is a no-op (no infinite loop).
    try std.testing.expect(strEq(s2(api.mkString("xy", 2), "replace:with:", api.mkString("", 0), api.mkString("Z", 1)), "xy"));

    // words splits on runs of whitespace, dropping empties.
    const w = s0(api.mkString("  the  cat sat\n", 15), "words");
    try std.testing.expectEqual(@as(i64, 3), ival(s0(w, "size")));
    try std.testing.expect(strEq(s1(w, "at:", api.mkSmallInt(0)), "the"));
    try std.testing.expect(strEq(s1(w, "at:", api.mkSmallInt(2)), "sat"));
    try std.testing.expectEqual(@as(i64, 0), ival(s0(s0(api.mkString("   ", 3), "words"), "size")));
}

test "collection: String asSymbol interns (equal strings → identical symbol)" {
    const api = setup();
    const a = s0(api.mkString("apple", 5), "asSymbol");
    const b = s0(api.mkString("apple", 5), "asSymbol");
    const c = s0(api.mkString("banana", 6), "asSymbol");
    try std.testing.expect(a == b); // interned: same identity
    try std.testing.expect(a != c);
    try std.testing.expect(a == api.intern("apple")); // same as direct intern
    // a symbol key dedupes in the identity-keyed map across distinct string objs
    var m = mapNew(api);
    m = s2(m, "at:put:", a, api.mkSmallInt(1));
    m = s2(m, "at:put:", b, api.mkSmallInt(9));
    try std.testing.expectEqual(@as(i64, 1), ival(s0(m, "size")));
    try std.testing.expectEqual(@as(i64, 9), ival(s1(m, "at:", a)));
}

test "collection: Vector sortBy: orders by key (ascending and descending)" {
    const api = setup();
    var v = vecNew(api);
    v = s1(v, "add:", api.mkSmallInt(3));
    v = s1(v, "add:", api.mkSmallInt(1));
    v = s1(v, "add:", api.mkSmallInt(2));

    const asc = s1(v, "sortBy:", block1(api, &identityBlk));
    try std.testing.expectEqual(@as(i64, 1), ival(s1(asc, "at:", api.mkSmallInt(0))));
    try std.testing.expectEqual(@as(i64, 2), ival(s1(asc, "at:", api.mkSmallInt(1))));
    try std.testing.expectEqual(@as(i64, 3), ival(s1(asc, "at:", api.mkSmallInt(2))));

    const desc = s1(v, "sortBy:", block1(api, &negateBlk));
    try std.testing.expectEqual(@as(i64, 3), ival(s1(desc, "at:", api.mkSmallInt(0))));
    try std.testing.expectEqual(@as(i64, 1), ival(s1(desc, "at:", api.mkSmallInt(2))));

    // original is unchanged (persistent).
    try std.testing.expectEqual(@as(i64, 3), ival(s1(v, "at:", api.mkSmallInt(0))));
}
