//! Lang — Phase 10.1.b dylib module.
//!
//! Asserts the birnam grammar (~G2 lockset) as PEG facts at module-init,
//! via send-dispatched calls to PEG_VT / GRAMMAR_VT methods (installed by
//! the PEG dylib in 10.0 step 4+5). The dylib's reason to exist is to:
//!   - own grammar-fact provenance (uses its module-manager-allocated
//!     module_eid for the `m` column on every grammar assertion)
//!   - mint the birnam Grammar entity once per image
//!   - export `grammar` on the module entity so user code reaches it via
//!     `[lang grammar]` (Phase 10.2 parser bootstrap will read it)
//!
//! Scope of this commit (10.1.b minimal subset):
//!   - whitespace + comments skip rule
//!   - digit, letter, alphanumeric, identifier
//!   - integer literal, string literal
//!   - nil / true / false keyword literals
//!   - simple atom = literal | identifier
//!
//! Phase 10.1.b follow-ups: bracket message-sends (unary/binary/keyword),
//! blocks `[:x | body]`, pattern-sends `[?e :a ?v]`, statement separators,
//! `^` return, `:=` assignment, `let` binding. Right-recursive rewrite
//! per X4 for binary chains.
//!
//! IdApi layout MUST stay layout-compatible with src/kernel/id_api.zig
//! (Phase 9.5.5 ABI discipline — fields append at end of struct).

const std = @import("std");

const Ref = u64;
const NIL: Ref = 0b010;

fn mkEid(eid: u64) Ref {
    return (eid << 3) | 0b010;
}

// Kernel-fixed EIDs (mirror src/kernel/kernel_eids.zig).
const PEG_EID: u64 = 99;

// AST kind vts + binding/scope vts + singletons.
const AST_LITERAL_VT: u64 = 105;
const AST_VAR_REF_VT: u64 = 106;
const AST_SELF_VT: u64 = 107;
const AST_KEYWORD_SEND_VT: u64 = 108;
const AST_UNARY_SEND_VT: u64 = 109;
const AST_BINARY_SEND_VT: u64 = 110;
const SMALLINT_VT: u64 = 68;
const TRUE_EID: u64 = 72;
const FALSE_EID: u64 = 73;

const ATTR_AST_VALUE: u64 = 118;
const ATTR_AST_NAME: u64 = 119;
const ATTR_AST_RECEIVER: u64 = 120;
const ATTR_AST_SELECTOR: u64 = 121;
const ATTR_AST_FIRST_ARG: u64 = 122;
const ATTR_AST_NEXT_SIBLING: u64 = 93;
const ATTR_AST_FIRST_CHILD: u64 = 92;
const ATTR_AST_RULE_NAME: u64 = 91;
const ATTR_AST_BYTE_START: u64 = 94;
const ATTR_AST_BYTE_END: u64 = 95;
const ATTR_VTABLE: u64 = 19;
const ATTR_DELEGATE: u64 = 22;

const ShellContext = extern struct {
    log: *const fn ([*:0]const u8) callconv(.c) void,
    set_repl_handler: *const fn (?*const fn ([*:0]const u8) callconv(.c) void) callconv(.c) void,
};

const FactCallback = fn (e: u64, a: u64, v: Ref, ctx: ?*anyopaque) callconv(.c) void;

