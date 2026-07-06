//! Proof for the FFI: wrap C library functions as birnam methods by generating C
//! shims and compiling them with Tcc. Uses libc functions (resolvable in the
//! process, no external lib needed) to exercise int / string-in / string-out, and
//! `export fn`s DEFINED IN THIS FILE (also dlsym-resolvable — the test binary's
//! exports are dynamic symbols) to exercise struct-by-value: args, returns, HFA
//! doubles/floats, mixed int/float, nested structs, array fields, and the >16-byte
//! sret return path. Structs shuttle through positional Vectors, so struct tests
//! bring up the collection module; graceful-failure tests bring up the error
//! module (a DNU then yields an Error value instead of a panic).
//!
//! ORDER MATTERS for the first test: id state is process-global across tests, so
//! the "struct sig without collection" test must run before anything exports
//! `Vector`. Keep it first in this file.

const std = @import("std");
const id = @import("id.zig");
const tcc = @import("tcc.zig");
const alien = @import("alien.zig");
const collection = @import("collection.zig");
const err_mod = @import("error.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

/// The floor + Tcc + Alien only — NO collection (see the first test).
fn setupBare() *idapi.IdApi {
    const api = id.id_init();
    tcc.bringUp(api);
    alien.bringUp(api);
    return api;
}

/// The full FFI stack: floor + Tcc + collection (Vectors for struct marshal) +
/// error (DNU → Error value) + Alien, in the real load order.
fn setup() *idapi.IdApi {
    const api = id.id_init();
    tcc.bringUp(api);
    collection.bringUp(api);
    err_mod.bringUp(api);
    alien.bringUp(api);
    return api;
}

fn str(api: *idapi.IdApi, s: []const u8) Oop {
    return api.mkString(s.ptr, s.len);
}

fn wrap(api: *idapi.IdApi, fn_name: []const u8, sig: []const u8, sel: [*:0]const u8, vt: Oop) Oop {
    const Alien = api.import("Alien").?;
    return api.send(Alien, api.intern("wrap:signature:as:on:"), &[_]Oop{ str(api, fn_name), str(api, sig), api.intern(sel), vt }, 4);
}

/// Build a `#[…]` Vector from elems via the factory (`new` + persistent `add:`).
fn vec(api: *idapi.IdApi, elems: []const Oop) Oop {
    var v = api.send(api.import("Vector").?, api.intern("new"), &[_]Oop{}, 0);
    for (elems) |e| v = api.send(v, api.intern("add:"), &[_]Oop{e}, 1);
    return v;
}

fn vAt(api: *idapi.IdApi, v: Oop, i: i64) Oop {
    return api.send(v, api.intern("at:"), &[_]Oop{api.mkSmallInt(i)}, 1);
}
fn vSize(api: *idapi.IdApi, v: Oop) i64 {
    return api.smallIntValue(api.send(v, api.intern("size"), &[_]Oop{}, 0));
}
fn intAt(api: *idapi.IdApi, v: Oop, i: i64) i64 {
    return api.smallIntValue(vAt(api, v, i));
}
fn floatAt(api: *idapi.IdApi, v: Oop, i: i64) f64 {
    return api.floatValue(vAt(api, v, i));
}
fn expectVector(api: *idapi.IdApi, v: Oop) !void {
    try std.testing.expect(api.vtableOf(v) == api.import("VectorVt").?);
}

// -- the struct-by-value C test functions --------------------------------------
// `export fn` puts these in the test binary's dynamic symbol table, so the normal
// wrap path (dlsym RTLD_DEFAULT) resolves them like any libc function.

const CPoint = extern struct { x: c_long, y: c_long };
export fn birnam_t_sumPoint(p: CPoint) c_long {
    return p.x + p.y;
}
export fn birnam_t_makePoint(x: c_long, y: c_long) CPoint {
    return .{ .x = x, .y = y };
}
export fn birnam_t_movePoint(p: CPoint, dx: c_long, dy: c_long) CPoint {
    return .{ .x = p.x + dx, .y = p.y + dy };
}
export fn birnam_t_addPoints(a: CPoint, b: CPoint) CPoint {
    return .{ .x = a.x + b.x, .y = a.y + b.y };
}

// Pointer round-trip: a bit-63-set address that a 63-bit SmallInt cannot hold.
// Never dereferenced — only its bits are checked, so a bogus value is safe.
export fn birnam_t_highPtr() ?*anyopaque {
    return @ptrFromInt(0xFFFF_0000_1234_5678);
}
export fn birnam_t_ptrIdentity(p: ?*anyopaque) ?*anyopaque {
    return p;
}

const CVec2 = extern struct { x: f64, y: f64 }; // an HFA of doubles on arm64
export fn birnam_t_dotVec2(a: CVec2, b: CVec2) f64 {
    return a.x * b.x + a.y * b.y;
}
export fn birnam_t_scaleVec2(v: CVec2, k: f64) CVec2 {
    return .{ .x = v.x * k, .y = v.y * k };
}

const CF2 = extern struct { a: f32, b: f32 }; // an HFA of floats
export fn birnam_t_sumF2(v: CF2) f32 {
    return v.a + v.b;
}

const CMixed = extern struct { tag: c_int, val: f64 }; // mixed int/float (not an HFA)
export fn birnam_t_mixedVal(m: CMixed) f64 {
    return @as(f64, @floatFromInt(m.tag)) + m.val;
}
export fn birnam_t_mkMixed(t: c_int, v: f64) CMixed {
    return .{ .tag = t, .val = v };
}

const CLine = extern struct { a: CPoint, b: CPoint }; // struct-of-structs
export fn birnam_t_sumLine(l: CLine) c_long {
    return l.a.x + l.a.y + l.b.x + l.b.y;
}
export fn birnam_t_flipLine(l: CLine) CLine {
    return .{ .a = l.b, .b = l.a };
}

const CArr = extern struct { base: c_long, v: [3]c_int }; // fixed scalar array field
export fn birnam_t_sumArr(a: CArr) c_long {
    return a.base + a.v[0] + a.v[1] + a.v[2];
}
export fn birnam_t_iota(base: c_int) CArr {
    return .{ .base = base, .v = .{ base + 1, base + 2, base + 3 } };
}

const CBig = extern struct { a: c_long, b: c_long, c: c_long, d: c_long, e: c_long }; // 40 bytes → sret return
export fn birnam_t_mkBig(x: c_long) CBig {
    return .{ .a = x, .b = x + 1, .c = x + 2, .d = x + 3, .e = x + 4 };
}
export fn birnam_t_sumBig(v: CBig) c_long {
    return v.a + v.b + v.c + v.d + v.e;
}

// -- callback harness: C functions that INVOKE a callback ------------------------

export fn birnam_t_apply(f: *const fn (c_long) callconv(.c) c_long, x: c_long) c_long {
    return f(x);
}
export fn birnam_t_apply2(f: *const fn (c_long, c_long) callconv(.c) c_long, x: c_long, y: c_long) c_long {
    return f(x, y);
}
export fn birnam_t_applyD(f: *const fn (f64) callconv(.c) f64, x: f64) f64 {
    return f(x);
}
export fn birnam_t_fire(f: *const fn () callconv(.c) void) c_long {
    f();
    return 1;
}
export fn birnam_t_applyS(f: *const fn ([*:0]const u8) callconv(.c) c_long, s: [*:0]const u8) c_long {
    return f(s);
}
export fn birnam_t_applyPt(f: *const fn (CPoint) callconv(.c) c_long) c_long {
    return f(.{ .x = 20, .y = 22 }); // struct INTO the callback, by value
}
export fn birnam_t_viaPtCb(f: *const fn (c_long) callconv(.c) CPoint, x: c_long) c_long {
    const p = f(x); // struct RETURNED from the callback, by value
    return p.x * 100 + p.y;
}

// -- callback responders ----------------------------------------------------------
// A callback target is ANYTHING answering the arity-matched `value…` selector —
// tests use hand-built responder objects (a compiled birnam block is exercised in
// codegen_test's integration test and examples/ffi.birnam).

var gapi: *idapi.IdApi = undefined;
var g_fired: bool = false;

fn responder(api: *idapi.IdApi, sel: [*:0]const u8, f: idapi.Method) Oop {
    gapi = api;
    const vt = api.proto(api.object_vt);
    api.method(vt, api.intern(sel), f);
    return api.alloc(vt, 0);
}

fn rDouble(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return gapi.mkSmallInt(gapi.smallIntValue(a[0]) * 2);
}
fn rSum(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return gapi.mkSmallInt(gapi.smallIntValue(a[0]) + gapi.smallIntValue(a[1]));
}
fn rHalf(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return gapi.mkFloat(gapi.floatValue(a[0]) / 2.0);
}
fn rFire(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    g_fired = true;
    return gapi.nilObj;
}
fn rStrLen(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    var len: usize = 0;
    _ = gapi.stringBytes(a[0], &len);
    return gapi.mkSmallInt(@intCast(len));
}
fn rVecSum(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    // the arg is a struct-shaped Vector: sum its two fields
    const x = gapi.smallIntValue(vAt(gapi, a[0], 0));
    const y = gapi.smallIntValue(vAt(gapi, a[0], 1));
    return gapi.mkSmallInt(x + y);
}
fn rMkPoint(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    // answer #[x+1 x+2] — comes back to C as CPoint{x+1, x+2}
    const x = gapi.smallIntValue(a[0]);
    return vec(gapi, &[_]Oop{ gapi.mkSmallInt(x + 1), gapi.mkSmallInt(x + 2) });
}

// -- tests ----------------------------------------------------------------------

test "alien: a struct signature without the collection module is rejected (nil)" {
    // MUST RUN FIRST: once any test brings up collection, `Vector` stays exported
    // for the rest of the process and this rejection can no longer be observed.
    const api = setupBare();
    const vt = api.proto(api.object_vt);
    const r = wrap(api, "birnam_t_sumPoint", "l {ll}", "pointSum", vt);
    try std.testing.expect(r == api.nilObj);
}

test "alien: wrap libc strlen — receiver (a String) is the C argument" {
    const api = setup();
    // Wrap onto the String vtable so a String receiver IS the const char* argument.
    const ok = wrap(api, "strlen", "l s", "clen", api.string_vt);
    try std.testing.expect(ok == api.string_vt);

    const r = api.send(str(api, "hello, world"), api.intern("clen"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 12), api.smallIntValue(r));
}

test "alien: wrap libc abs — receiver (an Integer) is the C argument" {
    const api = setup();
    _ = wrap(api, "abs", "i i", "absVia", api.smallint_vt);
    const r = api.send(api.mkSmallInt(-7), api.intern("absVia"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(r));
}

test "alien: wrap libc getenv (string receiver → string return; nil when absent)" {
    const api = setup();
    _ = wrap(api, "getenv", "s s", "getenvVia", api.string_vt);

    // PATH is always set → a non-empty String; an absent var returns nil.
    const r = api.send(str(api, "PATH"), api.intern("getenvVia"), &[_]Oop{}, 0);
    try std.testing.expect(api.vtableOf(r) == api.string_vt);
    var len: usize = 0;
    _ = api.stringBytes(r, &len);
    try std.testing.expect(len > 0);

    const miss = api.send(str(api, "BIRNAM_NO_SUCH_VAR_XYZ"), api.intern("getenvVia"), &[_]Oop{}, 0);
    try std.testing.expect(miss == api.nilObj);
}

test "alien: wrap libc atof — string receiver → double return (a Float)" {
    const api = setup();
    // double atof(const char*): "d s" — the String receiver IS the C argument,
    // the result comes back as a boxed Float.
    _ = wrap(api, "atof", "d s", "asCDouble", api.string_vt);
    const r = api.send(str(api, "3.25"), api.intern("asCDouble"), &[_]Oop{}, 0);
    try std.testing.expect(api.isFloat(r) != 0);
    try std.testing.expectEqual(@as(f64, 3.25), api.floatValue(r));
}

test "alien: wrap libc pow — double receiver + double arg + double return" {
    const api = setup();
    // double pow(double, double): "d d d" — receiver (a Float) is the base, the
    // one message arg is the exponent; the result is a Float.
    _ = wrap(api, "pow", "d d d", "raisedTo:", api.float_vt);
    const r = api.send(api.mkFloat(2.0), api.intern("raisedTo:"), &[_]Oop{api.mkFloat(10.0)}, 1);
    try std.testing.expect(api.isFloat(r) != 0);
    try std.testing.expectEqual(@as(f64, 1024.0), api.floatValue(r));
}

fn isErr(api: *idapi.IdApi, o: Oop) bool {
    return api.send(o, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj;
}
fn errKind(api: *idapi.IdApi, o: Oop) Oop {
    return api.send(o, api.intern("kind"), &[_]Oop{}, 0);
}

test "alien: an invalid signature is rejected with a #badSignature error (§C1)" {
    const api = setup();
    const vt = api.proto(api.object_vt);
    // 'q' is not a known type char → an honest kinded error, not a silent nil.
    const r1 = wrap(api, "strlen", "i q", "nope:", vt);
    try std.testing.expect(isErr(api, r1));
    try std.testing.expect(errKind(api, r1) == api.intern("badSignature"));
    // An empty signature is rejected too.
    try std.testing.expect(isErr(api, wrap(api, "strlen", "", "nope2:", vt)));
}

test "alien: FFI failures are DISTINGUISHABLE by kind (§C1)" {
    const api = setup();
    const vt = api.proto(api.object_vt);
    // A missing symbol and a malformed signature no longer collapse to the same nil.
    const missing = wrap(api, "no_such_c_function_xyzzy", "i s", "m:", vt);
    try std.testing.expect(errKind(api, missing) == api.intern("symbolNotFound"));
    const badsig = wrap(api, "strlen", "i zzz", "b:", vt);
    try std.testing.expect(errKind(api, badsig) == api.intern("badSignature"));
    try std.testing.expect(errKind(api, missing) != errKind(api, badsig));
}

// -- struct-by-value ------------------------------------------------------------

test "alien: struct arg — the receiver Vector marshals by value (sumPoint)" {
    const api = setup();
    const vvt = api.import("VectorVt").?;
    const ok = wrap(api, "birnam_t_sumPoint", "l {ll}", "pointSum", vvt);
    try std.testing.expect(ok == vvt);

    const p = vec(api, &[_]Oop{ api.mkSmallInt(3), api.mkSmallInt(4) });
    const r = api.send(p, api.intern("pointSum"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(r));
}

test "alien: struct return — comes back as a fresh Vector (makePoint)" {
    const api = setup();
    _ = wrap(api, "birnam_t_makePoint", "{ll} l l", "pointWith:", api.smallint_vt);

    const r = api.send(api.mkSmallInt(7), api.intern("pointWith:"), &[_]Oop{api.mkSmallInt(35)}, 1);
    try expectVector(api, r);
    try std.testing.expectEqual(@as(i64, 2), vSize(api, r));
    try std.testing.expectEqual(@as(i64, 7), intAt(api, r, 0));
    try std.testing.expectEqual(@as(i64, 35), intAt(api, r, 1));
}

test "alien: struct arg AND return round-trip (movePoint)" {
    const api = setup();
    const vvt = api.import("VectorVt").?;
    _ = wrap(api, "birnam_t_movePoint", "{ll} {ll} l l", "moveBy:and:", vvt);

    const p = vec(api, &[_]Oop{ api.mkSmallInt(10), api.mkSmallInt(20) });
    const r = api.send(p, api.intern("moveBy:and:"), &[_]Oop{ api.mkSmallInt(5), api.mkSmallInt(6) }, 2);
    try expectVector(api, r);
    try std.testing.expectEqual(@as(i64, 15), intAt(api, r, 0));
    try std.testing.expectEqual(@as(i64, 26), intAt(api, r, 1));
}

test "alien: two struct args (addPoints)" {
    const api = setup();
    const vvt = api.import("VectorVt").?;
    _ = wrap(api, "birnam_t_addPoints", "{ll} {ll} {ll}", "plusPoint:", vvt);

    const a = vec(api, &[_]Oop{ api.mkSmallInt(1), api.mkSmallInt(2) });
    const b = vec(api, &[_]Oop{ api.mkSmallInt(30), api.mkSmallInt(40) });
    const r = api.send(a, api.intern("plusPoint:"), &[_]Oop{b}, 1);
    try expectVector(api, r);
    try std.testing.expectEqual(@as(i64, 31), intAt(api, r, 0));
    try std.testing.expectEqual(@as(i64, 42), intAt(api, r, 1));
}

test "alien: HFA double structs — args in float registers (dotVec2)" {
    const api = setup();
    const vvt = api.import("VectorVt").?;
    _ = wrap(api, "birnam_t_dotVec2", "d {dd} {dd}", "dot:", vvt);

    const a = vec(api, &[_]Oop{ api.mkFloat(1.5), api.mkFloat(2.0) });
    const b = vec(api, &[_]Oop{ api.mkFloat(4.0), api.mkFloat(0.5) });
    const r = api.send(a, api.intern("dot:"), &[_]Oop{b}, 1);
    try std.testing.expect(api.isFloat(r) != 0);
    try std.testing.expectEqual(@as(f64, 7.0), api.floatValue(r));
}

test "alien: HFA double struct RETURN (scaleVec2)" {
    const api = setup();
    const vvt = api.import("VectorVt").?;
    _ = wrap(api, "birnam_t_scaleVec2", "{dd} {dd} d", "scaledBy:", vvt);

    const v = vec(api, &[_]Oop{ api.mkFloat(3.0), api.mkFloat(5.0) });
    const r = api.send(v, api.intern("scaledBy:"), &[_]Oop{api.mkFloat(2.0)}, 1);
    try expectVector(api, r);
    try std.testing.expectEqual(@as(f64, 6.0), floatAt(api, r, 0));
    try std.testing.expectEqual(@as(f64, 10.0), floatAt(api, r, 1));
}

test "alien: HFA of 32-bit floats — 'f' fields narrow and widen (sumF2)" {
    const api = setup();
    const vvt = api.import("VectorVt").?;
    _ = wrap(api, "birnam_t_sumF2", "f {ff}", "f2sum", vvt);

    const v = vec(api, &[_]Oop{ api.mkFloat(1.25), api.mkFloat(2.5) });
    const r = api.send(v, api.intern("f2sum"), &[_]Oop{}, 0);
    try std.testing.expect(api.isFloat(r) != 0);
    try std.testing.expectEqual(@as(f64, 3.75), api.floatValue(r));
}

test "alien: mixed int/float struct — not an HFA (mixedVal, mkMixed)" {
    const api = setup();
    const vvt = api.import("VectorVt").?;
    _ = wrap(api, "birnam_t_mixedVal", "d {id}", "mixedVal", vvt);
    _ = wrap(api, "birnam_t_mkMixed", "{id} i d", "mixedWith:", api.smallint_vt);

    const m = vec(api, &[_]Oop{ api.mkSmallInt(40), api.mkFloat(2.5) });
    const r = api.send(m, api.intern("mixedVal"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(f64, 42.5), api.floatValue(r));

    const mk = api.send(api.mkSmallInt(7), api.intern("mixedWith:"), &[_]Oop{api.mkFloat(0.5)}, 1);
    try expectVector(api, mk);
    try std.testing.expectEqual(@as(i64, 7), intAt(api, mk, 0));
    try std.testing.expectEqual(@as(f64, 0.5), floatAt(api, mk, 1));
}

test "alien: nested struct — a nested Vector, both directions (sumLine, flipLine)" {
    const api = setup();
    const vvt = api.import("VectorVt").?;
    _ = wrap(api, "birnam_t_sumLine", "l {{ll}{ll}}", "lineSum", vvt);
    _ = wrap(api, "birnam_t_flipLine", "{{ll}{ll}} {{ll}{ll}}", "lineFlip", vvt);

    const a = vec(api, &[_]Oop{ api.mkSmallInt(1), api.mkSmallInt(2) });
    const b = vec(api, &[_]Oop{ api.mkSmallInt(3), api.mkSmallInt(4) });
    const line = vec(api, &[_]Oop{ a, b });

    const s = api.send(line, api.intern("lineSum"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 10), api.smallIntValue(s));

    const flipped = api.send(line, api.intern("lineFlip"), &[_]Oop{}, 0);
    try expectVector(api, flipped);
    const fa = vAt(api, flipped, 0); // was b
    const fb = vAt(api, flipped, 1); // was a
    try expectVector(api, fa);
    try std.testing.expectEqual(@as(i64, 3), intAt(api, fa, 0));
    try std.testing.expectEqual(@as(i64, 4), intAt(api, fa, 1));
    try std.testing.expectEqual(@as(i64, 1), intAt(api, fb, 0));
    try std.testing.expectEqual(@as(i64, 2), intAt(api, fb, 1));
}

test "alien: scalar array field — a nested Vector element (sumArr, iota)" {
    const api = setup();
    const vvt = api.import("VectorVt").?;
    _ = wrap(api, "birnam_t_sumArr", "l {l 3i}", "arrSum", vvt);
    _ = wrap(api, "birnam_t_iota", "{l 3i} i", "iotaFrom", api.smallint_vt);

    // struct { long base; int v[3]; } ⇄ #[base #[v0 v1 v2]]
    const arr = vec(api, &[_]Oop{ api.mkSmallInt(1), api.mkSmallInt(2), api.mkSmallInt(3) });
    const s = vec(api, &[_]Oop{ api.mkSmallInt(100), arr });
    const r = api.send(s, api.intern("arrSum"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 106), api.smallIntValue(r));

    const made = api.send(api.mkSmallInt(10), api.intern("iotaFrom"), &[_]Oop{}, 0);
    try expectVector(api, made);
    try std.testing.expectEqual(@as(i64, 2), vSize(api, made));
    try std.testing.expectEqual(@as(i64, 10), intAt(api, made, 0));
    const mv = vAt(api, made, 1);
    try expectVector(api, mv);
    try std.testing.expectEqual(@as(i64, 3), vSize(api, mv));
    try std.testing.expectEqual(@as(i64, 11), intAt(api, mv, 0));
    try std.testing.expectEqual(@as(i64, 12), intAt(api, mv, 1));
    try std.testing.expectEqual(@as(i64, 13), intAt(api, mv, 2));
}

test "alien: >16-byte struct — the sret/memory-return ABI path (mkBig, sumBig)" {
    const api = setup();
    const vvt = api.import("VectorVt").?;
    _ = wrap(api, "birnam_t_mkBig", "{lllll} l", "bigFrom", api.smallint_vt);
    _ = wrap(api, "birnam_t_sumBig", "l {lllll}", "bigSum", vvt);

    const big = api.send(api.mkSmallInt(100), api.intern("bigFrom"), &[_]Oop{}, 0);
    try expectVector(api, big);
    try std.testing.expectEqual(@as(i64, 5), vSize(api, big));
    try std.testing.expectEqual(@as(i64, 100), intAt(api, big, 0));
    try std.testing.expectEqual(@as(i64, 104), intAt(api, big, 4));

    const s = api.send(big, api.intern("bigSum"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 510), api.smallIntValue(s));
}

test "alien: libc div — a real dlsym'd struct-returning function ({ii} i i)" {
    const api = setup();
    _ = wrap(api, "div", "{ii} i i", "divBy:", api.smallint_vt);

    const r = api.send(api.mkSmallInt(7), api.intern("divBy:"), &[_]Oop{api.mkSmallInt(2)}, 1);
    try expectVector(api, r);
    try std.testing.expectEqual(@as(i64, 3), intAt(api, r, 0)); // quot
    try std.testing.expectEqual(@as(i64, 1), intAt(api, r, 1)); // rem
}

test "alien: a wrong-shape Vector answers nil (no crash), at every nesting level" {
    const api = setup();
    const vvt = api.import("VectorVt").?;
    _ = wrap(api, "birnam_t_sumPoint", "l {ll}", "pointSum2", vvt);
    _ = wrap(api, "birnam_t_sumLine", "l {{ll}{ll}}", "lineSum2", vvt);
    _ = wrap(api, "birnam_t_sumArr", "l {l 3i}", "arrSum2", vvt);

    // Top level: #[1] is not a 2-field point.
    const short = vec(api, &[_]Oop{api.mkSmallInt(1)});
    try std.testing.expect(api.send(short, api.intern("pointSum2"), &[_]Oop{}, 0) == api.nilObj);

    // Nested level: the first sub-point has 1 field.
    const ok_pt = vec(api, &[_]Oop{ api.mkSmallInt(3), api.mkSmallInt(4) });
    const bad_line = vec(api, &[_]Oop{ short, ok_pt });
    try std.testing.expect(api.send(bad_line, api.intern("lineSum2"), &[_]Oop{}, 0) == api.nilObj);

    // Array level: 2 elements where int[3] needs 3.
    const bad_arr = vec(api, &[_]Oop{ api.mkSmallInt(1), api.mkSmallInt(2) });
    const bad_s = vec(api, &[_]Oop{ api.mkSmallInt(100), bad_arr });
    try std.testing.expect(api.send(bad_s, api.intern("arrSum2"), &[_]Oop{}, 0) == api.nilObj);
}

test "alien: a non-Vector where a struct is expected answers nil (error module absorbs the DNU)" {
    const api = setup();
    const vvt = api.import("VectorVt").?;
    _ = wrap(api, "birnam_t_addPoints", "{ll} {ll} {ll}", "plusPoint2:", vvt);

    // The ARG is a SmallInt, not a Vector: the shim's `size` send DNUs into an
    // Error value, the shape check fails, the shim answers nil — no crash.
    const a = vec(api, &[_]Oop{ api.mkSmallInt(1), api.mkSmallInt(2) });
    const r = api.send(a, api.intern("plusPoint2:"), &[_]Oop{api.mkSmallInt(7)}, 1);
    try std.testing.expect(r == api.nilObj);
}

test "alien: invalid struct signatures are rejected with #badSignature errors (§C1)" {
    const api = setup();
    const vt = api.proto(api.object_vt);
    // Unclosed brace.
    try std.testing.expect(isErr(api, wrap(api, "birnam_t_sumPoint", "l {ll", "bad1", vt)));
    // Empty struct.
    try std.testing.expect(isErr(api, wrap(api, "birnam_t_sumPoint", "l {}", "bad2", vt)));
    // 'v' is not a field type.
    try std.testing.expect(isErr(api, wrap(api, "birnam_t_sumPoint", "l {iv}", "bad3", vt)));
    // 's' fields are not supported (v1: freeing char* temporaries in aggregates).
    try std.testing.expect(isErr(api, wrap(api, "birnam_t_sumPoint", "l {is}", "bad4", vt)));
    // Zero-count array.
    try std.testing.expect(isErr(api, wrap(api, "birnam_t_sumArr", "l {l 0i}", "bad5", vt)));
    // Arrays are struct-field-only (C cannot pass a bare array by value).
    try std.testing.expect(isErr(api, wrap(api, "birnam_t_sumArr", "l 3i", "bad6", vt)));
    // Struct arrays (count before a brace) are rejected.
    try std.testing.expect(isErr(api, wrap(api, "birnam_t_sumLine", "l {3{ll}}", "bad7", vt)));
    // A struct return can't be 'v'-flavored nonsense: stray chars after a valid sig.
    try std.testing.expect(isErr(api, wrap(api, "birnam_t_sumPoint", "l {ll}}", "bad8", vt)));
    // 'v' as an argument.
    try std.testing.expect(isErr(api, wrap(api, "birnam_t_sumPoint", "l v", "bad9", vt)));
}

test "alien: signature whitespace is insignificant, even inside braces" {
    const api = setup();
    const vvt = api.import("VectorVt").?;
    const ok = wrap(api, "birnam_t_addPoints", " { l l }  { l l }  { l l } ", "spacedPlus:", vvt);
    try std.testing.expect(ok == vvt);

    const a = vec(api, &[_]Oop{ api.mkSmallInt(1), api.mkSmallInt(2) });
    const b = vec(api, &[_]Oop{ api.mkSmallInt(10), api.mkSmallInt(20) });
    const r = api.send(a, api.intern("spacedPlus:"), &[_]Oop{b}, 1);
    try expectVector(api, r);
    try std.testing.expectEqual(@as(i64, 11), intAt(api, r, 0));
    try std.testing.expectEqual(@as(i64, 22), intAt(api, r, 1));
}

// -- callbacks: C → birnam --------------------------------------------------------

/// Make a callback for `resp` and wrap a `birnam_t_*` harness that invokes it.
/// Answers the boxed callback fn-pointer (a SmallInt, receiver for the harness).
fn callback(api: *idapi.IdApi, resp: Oop, sig: []const u8) Oop {
    const Alien = api.import("Alien").?;
    return api.send(Alien, api.intern("callback:signature:"), &[_]Oop{ resp, str(api, sig) }, 2);
}

test "alien: callback — C calls back into a value: responder (apply)" {
    const api = setup();
    const cb = callback(api, responder(api, "value:", &rDouble), "l l");
    try std.testing.expect(cb != api.nilObj);

    // long birnam_t_apply(fp, long): the boxed fn-pointer is the receiver.
    _ = wrap(api, "birnam_t_apply", "l p l", "applyWith:", api.smallint_vt);
    const r = api.send(cb, api.intern("applyWith:"), &[_]Oop{api.mkSmallInt(21)}, 1);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(r));
}

test "alien: callback — two C args become value:value: (apply2)" {
    const api = setup();
    const cb = callback(api, responder(api, "value:value:", &rSum), "l l l");
    _ = wrap(api, "birnam_t_apply2", "l p l l", "applyWith:and:", api.smallint_vt);
    const r = api.send(cb, api.intern("applyWith:and:"), &[_]Oop{ api.mkSmallInt(40), api.mkSmallInt(2) }, 2);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(r));
}

test "alien: callback — doubles cross both ways (applyD)" {
    const api = setup();
    const cb = callback(api, responder(api, "value:", &rHalf), "d d");
    _ = wrap(api, "birnam_t_applyD", "d p d", "applyDWith:", api.smallint_vt);
    const r = api.send(cb, api.intern("applyDWith:"), &[_]Oop{api.mkFloat(85.0)}, 1);
    try std.testing.expect(api.isFloat(r) != 0);
    try std.testing.expectEqual(@as(f64, 42.5), api.floatValue(r));
}

test "alien: callback — void return, zero args (fire)" {
    const api = setup();
    g_fired = false;
    const cb = callback(api, responder(api, "value", &rFire), "v");
    _ = wrap(api, "birnam_t_fire", "l p", "fireIt", api.smallint_vt);
    const r = api.send(cb, api.intern("fireIt"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(r));
    try std.testing.expect(g_fired);
}

test "alien: callback — a C string arg arrives as a String (applyS)" {
    const api = setup();
    const cb = callback(api, responder(api, "value:", &rStrLen), "l s");
    _ = wrap(api, "birnam_t_applyS", "l p s", "applySOn:", api.smallint_vt);
    const r = api.send(cb, api.intern("applySOn:"), &[_]Oop{str(api, "hello")}, 1);
    try std.testing.expectEqual(@as(i64, 5), api.smallIntValue(r));
}

test "alien: callback — a struct passed INTO the callback arrives as a Vector (applyPt)" {
    const api = setup();
    const cb = callback(api, responder(api, "value:", &rVecSum), "l {ll}");
    _ = wrap(api, "birnam_t_applyPt", "l p", "applyPt", api.smallint_vt);
    const r = api.send(cb, api.intern("applyPt"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(r)); // 20 + 22
}

test "alien: callback — a Vector answered by the callback returns to C as a struct (viaPtCb)" {
    const api = setup();
    const cb = callback(api, responder(api, "value:", &rMkPoint), "{ll} l");
    _ = wrap(api, "birnam_t_viaPtCb", "l p l", "viaPtWith:", api.smallint_vt);
    // responder answers #[x+1 x+2]; harness computes p.x*100 + p.y.
    const r = api.send(cb, api.intern("viaPtWith:"), &[_]Oop{api.mkSmallInt(7)}, 1);
    try std.testing.expectEqual(@as(i64, 809), api.smallIntValue(r)); // 8*100 + 9
}

test "alien: callback — invalid signatures are rejected with kinded errors (§C1)" {
    const api = setup();
    const resp = responder(api, "value:", &rDouble);
    // 's' return: nobody could free the C string.
    try std.testing.expect(isErr(api, callback(api, resp, "s l")));
    // Unknown char / empty / malformed struct.
    try std.testing.expect(isErr(api, callback(api, resp, "q l")));
    try std.testing.expect(isErr(api, callback(api, resp, "")));
    try std.testing.expect(isErr(api, callback(api, resp, "l {ll")));
}

test "alien: callback — survives GC churn (block + unit are rooted)" {
    const api = setup();
    const cb = callback(api, responder(api, "value:", &rDouble), "l l");
    _ = wrap(api, "birnam_t_apply", "l p l", "applyWith2:", api.smallint_vt);

    // Anchor the collector, churn allocations, and collect: the responder and the
    // callback's code unit are reachable ONLY through alien's root registry.
    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    var i: usize = 0;
    while (i < 10_000) : (i += 1) _ = api.mkString("churn", 5);
    _ = api.gcCollect();

    const r = api.send(cb, api.intern("applyWith2:"), &[_]Oop{api.mkSmallInt(21)}, 1);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(r));
}

test "alien: a pointer return is a full-width Word, round-tripped through a p arg" {
    const api = setup();
    // `p` return (bit 63 set): must come back as a Word, NOT a truncated SmallInt.
    _ = wrap(api, "birnam_t_highPtr", "p", "highPtr", api.word_vt);
    const ptr = api.send(api.mkWord(0), api.intern("highPtr"), &[_]Oop{}, 0);
    try std.testing.expect(api.isWord(ptr) != 0);
    try std.testing.expectEqual(@as(u64, 0xFFFF_0000_1234_5678), api.wordValue(ptr));

    // Round-trip: the Word receiver IS the `p` argument (receiver = first C arg),
    // so identity returns the very same address, still full-width.
    _ = wrap(api, "birnam_t_ptrIdentity", "p p", "idPtr", api.word_vt);
    const back = api.send(ptr, api.intern("idPtr"), &[_]Oop{}, 0);
    try std.testing.expect(api.isWord(back) != 0);
    try std.testing.expectEqual(api.wordValue(ptr), api.wordValue(back));
}
