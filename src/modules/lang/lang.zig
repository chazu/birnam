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
const AST_BLOCK_VT: u64 = 112;
const AST_LET_VT: u64 = 113;
const AST_ASSIGN_VT: u64 = 114;
const AST_STMT_SEQ_VT: u64 = 117;
const ATTR_AST_STMT_FIRST: u64 = 134;
const AST_RETURN_VT: u64 = 115;
const ATTR_AST_RETURN_VALUE: u64 = 128;
const ATTR_AST_BLOCK_BODY: u64 = 124;
const ATTR_AST_LET_INIT: u64 = 125;
const ATTR_AST_ASSIGN_VALUE: u64 = 127;
const CLOSURE_VT: u64 = 4;
const VTABLE_VT: u64 = 1;
const OBJECT_VT: u64 = 2;
const SMALLINT_VT: u64 = 68;
const TRUE_EID: u64 = 72;
const FALSE_EID: u64 = 73;
// Phase 15.R R1d — mirrors of kernel_eids.
const SNAPSHOT_VT: u64 = 195;
const DB_PROTO_VT: u64 = 196;
const DB_PROTO: u64 = 197;
// Phase 16 — RRB-backed vector vtable (mirror of kernel_eids.RRB_VT).
const RRB_VT: u64 = 9;
const ATTR_ERROR_MESSAGE: u64 = 44; // mirror of kernel_eids.ATTR_ERROR_MESSAGE

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
/// Mirror of kernel_eids.M_USER (184) — provenance tag for runtime DATA
/// facts. Phase 12.0 provenance split.
const M_USER: u64 = 184;

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
    object_at_eid: *const fn (u64) callconv(.c) Ref,
    make_float: *const fn (f64) callconv(.c) Ref,

    /// Phase 11.2 — compile a block AST as a bytecode method. Returns
    /// method-EID for installation via [vt sel mkEid(method_eid)].
    /// Append-only ABI discipline (Phase 9.5.5).
    compile_block_as_method: *const fn (block_ast_eid: u64) callconv(.c) u64,

    // -- Phase 15.R R1d: db-value surface (append-only) --------------------
    snapshot_now: *const fn () callconv(.c) Ref,
    snapshot_as_of: *const fn (u64) callconv(.c) Ref,
    snapshot_get: *const fn (Ref, u64, u64) callconv(.c) Ref,
    snapshot_basis: *const fn (Ref) callconv(.c) Ref,
    snapshot_release: *const fn (Ref) callconv(.c) void,
    set_retention: *const fn (u32, u64) callconv(.c) void,
    excise_entity: *const fn (u64) callconv(.c) bool,

    // -- Phase 16 Tier A: vectors (append-only) ----------------------------
    make_rrb: *const fn ([*]const Ref, usize) callconv(.c) Ref,
    rrb_with: *const fn (Ref, Ref) callconv(.c) Ref,
    rrb_len: *const fn (Ref) callconv(.c) u64,
    rrb_nth: *const fn (Ref, u64) callconv(.c) Ref,
};

// -- Module-static state -----------------------------------------------------

var g_api: ?*const IdApi = null;
var g_log: ?*const fn ([*:0]const u8) callconv(.c) void = null;
var g_ctx: ?*ShellContext = null;
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
var rn_symbol_literal: u64 = 0;
var rn_block_bare: u64 = 0;
var rn_block_param: u64 = 0;
var rn_block_params: u64 = 0;
var rn_block_with_params: u64 = 0;
var rn_bracket_binary: u64 = 0;
var rn_vector_literal: u64 = 0;
var rn_let_statement: u64 = 0;
var rn_top_form: u64 = 0;
var rn_assignment: u64 = 0;
var rn_stmt_list: u64 = 0;
var rn_float_literal: u64 = 0;
var rn_return_statement: u64 = 0;

// Phase 10.3 ADR-10-E4 interim — `g_unwinding` side-channel for `^expr`.
// Phase 11 bytecode VM retires this with proper unwind opcodes per
// ROADMAP 11.5. The minimum-viable spike catches at the first
// method-frame shim (where dispatch selector is NOT a value-family
// selector). Inner block invocations (via [block value]) propagate.
var g_unwinding: bool = false;
var g_unwind_value: Ref = 0;

var sel_value_of_g: u64 = 0;
var sel_value_value_g: u64 = 0;