const IdApi = extern struct {
    nil_eid: u64,
    vtable_vt_eid: u64,
    object_vt_eid: u64,
    selector_vt_eid: u64,
    closure_vt_eid: u64,
    nil_vt_eid: u64,

    sel_lookup: u64,
    sel_methodAtPutWith: u64,
    sel_delegated: u64,
    sel_init: u64,
    sel_allocate: u64,
    sel_vtable: u64,

    attr_vtable: u64,
    attr_delegate: u64,
    attr_name: u64,
    attr_module: u64,

    alloc_eid: *const fn () callconv(.c) u64,
    promote: *const fn (u64, Ref) callconv(.c) void,
    demote: *const fn (u64) callconv(.c) void,
    alloc_object: *const fn (Ref, usize) callconv(.c) ?*anyopaque,
    object_to_ref: *const fn (?*anyopaque) callconv(.c) Ref,
    make_closure: *const fn (?*const anyopaque, Ref) callconv(.c) u64,

    intern: *const fn ([*]const u8, usize) callconv(.c) u64,
    make_string: *const fn ([*]const u8, usize) callconv(.c) Ref,

    assert_fact: *const fn (u64, u64, Ref, u64) callconv(.c) u64,
    retract_fact: *const fn (u64, u64, Ref, u64) callconv(.c) u64,
    query_eavt: *const fn (u64, u64) callconv(.c) Ref,
    facts_of_module: *const fn (u64, *const FactCallback, ?*anyopaque) callconv(.c) usize,

    send0: *const fn (Ref, u64) callconv(.c) Ref,
    send1: *const fn (Ref, u64, Ref) callconv(.c) Ref,
    send2: *const fn (Ref, u64, Ref, Ref) callconv(.c) Ref,
    send3: *const fn (Ref, u64, Ref, Ref, Ref) callconv(.c) Ref,

    slot_at: *const fn (?*anyopaque, usize) callconv(.c) Ref,
    set_slot: *const fn (?*anyopaque, usize, Ref) callconv(.c) void,

    make_query: *const fn () callconv(.c) ?*anyopaque,
    free_query: *const fn (?*anyopaque) callconv(.c) void,
    query_variable: *const fn (?*anyopaque, [*]const u8, usize) callconv(.c) u32,
    query_add_triple: *const fn (?*anyopaque, u32, u64, u32, u64, u32, u64) callconv(.c) void,
    query_add_predicate: *const fn (?*anyopaque, u32, u32, u64, u32, u64) callconv(.c) void,
    query_add_find: *const fn (?*anyopaque, u32) callconv(.c) void,
    query_set_asof: *const fn (?*anyopaque, u64) callconv(.c) void,
    query_set_in_module: *const fn (?*anyopaque, u64) callconv(.c) void,

    query_execute: *const fn (?*anyopaque) callconv(.c) ?*anyopaque,
    result_count: *const fn (?*anyopaque) callconv(.c) u64,
    result_arity: *const fn (?*anyopaque) callconv(.c) u32,
    result_at: *const fn (?*anyopaque, u64, u32) callconv(.c) Ref,
    free_result: *const fn (?*anyopaque) callconv(.c) void,

    query_set_since: *const fn (?*anyopaque, u64) callconv(.c) void,
    query_set_between: *const fn (?*anyopaque, u64, u64) callconv(.c) void,
    query_set_max_iter: *const fn (?*anyopaque, u32) callconv(.c) void,
    query_set_max_tuples: *const fn (?*anyopaque, u64) callconv(.c) void,
    query_begin_negation: *const fn (?*anyopaque) callconv(.c) void,
    query_end_negation: *const fn (?*anyopaque) callconv(.c) void,
    query_begin_disjunction: *const fn (?*anyopaque) callconv(.c) void,
    query_begin_branch: *const fn (?*anyopaque) callconv(.c) void,
    query_end_branch: *const fn (?*anyopaque) callconv(.c) void,
    query_end_disjunction: *const fn (?*anyopaque) callconv(.c) void,
    query_add_rule_invocation: *const fn (?*anyopaque, u64, usize, [*]const u32, [*]const u64) callconv(.c) void,
    define_rule: *const fn (?*anyopaque, u64, u32, u32) callconv(.c) bool,
    query_use_global_rules: *const fn (?*anyopaque) callconv(.c) void,
    query_add_find_aggregate: *const fn (?*anyopaque, u32, u32) callconv(.c) void,
    rrb_count: *const fn (?*anyopaque) callconv(.c) u64,
    rrb_at: *const fn (?*anyopaque, u64) callconv(.c) Ref,

    save_query: *const fn ([*]const u8, usize, ?*anyopaque) callconv(.c) u64,
    run_query: *const fn ([*]const u8, usize) callconv(.c) ?*anyopaque,

    query_add_pred_call: *const fn (?*anyopaque, u64, usize, [*]const u32, [*]const u64) callconv(.c) void,
    query_add_fn_call: *const fn (?*anyopaque, u64, usize, [*]const u32, [*]const u64, u32) callconv(.c) void,
    register_pred: *const fn (u64, ?*const anyopaque) callconv(.c) u64,
    register_fn: *const fn (u64, ?*const anyopaque) callconv(.c) u64,

    promote_eid: *const fn (u64) callconv(.c) bool,
    demote_eid: *const fn (u64) callconv(.c) void,
    hotness_of: *const fn (u64) callconv(.c) u32,
    is_materialized: *const fn (u64) callconv(.c) bool,
    ffi_enter: *const fn () callconv(.c) void,
    ffi_exit: *const fn () callconv(.c) void,

    peg_install_send_api: *const fn (u64) callconv(.c) void,

    // Phase 10.1.b procedural escape hatch — bootstrap-only, see
    // ADR-10-Send-Arity in ROADMAP. User-facing seq/choice/charset land
    // via the send-dispatched surface once Phase 10.2.5 (sendN) ships.
    peg_mk_seq: *const fn ([*]const u64, usize) callconv(.c) u64,
    peg_mk_choice: *const fn ([*]const u64, usize) callconv(.c) u64,
    peg_mk_charset: *const fn ([*]const u8, usize) callconv(.c) u64,

    sendN: *const fn (Ref, u64, [*]const Ref, usize) callconv(.c) Ref,
};

// -- Module-static state -----------------------------------------------------

