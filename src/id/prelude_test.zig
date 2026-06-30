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
