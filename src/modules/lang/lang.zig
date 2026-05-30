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

// Procedural seq/choice/charset (10.1.b escape hatch — bootstrap-only).
fn peg_seq(children: []const u64) Ref {
    const eid = g_api.?.peg_mk_seq(children.ptr, children.len);
    return mkEid(eid);
}

fn peg_choice(children: []const u64) Ref {
    const eid = g_api.?.peg_mk_choice(children.ptr, children.len);
    return mkEid(eid);
}

fn peg_charset(ranges: []const u8) Ref {
    const eid = g_api.?.peg_mk_charset(ranges.ptr, ranges.len);
    return mkEid(eid);
}

fn untagEid(r: Ref) u64 {
    return r >> 3;
}

// -- Birnam grammar construction --------------------------------------------

fn buildGrammar() void {
    const api = g_api.?;
    const grammar = api.send0(mkEid(PEG_EID), sel_new_grammar);
    g_grammar_eid = untagEid(grammar);

    // -- Primitives via charsets ------------------------------------------
    // whitespace-char = [ \t\n\r]
    const ws_ranges = [_]u8{ ' ', ' ', '\t', '\t', '\n', '\n', '\r', '\r' };
    defineRule(grammar, "whitespace-char", peg_charset(&ws_ranges));
    // whitespace = whitespace-char*
    defineRule(grammar, "whitespace", peg_star(peg_rule_ref("whitespace-char")));

    // digit = [0-9]
    const digit_ranges = [_]u8{ '0', '9' };
    defineRule(grammar, "digit", peg_charset(&digit_ranges));

    // letter = [a-zA-Z_]
    const letter_ranges = [_]u8{ 'a', 'z', 'A', 'Z', '_', '_' };
    defineRule(grammar, "letter", peg_charset(&letter_ranges));

    // alphanumeric = letter | digit
    {
        const alts = [_]u64{ untagEid(peg_rule_ref("letter")), untagEid(peg_rule_ref("digit")) };
        defineRule(grammar, "alphanumeric", peg_choice(&alts));
    }

    // identifier = letter alphanumeric*
    {
        const parts = [_]u64{
            untagEid(peg_rule_ref("letter")),
            untagEid(peg_star(peg_rule_ref("alphanumeric"))),
        };
        defineRule(grammar, "identifier", peg_seq(&parts));
    }

    // integer = digit+
    defineRule(grammar, "integer", peg_plus(peg_rule_ref("digit")));

    // -- Keyword literals (G4 reserved set) --------------------------------
    defineRule(grammar, "kw-nil", peg_literal("nil"));
    defineRule(grammar, "kw-true", peg_literal("true"));
    defineRule(grammar, "kw-false", peg_literal("false"));

    // atom = integer | kw-nil | kw-true | kw-false | identifier
    //                                              (order matters — PEG)
    // To avoid "true" being eaten as an identifier first, keyword rules
    // come before identifier in the choice. The proper fix (followed by
    // !alphanumeric in a keyword's seq) lands when reserved-word handling
    // is wired in a follow-up.
    {
        const atom_alts = [_]u64{
            untagEid(peg_rule_ref("integer")),
            untagEid(peg_rule_ref("kw-nil")),
            untagEid(peg_rule_ref("kw-true")),
            untagEid(peg_rule_ref("kw-false")),
            untagEid(peg_rule_ref("identifier")),
        };
        defineRule(grammar, "atom", peg_choice(&atom_alts));
    }

    // start = whitespace atom whitespace eof
    {
        const start_parts = [_]u64{
            untagEid(peg_rule_ref("whitespace")),
            untagEid(peg_rule_ref("atom")),
            untagEid(peg_rule_ref("whitespace")),
            untagEid(peg_eof()),
        };
        defineRule(grammar, "start", peg_seq(&start_parts));
    }
}

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
    sel_define_rule_as = internSel("defineRule:as:");

    buildGrammar();

    // Export grammar EID on the module entity: `[<module_eid> grammar]`
    // returns mkEid(g_grammar_eid). Phase 10.2 parser bootstrap reads it.
    const sel_grammar = api.intern("grammar", 7);
    _ = api.assert_fact(module_eid, sel_grammar, mkEid(g_grammar_eid), module_eid);

    ctx.log("[lang] ready");
}