var g_api: ?*const IdApi = null;
var g_log: ?*const fn ([*:0]const u8) callconv(.c) void = null;
var g_module_eid: u64 = 0;
var g_grammar_eid: u64 = 0;
var g_source: []const u8 = "";

// Cached rule-name EIDs (interned at module init).
var rn_integer: u64 = 0;
var rn_string: u64 = 0;
var rn_kw_nil: u64 = 0;
var rn_kw_true: u64 = 0;
var rn_kw_false: u64 = 0;
var rn_kw_self: u64 = 0;
var rn_identifier: u64 = 0;
var rn_atom: u64 = 0;
var rn_expression: u64 = 0;
var rn_bracket_form: u64 = 0;
var rn_bracket_unary: u64 = 0;
var rn_bracket_keyword: u64 = 0;
var rn_unary_tail: u64 = 0;
var rn_kw_arg: u64 = 0;
var rn_kw_suffix: u64 = 0;
var rn_start: u64 = 0;
var rn_ws: u64 = 0;

// Cached selector EIDs.
var sel_new_grammar: u64 = 0;
var sel_literal: u64 = 0;
var sel_rule_ref: u64 = 0;
var sel_any_byte: u64 = 0;
var sel_eof: u64 = 0;
var sel_star: u64 = 0;
var sel_plus: u64 = 0;
var sel_opt: u64 = 0;
var sel_and_p: u64 = 0;
var sel_not_p: u64 = 0;
var sel_seq: u64 = 0;
var sel_choice: u64 = 0;
var sel_charset: u64 = 0;
var sel_define_rule_as: u64 = 0;

fn internSel(comptime name: []const u8) u64 {
    return g_api.?.intern(name.ptr, name.len);
}

fn mkS(comptime s: []const u8) Ref {
    return g_api.?.make_string(s.ptr, s.len);
}

// -- PEG-send helpers --------------------------------------------------------

fn peg_literal(s: []const u8) Ref {
    const api = g_api.?;
    return api.send1(mkEid(PEG_EID), sel_literal, api.make_string(s.ptr, s.len));
}

fn peg_rule_ref(name: []const u8) Ref {
    const api = g_api.?;
    return api.send1(mkEid(PEG_EID), sel_rule_ref, api.make_string(name.ptr, name.len));
}

fn peg_any() Ref {
    return g_api.?.send0(mkEid(PEG_EID), sel_any_byte);
}

fn peg_eof() Ref {
    return g_api.?.send0(mkEid(PEG_EID), sel_eof);
}

fn peg_star(child: Ref) Ref {
    return g_api.?.send1(mkEid(PEG_EID), sel_star, child);
}

fn peg_plus(child: Ref) Ref {
    return g_api.?.send1(mkEid(PEG_EID), sel_plus, child);
}

fn peg_opt(child: Ref) Ref {
    return g_api.?.send1(mkEid(PEG_EID), sel_opt, child);
}

fn peg_not(child: Ref) Ref {
    return g_api.?.send1(mkEid(PEG_EID), sel_not_p, child);
}

fn defineRule(grammar: Ref, name: []const u8, pat: Ref) void {
    const api = g_api.?;
    _ = api.send2(grammar, sel_define_rule_as, api.make_string(name.ptr, name.len), pat);
}

// Phase 10.2.5: seq/choice/charset now go through sendN against PEG_VT.
// (The peg_mk_* IdApi entries remain available but are deprecated — see
// ADR-10-Send-Arity resolution note in ROADMAP.)

fn mkSmallInt(n: i64) Ref {
    return @as(u64, @bitCast(n << 3)) | 0b001;
}

fn peg_seq(children: []const u64) Ref {
    const api = g_api.?;
    // Convert child EIDs to EID-tagged Refs in a stack buffer.
    var buf: [16]Ref = undefined;
    if (children.len > buf.len) return NIL;
    for (children, 0..) |c, i| buf[i] = mkEid(c);
    return api.sendN(mkEid(PEG_EID), sel_seq, &buf, children.len);
}

fn peg_choice(children: []const u64) Ref {
    const api = g_api.?;
    var buf: [16]Ref = undefined;
    if (children.len > buf.len) return NIL;
    for (children, 0..) |c, i| buf[i] = mkEid(c);
    return api.sendN(mkEid(PEG_EID), sel_choice, &buf, children.len);
}

fn peg_charset(ranges: []const u8) Ref {
    const api = g_api.?;
    // charset takes lo/hi pairs as SmallInts.
    var buf: [32]Ref = undefined;
    if (ranges.len > buf.len) return NIL;
    for (ranges, 0..) |b, i| buf[i] = mkSmallInt(@intCast(b));
    return api.sendN(mkEid(PEG_EID), sel_charset, &buf, ranges.len);
}

fn untagEid(r: Ref) u64 {
    return r >> 3;
}

// -- Birnam grammar construction --------------------------------------------

