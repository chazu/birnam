//! Proof for rung 3c (part 2): the birnam → C emitter.
//!
//! Boots the whole front end (peg → lang → tcc → codegen), then evaluates birnam
//! arithmetic expressions: each is parsed, emitted as Method-ABI C, compiled by
//! libtcc, and run — the answer comes back from native code through the id api.

const std = @import("std");
const id = @import("id.zig");
const store = @import("store.zig");
const peg = @import("peg.zig");
const lang = @import("lang.zig");
const tcc = @import("tcc.zig");
const codegen = @import("codegen.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

var gapi: *idapi.IdApi = undefined;

fn setup() *idapi.IdApi {
    const api = id.id_init();
    gapi = api;
    store.bringUp(api); // codegen resolves globals through the store
    peg.bringUp(api);
    lang.bringUp(api);
    tcc.bringUp(api);
    codegen.bringUp(api);
    return api;
}

// Primitive methods to install on smallint_vt so emitted birnam sends have real
// targets to dispatch to (the floor's SmallInt has no protocol of its own).
fn mDouble(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    return gapi.mkSmallInt(gapi.smallIntValue(self) * 2);
}
fn mPlus(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return gapi.mkSmallInt(gapi.smallIntValue(self) + gapi.smallIntValue(args[0]));
}
fn mTilde(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n; // non-arithmetic binary op: self*10 + arg, just to be distinguishable
    return gapi.mkSmallInt(gapi.smallIntValue(self) * 10 + gapi.smallIntValue(args[0]));
}
fn mFromTo(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return gapi.mkSmallInt(gapi.smallIntValue(args[0]) * 100 + gapi.smallIntValue(args[1]));
}
// `[x with: aBlock]` invokes the block argument as `[aBlock value: x]`.
fn mWith(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return gapi.send(args[0], gapi.intern("value:"), &[_]Oop{self}, 1);
}

fn eval(api: *idapi.IdApi, src: []const u8) Oop {
    const Codegen = api.import("Codegen").?;
    return api.send(Codegen, api.intern("eval:"), &[_]Oop{api.mkString(src.ptr, src.len)}, 1);
}
fn evalInt(api: *idapi.IdApi, src: []const u8) !i64 {
    const r = eval(api, src);
    if (r == api.nilObj) return error.EvalFailed;
    return api.smallIntValue(r);
}
fn evalStr(api: *idapi.IdApi, src: []const u8) ![]const u8 {
    const r = eval(api, src);
    if (r == api.nilObj) return error.EvalFailed;
    var len: usize = 0;
    const p = api.stringBytes(r, &len);
    return p[0..len];
}

test "codegen 3c.2: birnam arithmetic → C → native → value" {
    const api = setup();

    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "42"));
    try std.testing.expectEqual(@as(i64, 7), try evalInt(api, "[3 + 4]"));
    try std.testing.expectEqual(@as(i64, 14), try evalInt(api, "[[3 + 4] * 2]"));
    try std.testing.expectEqual(@as(i64, 5), try evalInt(api, "[[10 - 3] - 2]"));
    try std.testing.expectEqual(@as(i64, 25), try evalInt(api, "[100 / 4]"));
    try std.testing.expectEqual(@as(i64, -8), try evalInt(api, "[-10 + 2]"));

    // Deeper nesting recurses correctly.
    try std.testing.expectEqual(@as(i64, 20), try evalInt(api, "[[2 * 3] + [7 * 2]]"));
}

test "codegen: parse failure / empty / undeclared global all yield nil (no crash)" {
    const api = setup();

    try std.testing.expect(eval(api, "@#$") == api.nilObj); // parse failure
    try std.testing.expect(eval(api, "") == api.nilObj); // empty program
    try std.testing.expect(eval(api, "hello") == api.nilObj); // undeclared global → nil
}

