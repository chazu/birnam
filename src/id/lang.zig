//! src/id/lang.zig — rung 3b: the birnam language module.
//!
//! `lang` is a sibling module over libid that `api.import("Peg")` and builds
//! **birnam's grammar through peg's message api** (`docs/32` rung 3; the user's
//! directive: "the lang module will leverage that api to construct the birnam
//! grammar and parser"). It exposes a `Lang` facade: `[Lang parse: aString]`
//! parses birnam source and returns the transient CST (a tree of peg `CstNode`
//! objects), or nil.
//!
//! The grammar is a CORE subset that parses the example programs (let-bindings,
//! blocks with/without params, unary/binary/keyword sends, int/string literals,
//! `//` comments, `;`-separated statements). It grows from here (bracket-literal
//! family, ranges, multi-char operators) as codegen (3c) needs them.
//!
//! Everything is built by sending messages to the imported `Peg` facade — no Zig
//! linkage to the peg module. Depends only on the IdApi.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

var g_api: *idapi.IdApi = undefined;
var peg: Oop = undefined; // the imported Peg facade
var grammar: Oop = undefined; // the birnam grammar (a peg Grammar object)

pub var lang_vt: Oop = null;
pub var lang_obj: Oop = null; // the Lang facade (exported "Lang")

// -- thin send helpers -------------------------------------------------------

fn send0(rcv: Oop, name: [*:0]const u8) Oop {
    return g_api.send(rcv, g_api.intern(name), &[_]Oop{}, 0);
}
fn send1(rcv: Oop, name: [*:0]const u8, a: Oop) Oop {
    return g_api.send(rcv, g_api.intern(name), &[_]Oop{a}, 1);
}
fn send2(rcv: Oop, name: [*:0]const u8, a: Oop, b: Oop) Oop {
    return g_api.send(rcv, g_api.intern(name), &[_]Oop{ a, b }, 2);
}

// -- grammar-building helpers (peg message api) ------------------------------

fn lit(bytes: []const u8) Oop {
    return send1(peg, "literal:", g_api.mkString(bytes.ptr, bytes.len));
}
fn rref(name: [*:0]const u8) Oop {
    return send1(peg, "ruleRef:", g_api.intern(name));
}
fn anyc() Oop {
    return send0(peg, "any");
}
fn eofc() Oop {
    return send0(peg, "eof");
}
fn star(p: Oop) Oop {
    return send0(p, "star");
}
fn plus(p: Oop) Oop {
    return send0(p, "plus");
}
fn optp(p: Oop) Oop {
    return send0(p, "opt");
}
fn notp(p: Oop) Oop {
    return send0(p, "negate");
}
fn seqN(children: []const Oop) Oop {
    const q = send0(peg, "seq");
    for (children) |c| _ = send1(q, "add:", c);
    return q;
}
fn altN(children: []const Oop) Oop {
    const q = send0(peg, "choice");
    for (children) |c| _ = send1(q, "add:", c);
    return q;
}
fn cset(pairs: []const [2]u8) Oop {
    const c = send0(peg, "charset");
    for (pairs) |p| _ = send2(c, "from:to:", g_api.mkSmallInt(p[0]), g_api.mkSmallInt(p[1]));
    return c;
}
fn defRule(name: [*:0]const u8, pat: Oop) void {
    _ = send2(grammar, "rule:is:", g_api.intern(name), pat);
}

// -- birnam's grammar --------------------------------------------------------

