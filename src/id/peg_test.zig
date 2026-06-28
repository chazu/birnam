//! Proof for rung 3a: the PEG engine on the libid floor.
//!
//! Boots libid, brings up the peg module, builds small grammars from libid
//! Pattern objects, and parses — checking the transient CST (rule spans + nested
//! rule nodes). Proves the ported operator semantics (charset / plus / rule-ref /
//! seq / choice / literal / eof) and packrat memoization on the new representation.

const std = @import("std");
const id = @import("id.zig");
const peg = @import("peg.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

test "peg 3a: number grammar — charset + plus + rule-ref build a nested CST" {
    const api = id.id_init();
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
    const api = id.id_init();
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

test "peg 3a: opt + star + not, and left-recursion-free repetition" {
    const api = id.id_init();
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