test "codegen 3c.3: sends dispatch through libid (unary / binary / keyword)" {
    const api = setup();

    // Give SmallInt a protocol to dispatch to.
    api.method(api.smallint_vt, api.intern("double"), &mDouble);
    api.method(api.smallint_vt, api.intern("plus:"), &mPlus);
    api.method(api.smallint_vt, api.intern("~"), &mTilde);
    api.method(api.smallint_vt, api.intern("from:to:"), &mFromTo);

    // unary
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "[21 double]"));
    // keyword, one arg
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "[20 plus: 22]"));
    // keyword, two parts → selector "from:to:", two args
    try std.testing.expectEqual(@as(i64, 102), try evalInt(api, "[0 from: 1 to: 2]"));
    // non-arithmetic binary op routes through send (not the inline fast path)
    try std.testing.expectEqual(@as(i64, 53), try evalInt(api, "[5 ~ 3]"));

    // Receivers and args may themselves be sends or inline arithmetic.
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "[[21 double] plus: 0]"));
    try std.testing.expectEqual(@as(i64, 22), try evalInt(api, "[20 plus: [1 + 1]]"));
    try std.testing.expectEqual(@as(i64, 84), try evalInt(api, "[[20 plus: 22] double]"));
}

test "codegen 3c.3: let-bindings, identifiers, and multi-statement programs" {
    const api = setup();
    api.method(api.smallint_vt, api.intern("double"), &mDouble);
    api.method(api.smallint_vt, api.intern("plus:"), &mPlus);

    // let introduces a variable; later statements see it; value = last statement.
    try std.testing.expectEqual(@as(i64, 21), try evalInt(api, "let x := 21"));
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let x := 21; [x double]"));
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let x := 20; let y := 22; [x plus: y]"));
    // a binding's RHS may reference earlier bindings
    try std.testing.expectEqual(@as(i64, 84), try evalInt(api, "let x := 21; let y := [x double]; [y double]"));
    // identifier as both receiver and argument
    try std.testing.expectEqual(@as(i64, 10), try evalInt(api, "let x := 5; [x plus: x]"));

    // an undeclared identifier resolves to a nil global (forward/undeclared ref)
    try std.testing.expect(eval(api, "y") == api.nilObj);
}

test "codegen 3c.3: string literals construct String objects" {
    const api = setup();

    try std.testing.expectEqualStrings("hello", try evalStr(api, "\"hello\""));
    try std.testing.expectEqualStrings("", try evalStr(api, "\"\""));
    // chars that must be escaped into the emitted C literal but ARE valid in a
    // birnam string (the grammar allows any byte but `"`): backslash, newline.
    try std.testing.expectEqualStrings("a\\c", try evalStr(api, "\"a\\c\""));
    try std.testing.expectEqualStrings("a\nb", try evalStr(api, "\"a\nb\""));
    // strings flow through let + are returned as the program value
    try std.testing.expectEqualStrings("hi", try evalStr(api, "let s := \"hi\"; s"));
}

test "codegen 3c.3: globals — define, reference from native, late binding" {
    const api = setup();
    api.method(api.smallint_vt, api.intern("double"), &mDouble);
    api.method(api.smallint_vt, api.intern("plus:"), &mPlus);

    // Define a global via the Globals facade (an ordinary send), then reference
    // it as a bare identifier from compiled native code.
    try std.testing.expectEqual(@as(i64, 21), try evalInt(api, "[Globals at: #x put: 21]"));
    try std.testing.expectEqual(@as(i64, 21), try evalInt(api, "x"));
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "[x double]"));

    // Late binding: one compiled block reads x through its binding on each call,
    // so it sees a redefinition that happens between calls.
    try std.testing.expectEqual(@as(i64, 3), try evalInt(api,
        "let g := [| x]; [Globals at: #x put: 1]; let a := [g value]; [Globals at: #x put: 2]; let b := [g value]; [a plus: b]"));

    // A global can hold a block; fetch and invoke it.
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api,
        "[Globals at: #f put: [:n | [n double]]]; [[Globals at: #f] value: 21]"));
}