fn buildGrammar() void {
    grammar = send0(peg, "grammar");

    // Shared leaf patterns (NOT rules → produce no CST nodes).
    const nl = cset(&.{.{ '\n', '\n' }});
    const comment = seqN(&.{ lit("//"), star(seqN(&.{ notp(nl), anyc() })) });
    const spacec = cset(&.{ .{ ' ', ' ' }, .{ '\t', '\t' }, .{ '\n', '\n' }, .{ '\r', '\r' } });
    const ws = star(altN(&.{ spacec, comment })); // optional whitespace/comments (shared)
    const alpha = cset(&.{ .{ 'a', 'z' }, .{ 'A', 'Z' }, .{ '_', '_' } });
    const alnum = cset(&.{ .{ 'a', 'z' }, .{ 'A', 'Z' }, .{ '0', '9' }, .{ '_', '_' } });
    const digit = cset(&.{.{ '0', '9' }});

    // Leaf TOKENS as rules (labelled spans codegen can read text from).
    defRule("ident", seqN(&.{ alpha, star(alnum) }));
    defRule("number", seqN(&.{ optp(lit("-")), plus(digit) }));
    // float = '-'? digit+ '.' digit+ — a boxed-f64 literal (3.14, -0.5). Tried
    // BEFORE number in `primary` so `3.5` is one float, not `3` then a stray `.5`.
    defRule("float", seqN(&.{ optp(lit("-")), plus(digit), lit("."), plus(digit) }));
    // string = '"' ( '\' any / !'"' any )* '"' — a backslash escapes the next char
    // (so `\"` is a literal quote, `\\` a backslash); codegen unescapes. The escape
    // alt is tried first so `\"` is consumed as one unit and does not close the
    // string.
    defRule("string", seqN(&.{
        lit("\""),
        star(altN(&.{
            seqN(&.{ lit("\\"), anyc() }),
            seqN(&.{ notp(lit("\"")), anyc() }),
        })),
        lit("\""),
    }));
    // Binary-operator characters. Dispatch handles ANY interned selector, so this
    // set is the ONLY thing that decides which punctuation can be a binary selector
    // (§G9); `binops` below runs `plus` over it, so any RUN of these chars is one
    // multi-char operator (`>=`, `~=`, `@%`). Excluded on purpose: `.` (float
    // literal), `:` (keyword marker), `?` (datalog logicVar prefix) — each already
    // owns its char elsewhere in the grammar.
    defRule("binop", cset(&.{
        .{ '=', '=' }, .{ '<', '<' }, .{ '>', '>' }, .{ '&', '&' },
        .{ '|', '|' }, .{ ',', ',' }, .{ '+', '+' }, .{ '-', '-' },
        .{ '*', '*' }, .{ '/', '/' }, .{ '~', '~' }, .{ '%', '%' },
        .{ '@', '@' }, .{ '!', '!' }, .{ '^', '^' },
    }));

    // symbol = '#' ( (ident ':')+ / ident / binop+ ) — names a selector:
    // #double, #plus:, #fooWithBar:andBaz:andQuux:, #+. Codegen reads the span
    // text (minus the '#') and interns it; the child nodes don't matter.
    defRule("symbol", seqN(&.{
        lit("#"),
        altN(&.{
            plus(seqN(&.{ rref("ident"), lit(":") })),
            rref("ident"),
            plus(rref("binop")),
        }),
    }));

    // primary = float / number / string / symbol / ident  (float before number)
    defRule("primary", altN(&.{ rref("float"), rref("number"), rref("string"), rref("symbol"), rref("ident") }));

    // keyword = ident ':'   (the ':' of ':=' only appears in letBinding, matched first)
    defRule("keyword", seqN(&.{ rref("ident"), lit(":") }));

    // message = keywordMsg / binaryMsg / unaryMsg
    defRule("keywordMsg", plus(seqN(&.{ rref("keyword"), ws, rref("expr"), ws })));
    // binops = one-or-more operator chars as ONE node, so multi-char operators like
    // `>=` `<=` `~=` `//`(no, that's a comment) parse as a single selector — not
    // just single-char `+ - * / < > =`.
    defRule("binops", plus(rref("binop")));
    defRule("binaryMsg", seqN(&.{ rref("binops"), ws, rref("expr") }));
    defRule("unaryMsg", rref("ident"));
    // A send may CHAIN same-category messages, folded left: `[a , b , c]` is
    // `((a , b) , c)`, `[3 factorial printString]` is `((3 factorial) printString)`.
    // Keyword parts already fold (keywordMsg is `plus`). Mixing categories in one
    // `[…]` still needs brackets (their Smalltalk precedence isn't modeled here);
    // a single message keeps the exact old CST shape, so existing parses are
    // unchanged (the `message` node collapses to the lone message via `skip`).
    defRule("message", altN(&.{
        rref("keywordMsg"),
        plus(seqN(&.{ rref("binaryMsg"), ws })),
        plus(seqN(&.{ rref("unaryMsg"), ws })),
    }));

    // send = '[' expr message ']'   (a bare '[var]' / '[lit]' has no message → not a send)
    defRule("send", seqN(&.{ lit("["), ws, rref("expr"), ws, rref("message"), ws, lit("]") }));

    // block = '[' (':'ident)* '|' statements? ']'
    const blockHeader = seqN(&.{ star(seqN(&.{ lit(":"), rref("ident"), ws })), lit("|") });
    defRule("block", seqN(&.{ lit("["), ws, blockHeader, ws, optp(rref("statements")), ws, lit("]") }));

    // vectorLit = '#[' (expr ws)* ']'   — round-trips Vector printString (#[e e e]).
    // Each child is an element expr; ws/brackets produce no CST nodes.
    defRule("vectorLit", seqN(&.{
        lit("#["), ws,
        star(seqN(&.{ rref("expr"), ws })),
        lit("]"),
    }));

    // mapLit = '{' (mapEntry ws ','? ws)* '}'   — round-trips Map printString
    // ({k -> v, …}). Entries are comma- or whitespace-separated; a trailing ','
    // is allowed. Each mapEntry is a rule with two children: the key and value
    // exprs. The separator is `->` (not `:`) so a symbol key keeps its colons —
    // `#x:` is a legitimate keyword-selector symbol, and `{#x: -> 1}` is
    // unambiguous where `{#x: : 1}` would not be.
    defRule("mapEntry", seqN(&.{ rref("expr"), ws, lit("->"), ws, rref("expr") }));
    defRule("mapLit", seqN(&.{
        lit("{"), ws,
        star(seqN(&.{ rref("mapEntry"), ws, optp(lit(",")), ws })),
        lit("}"),
    }));

    // logicVar = '?' ident   — a datalog query variable (#[?p …]); evaluates to a
    // canonical LogicVar object (codegen lowers it to `[LogicVar named: #p]`).
    defRule("logicVar", seqN(&.{ lit("?"), rref("ident") }));

    // expr = block / send / vectorLit / mapLit / logicVar / primary
    // (block before send: both open '['; vectorLit opens '#[' so it must beat the
    // symbol rule inside primary, which also opens '#'. '?' is unambiguous.)
    defRule("expr", altN(&.{ rref("block"), rref("send"), rref("vectorLit"), rref("mapLit"), rref("logicVar"), rref("primary") }));

    // letBinding = 'let' !alnum ident ':=' expr
    defRule("letBinding", seqN(&.{
        lit("let"), notp(alnum), ws,
        rref("ident"), ws, lit(":="), ws, rref("expr"),
    }));

    // statement = letBinding / expr ; statements = statement (';' statement)* ';'?
    defRule("statement", altN(&.{ rref("letBinding"), rref("expr") }));
    defRule("statements", seqN(&.{
        rref("statement"),
        star(seqN(&.{ ws, lit(";"), ws, rref("statement") })),
        optp(seqN(&.{ ws, lit(";") })),
    }));

    // program = ws statements? ws eof
    defRule("program", seqN(&.{ ws, optp(rref("statements")), ws, eofc() }));
}

