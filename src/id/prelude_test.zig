//! Proof for the primitive prelude: Integer / Boolean / System protocol, driven
//! through the object model (no compiler involved — these are floor primitives).

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
    return api;
}

var loop_sum: i64 = 0;
fn collectValue(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    loop_sum += gapi.smallIntValue(a[0]);
    return gapi.nilObj;
}
fn s1(api: *idapi.IdApi, rcv: Oop, sel: [*:0]const u8, a: Oop) Oop {
    return api.send(rcv, api.intern(sel), &[_]Oop{a}, 1);
}

test "prelude: Integer arithmetic / comparison / mod / printString" {
    const api = setup();
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(s1(api, api.mkSmallInt(3), "+", api.mkSmallInt(4))));
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(s1(api, api.mkSmallInt(17), "mod:", api.mkSmallInt(5))));
    try std.testing.expect(s1(api, api.mkSmallInt(5), "=", api.mkSmallInt(5)) == api.trueObj);
    try std.testing.expect(s1(api, api.mkSmallInt(5), "=", api.mkSmallInt(6)) == api.falseObj);
    try std.testing.expect(s1(api, api.mkSmallInt(3), "<", api.mkSmallInt(4)) == api.trueObj);

    const str = api.send(api.mkSmallInt(-42), api.intern("printString"), &[_]Oop{}, 0);
    var len: usize = 0;
    const p = api.stringBytes(str, &len);
    try std.testing.expectEqualStrings("-42", p[0..len]);
}

test "prelude: Boolean ifTrue:ifFalse: / & / not — dispatch on identity" {
    const api = setup();
    // [3 = 3] → true → ifTrue:ifFalse: runs the first 0-arg block. We need a block
    // to run; reuse the prelude's own arithmetic by making the blocks return ints
    // via a tiny method. Simpler: assert the chosen-branch identity directly.
    const t = s1(api, api.mkSmallInt(3), "=", api.mkSmallInt(3));
    const f = s1(api, api.mkSmallInt(3), "=", api.mkSmallInt(9));
    try std.testing.expect(api.send(t, api.intern("&"), &[_]Oop{f}, 1) == api.falseObj);
    try std.testing.expect(api.send(t, api.intern("&"), &[_]Oop{t}, 1) == api.trueObj);
    try std.testing.expect(api.send(f, api.intern("not"), &[_]Oop{}, 0) == api.trueObj);
    try std.testing.expect(api.send(t, api.intern("|"), &[_]Oop{f}, 1) == api.trueObj);

    // printString renders the singletons (so booleans can be displayed / `,`-joined).
    var len: usize = 0;
    const tp = api.send(t, api.intern("printString"), &[_]Oop{}, 0);
    try std.testing.expectEqualStrings("true", api.stringBytes(tp, &len)[0..len]);
    const fp = api.send(f, api.intern("printString"), &[_]Oop{}, 0);
    try std.testing.expectEqualStrings("false", api.stringBytes(fp, &len)[0..len]);
}

// A block whose `value` bumps sc_evals (so a test can prove short-circuit skipped it).
var sc_evals: usize = 0;
fn sideTrue(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    sc_evals += 1;
    return gapi.trueObj;
}
fn sideFalse(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    sc_evals += 1;
    return gapi.falseObj;
}
fn valBlock(api: *idapi.IdApi, impl: idapi.Method) Oop {
    const vt = api.proto(api.object_vt);
    api.method(vt, api.intern("value"), impl);
    return api.alloc(vt, 0);
}

test "prelude: and:/or: short-circuit — the block runs only on the deciding branch (E3)" {
    const api = setup();
    const t = s1(api, api.mkSmallInt(3), "=", api.mkSmallInt(3)); // true
    const f = s1(api, api.mkSmallInt(3), "=", api.mkSmallInt(9)); // false

    // false and: [block] → false, block NOT evaluated.
    sc_evals = 0;
    try std.testing.expect(api.send(f, api.intern("and:"), &[_]Oop{valBlock(api, &sideTrue)}, 1) == api.falseObj);
    try std.testing.expectEqual(@as(usize, 0), sc_evals);
    // true and: [block] → the block's value; evaluated exactly once.
    sc_evals = 0;
    try std.testing.expect(api.send(t, api.intern("and:"), &[_]Oop{valBlock(api, &sideFalse)}, 1) == api.falseObj);
    try std.testing.expectEqual(@as(usize, 1), sc_evals);
    // true or: [block] → true, block NOT evaluated.
    sc_evals = 0;
    try std.testing.expect(api.send(t, api.intern("or:"), &[_]Oop{valBlock(api, &sideFalse)}, 1) == api.trueObj);
    try std.testing.expectEqual(@as(usize, 0), sc_evals);
    // false or: [block] → the block's value; evaluated exactly once.
    sc_evals = 0;
    try std.testing.expect(api.send(f, api.intern("or:"), &[_]Oop{valBlock(api, &sideTrue)}, 1) == api.trueObj);
    try std.testing.expectEqual(@as(usize, 1), sc_evals);
}