fn buildGrammar() void {
    const api = g_api.?;
    const grammar = api.send0(mkEid(PEG_EID), sel_new_grammar);
    g_grammar_eid = untagEid(grammar);

    // -- Primitives -------------------------------------------------------
    const ws_ranges = [_]u8{ ' ', ' ', '\t', '\t', '\n', '\n', '\r', '\r' };
    defineRule(grammar, "whitespace-char", peg_charset(&ws_ranges));
    defineRule(grammar, "ws", peg_star(peg_rule_ref("whitespace-char")));

    const digit_ranges = [_]u8{ '0', '9' };
    defineRule(grammar, "digit", peg_charset(&digit_ranges));

    const letter_ranges = [_]u8{ 'a', 'z', 'A', 'Z', '_', '_' };
    defineRule(grammar, "letter", peg_charset(&letter_ranges));

    {
        const alts = [_]u64{ untagEid(peg_rule_ref("letter")), untagEid(peg_rule_ref("digit")) };
        defineRule(grammar, "alphanumeric", peg_choice(&alts));
    }

    {
        const parts = [_]u64{
            untagEid(peg_rule_ref("letter")),
            untagEid(peg_star(peg_rule_ref("alphanumeric"))),
        };
        defineRule(grammar, "identifier", peg_seq(&parts));
    }

    defineRule(grammar, "integer", peg_plus(peg_rule_ref("digit")));

    // -- String literal: "..." (no escapes for spike) ---------------------
    // string = '"' (!'"' any)* '"'
    {
        const quote_char = [_]u8{ '"', '"' };
        const quote = peg_charset(&quote_char);
        const not_quote = peg_not(peg_charset(&quote_char));
        const body_step = [_]u64{
            untagEid(not_quote),
            untagEid(peg_any()),
        };
        const body = peg_seq(&body_step);
        const body_star = peg_star(body);
        const string_parts = [_]u64{
            untagEid(quote),
            untagEid(body_star),
            untagEid(peg_charset(&quote_char)),
        };
        defineRule(grammar, "string", peg_seq(&string_parts));
    }

    // -- Reserved-word literals (G4) with !alphanumeric tail --------------
    // Each kw rule = literal "X" !alphanumeric. Ensures "true2" is an
    // identifier, not a true literal followed by 2.
    inline for ([_][]const u8{ "nil", "true", "false", "self" }) |kw| {
        const parts = [_]u64{
            untagEid(peg_literal(kw)),
            untagEid(peg_not(peg_rule_ref("alphanumeric"))),
        };
        defineRule(grammar, "kw-" ++ kw, peg_seq(&parts));
    }

    // -- Atom = literal-shaped atomic value -------------------------------
    // atom = integer | string | kw-nil | kw-true | kw-false | kw-self
    //      | identifier
    // Ordered choice — reserved words first so "self" doesn't get eaten as
    // identifier.
    {
        const atom_alts = [_]u64{
            untagEid(peg_rule_ref("integer")),
            untagEid(peg_rule_ref("string")),
            untagEid(peg_rule_ref("kw-nil")),
            untagEid(peg_rule_ref("kw-true")),
            untagEid(peg_rule_ref("kw-false")),
            untagEid(peg_rule_ref("kw-self")),
            untagEid(peg_rule_ref("identifier")),
        };
        defineRule(grammar, "atom", peg_choice(&atom_alts));
    }

    // -- Bracket forms ----------------------------------------------------
    // kw-suffix = identifier ":"
    {
        const parts = [_]u64{
            untagEid(peg_rule_ref("identifier")),
            untagEid(peg_literal(":")),
        };
        defineRule(grammar, "kw-suffix", peg_seq(&parts));
    }

    // kw-arg = ws kw-suffix ws expression
    {
        const parts = [_]u64{
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("kw-suffix")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("expression")),
        };
        defineRule(grammar, "kw-arg", peg_seq(&parts));
    }

    // unary-tail = ws identifier (!":" so we don't eat a kw-suffix)
    {
        const parts = [_]u64{
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("identifier")),
            untagEid(peg_not(peg_literal(":"))),
        };
        defineRule(grammar, "unary-tail", peg_seq(&parts));
    }

    // bracket-keyword = "[" ws expression kw-arg+ ws "]"
    {
        const parts = [_]u64{
            untagEid(peg_literal("[")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("expression")),
            untagEid(peg_plus(peg_rule_ref("kw-arg"))),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_literal("]")),
        };
        defineRule(grammar, "bracket-keyword", peg_seq(&parts));
    }

    // bracket-unary = "[" ws expression unary-tail+ ws "]"
    {
        const parts = [_]u64{
            untagEid(peg_literal("[")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("expression")),
            untagEid(peg_plus(peg_rule_ref("unary-tail"))),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_literal("]")),
        };
        defineRule(grammar, "bracket-unary", peg_seq(&parts));
    }

    // bracket-form = bracket-keyword | bracket-unary
    {
        const alts = [_]u64{
            untagEid(peg_rule_ref("bracket-keyword")),
            untagEid(peg_rule_ref("bracket-unary")),
        };
        defineRule(grammar, "bracket-form", peg_choice(&alts));
    }

    // expression = bracket-form | atom (ordered: try the heavier form first)
    {
        const alts = [_]u64{
            untagEid(peg_rule_ref("bracket-form")),
            untagEid(peg_rule_ref("atom")),
        };
        defineRule(grammar, "expression", peg_choice(&alts));
    }

    // start = ws expression ws eof
    {
        const start_parts = [_]u64{
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("expression")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_eof()),
        };
        defineRule(grammar, "start", peg_seq(&start_parts));
    }
}

