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
const prelude = @import("prelude.zig");
const collection = @import("collection.zig");
const errmod = @import("error.zig");
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
    // The prelude provides the SmallInt protocol. Arithmetic `[a + b]` compiles to
    // a real send now (not inlined C math), so it dispatches here — without the
    // prelude a compiled `+` would have no method to reach.
    prelude.bringUp(api);
    return api;
}

/// `setup` + the collection factories (`Vector`/`Map`) that `#[…]`/`{…}` literals
/// lower onto, and the F7 error model (so `[10 / 0]` yields a #zeroDivide value).
fn setupFull() *idapi.IdApi {
    const api = setup();
    collection.bringUp(api);
    errmod.bringUp(api);
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

test "codegen: multi-char binary operators parse and dispatch (>=, <=)" {
    const api = setupFull(); // prelude has >= <= on Integer, & on Boolean

    try std.testing.expect(eval(api, "[5 >= 3]") == api.trueObj);
    try std.testing.expect(eval(api, "[3 >= 5]") == api.falseObj);
    try std.testing.expect(eval(api, "[5 <= 5]") == api.trueObj);
    try std.testing.expect(eval(api, "[6 <= 5]") == api.falseObj);
    // Combined with single-char ops and a boolean `&` (a between-style test).
    try std.testing.expect(eval(api, "[[4 >= 1] & [4 <= 10]]") == api.trueObj);
    try std.testing.expect(eval(api, "[[12 >= 1] & [12 <= 10]]") == api.falseObj);
}

test "codegen: arithmetic is dispatched — zeroDivide is a value and propagates" {
    const api = setupFull(); // prelude + collection + error model (F7)

    // `[10 / 0]` reaches the prelude's guarded `/`, yielding a #zeroDivide error
    // value — not undefined behavior from untagging-and-dividing in raw C.
    const e = eval(api, "[10 / 0]");
    try std.testing.expect(api.send(e, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj);
    try std.testing.expect(api.send(e, api.intern("kind"), &[_]Oop{}, 0) == api.intern("zeroDivide"));

    // The error flows THROUGH further arithmetic (doesNotUnderstand: returns self),
    // rather than being untagged into garbage as the old inline path would.
    const e2 = eval(api, "[[10 / 0] + 1]");
    try std.testing.expect(api.send(e2, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj);
    try std.testing.expect(api.send(e2, api.intern("kind"), &[_]Oop{}, 0) == api.intern("zeroDivide"));

    // Ordinary arithmetic still works end to end through the same dispatch path.
    try std.testing.expectEqual(@as(i64, 25), try evalInt(api, "[100 / 4]"));
}

test "codegen: evalPersisting: top-level lets survive across evaluations (REPL)" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;
    const evalP = struct {
        fn e(a: *idapi.IdApi, cg: Oop, src: []const u8) Oop {
            return a.send(cg, a.intern("evalPersisting:"), &[_]Oop{a.mkString(src.ptr, src.len)}, 1);
        }
    }.e;

    // Bind in one evaluation, use in the next — the binding persists as a global.
    _ = evalP(api, Codegen, "let x := 40");
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(evalP(api, Codegen, "[x + 2]")));
    // Rebinding updates it; a later line sees the new value.
    _ = evalP(api, Codegen, "let x := [x * 2]"); // x := 80
    try std.testing.expectEqual(@as(i64, 80), api.smallIntValue(evalP(api, Codegen, "x")));

    // Plain `eval:` does NOT persist (a block-local let stays local), so this name
    // is undeclared → nil.
    try std.testing.expect(eval(api, "y") == api.nilObj);
    _ = eval(api, "let y := 5"); // eval: (non-persisting)
    try std.testing.expect(eval(api, "y") == api.nilObj); // still undeclared next time
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

test "codegen §B9: lastError reflects only the top-level eval, cleared on success" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;
    const lastErr = api.intern("lastError");

    // A failed top-level compile records an error.
    _ = eval(api, "let :=");
    try std.testing.expect(api.send(Codegen, lastErr, &[_]Oop{}, 0) != api.nilObj);

    // A subsequent SUCCESSFUL eval clears it — so a program that internally probes
    // a bad compile (aot.birnam) can't leave a false "compile error" for the CLI's
    // post-eval check. (A failed top-level compile returns before the clear, so its
    // note survives.)
    try std.testing.expectEqual(@as(i64, 7), try evalInt(api, "[3 + 4]"));
    try std.testing.expect(api.send(Codegen, lastErr, &[_]Oop{}, 0) == api.nilObj);
}

test "docs/42 §D1: backfillMirror makes the MOP total over the native protocol" {
    const api = setupFull();
    store.backfillMirror(api);
    const primitive = api.intern("primitive");
    const string_vt = api.string_vt;

    // A native type now reflects its protocol through the fact store: enumerate its
    // selectors, and each native binding reads back as the GC-safe #primitive marker.
    const nsel: usize = @intCast(api.smallIntValue(api.send(store.store_instance, store.s_attrCount, &[_]Oop{string_vt}, 1)));
    try std.testing.expect(nsel > 0);
    const sel0 = api.send(store.store_instance, store.s_attrAt, &[_]Oop{ string_vt, api.mkSmallInt(0) }, 2);
    try std.testing.expect(api.send(store.store_instance, store.s_currentValue, &[_]Oop{ string_vt, sel0 }, 2) == primitive);

    // Idempotent: a second backfill changes nothing (assert-if-absent).
    store.backfillMirror(api);
    const nsel2: usize = @intCast(api.smallIntValue(api.send(store.store_instance, store.s_attrCount, &[_]Oop{string_vt}, 1)));
    try std.testing.expectEqual(nsel, nsel2);

    // Internal dicts (the intern table / export env) are NOT types — they delegate
    // to nothing — so their data keys never become mirror facts.
}

test "codegen E1: a captured var in a chained send is not dropped by capture analysis" {
    const api = setupFull();

    // `x + base + 1` is a same-category CHAIN — one r_message node with two
    // binaryMsg children — which the emitter folds via emitSendChain. The capture
    // pass used to inspect only the single-message shape, so a free variable in a
    // chain's arg position (`base`) was never captured; the generated C then
    // referenced an undeclared name and failed to compile (→ nil block). Regression.
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let base := 31; let f := [:x | [x + base + 1]]; [f value: 10]"));
    // longer chain, capture is not the first arg either.
    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "let a := 10; let b := 20; let f := [:x | [x + a + b + 2]]; [f value: 10]"));
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

