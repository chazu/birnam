//! Proof for rung 3a: the PEG engine on the libid floor.
//!
//! Boots libid, brings up the peg module, builds small grammars from libid
//! Pattern objects, and parses — checking the transient CST (rule spans + nested
//! rule nodes). Proves the ported operator semantics (charset / plus / rule-ref /
//! seq / choice / literal / eof) and packrat memoization on the new representation.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const peg = @import("peg.zig");
const err_mod = @import("error.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

fn str(api: *idapi.IdApi, bytes: []const u8) Oop {
    return api.mkString(bytes.ptr, bytes.len);
}

test "peg §C2/§C3: parse failures are kinded error values, distinguishable by kind" {
    const api = floor.api();
    peg.bringUp(api);
    err_mod.bringUp(api); // so parse:from: can yield kinded error values

    const g = peg.mkGrammar(api);
    const number = api.intern("number");
    const digit = api.intern("digit");
    // number = digit+  — but `digit` is (for now) never defined: a typo.
    peg.defineRule(g, number, peg.mkPlus(api, peg.mkRuleRef(api, digit)));

    const parseFrom = api.intern("parse:from:");
    const isError = api.intern("isError");
    const kind = api.intern("kind");

    // §C3: a reference to the undefined `digit` is a distinct #undefinedRule error,
    // not the same silent nil a legitimate non-match once produced.
    const r1 = api.send(g, parseFrom, &[_]Oop{ str(api, "123"), number }, 2);
    try std.testing.expect(api.send(r1, isError, &[_]Oop{}, 0) == api.trueObj);
    try std.testing.expect(api.send(r1, kind, &[_]Oop{}, 0) == api.intern("undefinedRule"));

    // Now define digit; a non-digit input is a plain #syntaxError (a DIFFERENT kind).
    peg.defineRule(g, digit, peg.mkCharset(api, &.{.{ .lo = '0', .hi = '9' }}));
    const r2 = api.send(g, parseFrom, &[_]Oop{ str(api, "abc"), number }, 2);
    try std.testing.expect(api.send(r2, isError, &[_]Oop{}, 0) == api.trueObj);
    try std.testing.expect(api.send(r2, kind, &[_]Oop{}, 0) == api.intern("syntaxError"));

    // A successful parse still yields a CST, not an error.
    const r3 = api.send(g, parseFrom, &[_]Oop{ str(api, "42"), number }, 2);
    try std.testing.expect(api.send(r3, isError, &[_]Oop{}, 0) == api.falseObj);
}

test "peg §G4: a left-recursive rule is a kinded #leftRecursion error, not a stack overflow" {
    const api = floor.api();
    peg.bringUp(api);
    err_mod.bringUp(api);

    const g = peg.mkGrammar(api);
    const expr = api.intern("expr");
    const digit = api.intern("digit");
    peg.defineRule(g, digit, peg.mkCharset(api, &.{.{ .lo = '0', .hi = '9' }}));
    // expr = expr '+' digit / digit  — direct left recursion: the first alternative
    // re-enters `expr` at the same position. Before §G4 this recursed to a crash.
    peg.defineRule(g, expr, peg.mkChoice(api, &.{
        peg.mkSeq(api, &.{ peg.mkRuleRef(api, expr), peg.mkLiteral(api, "+"), peg.mkRuleRef(api, digit) }),
        peg.mkRuleRef(api, digit),
    }));

    const parseFrom = api.intern("parse:from:");
    const isError = api.intern("isError");
    const kind = api.intern("kind");

    // The degraded fallback would match just "1"; but the eof-anchored root below
    // rejects the trailing "+2", so the parse fails carrying the §G4 diagnostic.
    const program = api.intern("program");
    peg.defineRule(g, program, peg.mkSeq(api, &.{ peg.mkRuleRef(api, expr), peg.mkEof(api) }));

    const r = api.send(g, parseFrom, &[_]Oop{ str(api, "1+2"), program }, 2);
    try std.testing.expect(api.send(r, isError, &[_]Oop{}, 0) == api.trueObj);
    try std.testing.expect(api.send(r, kind, &[_]Oop{}, 0) == api.intern("leftRecursion"));
}

test "peg 3a: number grammar — charset + plus + rule-ref build a nested CST" {
    const api = floor.api();
    peg.bringUp(api);
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const na = arena.allocator();

    // digit  = [0-9]
    // number = digit+
    const g = peg.mkGrammar(api);
    const digit = api.intern("digit");
    const number = api.intern("number");
    peg.defineRule(g, digit, peg.mkCharset(api, &.{.{ .lo = '0', .hi = '9' }}));
    peg.defineRule(g, number, peg.mkPlus(api, peg.mkRuleRef(api, digit)));

    const root = peg.parse(api, na, g, "123", number) orelse return error.ParseFailed;
    try std.testing.expect(root.rule == number);
    try std.testing.expectEqual(@as(usize, 0), root.start);
    try std.testing.expectEqual(@as(usize, 3), root.end);

    // Three `digit` rule applications → three CST children, each one byte wide.
    try std.testing.expectEqual(@as(usize, 3), root.children.len);
    try std.testing.expect(root.children[0].rule == digit);
    try std.testing.expectEqual(@as(usize, 0), root.children[0].start);
    try std.testing.expectEqual(@as(usize, 1), root.children[0].end);
    try std.testing.expectEqual(@as(usize, 2), root.children[2].start);
    try std.testing.expectEqual(@as(usize, 3), root.children[2].end);

    // Non-digits fail.
    try std.testing.expect(peg.parse(api, na, g, "abc", number) == null);
}

test "peg 3a: choice of literals + eof + ordered-choice failure" {
    const api = floor.api();
    peg.bringUp(api);
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const na = arena.allocator();

    // kw = ("let" / "set") eof   — eof forces full consumption
    const g = peg.mkGrammar(api);
    const kw = api.intern("kw");
    peg.defineRule(g, kw, peg.mkSeq(api, &.{
        peg.mkChoice(api, &.{ peg.mkLiteral(api, "let"), peg.mkLiteral(api, "set") }),
        peg.mkEof(api),
    }));

    try std.testing.expect(peg.parse(api, na, g, "let", kw) != null);
    try std.testing.expect(peg.parse(api, na, g, "set", kw) != null);
    try std.testing.expect(peg.parse(api, na, g, "lets", kw) == null); // trailing input → eof fails
    try std.testing.expect(peg.parse(api, na, g, "xyz", kw) == null); // neither alternative

    // The kw node has no rule children (choice/literal/eof are not rule-refs).
    const r = peg.parse(api, na, g, "let", kw) orelse return error.ParseFailed;
    try std.testing.expectEqual(@as(usize, 0), r.children.len);
    try std.testing.expectEqual(@as(usize, 3), r.end);
}

test "peg 3a: the message (id) api — build + parse + walk CST through api.send" {
    const api = floor.api();
    peg.bringUp(api);
    const Peg = api.import("Peg") orelse return error.NoPeg;
    const empty = &[_]Oop{};

    // digit = [0-9] ; number = digit+ — built entirely through messages.
    const g = api.send(Peg, api.intern("grammar"), empty, 0);
    const digit = api.intern("digit");
    const number = api.intern("number");

    const cs = api.send(Peg, api.intern("charset"), empty, 0);
    _ = api.send(cs, api.intern("from:to:"), &[_]Oop{ api.mkSmallInt('0'), api.mkSmallInt('9') }, 2);
    _ = api.send(g, api.intern("rule:is:"), &[_]Oop{ digit, cs }, 2);

    const dref = api.send(Peg, api.intern("ruleRef:"), &[_]Oop{digit}, 1);
    const plus = api.send(dref, api.intern("plus"), empty, 0);
    _ = api.send(g, api.intern("rule:is:"), &[_]Oop{ number, plus }, 2);

    const root = api.send(g, api.intern("parse:from:"), &[_]Oop{ str(api, "123"), number }, 2);
    try std.testing.expect(root != api.nilObj);
    try std.testing.expect(api.send(root, api.intern("rule"), empty, 0) == number);
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(api.send(root, api.intern("end"), empty, 0)));
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(api.send(root, api.intern("childCount"), empty, 0)));

    const c0 = api.send(root, api.intern("childAt:"), &[_]Oop{api.mkSmallInt(0)}, 1);
    try std.testing.expect(api.send(c0, api.intern("rule"), empty, 0) == digit);
    try std.testing.expect(api.send(root, api.intern("childAt:"), &[_]Oop{api.mkSmallInt(9)}, 1) == api.nilObj);

    // Failure surfaces as a kinded error value when the error module is loaded
    // (§C2), or the canonical nil singleton otherwise — either way, not a CST.
    const bad = api.send(g, api.intern("parse:from:"), &[_]Oop{ str(api, "abc"), number }, 2);
    try std.testing.expect(bad == api.nilObj or api.send(bad, api.intern("isError"), empty, 0) == api.trueObj);
}