// -- Desugar (CST → typed AST) -----------------------------------------------

fn cstRuleName(cst_eid: u64) u64 {
    const api = g_api.?;
    const ref = api.query_eavt(cst_eid, ATTR_AST_RULE_NAME);
    if ((ref & 0b111) != 0b010) return 0;
    return ref >> 3;
}

fn cstByteRange(cst_eid: u64) struct { start: usize, end: usize } {
    const api = g_api.?;
    const sr = api.query_eavt(cst_eid, ATTR_AST_BYTE_START);
    const er = api.query_eavt(cst_eid, ATTR_AST_BYTE_END);
    const start: usize = @intCast(untagSmallInt(sr));
    const end: usize = @intCast(untagSmallInt(er));
    return .{ .start = start, .end = end };
}

fn untagSmallInt(r: Ref) i64 {
    return @as(i64, @bitCast(r)) >> 3;
}

fn cstFirstChild(cst_eid: u64) u64 {
    const ref = g_api.?.query_eavt(cst_eid, ATTR_AST_FIRST_CHILD);
    if ((ref & 0b111) != 0b010) return 0;
    return ref >> 3;
}

fn cstNextSibling(cst_eid: u64) u64 {
    const ref = g_api.?.query_eavt(cst_eid, ATTR_AST_NEXT_SIBLING);
    if ((ref & 0b111) != 0b010) return 0;
    return ref >> 3;
}

fn newAstNode(kind_vt: u64, byte_start: usize, byte_end: usize) u64 {
    const api = g_api.?;
    const eid = api.alloc_eid();
    _ = api.assert_fact(eid, ATTR_VTABLE, mkEid(kind_vt), g_module_eid);
    _ = api.assert_fact(eid, ATTR_AST_BYTE_START, mkSmallInt(@intCast(byte_start)), g_module_eid);
    _ = api.assert_fact(eid, ATTR_AST_BYTE_END, mkSmallInt(@intCast(byte_end)), g_module_eid);
    return eid;
}