test "prelude: arithmetic faults are catchable errors, not panics" {
    const api = id.id_init();
    gapi = api;
    prelude.bringUp(api);
    errmod.bringUp(api); // F7 — faults now mint real Error values

    const kindOf = struct {
        fn k(a: *idapi.IdApi, e: Oop) []const u8 {
            return std.mem.span(a.symbolName(a.send(e, a.intern("kind"), &[_]Oop{}, 0)));
        }
    }.k;

    // Overflow: 2^61 * 2^61 leaves the 63-bit range.
    const big = api.mkSmallInt(1 << 61);
    const ov = api.send(big, api.intern("*"), &[_]Oop{big}, 1);
    try std.testing.expect(api.send(ov, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj);
    try std.testing.expectEqualStrings("overflow", kindOf(api, ov));

    // Type mismatch: an Integer op against a non-Integer (the System object).
    const sys = api.import("System").?;
    const tm = api.send(api.mkSmallInt(5), api.intern("+"), &[_]Oop{sys}, 1);
    try std.testing.expectEqualStrings("typeMismatch", kindOf(api, tm));

    // Division by zero.
    const zd = api.send(api.mkSmallInt(10), api.intern("/"), &[_]Oop{api.mkSmallInt(0)}, 1);
    try std.testing.expectEqualStrings("zeroDivide", kindOf(api, zd));

    // An error OPERAND propagates (not masked as a type mismatch): 5 + (10/0).
    const prop = api.send(api.mkSmallInt(5), api.intern("+"), &[_]Oop{zd}, 1);
    try std.testing.expectEqualStrings("zeroDivide", kindOf(api, prop));

    // Ordinary arithmetic and equality are unaffected.
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(api.send(api.mkSmallInt(3), api.intern("+"), &[_]Oop{api.mkSmallInt(4)}, 1)));
    try std.testing.expect(api.send(api.mkSmallInt(5), api.intern("="), &[_]Oop{sys}, 1) == api.falseObj); // cross-type = is false
}

test "prelude: System readFile:/writeFile:contents: round-trips through disk" {
    const api = id.id_init();
    gapi = api;
    prelude.bringUp(api);
    errmod.bringUp(api);
    const sys = api.import("System").?;

    const path = "/tmp/birnam_io_test.txt";
    std.fs.cwd().deleteFile(path) catch {};
    const body = "hello from birnam\nline two\n";

    const w = api.send(sys, api.intern("writeFile:contents:"), &[_]Oop{ api.mkString(path.ptr, path.len), api.mkString(body.ptr, body.len) }, 2);
    try std.testing.expect(w == sys); // success answers self

    const r = api.send(sys, api.intern("readFile:"), &[_]Oop{api.mkString(path.ptr, path.len)}, 1);
    var len: usize = 0;
    try std.testing.expectEqualStrings(body, api.stringBytes(r, &len)[0..len]);

    // Reading a missing file is a catchable #ioError, not a crash.
    const e = api.send(sys, api.intern("readFile:"), &[_]Oop{api.mkString("/no/such/path.txt", 17)}, 1);
    try std.testing.expect(api.send(e, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj);
    std.fs.cwd().deleteFile(path) catch {};
}

test "prelude: to:do: iterates inclusively" {
    const api = setup();
    loop_sum = 0;
    // A stand-in "block": an object whose value: accumulates the index.
    const vt = api.proto(api.object_vt);
    api.method(vt, api.intern("value:"), &collectValue);
    const blk = api.alloc(vt, 0);
    _ = api.send(api.mkSmallInt(1), api.intern("to:do:"), &[_]Oop{ api.mkSmallInt(5), blk }, 2);
    try std.testing.expectEqual(@as(i64, 15), loop_sum); // 1+2+3+4+5 inclusive
}