test "codegen 3c.4: vector literals lower to a persistent add: chain" {
    const api = setupFull();

    // empty + non-empty, via the collection protocol (size / at: / inject:into:).
    try std.testing.expectEqual(@as(i64, 0), try evalInt(api, "[#[] size]"));
    try std.testing.expectEqual(@as(i64, 3), try evalInt(api, "[#[10 20 30] size]"));
    try std.testing.expectEqual(@as(i64, 20), try evalInt(api, "[#[10 20 30] at: 1]"));
    try std.testing.expectEqual(@as(i64, 60), try evalInt(api, "[#[10 20 30] inject: 0 into: [:a :x | [a + x]]]"));

    // elements are arbitrary exprs (sends, idents), evaluated in order.
    try std.testing.expectEqual(@as(i64, 9), try evalInt(api, "let y := 4; [#[[1 + 1] y 3] inject: 0 into: [:a :x | [a + x]]]"));

    // printString round-trips the literal syntax.
    try std.testing.expectEqualStrings("#[10 20 30]", try evalStr(api, "[#[10 20 30] printString]"));
    try std.testing.expectEqualStrings("#[]", try evalStr(api, "[#[] printString]"));

    // nested vector literal.
    try std.testing.expectEqualStrings("#[#[1 2] #[3 4]]", try evalStr(api, "[#[#[1 2] #[3 4]] printString]"));

    // a literal whose elements capture an enclosing variable inside a block. Uses
    // the native fold primitive `inject:into:` + `add:` directly — the higher-order
    // `collect:` now lives on Collection in stdlib.birnam (docs/design/41 §B), which
    // this stdlib-less codegen harness does not load.
    try std.testing.expectEqualStrings("#[101 102 103]", try evalStr(api, "let b := 100; [[#[1 2 3] inject: #[] into: [:a :x | [a add: [x + b]]]] printString]"));
}