/// Walk a CST node, return a typed AST node EID.
fn desugar(cst_eid: u64) u64 {
    const api = g_api.?;
    const rn = cstRuleName(cst_eid);
    const span = cstByteRange(cst_eid);

    if (rn == rn_integer) {
        // Parse digits between span.start..span.end.
        const bytes = g_source[span.start..span.end];
        var n: i64 = 0;
        for (bytes) |c| n = n * 10 + (c - '0');
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, mkSmallInt(n), g_module_eid);
        return ast;
    }
    if (rn == rn_string) {
        // Strip surrounding quotes — span.start+1 .. span.end-1.
        const bytes = g_source[span.start + 1 .. span.end - 1];
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, api.make_string(bytes.ptr, bytes.len), g_module_eid);
        return ast;
    }
    if (rn == rn_kw_nil) {
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, NIL, g_module_eid);
        return ast;
    }
    if (rn == rn_kw_true) {
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, mkEid(TRUE_EID), g_module_eid);
        return ast;
    }
    if (rn == rn_kw_false) {
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, mkEid(FALSE_EID), g_module_eid);
        return ast;
    }
    if (rn == rn_kw_self) {
        return newAstNode(AST_SELF_VT, span.start, span.end);
    }
    if (rn == rn_identifier) {
        const bytes = g_source[span.start..span.end];
        const name_eid = api.intern(bytes.ptr, bytes.len);
        const ast = newAstNode(AST_VAR_REF_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_NAME, mkEid(name_eid), g_module_eid);
        return ast;
    }
    if (rn == rn_atom or rn == rn_expression or rn == rn_bracket_form or rn == rn_start) {
        const child = cstFirstChild(cst_eid);
        // For start: children are ws, expression, ws (ws-CST). Skip ws-shaped children.
        var c = child;
        while (c != 0) : (c = cstNextSibling(c)) {
            const crn = cstRuleName(c);
            if (crn == rn_ws) continue;
            return desugar(c);
        }
        // Fallback: literal NIL node.
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, NIL, g_module_eid);
        return ast;
    }
    if (rn == rn_bracket_unary) {
        // Children include: expression (receiver) + N unary-tail entries
        // (each containing a ws + identifier). Walk children, ignore ws.
        var c = cstFirstChild(cst_eid);
        // Find the receiver (first expression).
        var receiver_ast: u64 = 0;
        while (c != 0) : (c = cstNextSibling(c)) {
            const crn = cstRuleName(c);
            if (crn == rn_ws) continue;
            if (crn == rn_expression) {
                receiver_ast = desugar(c);
                c = cstNextSibling(c);
                break;
            }
        }
        // Now iterate unary-tails — each contributes a selector name. Fold
        // left so chained `[1 succ squared]` = (1 succ) squared.
        while (c != 0) : (c = cstNextSibling(c)) {
            const crn = cstRuleName(c);
            if (crn != rn_unary_tail) continue;
            // unary-tail's children: identifier (the selector name).
            const tail_first = cstFirstChild(c);
            var tc = tail_first;
            while (tc != 0) : (tc = cstNextSibling(tc)) {
                if (cstRuleName(tc) == rn_identifier) {
                    const id_span = cstByteRange(tc);
                    const sel_bytes = g_source[id_span.start..id_span.end];
                    const sel_eid = api.intern(sel_bytes.ptr, sel_bytes.len);
                    const send_ast = newAstNode(AST_UNARY_SEND_VT, span.start, id_span.end);
                    _ = api.assert_fact(send_ast, ATTR_AST_RECEIVER, mkEid(receiver_ast), g_module_eid);
                    _ = api.assert_fact(send_ast, ATTR_AST_SELECTOR, mkEid(sel_eid), g_module_eid);
                    receiver_ast = send_ast;
                    break;
                }
            }
        }
        return receiver_ast;
    }
    if (rn == rn_bracket_keyword) {
        var c = cstFirstChild(cst_eid);
        var receiver_ast: u64 = 0;
        while (c != 0) : (c = cstNextSibling(c)) {
            const crn = cstRuleName(c);
            if (crn == rn_ws) continue;
            if (crn == rn_expression) {
                receiver_ast = desugar(c);
                c = cstNextSibling(c);
                break;
            }
        }
        // Build compound selector + args list from kw-arg children.
        // Each kw-arg's children: ws, kw-suffix, ws, expression.
        // kw-suffix's children: identifier (+ literal ":").
        var sel_buf: [128]u8 = undefined;
        var sel_len: usize = 0;
        var first_arg_ast: u64 = 0;
        var last_arg_ast: u64 = 0;
        while (c != 0) : (c = cstNextSibling(c)) {
            if (cstRuleName(c) != rn_kw_arg) continue;
            // Walk kw-arg's children.
            var ac = cstFirstChild(c);
            var arg_ast: u64 = 0;
            while (ac != 0) : (ac = cstNextSibling(ac)) {
                const acrn = cstRuleName(ac);
                if (acrn == rn_kw_suffix) {
                    // suffix-id is the first identifier child of kw-suffix.
                    var sc = cstFirstChild(ac);
                    while (sc != 0) : (sc = cstNextSibling(sc)) {
                        if (cstRuleName(sc) == rn_identifier) {
                            const id_span = cstByteRange(sc);
                            const name_bytes = g_source[id_span.start..id_span.end];
                            if (sel_len + name_bytes.len + 1 < sel_buf.len) {
                                @memcpy(sel_buf[sel_len .. sel_len + name_bytes.len], name_bytes);
                                sel_len += name_bytes.len;
                                sel_buf[sel_len] = ':';
                                sel_len += 1;
                            }
                            break;
                        }
                    }
                } else if (acrn == rn_expression) {
                    arg_ast = desugar(ac);
                }
            }
            if (arg_ast != 0) {
                if (first_arg_ast == 0) {
                    first_arg_ast = arg_ast;
                } else {
                    _ = api.assert_fact(last_arg_ast, ATTR_AST_NEXT_SIBLING, mkEid(arg_ast), g_module_eid);
                }
                last_arg_ast = arg_ast;
            }
        }
        const sel_eid = api.intern(sel_buf[0..sel_len].ptr, sel_len);
        const send_ast = newAstNode(AST_KEYWORD_SEND_VT, span.start, span.end);
        _ = api.assert_fact(send_ast, ATTR_AST_RECEIVER, mkEid(receiver_ast), g_module_eid);
        _ = api.assert_fact(send_ast, ATTR_AST_SELECTOR, mkEid(sel_eid), g_module_eid);
        if (first_arg_ast != 0) {
            _ = api.assert_fact(send_ast, ATTR_AST_FIRST_ARG, mkEid(first_arg_ast), g_module_eid);
        }
        return send_ast;
    }
    // Unknown rule — fallback to NIL literal.
    const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
    _ = api.assert_fact(ast, ATTR_AST_VALUE, NIL, g_module_eid);
    return ast;
}