fn isValueSelector(sel: u64) bool {
    return sel == sel_value_g or sel == sel_value_of_g or sel == sel_value_value_g;
}
var ATTR_AST_BLOCK_PARAMS_CONST: u64 = 123;

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

    // line-comment = "//" (!"\n" any)* "\n"?  (trailing newline optional for EOF)
    {
        const not_nl_ranges = [_]u8{ '\n', '\n' };
        const not_nl = peg_not(peg_charset(&not_nl_ranges));
        const step_parts = [_]u64{ untagEid(not_nl), untagEid(peg_any()) };
        const step = peg_seq(&step_parts);
        const newline_or_eof = peg_opt(peg_charset(&not_nl_ranges));
        const parts = [_]u64{
            untagEid(peg_literal("//")),
            untagEid(peg_star(step)),
            untagEid(newline_or_eof),
        };
        defineRule(grammar, "line-comment", peg_seq(&parts));
    }

    // block-comment = "/*" (!"*/" any)* "*/"
    {
        const not_close = peg_not(peg_literal("*/"));
        const step_parts = [_]u64{ untagEid(not_close), untagEid(peg_any()) };
        const step = peg_seq(&step_parts);
        const parts = [_]u64{
            untagEid(peg_literal("/*")),
            untagEid(peg_star(step)),
            untagEid(peg_literal("*/")),
        };
        defineRule(grammar, "block-comment", peg_seq(&parts));
    }

    // ws-token = whitespace-char | line-comment | block-comment
    {
        const alts = [_]u64{
            untagEid(peg_rule_ref("whitespace-char")),
            untagEid(peg_rule_ref("line-comment")),
            untagEid(peg_rule_ref("block-comment")),
        };
        defineRule(grammar, "ws-token", peg_choice(&alts));
    }

    defineRule(grammar, "ws", peg_star(peg_rule_ref("ws-token")));

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

    // float-literal = digit+ "." digit+   (no exponent yet — spike scope)
    {
        const parts = [_]u64{
            untagEid(peg_plus(peg_rule_ref("digit"))),
            untagEid(peg_literal(".")),
            untagEid(peg_plus(peg_rule_ref("digit"))),
        };
        defineRule(grammar, "float-literal", peg_seq(&parts));
    }

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
    inline for ([_][]const u8{ "nil", "true", "false", "self", "let" }) |kw| {
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
    // Note: kw-let is excluded from atom intentionally. `let` is a
    // statement keyword only — appearing in expression position is an
    // error. let-statement is at top-form level.
    {
        const atom_alts = [_]u64{
            untagEid(peg_rule_ref("float-literal")),
            untagEid(peg_rule_ref("integer")),
            untagEid(peg_rule_ref("string")),
            untagEid(peg_rule_ref("symbol-literal")),
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

    // symbol-keyword-tail = identifier ":"
    {
        const parts = [_]u64{
            untagEid(peg_rule_ref("identifier")),
            untagEid(peg_literal(":")),
        };
        defineRule(grammar, "symbol-keyword-tail", peg_seq(&parts));
    }
    // symbol-keyword = symbol-keyword-tail+
    {
        defineRule(grammar, "symbol-keyword", peg_plus(peg_rule_ref("symbol-keyword-tail")));
    }
    // symbol-literal = "#" (symbol-keyword | identifier)
    {
        const inner_alts = [_]u64{
            untagEid(peg_rule_ref("symbol-keyword")),
            untagEid(peg_rule_ref("identifier")),
        };
        const parts = [_]u64{
            untagEid(peg_literal("#")),
            untagEid(peg_choice(&inner_alts)),
        };
        defineRule(grammar, "symbol-literal", peg_seq(&parts));
    }

    // stmt-list = top-form (ws ";" ws top-form)*
    {
        const tail_parts = [_]u64{
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_literal(";")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("top-form")),
        };
        const tail = peg_seq(&tail_parts);
        const parts = [_]u64{
            untagEid(peg_rule_ref("top-form")),
            untagEid(peg_star(tail)),
        };
        defineRule(grammar, "stmt-list", peg_seq(&parts));
    }

    // block-bare = "[" ws "|" ws stmt-list ws "]"  (no params)
    {
        const parts = [_]u64{
            untagEid(peg_literal("[")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_literal("|")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("stmt-list")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_literal("]")),
        };
        defineRule(grammar, "block-bare", peg_seq(&parts));
    }

    // block-param = ":" identifier
    {
        const parts = [_]u64{
            untagEid(peg_literal(":")),
            untagEid(peg_rule_ref("identifier")),
        };
        defineRule(grammar, "block-param", peg_seq(&parts));
    }

    // block-params = block-param (ws block-param)*
    {
        const more_parts = [_]u64{
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("block-param")),
        };
        const more = peg_seq(&more_parts);
        const parts = [_]u64{
            untagEid(peg_rule_ref("block-param")),
            untagEid(peg_star(more)),
        };
        defineRule(grammar, "block-params", peg_seq(&parts));
    }

    // block-with-params = "[" ws block-params ws "|" ws stmt-list ws "]"
    {
        const parts = [_]u64{
            untagEid(peg_literal("[")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("block-params")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_literal("|")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("stmt-list")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_literal("]")),
        };
        defineRule(grammar, "block-with-params", peg_seq(&parts));
    }

    // binary-op = [+\-*/<>=,]   (single-char operator; `,` = concat, Phase 16)
    {
        const op_ranges = [_]u8{ '+', '+', '-', '-', '*', '*', '/', '/', '<', '<', '>', '>', '=', '=', ',', ',' };
        defineRule(grammar, "binary-op", peg_charset(&op_ranges));
    }

    // bracket-binary = "[" ws expression ws binary-op ws expression ws "]"
    {
        const parts = [_]u64{
            untagEid(peg_literal("[")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("expression")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("binary-op")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("expression")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_literal("]")),
        };
        defineRule(grammar, "bracket-binary", peg_seq(&parts));
    }

    // bracket-form = block-with-params | block-bare | bracket-keyword
    //             | bracket-binary | bracket-unary
    {
        const alts = [_]u64{
            untagEid(peg_rule_ref("block-with-params")),
            untagEid(peg_rule_ref("block-bare")),
            untagEid(peg_rule_ref("bracket-keyword")),
            untagEid(peg_rule_ref("bracket-binary")),
            untagEid(peg_rule_ref("bracket-unary")),
        };
        defineRule(grammar, "bracket-form", peg_choice(&alts));
    }

    // -- Phase 16: vector literal -----------------------------------------
    // vector-literal = "#[" ws (expression ws)* "]"
    // Elements are expressions, evaluated at eval time (Clojure-like). Using a
    // star over (expression ws) handles zero elements without a nullable opt.
    {
        const elem_parts = [_]u64{
            untagEid(peg_rule_ref("expression")),
            untagEid(peg_rule_ref("ws")),
        };
        const elem = peg_seq(&elem_parts);
        const parts = [_]u64{
            untagEid(peg_literal("#[")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_star(elem)),
            untagEid(peg_literal("]")),
        };
        defineRule(grammar, "vector-literal", peg_seq(&parts));
    }

    // expression = bracket-form | vector-literal | atom
    // (ordered: heavier forms first; `#[` is tried before atom's symbol-literal)
    {
        const alts = [_]u64{
            untagEid(peg_rule_ref("bracket-form")),
            untagEid(peg_rule_ref("vector-literal")),
            untagEid(peg_rule_ref("atom")),
        };
        defineRule(grammar, "expression", peg_choice(&alts));
    }

    // let-statement = kw-let ws identifier ws ":=" ws expression
    {
        const parts = [_]u64{
            untagEid(peg_rule_ref("kw-let")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("identifier")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_literal(":=")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("expression")),
        };
        defineRule(grammar, "let-statement", peg_seq(&parts));
    }

    // assignment = identifier ws ":=" ws expression
    {
        const parts = [_]u64{
            untagEid(peg_rule_ref("identifier")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_literal(":=")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("expression")),
        };
        defineRule(grammar, "assignment", peg_seq(&parts));
    }

    // return-statement = "^" ws expression  (G4 reserved word `^`)
    {
        const parts = [_]u64{
            untagEid(peg_literal("^")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("expression")),
        };
        defineRule(grammar, "return-statement", peg_seq(&parts));
    }

    // top-form = let-statement | return-statement | assignment | expression  (ordered).
    {
        const alts = [_]u64{
            untagEid(peg_rule_ref("let-statement")),
            untagEid(peg_rule_ref("return-statement")),
            untagEid(peg_rule_ref("assignment")),
            untagEid(peg_rule_ref("expression")),
        };
        defineRule(grammar, "top-form", peg_choice(&alts));
    }

    // start = ws top-form ws eof
    {
        const start_parts = [_]u64{
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("top-form")),
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
    _ = api.assert_fact(eid, ATTR_VTABLE, mkEid(kind_vt), M_USER);
    _ = api.assert_fact(eid, ATTR_AST_BYTE_START, mkSmallInt(@intCast(byte_start)), M_USER);
    _ = api.assert_fact(eid, ATTR_AST_BYTE_END, mkSmallInt(@intCast(byte_end)), M_USER);
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
        _ = api.assert_fact(ast, ATTR_AST_VALUE, mkSmallInt(n), M_USER);
        return ast;
    }
    if (rn == rn_float_literal) {
        const bytes = g_source[span.start..span.end];
        const f = std.fmt.parseFloat(f64, bytes) catch 0.0;
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, api.make_float(f), M_USER);
        return ast;
    }
    if (rn == rn_string) {
        // Strip surrounding quotes — span.start+1 .. span.end-1.
        const bytes = g_source[span.start + 1 .. span.end - 1];
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, api.make_string(bytes.ptr, bytes.len), M_USER);
        return ast;
    }
    if (rn == rn_kw_nil) {
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, NIL, M_USER);
        return ast;
    }
    if (rn == rn_kw_true) {
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, mkEid(TRUE_EID), M_USER);
        return ast;
    }
    if (rn == rn_kw_false) {
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, mkEid(FALSE_EID), M_USER);
        return ast;
    }
    if (rn == rn_kw_self) {
        return newAstNode(AST_SELF_VT, span.start, span.end);
    }
    if (rn == rn_identifier) {
        const bytes = g_source[span.start..span.end];
        const name_eid = api.intern(bytes.ptr, bytes.len);
        const ast = newAstNode(AST_VAR_REF_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_NAME, mkEid(name_eid), M_USER);
        return ast;
    }
    if (rn == rn_symbol_literal) {
        // Span = "#" + (identifier | kw1:kw2:...). Strip the leading "#".
        if (span.end > span.start + 1) {
            const bytes = g_source[span.start + 1 .. span.end];
            const name_eid = api.intern(bytes.ptr, bytes.len);
            const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
            _ = api.assert_fact(ast, ATTR_AST_VALUE, mkEid(name_eid), M_USER);
            return ast;
        }
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, NIL, M_USER);
        return ast;
    }
    if (rn == rn_block_bare) {
        // Body is now stmt-list (was: single expression). Find stmt-list
        // child and use its desugared AST as the body.
        var c = cstFirstChild(cst_eid);
        while (c != 0) : (c = cstNextSibling(c)) {
            if (cstRuleName(c) == rn_stmt_list) {
                const body_ast = desugar(c);
                const ast = newAstNode(AST_BLOCK_VT, span.start, span.end);
                _ = api.assert_fact(ast, ATTR_AST_BLOCK_BODY, mkEid(body_ast), M_USER);
                return ast;
            }
        }
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, NIL, M_USER);
        return ast;
    }
    if (rn == rn_block_with_params) {
        // Children: ws + block-params + ws + expression. Find block-params
        // entry (list of block-param children, each carrying an identifier),
        // then expression as the body.
        var c = cstFirstChild(cst_eid);
        var first_param_eid: u64 = 0;
        var last_param_eid: u64 = 0;
        var body_ast: u64 = 0;
        while (c != 0) : (c = cstNextSibling(c)) {
            const crn = cstRuleName(c);
            if (crn == rn_block_params) {
                // Walk block-params children — each block-param has an
                // identifier child whose name we intern.
                var p = cstFirstChild(c);
                while (p != 0) : (p = cstNextSibling(p)) {
                    if (cstRuleName(p) != rn_block_param) continue;
                    var ic = cstFirstChild(p);
                    while (ic != 0) : (ic = cstNextSibling(ic)) {
                        if (cstRuleName(ic) == rn_identifier) {
                            const id_span = cstByteRange(ic);
                            const bytes = g_source[id_span.start..id_span.end];
                            const name_eid = api.intern(bytes.ptr, bytes.len);
                            // Allocate a param-entry entity carrying the name.
                            const pe = api.alloc_eid();
                            _ = api.assert_fact(pe, ATTR_AST_NAME, mkEid(name_eid), M_USER);
                            if (first_param_eid == 0) {
                                first_param_eid = pe;
                            } else {
                                _ = api.assert_fact(last_param_eid, ATTR_AST_NEXT_SIBLING, mkEid(pe), M_USER);
                            }
                            last_param_eid = pe;
                            break;
                        }
                    }
                }
            } else if (crn == rn_stmt_list) {
                body_ast = desugar(c);
            }
        }
        const ast = newAstNode(AST_BLOCK_VT, span.start, span.end);
        if (body_ast != 0) {
            _ = api.assert_fact(ast, ATTR_AST_BLOCK_BODY, mkEid(body_ast), M_USER);
        }
        if (first_param_eid != 0) {
            _ = api.assert_fact(ast, ATTR_AST_BLOCK_PARAMS_CONST, mkEid(first_param_eid), M_USER);
        }
        return ast;
    }
    if (rn == rn_return_statement) {
        // Children: "^" (no CST) + ws + expression.
        var c = cstFirstChild(cst_eid);
        var value_ast: u64 = 0;
        while (c != 0) : (c = cstNextSibling(c)) {
            if (cstRuleName(c) == rn_expression) {
                value_ast = desugar(c);
                break;
            }
        }
        const ast = newAstNode(AST_RETURN_VT, span.start, span.end);
        if (value_ast != 0) {
            _ = api.assert_fact(ast, ATTR_AST_RETURN_VALUE, mkEid(value_ast), M_USER);
        }
        return ast;
    }
    if (rn == rn_assignment) {
        // Children: identifier + ws + ":=" (no CST) + ws + expression.
        var c = cstFirstChild(cst_eid);
        var name_eid: u64 = 0;
        var value_ast: u64 = 0;
        while (c != 0) : (c = cstNextSibling(c)) {
            const crn = cstRuleName(c);
            if (crn == rn_identifier and name_eid == 0) {
                const id_span = cstByteRange(c);
                const bytes = g_source[id_span.start..id_span.end];
                name_eid = api.intern(bytes.ptr, bytes.len);
            } else if (crn == rn_expression) {
                value_ast = desugar(c);
            }
        }
        const ast = newAstNode(AST_ASSIGN_VT, span.start, span.end);
        if (name_eid != 0) {
            _ = api.assert_fact(ast, ATTR_AST_NAME, mkEid(name_eid), M_USER);
        }
        if (value_ast != 0) {
            _ = api.assert_fact(ast, ATTR_AST_ASSIGN_VALUE, mkEid(value_ast), M_USER);
        }
        return ast;
    }
    if (rn == rn_let_statement) {
        // Children include kw-let CST + ws + identifier + ws + ":=" (no CST)
        // + ws + expression. Skip kw-let and ws, find identifier + expression.
        var c = cstFirstChild(cst_eid);
        var name_eid: u64 = 0;
        var init_ast: u64 = 0;
        while (c != 0) : (c = cstNextSibling(c)) {
            const crn = cstRuleName(c);
            if (crn == rn_identifier and name_eid == 0) {
                const id_span = cstByteRange(c);
                const bytes = g_source[id_span.start..id_span.end];
                name_eid = api.intern(bytes.ptr, bytes.len);
            } else if (crn == rn_expression) {
                init_ast = desugar(c);
            }
        }
        const ast = newAstNode(AST_LET_VT, span.start, span.end);
        if (name_eid != 0) {
            _ = api.assert_fact(ast, ATTR_AST_NAME, mkEid(name_eid), M_USER);
        }
        if (init_ast != 0) {
            _ = api.assert_fact(ast, ATTR_AST_LET_INIT, mkEid(init_ast), M_USER);
        }
        return ast;
    }
    if (rn == rn_stmt_list) {
        // Collect each top-form child's desugared AST. Single child → return
        // that AST directly. Multiple children → wrap in AST_STMT_SEQ_VT
        // linked via :ast-stmt-first + :ast-next-sibling.
        var c = cstFirstChild(cst_eid);
        var first_child_ast: u64 = 0;
        var last_child_ast: u64 = 0;
        var count: usize = 0;
        while (c != 0) : (c = cstNextSibling(c)) {
            const crn = cstRuleName(c);
            if (crn != rn_top_form) continue;
            const stmt_ast = desugar(c);
            if (stmt_ast == 0) continue;
            count += 1;
            if (first_child_ast == 0) {
                first_child_ast = stmt_ast;
            } else {
                _ = api.assert_fact(last_child_ast, ATTR_AST_NEXT_SIBLING, mkEid(stmt_ast), M_USER);
            }
            last_child_ast = stmt_ast;
        }
        if (count == 1) return first_child_ast;
        const seq_ast = newAstNode(AST_STMT_SEQ_VT, span.start, span.end);
        if (first_child_ast != 0) {
            _ = api.assert_fact(seq_ast, ATTR_AST_STMT_FIRST, mkEid(first_child_ast), M_USER);
        }
        return seq_ast;
    }
    if (rn == rn_atom or rn == rn_expression or rn == rn_bracket_form or rn == rn_start or rn == rn_top_form) {
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
        _ = api.assert_fact(ast, ATTR_AST_VALUE, NIL, M_USER);
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
                    _ = api.assert_fact(send_ast, ATTR_AST_RECEIVER, mkEid(receiver_ast), M_USER);
                    _ = api.assert_fact(send_ast, ATTR_AST_SELECTOR, mkEid(sel_eid), M_USER);
                    receiver_ast = send_ast;
                    break;
                }
            }
        }
        return receiver_ast;
    }
    if (rn == rn_bracket_binary) {
        // Children (after ws-skip): expression (receiver), binary-op, expression (arg).
        var c = cstFirstChild(cst_eid);
        var receiver_ast: u64 = 0;
        var arg_ast: u64 = 0;
        var op_sel_eid: u64 = 0;
        var phase: u8 = 0; // 0 = receiver, 1 = op, 2 = arg
        while (c != 0) : (c = cstNextSibling(c)) {
            const crn = cstRuleName(c);
            if (crn == rn_ws) continue;
            if (phase == 0 and crn == rn_expression) {
                receiver_ast = desugar(c);
                phase = 1;
                continue;
            }
            if (phase == 1) {
                // binary-op CST — its byte-span IS the operator character.
                const op_span = cstByteRange(c);
                const op_bytes = g_source[op_span.start..op_span.end];
                op_sel_eid = api.intern(op_bytes.ptr, op_bytes.len);
                phase = 2;
                continue;
            }
            if (phase == 2 and crn == rn_expression) {
                arg_ast = desugar(c);
                break;
            }
        }
        const send_ast = newAstNode(AST_KEYWORD_SEND_VT, span.start, span.end);
        _ = api.assert_fact(send_ast, ATTR_AST_RECEIVER, mkEid(receiver_ast), M_USER);
        _ = api.assert_fact(send_ast, ATTR_AST_SELECTOR, mkEid(op_sel_eid), M_USER);
        if (arg_ast != 0) {
            _ = api.assert_fact(send_ast, ATTR_AST_FIRST_ARG, mkEid(arg_ast), M_USER);
        }
        return send_ast;
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
                    _ = api.assert_fact(last_arg_ast, ATTR_AST_NEXT_SIBLING, mkEid(arg_ast), M_USER);
                }
                last_arg_ast = arg_ast;
            }
        }
        const sel_eid = api.intern(sel_buf[0..sel_len].ptr, sel_len);
        const send_ast = newAstNode(AST_KEYWORD_SEND_VT, span.start, span.end);
        _ = api.assert_fact(send_ast, ATTR_AST_RECEIVER, mkEid(receiver_ast), M_USER);
        _ = api.assert_fact(send_ast, ATTR_AST_SELECTOR, mkEid(sel_eid), M_USER);
        if (first_arg_ast != 0) {
            _ = api.assert_fact(send_ast, ATTR_AST_FIRST_ARG, mkEid(first_arg_ast), M_USER);
        }
        return send_ast;
    }
    if (rn == rn_vector_literal) {
        // #[e0 e1 ...] desugars to a left-nested `with:` chain over an empty
        // vector literal: [[[ #[] with: e0] with: e1] ...]. Pure sends +
        // literal → no new AST kind / opcode; works in tree-walker AND
        // bytecode unchanged. Empty vector is a constant on the literal node.
        const empty_ast = newAstNode(AST_LITERAL_VT, span.start, span.start);
        _ = api.assert_fact(empty_ast, ATTR_AST_VALUE, api.make_rrb(&[_]Ref{}, 0), M_USER);
        const with_sel = api.intern("with:".ptr, 5);
        var recv_ast = empty_ast;
        var c = cstFirstChild(cst_eid);
        while (c != 0) : (c = cstNextSibling(c)) {
            if (cstRuleName(c) != rn_expression) continue;
            const elem_ast = desugar(c);
            const send = newAstNode(AST_KEYWORD_SEND_VT, span.start, span.end);
            _ = api.assert_fact(send, ATTR_AST_RECEIVER, mkEid(recv_ast), M_USER);
            _ = api.assert_fact(send, ATTR_AST_SELECTOR, mkEid(with_sel), M_USER);
            _ = api.assert_fact(send, ATTR_AST_FIRST_ARG, mkEid(elem_ast), M_USER);
            recv_ast = send;
        }
        return recv_ast;
    }

    // Unknown rule — fallback to NIL literal.
    const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
    _ = api.assert_fact(ast, ATTR_AST_VALUE, NIL, M_USER);
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
    rn_symbol_literal = api.intern("symbol-literal".ptr, 14);
    rn_block_bare = api.intern("block-bare".ptr, 10);
    rn_block_param = api.intern("block-param".ptr, 11);
    rn_block_params = api.intern("block-params".ptr, 12);
    rn_block_with_params = api.intern("block-with-params".ptr, 17);
    rn_bracket_binary = api.intern("bracket-binary".ptr, 14);
    rn_vector_literal = api.intern("vector-literal".ptr, 14);
    rn_let_statement = api.intern("let-statement".ptr, 13);
    rn_top_form = api.intern("top-form".ptr, 8);
    rn_assignment = api.intern("assignment".ptr, 10);
    rn_stmt_list = api.intern("stmt-list".ptr, 9);
    rn_float_literal = api.intern("float-literal".ptr, 13);
    rn_return_statement = api.intern("return-statement".ptr, 16);
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
var sel_parse_expr_g: u64 = 0;
var sel_value_g: u64 = 0;
const TRUE_VT: u64 = 70;
const FALSE_VT: u64 = 71;

