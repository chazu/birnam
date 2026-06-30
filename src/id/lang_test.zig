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

test "lang 3b: parses collection literals — #[…] vectors and {…} maps" {
    const api = setup();

    // Vector literals: empty, flat, nested, with expr elements.
    try std.testing.expect(parses(api, "#[]"));
    try std.testing.expect(parses(api, "#[1 2 3]"));
    try std.testing.expect(parses(api, "#[#[1 2] #[3 4]]"));
    try std.testing.expect(parses(api, "#[[1 + 1] x \"s\"]"));
    try std.testing.expect(parses(api, "let v := #[10 20 30]"));

    // Map literals: `->` separates key and value (so symbol keys keep their
    // colons). Empty, comma-separated, whitespace-separated, trailing comma.
    try std.testing.expect(parses(api, "{}"));
    try std.testing.expect(parses(api, "{\"a\" -> 1, \"b\" -> 2}"));
    try std.testing.expect(parses(api, "{\"a\" -> 1 \"b\" -> 2}"));
    try std.testing.expect(parses(api, "{\"a\" -> 1,}"));
    try std.testing.expect(parses(api, "let m := {1 -> 10, 2 -> 20}")); // integer keys
    // Symbol keys — including keyword-selector symbols with colons — are now
    // unambiguous, no whitespace tricks needed.
    try std.testing.expect(parses(api, "{#x -> 1, #y -> 2}"));
    try std.testing.expect(parses(api, "{#at:put: -> 1, #foo: -> 2}"));

    // Literals nest inside sends and don't disturb the symbol rule (#sel).
    try std.testing.expect(parses(api, "[#[1 2 3] inject: 0 into: [:a :x | [a + x]]]"));
    try std.testing.expect(parses(api, "[m at: #key put: #[1 2]]"));
    try std.testing.expect(parses(api, "#double")); // still a symbol, not a vector
}

test "lang 3b: parses ?logic variables in datalog query position" {
    const api = setup();

    try std.testing.expect(parses(api, "?p"));
    try std.testing.expect(parses(api, "let v := ?city"));
    // a query: result var + a where: clause vector of clause vectors
    try std.testing.expect(parses(api, "[db findAll: ?p where: #[ #[?p #city #Boston] ]]"));
    // a logic var sits beside symbols and bound idents inside a clause
    try std.testing.expect(parses(api, "#[ #[?p #city city] #[?p #pet ?animal] ]"));
}

test "lang 3b: rejects malformed input (full-consume via eof)" {
    const api = setup();

    try std.testing.expect(!parses(api, "let := 30")); // missing name
    try std.testing.expect(!parses(api, "[n printString")); // unclosed bracket
    try std.testing.expect(!parses(api, "@#$%")); // garbage
    try std.testing.expect(!parses(api, "[justAVar]")); // a bare var is not a send
    try std.testing.expect(!parses(api, "let n := 30 garbage")); // trailing junk → eof fails
}