fn cacheRuleNames() void {
    const api = g_api.?;
    rn_integer = api.intern("integer".ptr, 7);
    rn_string = api.intern("string".ptr, 6);
    rn_kw_nil = api.intern("kw-nil".ptr, 6);
    rn_kw_true = api.intern("kw-true".ptr, 7);
    rn_kw_false = api.intern("kw-false".ptr, 8);
    rn_kw_self = api.intern("kw-self".ptr, 7);
    rn_identifier = api.intern("identifier".ptr, 10);
    rn_atom = api.intern("atom".ptr, 4);
    rn_expression = api.intern("expression".ptr, 10);
    rn_bracket_form = api.intern("bracket-form".ptr, 12);
    rn_bracket_unary = api.intern("bracket-unary".ptr, 13);
    rn_bracket_keyword = api.intern("bracket-keyword".ptr, 15);
    rn_unary_tail = api.intern("unary-tail".ptr, 10);
    rn_kw_arg = api.intern("kw-arg".ptr, 6);
    rn_kw_suffix = api.intern("kw-suffix".ptr, 9);
    rn_start = api.intern("start".ptr, 5);
    rn_ws = api.intern("ws".ptr, 2);
}

// -- Eval (Phase 10.3 minimal) -----------------------------------------------

const Send = extern struct {
    selector: u64,
    receiver: Ref,
    n_args: u8,
    state: Ref,
    closure: ?*anyopaque,
};

var sel_eval: u64 = 0;

fn evalLiteralImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const api = g_api.?;
    if (!isEid(s.receiver)) return NIL;
    return api.query_eavt(s.receiver >> 3, ATTR_AST_VALUE);
}

fn evalSelfImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    return s.state;
}

fn evalUnarySendImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const api = g_api.?;
    if (!isEid(s.receiver)) return NIL;
    const ast_eid = s.receiver >> 3;
    const recv_ast_ref = api.query_eavt(ast_eid, ATTR_AST_RECEIVER);
    const sel_ref = api.query_eavt(ast_eid, ATTR_AST_SELECTOR);
    if (!isEid(sel_ref)) return NIL;
    const sel_eid = sel_ref >> 3;
    // Eval the receiver AST.
    const recv_val = api.send0(recv_ast_ref, sel_eval);
    return api.send0(recv_val, sel_eid);
}

fn evalKeywordSendImpl(s: *Send, args_in: [*]const Ref, n_args_in: usize) callconv(.c) Ref {
    _ = args_in;
    _ = n_args_in;
    const api = g_api.?;
    if (!isEid(s.receiver)) return NIL;
    const ast_eid = s.receiver >> 3;
    const recv_ast_ref = api.query_eavt(ast_eid, ATTR_AST_RECEIVER);
    const sel_ref = api.query_eavt(ast_eid, ATTR_AST_SELECTOR);
    if (!isEid(sel_ref)) return NIL;
    const sel_eid = sel_ref >> 3;
    const recv_val = api.send0(recv_ast_ref, sel_eval);
    // Collect args.
    var buf: [16]Ref = undefined;
    var count: usize = 0;
    var cur_ref = api.query_eavt(ast_eid, ATTR_AST_FIRST_ARG);
    while (isEid(cur_ref) and count < buf.len) : (count += 1) {
        const ast_arg = cur_ref >> 3;
        buf[count] = api.send0(cur_ref, sel_eval);
        cur_ref = api.query_eavt(ast_arg, ATTR_AST_NEXT_SIBLING);
    }
    return api.sendN(recv_val, sel_eid, &buf, count);
}

fn isEid(r: Ref) bool {
    return (r & 0b111) == 0b010;
}

// -- SmallInt primitives installed by lang dylib --------------------------

fn smallintNegateImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const n = untagSmallInt(s.receiver);
    return mkSmallInt(-n);
}

fn smallintSquaredImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const n = untagSmallInt(s.receiver);
    return mkSmallInt(n * n);
}

fn smallintSuccImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const n = untagSmallInt(s.receiver);
    return mkSmallInt(n + 1);
}

fn smallintPlusImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const a_ = untagSmallInt(s.receiver);
    const b_ = untagSmallInt(args[0]);
    return mkSmallInt(a_ + b_);
}

fn smallintTimesImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const a_ = untagSmallInt(s.receiver);
    const b_ = untagSmallInt(args[0]);
    return mkSmallInt(a_ * b_);
}

// -- parseExpr method ------------------------------------------------------