// ADR-10-E2 scope chain — Phase 10.4.5 S1.
//
// Replaces the Phase 10.3/10.4 interim (args[0]=self_ref threading +
// g_scope flat stack) with fact-shaped scope entities.
//
// Each invocation allocates a SCOPE_VT entity carrying:
//   [scope :vtable SCOPE_VT]
//   [scope :scope-parent <enclosing>]  (NIL for top-level)
//   [scope :scope-self <receiver Ref>] (self for AST_SELF_VT eval)
//   [scope :scope-first-binding <head BINDING_VT eid>]
//
// Each binding allocates a BINDING_VT entity carrying:
//   [bind :vtable BINDING_VT]
//   [bind :bind-name <interned-name EID>]
//   [bind :bind-value <Ref>]
//   [bind :bind-next <next BINDING_VT eid>]
//
// AST_VAR_REF_VT eval walks the scope chain. AST_SELF_VT eval queries
// :scope-self on the current scope.

const SCOPE_VT: u64 = 101;
const BINDING_VT: u64 = 100;
const ATTR_SCOPE_PARENT: u64 = 138;
const ATTR_SCOPE_SELF: u64 = 162;
const ATTR_SCOPE_FIRST_BINDING: u64 = 163;
const ATTR_SCOPE_TOP_LEVEL: u64 = 139;
const ATTR_BIND_NAME: u64 = 136;
const ATTR_BIND_VALUE: u64 = 137;
const ATTR_BIND_NEXT: u64 = 164;

/// Mirror of kernel_eids.TOP_SCOPE_EID (185) — stable well-known eid for the
/// top-level REPL scope so persisted bindings survive save/load.
const TOP_SCOPE_EID: u64 = 185;

/// Mirror of kernel_eids.GLOBALS_SCOPE_EID (186) — parent of TOP_SCOPE; holds
/// built-in global bindings (e.g. `Module`). Kept out of the top scope so
/// `:reset` and image `:load` (which touch only TOP_SCOPE's binding chain)
/// don't disturb globals.
const GLOBALS_SCOPE_EID: u64 = 186;

/// Mirror of kernel_eids.MODULE_PROTO (15) — the `Module` prototype object the
/// FFI surface installs `load:` on. Bound to the global name `Module`.
const MODULE_PROTO_EID: u64 = 15;

/// Mirrors of kernel_eids — the `language` prototype (file-in) + filed-in-unit
/// attributes. lang owns the `fileIn:` methods; the kernel just reserves eids.
const LANGUAGE_PROTO_VT: u64 = 191;
const LANGUAGE_PROTO_EID: u64 = 192;
const ATTR_FILED_IN_PATH: u64 = 193;
const ATTR_FILED_IN_SOURCE: u64 = 194;

// Singleton top-level scope EID — set to TOP_SCOPE_EID at lang-dylib init.
var g_top_scope_eid: u64 = 0;

fn newScopeEntity(parent_scope_ref: Ref, self_ref: Ref) u64 {
    const api = g_api.?;
    const eid = api.alloc_eid();
    _ = api.assert_fact(eid, ATTR_VTABLE, mkEid(SCOPE_VT), M_USER);
    if (parent_scope_ref != NIL) {
        _ = api.assert_fact(eid, ATTR_SCOPE_PARENT, parent_scope_ref, M_USER);
    }
    // Date discipline — only assert when self is non-NIL (absence = no self).
    if (self_ref != NIL) {
        _ = api.assert_fact(eid, ATTR_SCOPE_SELF, self_ref, M_USER);
    }
    return eid;
}

fn pushScopeBinding(scope_eid: u64, name_eid: u64, value: Ref) void {
    const api = g_api.?;
    const bind_eid = api.alloc_eid();
    _ = api.assert_fact(bind_eid, ATTR_VTABLE, mkEid(BINDING_VT), M_USER);
    _ = api.assert_fact(bind_eid, ATTR_BIND_NAME, mkEid(name_eid), M_USER);
    _ = api.assert_fact(bind_eid, ATTR_BIND_VALUE, value, M_USER);
    // Push at head — link new binding's :bind-next to current head, update head.
    const cur_head = api.query_eavt(scope_eid, ATTR_SCOPE_FIRST_BINDING);
    if (cur_head != NIL) {
        _ = api.assert_fact(bind_eid, ATTR_BIND_NEXT, cur_head, M_USER);
    }
    _ = api.assert_fact(scope_eid, ATTR_SCOPE_FIRST_BINDING, mkEid(bind_eid), M_USER);
}

/// Push a binding with CODE provenance (g_module_eid) instead of M_USER.
/// Used for built-in globals so they are reconstructed each boot and never
/// persisted into images (which save M_USER facts only).
fn pushGlobalBinding(scope_eid: u64, name_eid: u64, value: Ref) void {
    const api = g_api.?;
    const bind_eid = api.alloc_eid();
    _ = api.assert_fact(bind_eid, ATTR_VTABLE, mkEid(BINDING_VT), g_module_eid);
    _ = api.assert_fact(bind_eid, ATTR_BIND_NAME, mkEid(name_eid), g_module_eid);
    _ = api.assert_fact(bind_eid, ATTR_BIND_VALUE, value, g_module_eid);
    const cur_head = api.query_eavt(scope_eid, ATTR_SCOPE_FIRST_BINDING);
    if (cur_head != NIL) {
        _ = api.assert_fact(bind_eid, ATTR_BIND_NEXT, cur_head, g_module_eid);
    }
    _ = api.assert_fact(scope_eid, ATTR_SCOPE_FIRST_BINDING, mkEid(bind_eid), g_module_eid);
}

/// Build the globals scope (parent of TOP_SCOPE) and install built-in global
/// bindings. CODE provenance throughout; idempotent across reboots because it
/// runs at a fixed eid each boot.
fn installGlobals() void {
    const api = g_api.?;
    _ = api.assert_fact(GLOBALS_SCOPE_EID, ATTR_VTABLE, mkEid(SCOPE_VT), g_module_eid);
    _ = api.assert_fact(GLOBALS_SCOPE_EID, ATTR_SCOPE_TOP_LEVEL, mkEid(TRUE_EID), g_module_eid);
    // `Module` → the prototype object (kernel-side MODULE_PROTO).
    const name_module = api.intern("Module".ptr, 6);
    pushGlobalBinding(GLOBALS_SCOPE_EID, name_module, mkEid(MODULE_PROTO_EID));
}

fn lookupInScope(scope_eid: u64, name_eid: u64) ?Ref {
    const api = g_api.?;
    var cur = scope_eid;
    while (cur != 0) {
        var b_ref = api.query_eavt(cur, ATTR_SCOPE_FIRST_BINDING);
        while (isEid(b_ref) and b_ref != NIL) {
            const b_eid = b_ref >> 3;
            const name_ref = api.query_eavt(b_eid, ATTR_BIND_NAME);
            if (isEid(name_ref) and (name_ref >> 3) == name_eid) {
                return api.query_eavt(b_eid, ATTR_BIND_VALUE);
            }
            b_ref = api.query_eavt(b_eid, ATTR_BIND_NEXT);
        }
        // Walk to parent.
        const parent_ref = api.query_eavt(cur, ATTR_SCOPE_PARENT);
        if (!isEid(parent_ref) or parent_ref == NIL) return null;
        cur = parent_ref >> 3;
    }
    return null;
}

