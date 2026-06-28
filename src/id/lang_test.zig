//! Proof for rung 3b: the lang module builds birnam's grammar through peg's
//! message api and parses real birnam source.

const std = @import("std");
const id = @import("id.zig");
const peg = @import("peg.zig");
const lang = @import("lang.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

fn setup() *idapi.IdApi {
    const api = id.id_init();
    peg.bringUp(api); // exports "Peg"
    lang.bringUp(api); // imports "Peg", builds the grammar, exports "Lang"
    return api;
}

fn parse(api: *idapi.IdApi, src: []const u8) Oop {
    const Lang = api.import("Lang").?;
    return api.send(Lang, api.intern("parse:"), &[_]Oop{api.mkString(src.ptr, src.len)}, 1);
}

fn parses(api: *idapi.IdApi, src: []const u8) bool {
    return parse(api, src) != api.nilObj;
}

test "lang 3b: parses core birnam — let, blocks, sends, literals" {
    const api = setup();

    // The CST root is a `program` node.
    const root = parse(api, "let n := 30");
    try std.testing.expect(root != api.nilObj);
    try std.testing.expect(api.send(root, api.intern("rule"), &[_]Oop{}, 0) == api.intern("program"));

    // Real lines from the example programs.
    try std.testing.expect(parses(api, "let n := 30"));
    try std.testing.expect(parses(api, "let fizzy := [:n | [[n mod: 3] = 0]]"));
    try std.testing.expect(parses(api, "[System println: [classify value: n]]"));
    try std.testing.expect(parses(api, "[1 to: 20 do: [:n | [System println: n]]]"));
    try std.testing.expect(parses(api, "let primes := [[2 to: n] select: [:k | [isPrime value: k]]]"));
    try std.testing.expect(parses(api, "[[k > 1] & [[hasDivisor value: k] isNil]]"));

    // Strings, the comma (concat) binop, and zero-arg blocks.
    try std.testing.expect(parses(api, "[[\"a\" , \"b\"] printString]"));
    try std.testing.expect(parses(api, "[x ifTrue: [| \"yes\"] ifFalse: [| \"no\"]]"));
}

test "lang 3b: multi-statement programs with ';' and comments" {
    const api = setup();

    try std.testing.expect(parses(api,
        \\// a fizzbuzz fragment
        \\let fizzy := [:n | [[n mod: 3] = 0]];
        \\let buzzy := [:n | [[n mod: 5] = 0]];
        \\[System println: [fizzy value: 15]]
    ));

    // Trailing ';' allowed.
    try std.testing.expect(parses(api, "let a := 1; let b := 2;"));
}

test "lang 3b: rejects malformed input (full-consume via eof)" {
    const api = setup();

    try std.testing.expect(!parses(api, "let := 30")); // missing name
    try std.testing.expect(!parses(api, "[n printString")); // unclosed bracket
    try std.testing.expect(!parses(api, "@#$%")); // garbage
    try std.testing.expect(!parses(api, "[justAVar]")); // a bare var is not a send
    try std.testing.expect(!parses(api, "let n := 30 garbage")); // trailing junk → eof fails
}