fn parseExprImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = n_args;
    const api = g_api.?;
    const src_ref = args[0];
    if (!isString(src_ref)) return NIL;
    const src = stringBytes(src_ref);
    g_source = src;

    // Parse via PEG.
    const src_dup = api.make_string(src.ptr, src.len);
    const rule = api.make_string("start".ptr, 5);
    const cst_result = api.send2(mkEid(g_grammar_eid), internSel("parse:from:"), src_dup, rule);
    if (!isEid(cst_result)) return NIL;
    const cst_eid = cst_result >> 3;
    // Check it's not an ERROR.
    const vt = api.query_eavt(cst_eid, ATTR_VTABLE);
    if (vt != NIL) return cst_result; // pass-through ERROR_VT
    return mkEid(desugar(cst_eid));
}

fn isString(r: Ref) bool {
    return (r & 0b111) == 0b100;
}

const StringHeader = extern struct {
    len: u64,
};

fn stringBytes(r: Ref) []const u8 {
    const addr = r & ~@as(u64, 0b111);
    const header: *const StringHeader = @ptrFromInt(addr);
    const payload: [*]const u8 = @as([*]const u8, @ptrCast(header)) + @sizeOf(StringHeader);
    return payload[0..header.len];
}

fn installMethods() void {
    const api = g_api.?;
    sel_eval = api.intern("eval".ptr, 4);

    // Install eval methods on each AST kind-vt.
    const sel_eval_eid = sel_eval;
    _ = api.assert_fact(AST_LITERAL_VT, sel_eval_eid, mkEid(api.make_closure(@ptrCast(&evalLiteralImpl), NIL)), g_module_eid);
    _ = api.assert_fact(AST_SELF_VT, sel_eval_eid, mkEid(api.make_closure(@ptrCast(&evalSelfImpl), NIL)), g_module_eid);
    _ = api.assert_fact(AST_UNARY_SEND_VT, sel_eval_eid, mkEid(api.make_closure(@ptrCast(&evalUnarySendImpl), NIL)), g_module_eid);
    _ = api.assert_fact(AST_KEYWORD_SEND_VT, sel_eval_eid, mkEid(api.make_closure(@ptrCast(&evalKeywordSendImpl), NIL)), g_module_eid);

    // SmallInt primitives (10.0.x prereq — lang dylib owns the install).
    const negate_sel = api.intern("negate".ptr, 6);
    const squared_sel = api.intern("squared".ptr, 7);
    const succ_sel = api.intern("succ".ptr, 4);
    const plus_sel = api.intern("+".ptr, 1);
    const times_sel = api.intern("*".ptr, 1);
    _ = api.assert_fact(SMALLINT_VT, negate_sel, mkEid(api.make_closure(@ptrCast(&smallintNegateImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, squared_sel, mkEid(api.make_closure(@ptrCast(&smallintSquaredImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, succ_sel, mkEid(api.make_closure(@ptrCast(&smallintSuccImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, plus_sel, mkEid(api.make_closure(@ptrCast(&smallintPlusImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, times_sel, mkEid(api.make_closure(@ptrCast(&smallintTimesImpl), NIL)), g_module_eid);

    // Install parseExpr: on the lang module entity itself so callers
    // reach the desugar pipeline via send.
    const sel_parse_expr = api.intern("parseExpr:".ptr, 10);
    // The module entity needs a vt that responds to parseExpr:. Reuse
    // LANG_VT (singleton). Install on LANG_VT, then assert lang module's
    // :vtable = LANG_VT.
    _ = api.assert_fact(g_module_eid, ATTR_VTABLE, mkEid(LANG_VT), g_module_eid);
    _ = api.assert_fact(LANG_VT, sel_parse_expr, mkEid(api.make_closure(@ptrCast(&parseExprImpl), NIL)), g_module_eid);
}

const LANG_VT: u64 = 103;

// -- Module init -------------------------------------------------------------

export fn birnam_module_init(
    ctx: *ShellContext,
    api: *const IdApi,
    module_eid: u64,
) void {
    g_api = api;
    g_log = ctx.log;
    g_module_eid = module_eid;
    ctx.log("[lang] initializing");

    sel_new_grammar = internSel("newGrammar");
    sel_literal = internSel("literal:");
    sel_rule_ref = internSel("ruleRef:");
    sel_any_byte = internSel("anyByte");
    sel_eof = internSel("eof");
    sel_star = internSel("star:");
    sel_plus = internSel("plus:");
    sel_opt = internSel("opt:");
    sel_and_p = internSel("and:");
    sel_not_p = internSel("not:");
    sel_seq = internSel("seq:");
    sel_choice = internSel("choice:");
    sel_charset = internSel("charset:");
    sel_define_rule_as = internSel("defineRule:as:");

    buildGrammar();
    cacheRuleNames();
    installMethods();

    // Export grammar EID on the module entity: `[<module_eid> grammar]`
    // returns mkEid(g_grammar_eid). Phase 10.2 parser bootstrap reads it.
    const sel_grammar = api.intern("grammar", 7);
    _ = api.assert_fact(module_eid, sel_grammar, mkEid(g_grammar_eid), module_eid);

    ctx.log("[lang] ready");
}