/// Every eval method takes `args[0]` = current scope-Ref (a SCOPE_VT
/// entity Ref). When called with no args (top-level test invocations),
/// defaults to the singleton top-level scope. AST_SELF_VT eval queries
/// `:scope-self` on this scope; AST_VAR_REF_VT walks scope chain via
/// `:scope-parent`.
fn currentScope(args: [*]const Ref, n_args: usize) Ref {
    if (n_args == 0) return mkEid(g_top_scope_eid);
    return args[0];
}

fn evalLiteralImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const api = g_api.?;
    if (!isEid(s.receiver)) return NIL;
    return api.query_eavt(s.receiver >> 3, ATTR_AST_VALUE);
}

fn evalSelfImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    const api = g_api.?;
    const scope_ref = currentScope(args, n_args);
    if (!isEid(scope_ref)) return NIL;
    return api.query_eavt(scope_ref >> 3, ATTR_SCOPE_SELF);
}

fn evalVarRefImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    const api = g_api.?;
    if (!isEid(s.receiver)) return NIL;
    const ast_eid = s.receiver >> 3;
    const name_ref = api.query_eavt(ast_eid, ATTR_AST_NAME);
    if (!isEid(name_ref)) return NIL;
    const name_eid = name_ref >> 3;
    const scope_ref = currentScope(args, n_args);
    if (!isEid(scope_ref)) return NIL;
    return lookupInScope(scope_ref >> 3, name_eid) orelse NIL;
}

fn evalUnarySendImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    const api = g_api.?;
    if (!isEid(s.receiver)) return NIL;
    const scope_ref = currentScope(args, n_args);
    const ast_eid = s.receiver >> 3;
    const recv_ast_ref = api.query_eavt(ast_eid, ATTR_AST_RECEIVER);
    const sel_ref = api.query_eavt(ast_eid, ATTR_AST_SELECTOR);
    if (!isEid(sel_ref)) return NIL;
    const sel_eid = sel_ref >> 3;
    const recv_val = api.send1(recv_ast_ref, sel_eval, scope_ref);
    return api.send0(recv_val, sel_eid);
}

fn evalKeywordSendImpl(s: *Send, args_in: [*]const Ref, n_args_in: usize) callconv(.c) Ref {
    const api = g_api.?;
    if (!isEid(s.receiver)) return NIL;
    const scope_ref = currentScope(args_in, n_args_in);
    const ast_eid = s.receiver >> 3;
    const recv_ast_ref = api.query_eavt(ast_eid, ATTR_AST_RECEIVER);
    const sel_ref = api.query_eavt(ast_eid, ATTR_AST_SELECTOR);
    if (!isEid(sel_ref)) return NIL;
    const sel_eid = sel_ref >> 3;
    const recv_val = api.send1(recv_ast_ref, sel_eval, scope_ref);
    var buf: [16]Ref = undefined;
    var count: usize = 0;
    // Walk the arg sibling-chain. The null link is mkEid(0) (PEG's CST
    // terminator convention; see cstNextSibling) — NOT bare NIL — because an
    // AST node minted onto a recycled CST eid can retain that node's stale
    // `:next-sibling mkEid(0)` fact. Guarding only on isEid() would accept
    // mkEid(0) as a phantom arg and overrun the buffer (latent everywhere,
    // but only strict-arity consumers like FFI wraps observe it).
    var cur_ref = api.query_eavt(ast_eid, ATTR_AST_FIRST_ARG);
    while (isEid(cur_ref) and (cur_ref >> 3) != 0 and count < buf.len) : (count += 1) {
        const ast_arg = cur_ref >> 3;
        buf[count] = api.send1(cur_ref, sel_eval, scope_ref);
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

fn smallintMinusImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    return mkSmallInt(untagSmallInt(s.receiver) - untagSmallInt(args[0]));
}
fn smallintDivImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const b_ = untagSmallInt(args[0]);
    if (b_ == 0) return NIL;
    return mkSmallInt(@divTrunc(untagSmallInt(s.receiver), b_));
}
fn smallintEqImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    return if (untagSmallInt(s.receiver) == untagSmallInt(args[0])) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}
fn smallintLtImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    return if (untagSmallInt(s.receiver) < untagSmallInt(args[0])) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}
fn smallintGtImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    return if (untagSmallInt(s.receiver) > untagSmallInt(args[0])) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}

fn smallintAsFloatImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const api = g_api.?;
    return api.make_float(@floatFromInt(untagSmallInt(s.receiver)));
}

// -- FLOAT_VT primitives --------------------------------------------------
// Float arithmetic coerces SmallInt args to f64 automatically.

fn argAsF64(r: Ref) f64 {
    if (isSmallInt(r)) return @floatFromInt(untagSmallInt(r));
    if (isFloatRef(r)) return floatRefValue(r);
    return 0.0;
}

fn floatPlusImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    return g_api.?.make_float(floatRefValue(s.receiver) + argAsF64(args[0]));
}
fn floatMinusImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    return g_api.?.make_float(floatRefValue(s.receiver) - argAsF64(args[0]));
}
fn floatTimesImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    return g_api.?.make_float(floatRefValue(s.receiver) * argAsF64(args[0]));
}
fn floatDivImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const b_ = argAsF64(args[0]);
    if (b_ == 0.0) return NIL;
    return g_api.?.make_float(floatRefValue(s.receiver) / b_);
}
fn floatEqImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    return if (floatRefValue(s.receiver) == argAsF64(args[0])) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}
fn floatLtImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    return if (floatRefValue(s.receiver) < argAsF64(args[0])) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}
fn floatGtImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    return if (floatRefValue(s.receiver) > argAsF64(args[0])) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}

fn floatAsIntImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const f = floatRefValue(s.receiver);
    return mkSmallInt(@intFromFloat(f));
}

fn floatNegateImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    return g_api.?.make_float(-floatRefValue(s.receiver));
}

/// `[f exp: power]` — replaces float-literal exponent grammar. User writes
/// `[1.5 exp: 10]` for `1.5 * 10^10`.
fn floatExpImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const power = argAsF64(args[0]);
    return g_api.?.make_float(floatRefValue(s.receiver) * std.math.pow(f64, 10.0, power));
}

fn smallintVtableImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = args;
    _ = n_args;
    return mkEid(SMALLINT_VT);
}

// -- AST_BLOCK_VT eval + block-invocation shim ----------------------------

/// Closure data is a pointer Ref to a 2-slot wrapper object:
///   slot 0 = block AST entity Ref (mkEid)
///   slot 1 = captured-scope Ref (mkEid of SCOPE_VT entity)
/// The block-shim reads both at invocation time. Capturing the enclosing
/// scope at block-eval time (instead of at block-invocation time) is what
/// gives closures their Smalltalk semantics — inner blocks see outer
/// lexical bindings through the chain (ADR-10-E2 S2 / E3 by-reference).
fn allocBlockClosure(block_ast_ref: Ref, captured_scope: Ref) Ref {
    const api = g_api.?;
    const obj = api.alloc_object(mkEid(OBJECT_VT), 2) orelse return NIL;
    api.set_slot(obj, 0, block_ast_ref);
    api.set_slot(obj, 1, captured_scope);
    const wrapper_ref = api.object_to_ref(obj);
    const closure_eid = api.make_closure(@ptrCast(&blockEvalShim), wrapper_ref);
    return mkEid(closure_eid);
}

/// Block-invocation shim. `s.closure.data` = mkEid(block_ast_eid).
/// On invocation: walk :ast-block-params, push each name→arg binding,
/// eval :ast-block-body, pop bindings, return result.
fn blockEvalShim(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    const api = g_api.?;
    const closure_ptr = s.closure orelse return NIL;
    const slot_base: [*]const Ref = @ptrCast(@alignCast(closure_ptr));
    const wrapper_ref = slot_base[1]; // closure.data — pointer Ref to wrapper obj
    if ((wrapper_ref & 0b111) != 0) return NIL;
    const wrapper_obj: ?*anyopaque = @ptrFromInt(wrapper_ref);
    const block_ast_ref = api.slot_at(wrapper_obj, 0);
    const captured_scope_ref = api.slot_at(wrapper_obj, 1);
    if (!isEid(block_ast_ref)) return NIL;
    const block_ast_eid = block_ast_ref >> 3;

    // Activation scope: parent = captured-at-eval-time enclosing scope.
    // This is what lets inner blocks see outer lexical bindings (E3 by-ref).
    const new_scope_eid = newScopeEntity(captured_scope_ref, s.receiver);
    const new_scope_ref = mkEid(new_scope_eid);

    var cur_ref = api.query_eavt(block_ast_eid, ATTR_AST_BLOCK_PARAMS_CONST);
    var idx: usize = 0;
    // (cur_ref >> 3) != 0 also rejects a stale mkEid(0) null-link sibling;
    // idx < n_args already bounds this loop, but keep the terminator uniform.
    while (isEid(cur_ref) and (cur_ref >> 3) != 0 and idx < n_args) : (idx += 1) {
        const pe = cur_ref >> 3;
        const name_ref = api.query_eavt(pe, ATTR_AST_NAME);
        if (isEid(name_ref) and !isNil(name_ref)) {
            pushScopeBinding(new_scope_eid, name_ref >> 3, args[idx]);
        }
        cur_ref = api.query_eavt(pe, ATTR_AST_NEXT_SIBLING);
    }

    const body_ref = api.query_eavt(block_ast_eid, ATTR_AST_BLOCK_BODY);
    const result = api.send1(body_ref, sel_eval, new_scope_ref);
    // ADR-10-E4 interim catch: method-frame shims (dispatched via any
    // selector OTHER than value/value:/value:value:) catch the
    // unwinding sentinel and return the carried value. Value-family
    // shims propagate so `^` escapes outward.
    if (g_unwinding and !isValueSelector(s.selector)) {
        const carried = g_unwind_value;
        g_unwinding = false;
        g_unwind_value = NIL;
        return carried;
    }
    return result;
}

fn isNil(r: Ref) bool {
    return r == NIL;
}

fn evalBlockImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (!isEid(s.receiver)) return NIL;
    // Capture the current scope at eval-time (E3 by-reference closure).
    return allocBlockClosure(s.receiver, currentScope(args, n_args));
}

/// `let name := expr` — eval init, push (name, init_value) into the
/// current scope, return the bound value.
/// Walk scope chain looking for an existing binding; mutate :bind-value
/// on success. Returns true if mutated. Phase 8 cardinality `:one` already
/// implicit-retracts on overwrite for non-`:many` attrs.
fn mutateBinding(scope_eid: u64, name_eid: u64, new_value: Ref) bool {
    const api = g_api.?;
    var cur = scope_eid;
    while (cur != 0) {
        var b_ref = api.query_eavt(cur, ATTR_SCOPE_FIRST_BINDING);
        while (isEid(b_ref) and b_ref != NIL) {
            const b_eid = b_ref >> 3;
            const name_r = api.query_eavt(b_eid, ATTR_BIND_NAME);
            if (isEid(name_r) and (name_r >> 3) == name_eid) {
                const old_val = api.query_eavt(b_eid, ATTR_BIND_VALUE);
                if (old_val != NIL) {
                    _ = api.retract_fact(b_eid, ATTR_BIND_VALUE, old_val, M_USER);
                }
                _ = api.assert_fact(b_eid, ATTR_BIND_VALUE, new_value, M_USER);
                return true;
            }
            b_ref = api.query_eavt(b_eid, ATTR_BIND_NEXT);
        }
        const parent_ref = api.query_eavt(cur, ATTR_SCOPE_PARENT);
        if (!isEid(parent_ref) or parent_ref == NIL) return false;
        cur = parent_ref >> 3;
    }
    return false;
}