test "codegen 3c.3: symbol literals intern to selectors" {
    const api = setup();
    try std.testing.expect(eval(api, "#double") == api.intern("double"));
    try std.testing.expect(eval(api, "#plus:") == api.intern("plus:"));
    try std.testing.expect(eval(api, "#fooWithBar:andBaz:andQuux:") == api.intern("fooWithBar:andBaz:andQuux:"));
    try std.testing.expect(eval(api, "#+") == api.intern("+"));
}

test "codegen 3c.3: blocks compile to closures (params, capture, invoke)" {
    const api = setup();
    api.method(api.smallint_vt, api.intern("double"), &mDouble);
    api.method(api.smallint_vt, api.intern("plus:"), &mPlus);

    // one-param block, invoked with value:
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let f := [:n | [n double]]; [f value: 21]"));
    // zero-param block capturing an enclosing variable, invoked with value
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let x := 21; let f := [| [x double]]; [f value]"));
    // param + capture together
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let x := 10; let f := [:y | [x plus: y]]; [f value: 32]"));
    // two params → selector value:value:
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let g := [:a :b | [a plus: b]]; [g value: 20 value: 22]"));
    // a block is a value: stored, then invoked later via an identifier
    try std.testing.expectEqual(@as(i64, 84), try evalInt(api, "let x := 21; let f := [| [x double]]; let r := [f value]; [r double]"));
    // nested blocks: inner captures the outer param transitively
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let mk := [:a | [:b | [a plus: b]]]; let add20 := [mk value: 20]; [add20 value: 22]"));

    // a block literal passed directly as a message argument (a statement-
    // expression nested inside the call's compound-literal arg array)
    api.method(api.smallint_vt, api.intern("with:"), &mWith);
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "[21 with: [:x | [x double]]]"));
    // capturing block passed as an argument: [20 with: blk] → [[20+2] double] = 44
    try std.testing.expectEqual(@as(i64, 44), try evalInt(api, "let k := 2; [20 with: [:x | [[x plus: k] double]]]"));
}

test "codegen 3c.3: define-method installs a block as a native method on a type" {
    const api = setup();

    // unary method, body uses inline arithmetic (self + self)
    _ = eval(api, "[Reflect defineMethod: #twice on: Integer as: [:self | [self + self]]]");
    try std.testing.expectEqual(@as(i64, 10), try evalInt(api, "[5 twice]"));

    // keyword method, two args → selector combine:and:
    _ = eval(api, "[Reflect defineMethod: #combine:and: on: Integer as: [:self :a :b | [[a * b] + self]]]");
    try std.testing.expectEqual(@as(i64, 26), try evalInt(api, "[2 combine: 3 and: 8]")); // 3*8+2

    // the multi-keyword selector specifically requested
    _ = eval(api, "[Reflect defineMethod: #fooWithBar:andBaz:andQuux: on: Integer as: [:self :bar :baz :quux | [[bar + baz] + quux]]]");
    try std.testing.expectEqual(@as(i64, 15), try evalInt(api, "[0 fooWithBar: 4 andBaz: 5 andQuux: 6]")); // 4+5+6

    // a defined method whose body references a global is late-bound to it
    _ = eval(api, "[Globals at: #bump put: 100]");
    _ = eval(api, "[Reflect defineMethod: #bumped on: Integer as: [:self | [self + bump]]]");
    try std.testing.expectEqual(@as(i64, 105), try evalInt(api, "[5 bumped]"));
    _ = eval(api, "[Globals at: #bump put: 0]");
    try std.testing.expectEqual(@as(i64, 5), try evalInt(api, "[5 bumped]"));

    // a defined method is a first-class dispatch: callable on any SmallInt, nests
    try std.testing.expectEqual(@as(i64, 20), try evalInt(api, "[[5 twice] twice]"));
}