test "codegen 3c.4: the Collection protocol unifies over inject:into: (stdlib)" {
    const api = setupFull();
    // Load the real self-hosted stdlib: it defines the whole higher-order protocol
    // ONCE on the shared Collection parent over the single native fold `inject:into:`
    // (docs/design/41 §B). Vector, Range, Map and String each provide only that
    // primitive and inherit collect:/select:/detect:/count:/… through delegation.
    _ = eval(api, @embedFile("stdlib.birnam"));

    // collect: reaches all four families uniformly (Map folds over its values).
    try std.testing.expectEqualStrings("#[1 4 9]", try evalStr(api, "[[#[1 2 3] collect: [:x | [x * x]]] printString]"));
    try std.testing.expectEqualStrings("#[1 4 9]", try evalStr(api, "[[[1 to: 3] collect: [:x | [x * x]]] printString]"));
    try std.testing.expectEqualStrings("#[60]", try evalStr(api, "[[{#a -> 6} collect: [:v | [v * 10]]] printString]"));
    try std.testing.expectEqualStrings("#[a! b! c!]", try evalStr(api, "[[\"abc\" collect: [:c | [c , \"!\"]]] printString]"));

    // The rest of the protocol on types that previously had NO higher-order methods.
    try std.testing.expectEqualStrings("#[2 4]", try evalStr(api, "[[#[1 2 3 4] select: [:x | [x even]]] printString]"));
    try std.testing.expectEqualStrings("2", try evalStr(api, "[[#[1 2 3 4] count: [:x | [x even]]] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[\"hello\" includes: \"e\"] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[[1 to: 5] anySatisfy: [:x | [x > 4]]] printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[[1 to: 5] isEmpty] printString]"));
}

test "codegen B8: literal true/false/nil are real singletons, not late-bound globals" {
    const api = setupFull();
    // A literal boolean must be the actual Boolean object (with the full protocol),
    // not an unbound-global reference that reads nil. (docs/design/41 §B8.)
    try std.testing.expectEqualStrings("true", try evalStr(api, "[true printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[false printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[true not] printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[true & false] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[true | false] printString]"));
    try std.testing.expectEqualStrings("yes", try evalStr(api, "[true ifTrue: [| \"yes\"] ifFalse: [| \"no\"]]"));
    // nil is still the nil singleton (isNil answers true).
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[nil isNil] printString]"));
    // A literal boolean passed INTO a block is captured/forwarded, never treated as a
    // free-variable global — the block's `not` sees a real Boolean.
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[[:b | [b not]] value: true] printString]"));
}

test "codegen B3/B4/B5/B7: equality, integer math, range indexing, nil printString" {
    const api = setupFull();
    _ = eval(api, @embedFile("stdlib.birnam"));

    // `==` is pointer identity: distinct boxed integers are NOT identical, but a
    // literal singleton is identical to itself.
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[3 == 3] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[nil == nil] printString]"));

    // B3: structural `=` via double dispatch — by kind, then element-wise; element
    // comparison recurses through ordinary `=` (nested + mixed types work).
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[#[1 2 3] = #[1 2 3]] printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[#[1 2 3] = #[1 2 4]] printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[#[1 2] = #[1 2 3]] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[#[] = #[]] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[#[#[1] \"a\"] = #[#[1] \"a\"]] printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[#[1 2 3] = 5] printString]")); // non-collection
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[#[1 2 3] = [1 to: 3]] printString]")); // cross-kind
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[[1 to: 3] = [1 to: 3]] printString]"));
    // Maps: order-independent, compared by key→value content.
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[{#a -> 1, #b -> 2} = {#b -> 2, #a -> 1}] printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[{#a -> 1} = {#a -> 2}] printString]"));
    // Booleans/nil are singletons: value equality IS identity (inherited Object `=`).
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[true = true] printString]"));
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[true = false] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[nil = nil] printString]"));

    // B4: integer rounding/coercion are identity on a whole number; sqrt via Float.
    try std.testing.expectEqualStrings("3.0", try evalStr(api, "[[9 sqrt] printString]"));
    try std.testing.expectEqualStrings("7", try evalStr(api, "[[7 floor] printString]"));
    try std.testing.expectEqualStrings("7", try evalStr(api, "[[7 asInteger] printString]"));

    // B5: range indexing (0-based) + first/last/reversed.
    try std.testing.expectEqualStrings("2", try evalStr(api, "[[[2 to: 5] at: 0] printString]"));
    try std.testing.expectEqualStrings("5", try evalStr(api, "[[[2 to: 5] last] printString]"));
    try std.testing.expectEqualStrings("#[5 4 3 2]", try evalStr(api, "[[[2 to: 5] reversed] printString]"));

    // B7: nil prints as "nil", not "anObject".
    try std.testing.expectEqualStrings("nil", try evalStr(api, "[nil printString]"));
}