/// `name := expr` — eval rhs, walk scope chain for existing binding,
/// retract old value + assert new. Assignment to undefined returns NIL
/// (full parse-time check awaits S5 slot-resolution).
fn evalAssignImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    const api = g_api.?;
    if (!isEid(s.receiver)) return NIL;
    const ast_eid = s.receiver >> 3;
    const scope_ref = currentScope(args, n_args);
    if (!isEid(scope_ref)) return NIL;
    const name_ref = api.query_eavt(ast_eid, ATTR_AST_NAME);
    const value_ref = api.query_eavt(ast_eid, ATTR_AST_ASSIGN_VALUE);
    if (!isEid(name_ref) or !isEid(value_ref)) return NIL;
    const new_val = api.send1(value_ref, sel_eval, scope_ref);
    if (mutateBinding(scope_ref >> 3, name_ref >> 3, new_val)) {
        return new_val;
    }
    return NIL;
}

/// Multi-statement sequence — walk linked list, eval each, return last value.
/// Breaks early on g_unwinding (ADR-10-E4 sentinel) so subsequent stmts
/// don't execute after ^.
fn evalStmtSeqImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    const api = g_api.?;
    if (!isEid(s.receiver)) return NIL;
    const ast_eid = s.receiver >> 3;
    const scope_ref = currentScope(args, n_args);
    var cur_ref = api.query_eavt(ast_eid, ATTR_AST_STMT_FIRST);
    var last_val: Ref = NIL;
    // mkEid(0) is the null-link terminator (see the arg walk in
    // evalKeywordSendImpl). Guarding only on != NIL would treat a stale
    // mkEid(0) `:next-sibling` (left on a recycled eid) as a live node and
    // loop forever sending `eval` to entity 0.
    while (isEid(cur_ref) and (cur_ref >> 3) != 0) {
        last_val = api.send1(cur_ref, sel_eval, scope_ref);
        if (g_unwinding) return last_val;
        const cur_eid = cur_ref >> 3;
        cur_ref = api.query_eavt(cur_eid, ATTR_AST_NEXT_SIBLING);
    }
    return last_val;
}

/// `^expr` — sets the unwinding sentinel and carries the value out.
/// Per ADR-10-E4 interim: the first non-value-selector shim frame
/// catches; value-family frames propagate. Phase 11 retires this with
/// proper VM unwind opcodes.
fn evalReturnImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    const api = g_api.?;
    if (!isEid(s.receiver)) return NIL;
    const ast_eid = s.receiver >> 3;
    const value_ref = api.query_eavt(ast_eid, ATTR_AST_RETURN_VALUE);
    const scope_ref = currentScope(args, n_args);
    const val = if (isEid(value_ref) and value_ref != NIL)
        api.send1(value_ref, sel_eval, scope_ref)
    else
        NIL;
    g_unwinding = true;
    g_unwind_value = val;
    return val;
}

/// `[condBlock whileTrue: bodyBlock]` — Zig-native iteration primitive.
/// Loops: eval cond → if TRUE_EID then eval body and continue, else return NIL.
/// User-overrideable via [CLOSURE_VT method:#whileTrue: do:[...]].
fn closureWhileTrueImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const api = g_api.?;
    const body_ref = args[0];
    var safety_counter: u64 = 0;
    const SAFETY_LIMIT: u64 = 10_000_000;
    while (safety_counter < SAFETY_LIMIT) : (safety_counter += 1) {
        const cond_val = api.send0(s.receiver, sel_value_g);
        if (!isEid(cond_val)) return NIL;
        if ((cond_val >> 3) != TRUE_EID) return NIL;
        _ = api.send0(body_ref, sel_value_g);
    }
    return NIL;
}

fn closureWhileFalseImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const api = g_api.?;
    const body_ref = args[0];
    var safety_counter: u64 = 0;
    const SAFETY_LIMIT: u64 = 10_000_000;
    while (safety_counter < SAFETY_LIMIT) : (safety_counter += 1) {
        const cond_val = api.send0(s.receiver, sel_value_g);
        if (!isEid(cond_val)) return NIL;
        if ((cond_val >> 3) != FALSE_EID) return NIL;
        _ = api.send0(body_ref, sel_value_g);
    }
    return NIL;
}

/// `[N timesRepeat: bodyBlock]` — invoke body N times. Receiver is the
/// SmallInt count.
fn smallintTimesRepeatImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const api = g_api.?;
    const body_ref = args[0];
    var count = untagSmallInt(s.receiver);
    while (count > 0) : (count -= 1) {
        _ = api.send0(body_ref, sel_value_g);
    }
    return s.receiver;
}

/// `[start to: end do: bodyBlock]` — body invoked with the current
/// SmallInt index from start to end inclusive.
fn smallintToDoImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const api = g_api.?;
    const end = untagSmallInt(args[0]);
    const body_ref = args[1];
    var i = untagSmallInt(s.receiver);
    while (i <= end) : (i += 1) {
        _ = api.send1(body_ref, sel_value_g, mkSmallInt(i));
    }
    return s.receiver;
}

fn evalLetImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    const api = g_api.?;
    if (!isEid(s.receiver)) return NIL;
    const ast_eid = s.receiver >> 3;
    const scope_ref = currentScope(args, n_args);
    if (!isEid(scope_ref)) return NIL;
    const name_ref = api.query_eavt(ast_eid, ATTR_AST_NAME);
    const init_ref = api.query_eavt(ast_eid, ATTR_AST_LET_INIT);
    if (!isEid(name_ref) or !isEid(init_ref)) return NIL;
    const init_val = api.send1(init_ref, sel_eval, scope_ref);
    pushScopeBinding(scope_ref >> 3, name_ref >> 3, init_val);
    return init_val;
}

// -- Method install ---------------------------------------------------------

/// `[vt method: #sel do: blockClosureRef]` — installs the block as a method
/// on `vt`. Returns the receiver (target vt) for chaining.
///
/// Phase 11.2 + 11.11 switchover infrastructure is in place — closure
/// scope capture (11.11) closes the compile-time blocker. Wiring at the
/// vtMethodDoImpl call site still triggers a runtime regression in
/// DEMO 10.4b that the kernel-side `[:x | [x succ]]` mirror test
/// doesn't reproduce. Likely a cross-dylib state issue
/// (allocator/closure-pointer/EID resolution) that needs targeted
/// instrumentation. Keep tree-walker install path until the
/// regression's root cause is found.
fn vtMethodDoImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const api = g_api.?;
    if (!isEid(s.receiver) or !isEid(args[0]) or !isEid(args[1])) return NIL;
    const vt_eid = s.receiver >> 3;
    const sel_eid = args[0] >> 3;

    // Phase 11.12 — bytecode switchover. If the closure wraps an
    // AST_BLOCK_VT entity, compile to bytecode and install the
    // bytecode method. closureValueImpl + bytecodeDispatcherImpl
    // handle the value/value: path uniformly.
    if (extractBlockAstEidForCompile(args[1])) |block_ast_eid| {
        const method_eid = api.compile_block_as_method(block_ast_eid);
        if (method_eid != 0) {
            _ = api.assert_fact(vt_eid, sel_eid, mkEid(method_eid), M_USER);
            return s.receiver;
        }
    }

    _ = api.assert_fact(vt_eid, sel_eid, args[1], M_USER);
    return s.receiver;
}

fn extractBlockAstEidForCompile(closure_ref: Ref) ?u64 {
    const api = g_api.?;
    if (!isEid(closure_ref)) return null;
    const closure_eid = closure_ref >> 3;
    const closure_ptr_ref = api.object_at_eid(closure_eid);
    if ((closure_ptr_ref & 0b111) != 0) return null;
    if (closure_ptr_ref == 0) return null;
    const slot_base: [*]const Ref = @ptrFromInt(closure_ptr_ref);
    const wrapper_ref = slot_base[1];
    if ((wrapper_ref & 0b111) != 0) return null;
    const wrapper_obj: ?*anyopaque = @ptrFromInt(wrapper_ref);
    const block_ast_ref = api.slot_at(wrapper_obj, 0);
    if (!isEid(block_ast_ref)) return null;
    const block_ast_eid = block_ast_ref >> 3;
    const vt_ref = api.query_eavt(block_ast_eid, ATTR_VTABLE);
    if (!isEid(vt_ref)) return null;
    if ((vt_ref >> 3) != AST_BLOCK_VT) return null;
    return block_ast_eid;
}

/// `[block value]` / `[block value: x]` / `[block value: x value: y]`
/// dispatches via standard send.bind. The installed method's impl needs to
/// re-target s.closure at the receiver block (bind set it to the value-
/// method's own closure) before delegating to blockEvalShim.
fn closureValueImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    const api = g_api.?;
    if (!isEid(s.receiver)) return NIL;
    const closure_ptr_ref = api.object_at_eid(s.receiver >> 3);
    if ((closure_ptr_ref & 0b111) != 0) return NIL;
    if (closure_ptr_ref == 0) return NIL;
    var new_s = s.*;
    new_s.closure = @ptrFromInt(closure_ptr_ref);
    // Phase 11.12 — dispatch polymorphically through the receiver's own
    // closure method. Tree-walker block closures route to blockEvalShim;
    // bytecode-method closures route to the kernel's
    // bytecodeDispatcherImpl. Both have the same Imp signature so a
    // single indirect call suffices.
    const Imp = *const fn (*Send, [*]const Ref, usize) callconv(.c) Ref;
    const c = new_s.closure orelse return NIL;
    const slot_base: [*]const Ref = @ptrCast(@alignCast(c));
    const method_ptr_u64 = slot_base[0];
    const imp: Imp = @ptrFromInt(@as(usize, method_ptr_u64));
    return imp(&new_s, args, n_args);
}

fn trueIfTrueIfFalseImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = n_args;
    return g_api.?.send0(args[0], sel_value_g);
}

fn falseIfTrueIfFalseImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = n_args;
    return g_api.?.send0(args[1], sel_value_g);
}

fn trueIfTrueImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = n_args;
    return g_api.?.send0(args[0], sel_value_g);
}

fn falseIfTrueImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = args;
    _ = n_args;
    return NIL;
}

fn trueIfFalseImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = args;
    _ = n_args;
    return NIL;
}

fn falseIfFalseImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = n_args;
    return g_api.?.send0(args[0], sel_value_g);
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

// -- Tiered init (Phase 14 M3) ----------------------------------------------
// installMethods is split so AOT can keep runtime primitives while stripping
// the parser/compiler/eval frontend under --strip=malleable:
//   installPrimitives   — SmallInt/Float/Bool/Closure value methods (always)
//   installMethodDo     — VTABLE_VT method:do: (kept under --aot=dynamic)
//   installEvalMethods  — AST_*_VT tree-walker eval (frontend; stripped)
//   installParseSurface — parseExpr: on LANG_VT (frontend; stripped)
// Regrouping changes intern order vs. the old single fn, which is safe: the
// image carries the interner (Phase 12.4b), so selector eids are portable.