test "peg 3a: opt + star + not, and left-recursion-free repetition" {
    const api = floor.api();
    peg.bringUp(api);
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const na = arena.allocator();

    // sign   = "-"?
    // digits = [0-9]*
    // int    = sign digits eof
    const g = peg.mkGrammar(api);
    const int_rule = api.intern("int");
    peg.defineRule(g, int_rule, peg.mkSeq(api, &.{
        peg.mkOpt(api, peg.mkLiteral(api, "-")),
        peg.mkStar(api, peg.mkCharset(api, &.{.{ .lo = '0', .hi = '9' }})),
        peg.mkEof(api),
    }));

    try std.testing.expect(peg.parse(api, na, g, "-42", int_rule) != null);
    try std.testing.expect(peg.parse(api, na, g, "42", int_rule) != null);
    try std.testing.expect(peg.parse(api, na, g, "", int_rule) != null); // star matches zero
    try std.testing.expect(peg.parse(api, na, g, "4-2", int_rule) == null);

    // `not`: a non-empty string is `!eof` at position 0.
    const ne = api.intern("nonEmpty");
    peg.defineRule(g, ne, peg.mkNot(api, peg.mkEof(api)));
    try std.testing.expect(peg.parse(api, na, g, "x", ne) != null);
    try std.testing.expect(peg.parse(api, na, g, "", ne) == null);
}