test "codegen 3c.4: map literals lower to a persistent at:put: chain" {
    const api = setupFull();

    try std.testing.expectEqual(@as(i64, 0), try evalInt(api, "[{} size]"));
    try std.testing.expectEqual(@as(i64, 2), try evalInt(api, "[{\"a\" -> 1, \"b\" -> 2} size]"));
    try std.testing.expectEqual(@as(i64, 5), try evalInt(api, "[{\"apple\" -> 3, \"banana\" -> 5} at: \"banana\"]"));

    // absent key runs the ifAbsent: block.
    try std.testing.expectEqual(@as(i64, 0), try evalInt(api, "[{\"a\" -> 1} at: \"z\" ifAbsent: [| 0]]"));

    // later duplicate key wins (persistent at:put: overwrite).
    try std.testing.expectEqual(@as(i64, 9), try evalInt(api, "[{\"k\" -> 1, \"k\" -> 9} at: \"k\"]"));

    // keys/values that are themselves exprs.
    try std.testing.expectEqual(@as(i64, 30), try evalInt(api, "let n := 10; [{\"x\" -> [n + 20]} at: \"x\"]"));

    // symbol keys including keyword-selector symbols (the reason for `->`).
    try std.testing.expectEqual(@as(i64, 7), try evalInt(api, "[{#at:put: -> 7} at: #at:put:]"));
}

test "codegen: float literals + the numeric tower (int↔float promotion)" {
    const api = setupFull();

    const evalFloat = struct {
        fn f(a: *idapi.IdApi, src: []const u8) !f64 {
            const r = eval(a, src);
            if (a.isFloat(r) == 0) return error.NotAFloat;
            return a.floatValue(r);
        }
    }.f;

    // A bare float literal compiles to birnam_mkFloat and round-trips.
    try std.testing.expectEqual(@as(f64, 3.5), try evalFloat(api, "3.5"));
    try std.testing.expectEqual(@as(f64, -0.25), try evalFloat(api, "-0.25"));
    // Float arithmetic, and Integer→Float promotion in BOTH directions.
    try std.testing.expectEqual(@as(f64, 5.0), try evalFloat(api, "[2.5 + 2.5]"));
    try std.testing.expectEqual(@as(f64, 3.5), try evalFloat(api, "[1 + 2.5]"));
    try std.testing.expectEqual(@as(f64, 3.5), try evalFloat(api, "[2.5 + 1]"));
    try std.testing.expectEqual(@as(f64, 2.5), try evalFloat(api, "[10.0 / 4]"));
    // Integer / Integer stays exact (no accidental promotion).
    try std.testing.expectEqual(@as(i64, 2), try evalInt(api, "[10 / 4]"));
    // Cross-kind ORDERING coerces (producing/relating values widens)...
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[1.5 < 2] printString]"));
    // ...but cross-kind EQUALITY does NOT: integer and real are different domains, so
    // `1 = 1.0` is FALSE (docs/design/41 §B). Cross explicitly with `asFloat`.
    try std.testing.expectEqualStrings("false", try evalStr(api, "[[1 = 1.0] printString]"));
    try std.testing.expectEqualStrings("true", try evalStr(api, "[[[1 asFloat] = 1.0] printString]"));
    // printString forces a decimal point so a whole-valued float still reads float.
    try std.testing.expectEqualStrings("4.0", try evalStr(api, "[4.0 printString]"));
    // Conversions.
    try std.testing.expectEqual(@as(i64, 7), try evalInt(api, "[7.5 asInteger]"));
    try std.testing.expectEqual(@as(f64, 3.0), try evalFloat(api, "[3 asFloat]"));
}

// -- integration: a compiled birnam block as a C callback (alien) ---------------

const alien = @import("alien.zig");

// A C harness the callback test drives: invoke the fn pointer with 20 and 22.
export fn birnam_cg_apply2(f: *const fn (c_long, c_long) callconv(.c) c_long) c_long {
    return f(20, 22);
}

test "integration: [Alien callback:] on a REAL compiled birnam block" {
    const api = setupFull();
    alien.bringUp(api);

    // A capturing birnam block, compiled to native by codegen.
    const blk = eval(api, "let base := 100; [:a :b | [[a + b] + base]]");
    try std.testing.expect(blk != api.nilObj);

    // Alien turns it into a C function pointer…
    const Alien = api.import("Alien").?;
    const sig = api.mkString("l l l", 5);
    const cb = api.send(Alien, api.intern("callback:signature:"), &[_]Oop{ blk, sig }, 2);
    try std.testing.expect(cb != api.nilObj);

    // …which the C harness calls like any C function: 20 + 22 + 100.
    const wrapOk = api.send(Alien, api.intern("wrap:signature:as:on:"), &[_]Oop{
        api.mkString("birnam_cg_apply2", 16), api.mkString("l p", 3),
        api.intern("cgApply"),                api.smallint_vt,
    }, 4);
    try std.testing.expect(wrapOk == api.smallint_vt);
    const r = api.send(cb, api.intern("cgApply"), &[_]Oop{}, 0);
    try std.testing.expectEqual(@as(i64, 142), api.smallIntValue(r));
}