/// Tree-walker eval methods on the AST kind-vts. Frontend tier (stripped
/// under --strip=malleable — a pure-VM binary runs bytecode, not the AST).
fn installEvalMethods() void {
    const api = g_api.?;
    sel_eval = api.intern("eval".ptr, 4);
    const sel_eval_eid = sel_eval;
    _ = api.assert_fact(AST_LITERAL_VT, sel_eval_eid, mkEid(api.make_closure(@ptrCast(&evalLiteralImpl), NIL)), g_module_eid);
    _ = api.assert_fact(AST_SELF_VT, sel_eval_eid, mkEid(api.make_closure(@ptrCast(&evalSelfImpl), NIL)), g_module_eid);
    _ = api.assert_fact(AST_UNARY_SEND_VT, sel_eval_eid, mkEid(api.make_closure(@ptrCast(&evalUnarySendImpl), NIL)), g_module_eid);
    _ = api.assert_fact(AST_KEYWORD_SEND_VT, sel_eval_eid, mkEid(api.make_closure(@ptrCast(&evalKeywordSendImpl), NIL)), g_module_eid);
    _ = api.assert_fact(AST_BLOCK_VT, sel_eval_eid, mkEid(api.make_closure(@ptrCast(&evalBlockImpl), NIL)), g_module_eid);
    _ = api.assert_fact(AST_VAR_REF_VT, sel_eval_eid, mkEid(api.make_closure(@ptrCast(&evalVarRefImpl), NIL)), g_module_eid);
    _ = api.assert_fact(AST_LET_VT, sel_eval_eid, mkEid(api.make_closure(@ptrCast(&evalLetImpl), NIL)), g_module_eid);
    _ = api.assert_fact(AST_ASSIGN_VT, sel_eval_eid, mkEid(api.make_closure(@ptrCast(&evalAssignImpl), NIL)), g_module_eid);
    _ = api.assert_fact(AST_STMT_SEQ_VT, sel_eval_eid, mkEid(api.make_closure(@ptrCast(&evalStmtSeqImpl), NIL)), g_module_eid);
    _ = api.assert_fact(AST_RETURN_VT, sel_eval_eid, mkEid(api.make_closure(@ptrCast(&evalReturnImpl), NIL)), g_module_eid);
}

/// VTABLE_VT method:do: — user method install. Kept under --aot=dynamic
/// (runtime-loaded dylibs install bytecode methods); dropped under
/// static+malleable (frozen app, no install).
fn installMethodDo() void {
    const api = g_api.?;
    const method_do_sel = api.intern("method:do:".ptr, 10);
    _ = api.assert_fact(VTABLE_VT, method_do_sel, mkEid(api.make_closure(@ptrCast(&vtMethodDoImpl), NIL)), g_module_eid);
}

/// parseExpr: on the lang module entity. Frontend tier (stripped under
/// --strip=malleable — no source compilation in the shipped binary).
fn installParseSurface() void {
    const api = g_api.?;
    const sel_parse_expr = api.intern("parseExpr:".ptr, 10);
    sel_parse_expr_g = sel_parse_expr;
    _ = api.assert_fact(g_module_eid, ATTR_VTABLE, mkEid(LANG_VT), g_module_eid);
    _ = api.assert_fact(LANG_VT, sel_parse_expr, mkEid(api.make_closure(@ptrCast(&parseExprImpl), NIL)), g_module_eid);

    // `language` prototype (file-in). Pure facts (CODE / g_module_eid),
    // reconstructed each boot. fileIn: reads + evaluates a source file form by
    // form; needs the parser, so it lives in the frontend tier.
    _ = api.assert_fact(LANGUAGE_PROTO_VT, ATTR_VTABLE, mkEid(VTABLE_VT), g_module_eid);
    _ = api.assert_fact(LANGUAGE_PROTO_VT, ATTR_DELEGATE, mkEid(OBJECT_VT), g_module_eid);
    _ = api.assert_fact(LANGUAGE_PROTO_VT, api.intern("fileIn:".ptr, 7), mkEid(api.make_closure(@ptrCast(&fileInImpl), NIL)), g_module_eid);
    _ = api.assert_fact(LANGUAGE_PROTO_VT, api.intern("fileIn:onError:".ptr, 15), mkEid(api.make_closure(@ptrCast(&fileInOnErrorImpl), NIL)), g_module_eid);
    _ = api.assert_fact(LANGUAGE_PROTO_VT, api.intern("fileInSource:".ptr, 13), mkEid(api.make_closure(@ptrCast(&fileInSourceImpl), NIL)), g_module_eid);
    _ = api.assert_fact(LANGUAGE_PROTO_EID, ATTR_VTABLE, mkEid(LANGUAGE_PROTO_VT), g_module_eid);
    pushGlobalBinding(GLOBALS_SCOPE_EID, api.intern("language".ptr, 8), mkEid(LANGUAGE_PROTO_EID));
}

// -- file-in -----------------------------------------------------------------

/// Return the byte index of the end of the current top-form starting at
/// `start`: the next `;` at bracket-depth 0 that is not inside a string or
/// comment, or `src.len`. `;` is the statement separator the grammar already
/// uses inside blocks (stmt-list); splitting on it gives per-form granularity
/// for error reporting while letting forms span lines.
fn formEnd(src: []const u8, start: usize) usize {
    var i = start;
    var depth: i32 = 0;
    while (i < src.len) {
        const c = src[i];
        if (c == '"') { // string literal — no escapes in birnam grammar
            i += 1;
            while (i < src.len and src[i] != '"') : (i += 1) {}
            i += 1;
            continue;
        }
        if (c == '/' and i + 1 < src.len and src[i + 1] == '/') { // line comment
            i += 2;
            while (i < src.len and src[i] != '\n') : (i += 1) {}
            continue;
        }
        if (c == '/' and i + 1 < src.len and src[i + 1] == '*') { // block comment
            i += 2;
            while (i + 1 < src.len and !(src[i] == '*' and src[i + 1] == '/')) : (i += 1) {}
            i += 2;
            continue;
        }
        if (c == '[') depth += 1;
        if (c == ']') depth -= 1;
        if (c == ';' and depth == 0) return i;
        i += 1;
    }
    return src.len;
}

/// A parse result must be an AST eid that is not an ERROR_VT diagnostic.
fn isParseError(ast: Ref) bool {
    if (!isEid(ast)) return true;
    return g_api.?.query_eavt(ast >> 3, ATTR_VTABLE) == mkEid(ERROR_VT);
}

/// An eval result is an error only if it is an ERROR_VT entity. A bare value
/// (SmallInt / float / string / nil) is a perfectly good result, not a failure.
fn isEvalError(val: Ref) bool {
    if (!isEid(val)) return false;
    return g_api.?.query_eavt(val >> 3, ATTR_VTABLE) == mkEid(ERROR_VT);
}

/// Core: split `src` into `;`-separated top-forms, parse+eval each against the
/// top-level scope, then (commit) resolve pending FFI modules. Atomic mode
/// (`continue_on_error == false`) stops at the first error and returns NIL
/// WITHOUT resolving — so a failed unit applies no `dlopen`/wrap side effects
/// (deferred-dlopen = no broken state). Continue mode evaluates every good form
/// and reports each error. Returns SmallInt(# forms evaluated OK).
fn doFileInSource(src: []const u8, continue_on_error: bool) Ref {
    const api = g_api.?;
    var ok: i64 = 0;
    var errors: i64 = 0;
    var form_no: i64 = 0;
    var cursor: usize = 0;
    while (cursor < src.len) {
        const end = formEnd(src, cursor);
        const raw = src[cursor..end];
        cursor = end + 1; // skip the ';'
        const form = std.mem.trim(u8, raw, " \t\r\n");
        if (form.len == 0) continue;
        form_no += 1;

        const ast = api.send1(mkEid(g_module_eid), sel_parse_expr_g, api.make_string(form.ptr, form.len));
        var failed = isParseError(ast);
        if (!failed) {
            const val = api.send1(ast, sel_eval, mkEid(g_top_scope_eid));
            failed = isEvalError(val);
        }
        if (failed) {
            errors += 1;
            if (g_log) |log| {
                var buf: [320]u8 = undefined;
                const m = std.fmt.bufPrintZ(&buf, "[lang] fileIn: error at form {d}: {s}", .{ form_no, form }) catch continue;
                log(m);
            }
            if (!continue_on_error) {
                if (g_log) |log| log("[lang] fileIn: aborted (atomic) — no modules resolved");
                return NIL; // deferred dlopen never ran → nothing applied
            }
        } else {
            ok += 1;
        }
    }

    // Commit: bind FFI modules declared by this unit. Atomic mode only reaches
    // here when errors == 0.
    resolvePendingModules();
    if (g_log) |log| {
        var buf: [160]u8 = undefined;
        const m = std.fmt.bufPrintZ(&buf, "[lang] filed in {d} forms ({d} errors)", .{ ok, errors }) catch return mkSmallInt(ok);
        log(m);
    }
    return mkSmallInt(ok);
}

/// Read `path` and file it in (records a `:filed-in-unit` carrying path +
/// source for re-edit / freeze). Returns NIL if the path is unreadable.
fn doFileIn(path_ref: Ref, continue_on_error: bool) Ref {
    const api = g_api.?;
    if (!isString(path_ref)) return NIL;
    const path = stringBytes(path_ref);

    const src = std.fs.cwd().readFileAlloc(std.heap.page_allocator, path, 1 << 20) catch {
        if (g_log) |log| {
            var buf: [256]u8 = undefined;
            const m = std.fmt.bufPrintZ(&buf, "[lang] fileIn: cannot read {s}", .{path}) catch return NIL;
            log(m);
        }
        return NIL;
    };
    defer std.heap.page_allocator.free(src);

    const unit = api.alloc_eid();
    _ = api.assert_fact(unit, ATTR_FILED_IN_PATH, api.make_string(path.ptr, path.len), M_USER);
    _ = api.assert_fact(unit, ATTR_FILED_IN_SOURCE, api.make_string(src.ptr, src.len), M_USER);
    return doFileInSource(src, continue_on_error);
}

/// `[language fileIn: <path-string>]` — atomic file-in.
fn fileInImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = n_args;
    return doFileIn(args[0], false);
}

/// `[language fileInSource: <source-string>]` — atomic file-in of a source
/// string (no file read). Frozen wrapper modules call this with their embedded
/// source at `birnam_module_init` to re-establish the wraps.
fn fileInSourceImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = n_args;
    if (!isString(args[0])) return NIL;
    return doFileInSource(stringBytes(args[0]), false);
}

/// `[language fileIn: <path> onError: <#continue|#abort>]` — `#continue` keeps
/// going past errors (debug arbitrary files); anything else is atomic.
fn fileInOnErrorImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = n_args;
    const api = g_api.?;
    // args[1] is the mode selector/symbol Ref. #continue interns to "continue".
    var continue_mode = false;
    if (isEid(args[1])) {
        const want = api.intern("continue".ptr, 8);
        if ((args[1] >> 3) == want) continue_mode = true;
    }
    return doFileIn(args[0], continue_mode);
}

/// Runtime value methods — always kept (a stripped binary still does
/// arithmetic, control flow, iteration, block invocation).
// -- Phase 15.R R1d: db prototype + snapshot methods ----------------------

