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
    // Reset the GC stack bottom each test: the GC-stress tests below set it to a
    // local frame that goes stale once they return. Auto-GC is on by default, and
    // (now that Integers are boxed) allocation pressure can trip it mid-test — with
    // a stale bottom that would scan no roots and sweep live objects. 0 = "unset",
    // so auto-GC stays inert until a test that wants it sets its own valid bottom.
    api.gcSetStackBottom(0);
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

/// `#[from … to]` via `add:` sends on the exported empty Vector prototype —
/// the bare-floor construction idiom (Range `asVector` is stdlib.birnam now,
/// the tier-B lift, so it does not exist at this test's rung). from > to → empty.
fn rangeVec(from: i64, to: i64) Oop {
    var v = gapi.import("Vector");
    var i = from;
    while (i <= to) : (i += 1) v = s1(v, "add:", gapi.mkSmallInt(i));
    return v;
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

test "collection: Range size / includes:" {
    const api = setup();
    const r = s1(api.mkSmallInt(2), "to:", api.mkSmallInt(10));
    try std.testing.expectEqual(@as(i64, 9), ival(s0(r, "size")));
    try std.testing.expect(s1(r, "includes:", api.mkSmallInt(5)) == api.trueObj);
    try std.testing.expect(s1(r, "includes:", api.mkSmallInt(11)) == api.falseObj);
    // asVector is Collection's (stdlib.birnam, the tier-B lift) — asserted
    // through the full stack in stdlib-test.birnam.
    try std.testing.expectEqual(@as(i64, 2), ival(s1(r, "at:", api.mkSmallInt(0))));
    try std.testing.expectEqual(@as(i64, 10), ival(s1(r, "at:", api.mkSmallInt(8))));
}

test "collection: empty range is empty; start>end" {
    const api = setup();
    const r = s1(api.mkSmallInt(2), "to:", api.mkSmallInt(1));
    try std.testing.expectEqual(@as(i64, 0), ival(s0(r, "size")));
    // isEmpty itself lives in stdlib.birnam now (the tier-A surface lift, over
    // size) — asserted through the full stack in stdlib-test.birnam.
}

// select:/collect:/detect: moved off the native collection vtables onto the shared
// `Collection` parent in stdlib.birnam, defined ONCE over the native fold primitive
// `inject:into:` (docs/design/41 §B). The fold basis is tested below ("inject:into:
// folds") and the unified higher-order protocol is integration-tested through the
// full stdlib pipeline in codegen_test ("the Collection protocol unifies…").

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
    const empty = rangeVec(1, 0); // size 0
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
    const v = rangeVec(1, 100);
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
    _ = setup();
    const a = rangeVec(1, 3); // #[1 2 3]
    const b = rangeVec(4, 5); // #[4 5]
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

test "collection: Map Float keys compare by VALUE (equal boxes dedupe, -0.0 is 0.0)" {
    const api = setup();
    var m = mapNew(api);
    m = s2(m, "at:put:", api.mkFloat(1.5), api.mkSmallInt(1));
    // A DISTINCT Float box with an equal value updates the SAME entry.
    m = s2(m, "at:put:", api.mkFloat(1.5), api.mkSmallInt(2));
    try std.testing.expectEqual(@as(i64, 1), ival(s0(m, "size")));
    try std.testing.expectEqual(@as(i64, 2), ival(s1(m, "at:", api.mkFloat(1.5))));
    try std.testing.expect(s1(m, "includesKey:", api.mkFloat(1.5)) == api.trueObj);
    try std.testing.expect(s1(m, "includesKey:", api.mkFloat(2.5)) == api.falseObj);
    // -0.0 == 0.0 under IEEE, so they must be ONE key (the hash normalizes).
    m = s2(m, "at:put:", api.mkFloat(0.0), api.mkSmallInt(3));
    m = s2(m, "at:put:", api.mkFloat(-0.0), api.mkSmallInt(4));
    try std.testing.expectEqual(@as(i64, 2), ival(s0(m, "size")));
    try std.testing.expectEqual(@as(i64, 4), ival(s1(m, "at:", api.mkFloat(0.0))));
    // Per-type: the Float 1.0 and the Integer 1 are DIFFERENT keys.
    m = s2(m, "at:put:", api.mkFloat(1.0), api.mkSmallInt(5));
    m = s2(m, "at:put:", api.mkSmallInt(1), api.mkSmallInt(6));
    try std.testing.expectEqual(@as(i64, 5), ival(s1(m, "at:", api.mkFloat(1.0))));
    try std.testing.expectEqual(@as(i64, 6), ival(s1(m, "at:", api.mkSmallInt(1))));
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

test "collection: Map printString renders key -> value pairs" {
    const api = setup();
    var m = mapNew(api);
    m = s2(m, "at:put:", api.mkString("a", 1), api.mkSmallInt(1));
    const s = s0(m, "printString");
    // Single entry → deterministic.
    try std.testing.expect(strEq(s, "{a -> 1}"));
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

test "collection: String replace:with: (the hot native rewriter)" {
    const api = setup();

    // asLowercase/asUppercase are self-hosted over StringBuilder now (stdlib.birnam,
    // pinned in modules/stdlib-test.birnam); replace:with: stays native — json/html
    // escaping runs it on every LSP message and docgen string.
    // replace: strips both punctuation marks; chained like word-frequency does.
    const cleaned = s2(api.mkString("a.b,c.", 6), "replace:with:", api.mkString(".", 1), api.mkString("", 0));
    try std.testing.expect(strEq(cleaned, "ab,c"));
    // empty needle is a no-op (no infinite loop).
    try std.testing.expect(strEq(s2(api.mkString("xy", 2), "replace:with:", api.mkString("", 0), api.mkString("Z", 1)), "xy"));
    // words (and the rest of the derived string protocol) is self-hosted in
    // stdlib.birnam — covered by modules/stdlib-test.birnam, not at this floor.
}

test "collection: String , coerces a non-String argument via printString" {
    const api = setup();
    // String , String — the plain concat.
    try std.testing.expect(strEq(s1(api.mkString("ab", 2), ",", api.mkString("cd", 2)), "abcd"));
    // String , SmallInt — coerced through printString instead of untagging as bytes
    // (which previously read a garbage ptr/len and crashed).
    try std.testing.expect(strEq(s1(api.mkString("n=", 2), ",", api.mkSmallInt(5)), "n=5"));
    // String , Boolean — also coerced (Boolean now answers printString).
    const t = api.send(api.mkSmallInt(1), api.intern("="), &[_]Oop{api.mkSmallInt(1)}, 1);
    try std.testing.expect(strEq(s1(api.mkString("ok? ", 4), ",", t), "ok? true"));
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

// -- F3 exact layouts (docs/40 §3c): precise trace vs the conservative scan ----

/// Allocate `count` throwaway objects and answer a Vector of Ranges whose raw
/// i64 endpoints ALIAS their addresses. After this returns, the objects are
/// garbage — but each address lives on inside a live Range's raw payload word,
/// which is exactly the false positive conservative tracing cannot see past.
noinline fn makeAliasedGarbage(api: *idapi.IdApi, count: usize) Oop {
    var v = s0(api.import("Vector"), "new");
    var i: usize = 0;
    while (i < count) : (i += 1) {
        const garbage = api.alloc(api.object_vt, 64);
        const addr: i64 = @bitCast(@intFromPtr(garbage.?));
        const rng = s1(api.mkSmallInt(addr), "to:", api.mkSmallInt(addr));
        v = s1(v, "add:", rng);
    }
    return v;
}

/// Overwrite a stretch of stack so dead objects' addresses from an earlier
/// frame stop being (conservative) stack roots.
noinline fn launderStack() void {
    var buf: [512]usize = undefined;
    for (&buf, 0..) |*w, i| w.* = i;
    std.mem.doNotOptimizeAway(&buf);
}

test "F3 layouts: precise trace frees garbage that a raw word aliases" {
    const api = setup();
    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    _ = &anchor;
    defer api.gcSetPrecise(0);

    const v = makeAliasedGarbage(api, 60);
    launderStack();

    // Conservative collection: the garbage objects stay retained — each address
    // sits in a live Range's payload and the scan must assume it is an Oop.
    _ = api.gcCollect();
    const live_conservative = api.gcLiveCount();

    // Precise: Range's layout declares both words raw, so the aliases stop
    // rooting. The garbage (allocated nothing since) is reclaimed NOW — proof
    // the conservative pass had been over-retaining it.
    api.gcSetPrecise(1);
    const reclaimed = api.gcCollect();
    try std.testing.expect(reclaimed >= 40);
    try std.testing.expectEqual(live_conservative - reclaimed, api.gcLiveCount());

    // And precision freed only true garbage: the structure is fully intact.
    try std.testing.expectEqual(@as(i64, 60), ival(s0(v, "size")));
}

noinline fn churn(api: *idapi.IdApi, count: usize) void {
    var i: usize = 0;
    var last: Oop = null;
    while (i < count) : (i += 1) {
        last = api.mkString("throwaway garbage string", 24);
    }
    std.mem.doNotOptimizeAway(&last);
}

test "F3 layouts: a mixed live structure survives precise collection under churn" {
    const api = setup();
    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    _ = &anchor;
    defer api.gcSetPrecise(0);
    api.gcSetPrecise(1);

    // Vector of strings, a map keyed by symbol AND string, a range — every
    // layout-bearing kind at once, reachable only through each other.
    var v = s0(api.import("Vector"), "new");
    v = s1(v, "add:", api.mkString("alpha", 5));
    v = s1(v, "add:", api.mkString("beta", 4));
    var m = s0(api.import("Map"), "new");
    m = s2(m, "at:put:", api.intern("vec"), v);
    m = s2(m, "at:put:", api.mkString("range", 5), s1(api.mkSmallInt(1), "to:", api.mkSmallInt(9)));

    churn(api, 20_000);
    _ = api.gcCollect();

    const v2 = s1(m, "at:", api.intern("vec"));
    try std.testing.expectEqual(@as(i64, 2), ival(s0(v2, "size")));
    try std.testing.expect(strEq(s1(v2, "at:", api.mkSmallInt(0)), "alpha"));
    try std.testing.expect(strEq(s1(v2, "at:", api.mkSmallInt(1)), "beta"));
    const r = s1(m, "at:", api.mkString("range", 5));
    try std.testing.expectEqual(@as(i64, 9), ival(s0(r, "size")));
}