// -- the Lang facade ---------------------------------------------------------

/// `[Lang parse: aString]` — parse birnam source; returns the CST root or nil.
fn mParse(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return send2(grammar, "parse:from:", a[0], g_api.intern("program"));
}

/// `[Lang grammar]` — the birnam Grammar object (for inspection / extension).
fn mGrammar(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return grammar;
}

/// `[Lang furthestFail]` — the furthest source offset the most recent `parse:`
/// reached before failing (for locating a syntax error). Only meaningful after a
/// `parse:` that returned nil.
fn mFurthestFail(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.send(grammar, g_api.intern("furthestFail"), &[_]Oop{}, 0);
}

// -- bringup / module entrypoint --------------------------------------------

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    peg = api.import("Peg") orelse @panic("lang: Peg not loaded — load the peg module first");

    const reloading = api.isExported("Lang") != 0;
    if (reloading) {
        lang_vt = api.import("LangVt");
        lang_obj = api.import("Lang");
    } else {
        lang_vt = api.proto(api.object_vt);
        lang_obj = api.alloc(lang_vt, 0);
    }

    api.method(lang_vt, api.intern("parse:"), &mParse);
    api.method(lang_vt, api.intern("grammar"), &mGrammar);
    api.method(lang_vt, api.intern("furthestFail"), &mFurthestFail);

    buildGrammar();
    // Root the grammar (F3): a Zig module global is NOT a GC root, so without
    // this export a collection would sweep the grammar object and the next
    // parse would be a use-after-free (the env's values are scanned as roots;
    // peg's trace hooks carry reachability on to the rules and patterns).
    // Re-exported on every bringUp so a reload re-points it at the fresh build.
    _ = api.exportObj("BirnamGrammar", grammar);

    if (!reloading) {
        _ = api.exportObj("LangVt", lang_vt);
        _ = api.exportObj("Lang", lang_obj);
    }
}

fn moduleInit(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}
// Export the dylib entrypoint only in non-test builds (see peg.zig).
comptime {
    if (!@import("builtin").is_test) @export(&moduleInit, .{ .name = "module_init" });
}