fn dbNowImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, args, n };
    return g_api.?.snapshot_now();
}
fn dbAsOfImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    return g_api.?.snapshot_as_of(@intCast(untagSmallInt(args[0])));
}
fn dbRetainAllImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    g_api.?.set_retention(1, 0);
    return s.receiver;
}
fn dbRetainCurrentImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    g_api.?.set_retention(0, 0);
    return s.receiver;
}
fn dbKeepLastImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    g_api.?.set_retention(3, @intCast(untagSmallInt(args[0])));
    return s.receiver;
}
fn dbKeepSinceImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    g_api.?.set_retention(2, @intCast(untagSmallInt(args[0])));
    return s.receiver;
}
fn dbExciseImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const ok = g_api.?.excise_entity(@intCast(untagSmallInt(args[0])));
    return if (ok) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}
fn snapBasisImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return g_api.?.snapshot_basis(s.receiver);
}
fn snapGetAttrImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return g_api.?.snapshot_get(s.receiver, @intCast(untagSmallInt(args[0])), @intCast(untagSmallInt(args[1])));
}
fn snapReleaseImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    g_api.?.snapshot_release(s.receiver);
    return NIL;
}

/// Install the `db` prototype + its methods + SNAPSHOT_VT methods, and bind the
/// global `db`. Phase 15.R R1d.
fn installDbSurface() void {
    const api = g_api.?;
    // db prototype object → its vtable.
    _ = api.assert_fact(DB_PROTO, ATTR_VTABLE, mkEid(DB_PROTO_VT), g_module_eid);

    const Pair = struct { name: []const u8, impl: *const anyopaque };
    const db_methods = [_]Pair{
        .{ .name = "now", .impl = @ptrCast(&dbNowImpl) },
        .{ .name = "asOf:", .impl = @ptrCast(&dbAsOfImpl) },
        .{ .name = "retainAll", .impl = @ptrCast(&dbRetainAllImpl) },
        .{ .name = "retainCurrent", .impl = @ptrCast(&dbRetainCurrentImpl) },
        .{ .name = "keepLast:", .impl = @ptrCast(&dbKeepLastImpl) },
        .{ .name = "keepSince:", .impl = @ptrCast(&dbKeepSinceImpl) },
        .{ .name = "excise:", .impl = @ptrCast(&dbExciseImpl) },
    };
    for (db_methods) |m| {
        const sel = api.intern(m.name.ptr, m.name.len);
        _ = api.assert_fact(DB_PROTO_VT, sel, mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }
    const snap_methods = [_]Pair{
        .{ .name = "basisT", .impl = @ptrCast(&snapBasisImpl) },
        .{ .name = "get:attr:", .impl = @ptrCast(&snapGetAttrImpl) },
        .{ .name = "release", .impl = @ptrCast(&snapReleaseImpl) },
    };
    for (snap_methods) |m| {
        const sel = api.intern(m.name.ptr, m.name.len);
        _ = api.assert_fact(SNAPSHOT_VT, sel, mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }

    pushGlobalBinding(GLOBALS_SCOPE_EID, api.intern("db".ptr, 2), mkEid(DB_PROTO));
}

// -- Phase 16 Tier A: persistent vector (RRB_VT) methods ------------------

/// Build a runtime ERROR_VT diagnostic carrying `msg`. The no-NULL "raise"
/// surface until a proper exception mechanism lands (Tier D).
fn mkRuntimeError(msg: []const u8) Ref {
    const api = g_api.?;
    const e = api.alloc_eid();
    _ = api.assert_fact(e, ATTR_VTABLE, mkEid(ERROR_VT), M_USER);
    _ = api.assert_fact(e, ATTR_ERROR_MESSAGE, api.make_string(msg.ptr, msg.len), M_USER);
    return mkEid(e);
}

fn vecWithImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return g_api.?.rrb_with(s.receiver, args[0]);
}
fn vecSizeImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return mkSmallInt(@intCast(g_api.?.rrb_len(s.receiver)));
}
fn vecAtImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    const i = untagSmallInt(args[0]);
    const len: i64 = @intCast(api.rrb_len(s.receiver));
    if (i < 0 or i >= len) return mkRuntimeError("vector index out of bounds");
    return api.rrb_nth(s.receiver, @intCast(i));
}
fn vecAtIfAbsentImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    const i = untagSmallInt(args[0]);
    const len: i64 = @intCast(api.rrb_len(s.receiver));
    if (i < 0 or i >= len) return api.send0(args[1], sel_value_g); // 0-arg ifAbsent block
    return api.rrb_nth(s.receiver, @intCast(i));
}
fn vecDoImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    const len = api.rrb_len(s.receiver);
    var i: u64 = 0;
    while (i < len) : (i += 1) _ = api.send1(args[0], sel_value_of_g, api.rrb_nth(s.receiver, i));
    return s.receiver;
}
fn vecInjectIntoImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    var acc = args[0];
    const block = args[1];
    const len = api.rrb_len(s.receiver);
    var i: u64 = 0;
    while (i < len) : (i += 1) acc = api.send2(block, sel_value_value_g, acc, api.rrb_nth(s.receiver, i));
    return acc;
}