// -- slice 1 of the images arc (docs/40 §3a): relocatable artifacts -------------

test "slice-1: compile birnam to a relocatable .dylib, load it, run it" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;

    const path = "/tmp/birnam_codegen_artifact.dylib";
    std.fs.cwd().deleteFile(path) catch {};

    // Exercises every relocatable piece at once: baked global handles (`Vector`
    // behind the #[…] literal and `Integer`'s protocol — re-derived by name in
    // __birnam_unit_init), a capturing closure (a file-scope block fn inside the
    // artifact), and dispatch through the undefined `birnam_*` externals the
    // dynamic loader resolves against this process at dlopen.
    const src =
        "let base := 40; let add := [:y | [base + y]]; " ++
        "let v := #[[add value: 1] [add value: 2]]; [[v size] + [add value: 0]]";
    const psrc = api.mkString(src.ptr, src.len);
    const ppath = api.mkString(path, path.len);
    const out = api.send(Codegen, api.intern("compile:output:"), &[_]Oop{ psrc, ppath }, 2);
    try std.testing.expect(out != api.nilObj);

    // The artifact must be pure: no `__g` handle may remain an undefined dynamic
    // import (nothing exports those — a baked JIT handle leaking into the artifact
    // would poison dlopen), and __birnam_unit_init must be present.
    const r = api.send(Codegen, api.intern("load:"), &[_]Oop{ppath}, 1);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(r)); // 2 + 40
}

test "slice-1: artifact failures are nil — bad source, missing library" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;

    const bad = api.mkString("let :=", 6);
    const ppath = api.mkString("/tmp/birnam_codegen_never.dylib", 31);
    try std.testing.expect(api.send(Codegen, api.intern("compile:output:"), &[_]Oop{ bad, ppath }, 2) == api.nilObj);

    const absent = api.mkString("/tmp/birnam_codegen_absent.dylib", 32);
    try std.testing.expect(api.send(Codegen, api.intern("load:"), &[_]Oop{absent}, 1) == api.nilObj);
}

// -- slice 2 of the images arc (docs/40 §3c): exact layouts for block captures --

noinline fn churnStrings(api: *idapi.IdApi, count: usize) void {
    var i: usize = 0;
    var last: Oop = null;
    while (i < count) : (i += 1) {
        last = api.mkString("throwaway garbage string", 24);
    }
    std.mem.doNotOptimizeAway(&last);
}

test "slice-2: a compiled closure's heap capture survives precise collection" {
    const api = setupFull();
    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    _ = &anchor;
    defer api.gcSetPrecise(0);
    api.gcSetPrecise(1);

    // The capture is a HEAP object (a String) — if the emitted
    // birnam_layoutAllOops on the block's vtable were missing or wrong, the
    // precise trace would skip the capture slot and the churn+collect below
    // would free the string out from under the closure.
    const blk = eval(api, "let base := \"birnam\"; [:a | [base , a]]");
    try std.testing.expect(blk != api.nilObj);

    churnStrings(api, 20_000);
    _ = api.gcCollect();

    const r = api.send(blk, api.intern("value:"), &[_]Oop{api.mkString(" rocks", 6)}, 1);
    var len: usize = 0;
    const p = api.stringBytes(r, &len);
    try std.testing.expectEqualStrings("birnam rocks", p[0..len]);
}

// -- slice 4 of the images arc (docs/40 §7.4): mapped code — install PRE-COMPILED --

test "slice-4: a defineMethod artifact installs a method with no recompile" {
    const api = setupFull();
    const Codegen = api.import("Codegen").?;

    const path = "/tmp/birnam_codegen_defs.dylib";
    std.fs.cwd().deleteFile(path) catch {};

    // In ARTIFACT mode, defineMethod lowers to a static trampoline (__amtramp) +
    // a direct birnam_method install — not the compiling runtime send. So loading
    // the artifact installs MAPPED code: the trampoline pointer comes from the
    // dlopen'd unit, and dispatch runs it with no recompile. (The live/JIT path
    // still uses the source-recording send — that's slice 3's replay truth.)
    const src = "[Reflect defineMethod: #triple on: Integer as: [:x | [x + [x + x]]]]; [7 triple]";
    const psrc = api.mkString(src.ptr, src.len);
    const ppath = api.mkString(path, path.len);
    try std.testing.expect(api.send(Codegen, api.intern("compile:output:"), &[_]Oop{ psrc, ppath }, 2) != api.nilObj);

    const r = api.send(Codegen, api.intern("load:"), &[_]Oop{ppath}, 1);
    try std.testing.expectEqual(@as(i64, 21), api.smallIntValue(r)); // 7 * 3
}