fn installCollections() void {
    const api = g_api.?;
    const M = struct { name: []const u8, impl: *const anyopaque };
    const methods = [_]M{
        .{ .name = "with:", .impl = @ptrCast(&vecWithImpl) },
        .{ .name = "size", .impl = @ptrCast(&vecSizeImpl) },
        .{ .name = "at:", .impl = @ptrCast(&vecAtImpl) },
        .{ .name = "at:ifAbsent:", .impl = @ptrCast(&vecAtIfAbsentImpl) },
        .{ .name = "do:", .impl = @ptrCast(&vecDoImpl) },
        .{ .name = "inject:into:", .impl = @ptrCast(&vecInjectIntoImpl) },
    };
    for (methods) |m| {
        const sel = api.intern(m.name.ptr, m.name.len);
        _ = api.assert_fact(RRB_VT, sel, mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }
}

/// Phase 16 Tier B — install a method whose BODY is birnam source, compiled to
/// bytecode. The receiver-vt + selector are given by Zig (no `vtable` reflection
/// needed in source); the body self-hosts over the Tier A primitives. `src` is
/// a block literal `[:params | body]` (or `[| body]` for unary). Best-effort:
/// a parse/compile failure logs and skips (the prelude is vetted source).
fn installSourceMethod(vt: u64, sel_name: []const u8, src: []const u8) void {
    const api = g_api.?;
    const src_ref = api.make_string(src.ptr, src.len);
    const ast = api.send1(mkEid(g_module_eid), sel_parse_expr_g, src_ref);
    if (!isEid(ast)) {
        if (g_log) |log| log("[lang] prelude parse failed");
        return;
    }
    const method_eid = api.compile_block_as_method(ast >> 3);
    if (method_eid == 0) {
        if (g_log) |log| log("[lang] prelude compile failed");
        return;
    }
    _ = api.assert_fact(vt, api.intern(sel_name.ptr, sel_name.len), mkEid(method_eid), g_module_eid);
}

/// Phase 16 Tier B — the self-hosted collection protocol: `collect:`, `select:`,
/// `count`, `,` derived in birnam source over Tier A's `do:`/`inject:into:`/
/// `with:`/`size`. `self` inside each body = the receiver vector.
fn installPrelude() void {
    installSourceMethod(RRB_VT, "collect:", "[:f | [self inject: #[] into: [:acc :x | [acc with: [f value: x]]]]]");
    installSourceMethod(RRB_VT, "select:", "[:p | [self inject: #[] into: [:acc :x | [[p value: x] ifTrue: [| [acc with: x]] ifFalse: [| acc]]]]]");
    installSourceMethod(RRB_VT, "count", "[| [self size]]");
    installSourceMethod(RRB_VT, ",", "[:other | [other inject: self into: [:acc :x | [acc with: x]]]]");
}

fn installPrimitives() void {
    const api = g_api.?;
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
    const minus_sel = api.intern("-".ptr, 1);
    const div_sel = api.intern("/".ptr, 1);
    const eq_sel = api.intern("=".ptr, 1);
    const lt_sel = api.intern("<".ptr, 1);
    const gt_sel = api.intern(">".ptr, 1);
    _ = api.assert_fact(SMALLINT_VT, minus_sel, mkEid(api.make_closure(@ptrCast(&smallintMinusImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, div_sel, mkEid(api.make_closure(@ptrCast(&smallintDivImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, eq_sel, mkEid(api.make_closure(@ptrCast(&smallintEqImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, lt_sel, mkEid(api.make_closure(@ptrCast(&smallintLtImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, gt_sel, mkEid(api.make_closure(@ptrCast(&smallintGtImpl), NIL)), g_module_eid);
    const as_float_sel = api.intern("asFloat".ptr, 7);
    _ = api.assert_fact(SMALLINT_VT, as_float_sel, mkEid(api.make_closure(@ptrCast(&smallintAsFloatImpl), NIL)), g_module_eid);

    // Float arithmetic + comparisons + asInt + negate + exp:
    _ = api.assert_fact(FLOAT_VT, plus_sel, mkEid(api.make_closure(@ptrCast(&floatPlusImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FLOAT_VT, minus_sel, mkEid(api.make_closure(@ptrCast(&floatMinusImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FLOAT_VT, times_sel, mkEid(api.make_closure(@ptrCast(&floatTimesImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FLOAT_VT, div_sel, mkEid(api.make_closure(@ptrCast(&floatDivImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FLOAT_VT, eq_sel, mkEid(api.make_closure(@ptrCast(&floatEqImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FLOAT_VT, lt_sel, mkEid(api.make_closure(@ptrCast(&floatLtImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FLOAT_VT, gt_sel, mkEid(api.make_closure(@ptrCast(&floatGtImpl), NIL)), g_module_eid);
    const as_int_sel = api.intern("asInt".ptr, 5);
    _ = api.assert_fact(FLOAT_VT, as_int_sel, mkEid(api.make_closure(@ptrCast(&floatAsIntImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FLOAT_VT, negate_sel, mkEid(api.make_closure(@ptrCast(&floatNegateImpl), NIL)), g_module_eid);
    const exp_sel = api.intern("exp:".ptr, 4);
    _ = api.assert_fact(FLOAT_VT, exp_sel, mkEid(api.make_closure(@ptrCast(&floatExpImpl), NIL)), g_module_eid);

    const vt_sel = api.intern("vtable".ptr, 6);
    _ = api.assert_fact(SMALLINT_VT, vt_sel, mkEid(api.make_closure(@ptrCast(&smallintVtableImpl), NIL)), g_module_eid);

    // Block-invocation methods on CLOSURE_VT — receiver is mkEid(closure_eid),
    // closureValueImpl re-targets s.closure at the receiver and forwards to
    // blockEvalShim. Param-arity disambiguation falls out of the variadic ABI.
    sel_value_g = api.intern("value".ptr, 5);
    sel_value_of_g = api.intern("value:".ptr, 6);
    sel_value_value_g = api.intern("value:value:".ptr, 12);
    _ = api.assert_fact(CLOSURE_VT, sel_value_g, mkEid(api.make_closure(@ptrCast(&closureValueImpl), NIL)), g_module_eid);
    _ = api.assert_fact(CLOSURE_VT, sel_value_of_g, mkEid(api.make_closure(@ptrCast(&closureValueImpl), NIL)), g_module_eid);
    _ = api.assert_fact(CLOSURE_VT, sel_value_value_g, mkEid(api.make_closure(@ptrCast(&closureValueImpl), NIL)), g_module_eid);

    const while_true_sel = api.intern("whileTrue:".ptr, 10);
    const while_false_sel = api.intern("whileFalse:".ptr, 11);
    _ = api.assert_fact(CLOSURE_VT, while_true_sel, mkEid(api.make_closure(@ptrCast(&closureWhileTrueImpl), NIL)), g_module_eid);
    _ = api.assert_fact(CLOSURE_VT, while_false_sel, mkEid(api.make_closure(@ptrCast(&closureWhileFalseImpl), NIL)), g_module_eid);

    const times_repeat_sel = api.intern("timesRepeat:".ptr, 12);
    const to_do_sel = api.intern("to:do:".ptr, 6);
    _ = api.assert_fact(SMALLINT_VT, times_repeat_sel, mkEid(api.make_closure(@ptrCast(&smallintTimesRepeatImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, to_do_sel, mkEid(api.make_closure(@ptrCast(&smallintToDoImpl), NIL)), g_module_eid);

    // Boolean control-flow methods. Per Phase 10.0.x lock: birnam-source
    // one-liners are the spec, but native fn-ptrs ship the same semantics
    // and stay user-overrideable via [TRUE_VT method:do:].
    const if_true_sel = api.intern("ifTrue:".ptr, 7);
    const if_false_sel = api.intern("ifFalse:".ptr, 8);
    const if_true_if_false_sel = api.intern("ifTrue:ifFalse:".ptr, 15);
    _ = api.assert_fact(TRUE_VT, if_true_sel, mkEid(api.make_closure(@ptrCast(&trueIfTrueImpl), NIL)), g_module_eid);
    _ = api.assert_fact(TRUE_VT, if_false_sel, mkEid(api.make_closure(@ptrCast(&trueIfFalseImpl), NIL)), g_module_eid);
    _ = api.assert_fact(TRUE_VT, if_true_if_false_sel, mkEid(api.make_closure(@ptrCast(&trueIfTrueIfFalseImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FALSE_VT, if_true_sel, mkEid(api.make_closure(@ptrCast(&falseIfTrueImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FALSE_VT, if_false_sel, mkEid(api.make_closure(@ptrCast(&falseIfFalseImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FALSE_VT, if_true_if_false_sel, mkEid(api.make_closure(@ptrCast(&falseIfTrueIfFalseImpl), NIL)), g_module_eid);

    installCollections(); // Phase 16 Tier A — RRB_VT vector methods
}

/// Full method install (dylib / non-AOT path) — every tier.
fn installMethods() void {
    installEvalMethods();
    installPrimitives();
    installMethodDo();
    installParseSurface();
}

const LANG_VT: u64 = 103;

// -- Phase 10.5 REPL handler -----------------------------------------------

const ERROR_VT: u64 = 13;
const NIL_VT_EID: u64 = 7;
const FLOAT_VT: u64 = 10;

fn isFloatRef(r: Ref) bool {
    if ((r & 0b111) != 0) return false;
    if (r == 0) return false;
    const vt_ptr: *const Ref = @ptrFromInt(r - @sizeOf(Ref));
    return vt_ptr.* == mkEid(FLOAT_VT);
}

fn floatRefValue(r: Ref) f64 {
    const bits_ptr: *const u64 = @ptrFromInt(r);
    return @bitCast(bits_ptr.*);
}

fn replHandler(line_z: [*:0]const u8) callconv(.c) void {
    const api = g_api.?;
    var len: usize = 0;
    while (line_z[len] != 0) : (len += 1) {}
    if (len == 0) return;
    const line = line_z[0..len];

    // `:` prefix = debug command (Phase 10.5 minimum: :reset clears scope).
    if (line[0] == ':') {
        if (std.mem.eql(u8, line, ":reset")) {
            // Retract :scope-first-binding on the top-level scope; existing
            // BINDING_VT entities become unreachable (Phase 13.4 sweep).
            const cur = api.query_eavt(g_top_scope_eid, ATTR_SCOPE_FIRST_BINDING);
            if (cur != NIL) {
                _ = api.retract_fact(g_top_scope_eid, ATTR_SCOPE_FIRST_BINDING, cur, M_USER);
            }
            if (g_log) |log| log("[lang] scope reset");
            return;
        }
        if (g_log) |log| {
            var buf: [128]u8 = undefined;
            const m = std.fmt.bufPrintZ(&buf, "[lang] unknown debug command: {s}", .{line}) catch return;
            log(m);
        }
        return;
    }

    // Parse + eval. Clear unwinding state per-line so stray ^ at REPL
    // top level doesn't poison subsequent lines.
    g_unwinding = false;
    g_unwind_value = NIL;
    const src_ref = api.make_string(line.ptr, line.len);
    const ast_ref = api.send1(mkEid(g_module_eid), sel_parse_expr_g, src_ref);
    if (!isEid(ast_ref)) {
        if (g_log) |log| log("[lang] parse failed (non-eid result)");
        return;
    }
    const ast_eid = ast_ref >> 3;
    // Check for ERROR_VT.
    const vt = api.query_eavt(ast_eid, ATTR_VTABLE);
    if (vt == mkEid(ERROR_VT)) {
        if (g_log) |log| log("[lang] => error");
        return;
    }
    const val = api.send1(ast_ref, sel_eval, mkEid(g_top_scope_eid));
    // Line-commit: bind any FFI modules/wraps declared on this line (deferred
    // dlopen → resolve here for interactive eager feel). No-op without FFI.
    resolvePendingModules();
    if (g_log) |log| {
        var buf: [256]u8 = undefined;
        const m = formatValue(&buf, val) catch return;
        log(m);
    }
}

/// Send `[Module resolve]` — opens + binds any pending FFI modules. Invoked at
/// each commit boundary (REPL line, file-in commit). Cheap no-op without FFI.
fn resolvePendingModules() void {
    const api = g_api orelse return;
    _ = api.send0(mkEid(MODULE_PROTO_EID), api.intern("resolve".ptr, 7));
}

/// Phase 16 — a Ref is a vector iff it's pointer-tagged with vt = RRB_VT at -1
/// (mirrors isFloatRef).
fn isVectorRef(r: Ref) bool {
    if ((r & 0b111) != 0) return false;
    if (r == 0) return false;
    const vt_ptr: *const Ref = @ptrFromInt(r - @sizeOf(Ref));
    return vt_ptr.* == mkEid(RRB_VT);
}

/// Append a human display of `val` into `buf` at `off`, returning the new
/// offset. Best-effort (truncates on overflow). Recurses into vectors → `#[..]`.
fn appendRef(buf: []u8, off: usize, val: Ref) usize {
    if (isSmallInt(val)) return off + (std.fmt.bufPrint(buf[off..], "{d}", .{untagSmallInt(val)}) catch return off).len;
    if (isFloatRef(val)) return off + (std.fmt.bufPrint(buf[off..], "{d}", .{floatRefValue(val)}) catch return off).len;
    if (val == NIL) return off + (std.fmt.bufPrint(buf[off..], "nil", .{}) catch return off).len;
    if (isVectorRef(val)) {
        var o = off + (std.fmt.bufPrint(buf[off..], "#[", .{}) catch return off).len;
        const len = g_api.?.rrb_len(val);
        var i: u64 = 0;
        while (i < len) : (i += 1) {
            if (i > 0) o += (std.fmt.bufPrint(buf[o..], " ", .{}) catch return o).len;
            o = appendRef(buf, o, g_api.?.rrb_nth(val, i));
        }
        return o + (std.fmt.bufPrint(buf[o..], "]", .{}) catch return o).len;
    }
    if (isEid(val)) {
        const e = val >> 3;
        if (e == TRUE_EID) return off + (std.fmt.bufPrint(buf[off..], "true", .{}) catch return off).len;
        if (e == FALSE_EID) return off + (std.fmt.bufPrint(buf[off..], "false", .{}) catch return off).len;
        return off + (std.fmt.bufPrint(buf[off..], "<eid:{d}>", .{e}) catch return off).len;
    }
    if (isString(val)) return off + (std.fmt.bufPrint(buf[off..], "\"{s}\"", .{stringBytes(val)}) catch return off).len;
    return off + (std.fmt.bufPrint(buf[off..], "<ref:0x{x}>", .{val}) catch return off).len;
}

fn formatValue(buf: []u8, val: Ref) ![:0]u8 {
    var tmp: [1024]u8 = undefined;
    const n = appendRef(&tmp, 0, val);
    return std.fmt.bufPrintZ(buf, "=> {s}", .{tmp[0..n]});
}

fn isSmallInt(r: Ref) bool {
    return (r & 0b111) == 0b001;
}

// -- Module init -------------------------------------------------------------

// -- Tiered init entry points (Phase 14 M3) ---------------------------------
// The AOT generator composes these per --aot/--strip flags; the dylib path
// (birnam_module_init) calls them all. langSetup must run first; the AOT
// driver runs image.load between langSetup and langPrimitivesTier so the
// interner is restored and primitive selector interns resolve to the image's
// eids.

/// Wire module globals only — no interning, no installs.
pub fn langSetup(ctx: *ShellContext, api: *const IdApi, module_eid: u64) void {
    g_api = api;
    g_log = ctx.log;
    g_ctx = ctx;
    g_module_eid = module_eid;
}

/// Always-kept tier: runtime value methods + the stable top-level scope.
pub fn langPrimitivesTier() void {
    const api = g_api.?;
    installPrimitives();
    // Stable top-level REPL scope (Phase 12.4a) — CODE facts at a fixed eid
    // so persisted M_USER bindings re-attach on reload.
    g_top_scope_eid = TOP_SCOPE_EID;
    _ = api.assert_fact(g_top_scope_eid, ATTR_VTABLE, mkEid(SCOPE_VT), g_module_eid);
    _ = api.assert_fact(g_top_scope_eid, ATTR_SCOPE_TOP_LEVEL, mkEid(TRUE_EID), g_module_eid);
    // Built-in globals live in a parent scope so :reset / image :load (which
    // touch only TOP_SCOPE's binding chain) don't disturb them. CODE facts.
    installGlobals();
    installDbSurface(); // Phase 15.R R1d — `db` prototype + snapshot methods
    _ = api.assert_fact(g_top_scope_eid, ATTR_SCOPE_PARENT, mkEid(GLOBALS_SCOPE_EID), g_module_eid);
}

/// `method:do:` install tier — kept under --aot=dynamic, dropped under
/// static+malleable.
pub fn langMethodDoTier() void {
    installMethodDo();
}

/// Frontend tier — grammar + tree-walker eval + parseExpr + REPL handler.
/// Dropped under --strip=malleable.
pub fn langFrontendTier() void {
    const api = g_api.?;
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
    installEvalMethods();
    installParseSurface();

    // Export grammar EID on the module entity for the parser bootstrap.
    const sel_grammar = api.intern("grammar", 7);
    _ = api.assert_fact(g_module_eid, sel_grammar, mkEid(g_grammar_eid), g_module_eid);

    // Register the birnam-source REPL handler (drives parseExpr + eval).
    if (g_ctx) |ctx| ctx.set_repl_handler(&replHandler);
}

// Phase 14 M4: C entry exported only for dylib builds (see peg.zig).
const build_opts = @import("build_opts");

pub fn moduleInitImpl(
    ctx: *ShellContext,
    api: *const IdApi,
    module_eid: u64,
) callconv(.c) void {
    langSetup(ctx, api, module_eid);
    ctx.log("[lang] initializing");
    langFrontendTier();
    langPrimitivesTier();
    langMethodDoTier();
    installPrelude(); // Phase 16 Tier B — self-hosted collection protocol
    ctx.log("[lang] ready");
}

comptime {
    if (build_opts.export_init) @export(&moduleInitImpl, .{ .name = "birnam_module_init" });
}
