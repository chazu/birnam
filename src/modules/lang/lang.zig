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

/// Well-known kernel EIDs, derived from the single source of truth
/// (src/kernel/kernel_eids.zig) via the build-time mirror generator
/// (tools/gen_eid_mirror.zig). The `const NAME = ke.NAME;` aliases below keep
/// every use-site spelling unchanged while making the *values* track the kernel
/// automatically — a renumber propagates on rebuild; a rename fails the build.
const ke = @import("kernel_eids");

const abi = @import("module_abi");
const Ref = abi.Ref;
const NIL = abi.NIL;

const mkEid = abi.mkEid;

// Kernel-fixed EIDs (mirror src/kernel/kernel_eids.zig).
const PEG_EID = ke.PEG_EID;
const PEG_VT = ke.PEG_VT;

// AST kind vts + binding/scope vts + singletons.
const AST_LITERAL_VT = ke.AST_LITERAL_VT;
const AST_VAR_REF_VT = ke.AST_VAR_REF_VT;
const AST_SELF_VT = ke.AST_SELF_VT;
const AST_KEYWORD_SEND_VT = ke.AST_KEYWORD_SEND_VT;
const AST_UNARY_SEND_VT = ke.AST_UNARY_SEND_VT;
const AST_BINARY_SEND_VT = ke.AST_BINARY_SEND_VT;
const AST_BLOCK_VT = ke.AST_BLOCK_VT;
const AST_LET_VT = ke.AST_LET_VT;
const AST_ASSIGN_VT = ke.AST_ASSIGN_VT;
const AST_STMT_SEQ_VT = ke.AST_STMT_SEQ_VT;
const ATTR_AST_STMT_FIRST = ke.ATTR_AST_STMT_FIRST;
const AST_RETURN_VT = ke.AST_RETURN_VT;
const ATTR_AST_RETURN_VALUE = ke.ATTR_AST_RETURN_VALUE;
const ATTR_AST_BLOCK_BODY = ke.ATTR_AST_BLOCK_BODY;
const ATTR_AST_LET_INIT = ke.ATTR_AST_LET_INIT;
const ATTR_AST_ASSIGN_VALUE = ke.ATTR_AST_ASSIGN_VALUE;
const CLOSURE_VT = ke.CLOSURE_VT;
const VTABLE_VT = ke.VTABLE_VT;
const OBJECT_VT = ke.OBJECT_VT;
const SMALLINT_VT = ke.SMALLINT_VT;
const TRUE_EID = ke.TRUE_EID;
const FALSE_EID = ke.FALSE_EID;
// Phase 15.R R1d — mirrors of kernel_eids.
const SNAPSHOT_VT = ke.SNAPSHOT_VT;
const DB_PROTO_VT = ke.DB_PROTO_VT;
const DB_PROTO = ke.DB_PROTO;
const FILE_PROTO_VT = ke.FILE_PROTO_VT; // Phase 17.8 I/O — File singleton vtable
const FILE_PROTO = ke.FILE_PROTO;
const SYSTEM_PROTO_VT = ke.SYSTEM_PROTO_VT; // Phase 17.8 I/O — System singleton vtable
const SYSTEM_PROTO = ke.SYSTEM_PROTO;
const CST_NODE_VT = ke.CST_NODE_VT; // Phase 17.11 — parse-tree node navigation
const SYMBOL_VT = ke.SYMBOL_VT; // Phase 17.12 — symbol value protocol
const TIME_PROTO_VT = ke.TIME_PROTO_VT; // Phase 17.9 — Time wall clock
const TIME_PROTO = ke.TIME_PROTO;
const RANDOM_PROTO_VT = ke.RANDOM_PROTO_VT; // Phase 17.9 — Random PRNG
const RANDOM_PROTO = ke.RANDOM_PROTO;
// Phase 16 — RRB-backed vector vtable (mirror of kernel_eids.RRB_VT).
const RRB_VT = ke.RRB_VT;
const ASSOC_VT = ke.ASSOC_VT; // Phase 16 Tier C — map vtable (mirror)
const SEL_DNU = ke.SEL_DNU; // Phase 16 Tier D — doesNotUnderstand: (mirror)
const RANGE_VT = ke.RANGE_VT; // Phase 16 deferred — integer range vtable (mirror)
const STRING_VT = ke.STRING_VT; // Phase 16 deferred — string vtable (mirror)
const CELL_VT = ke.CELL_VT; // Phase 16 Tier E — reference cell vtable (mirror)
const ATTR_CELL_VALUE = ke.ATTR_CELL_VALUE; // Phase 16 Tier E — cell value attr (mirror)
const ATTR_META = ke.ATTR_META; // Phase 16 Tier E — metadata attr (mirror)
const XF_VT = ke.XF_VT; // Phase 16 Tier E — transducer factory vtable (mirror)
const XF_PROTO = ke.XF_PROTO; // Phase 16 Tier E — `Xf` singleton (mirror)
const ATTR_ERROR_MESSAGE = ke.ATTR_ERROR_MESSAGE; // mirror of kernel_eids.ATTR_ERROR_MESSAGE
const ATTR_ERROR_KIND = ke.ATTR_ERROR_KIND; // mirror — error taxonomy symbol (Result model)

const ATTR_AST_VALUE = ke.ATTR_AST_VALUE;
const ATTR_AST_NAME = ke.ATTR_AST_NAME;
const ATTR_AST_RECEIVER = ke.ATTR_AST_RECEIVER;
const ATTR_AST_SELECTOR = ke.ATTR_AST_SELECTOR;
const ATTR_AST_FIRST_ARG = ke.ATTR_AST_FIRST_ARG;
const ATTR_AST_NEXT_SIBLING = ke.ATTR_AST_NEXT_SIBLING;
const ATTR_AST_FIRST_CHILD = ke.ATTR_AST_FIRST_CHILD;
const ATTR_AST_RULE_NAME = ke.ATTR_AST_RULE_NAME;
const ATTR_AST_BYTE_START = ke.ATTR_AST_BYTE_START;
const ATTR_AST_BYTE_END = ke.ATTR_AST_BYTE_END;
const ATTR_VTABLE = ke.ATTR_VTABLE;
const ATTR_DELEGATE = ke.ATTR_DELEGATE;
/// Mirror of kernel_eids.M_USER (184) — provenance tag for runtime DATA
/// facts. Phase 12.0 provenance split.
const M_USER = ke.M_USER;

const ShellContext = abi.ShellContext;

const FactCallback = abi.FactCallback;

const IdApi = abi.IdApi;

// -- Module-static state -----------------------------------------------------

var g_api: ?*const IdApi = null;
var g_log: ?*const fn ([*:0]const u8) callconv(.c) void = null;
var g_ctx: ?*ShellContext = null;
var g_module_eid: u64 = 0;
var g_grammar_eid: u64 = 0;
var g_source: []const u8 = "";

// Cached rule-name EIDs (interned at module init).
var rn_integer: u64 = 0;
var rn_neg_integer: u64 = 0;
var rn_neg_float: u64 = 0;
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
var rn_query_var: u64 = 0;
var rn_clause_quote: u64 = 0;
var rn_block_bare: u64 = 0;
var rn_block_param: u64 = 0;
var rn_block_params: u64 = 0;
var rn_block_with_params: u64 = 0;
var rn_bracket_binary: u64 = 0;
var rn_vector_literal: u64 = 0;
var rn_pipe_primary: u64 = 0;
var rn_pipe_kw_arg: u64 = 0;
var rn_pipe_tail: u64 = 0;
var rn_pipe_expr: u64 = 0;
var rn_map_literal: u64 = 0;
var rn_map_entry: u64 = 0;
var rn_let_destructure: u64 = 0;
var rn_destructure_pattern: u64 = 0;
var g_destructure_counter: u64 = 0;
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

// SmallInt payload is 61-bit signed (3 tag bits). Values outside this range
// can't be represented and `mkSmallInt` would silently drop high bits, so
// arithmetic range-checks with `mkSmallIntChecked` (Phase 17.7 type-mismatch
// hardening).
const SMALLINT_MIN: i64 = -(@as(i64, 1) << 60);
const SMALLINT_MAX: i64 = (@as(i64, 1) << 60) - 1;

/// Tag `n` as a SmallInt, or null if it overflows the 61-bit payload range.
fn mkSmallIntChecked(n: i64) ?Ref {
    if (n < SMALLINT_MIN or n > SMALLINT_MAX) return null;
    return mkSmallInt(n);
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

    // Negative numeric literals (2026-06-17). `-` immediately followed by a
    // numeric literal, recognized ONLY at atom (primary) position — so binary
    // minus is unaffected: in `[a - b]` the `-` is consumed by `bracket-binary`'s
    // explicit `binary-op` between two expressions, never by `atom` (which is
    // only tried at an operand's start). This makes `[foo: -1]` / `#[1 -2 3]`
    // parse as a literal instead of a parse error. neg-float must precede
    // neg-integer in `atom` so `-1.5` matches the float, not `-1` then `.5`.
    {
        const parts = [_]u64{ untagEid(peg_literal("-")), untagEid(peg_rule_ref("integer")) };
        defineRule(grammar, "neg-integer", peg_seq(&parts));
    }
    {
        const parts = [_]u64{ untagEid(peg_literal("-")), untagEid(peg_rule_ref("float-literal")) };
        defineRule(grammar, "neg-float", peg_seq(&parts));
    }

    // -- String literal: "..." with backslash escapes (Phase 17) ----------
    // string = '"' ( ('\' any) / (!'"' !'\' any) )* '"'
    // The escape alternative consumes `\` + any byte so `\"` / `\\` don't
    // terminate; decodeStringLiteral turns \n \t \r \\ \" into bytes.
    {
        const quote_char = [_]u8{ '"', '"' };
        const bs_char = [_]u8{ '\\', '\\' };
        const quote = peg_charset(&quote_char);
        // escape = '\' any
        const escape_step = [_]u64{
            untagEid(peg_charset(&bs_char)),
            untagEid(peg_any()),
        };
        const escape = peg_seq(&escape_step);
        // plain = !'"' !'\' any
        const plain_step = [_]u64{
            untagEid(peg_not(peg_charset(&quote_char))),
            untagEid(peg_not(peg_charset(&bs_char))),
            untagEid(peg_any()),
        };
        const plain = peg_seq(&plain_step);
        const alt = [_]u64{ untagEid(escape), untagEid(plain) };
        const body_star = peg_star(peg_choice(&alt));
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
            untagEid(peg_rule_ref("neg-float")),
            untagEid(peg_rule_ref("neg-integer")),
            untagEid(peg_rule_ref("float-literal")),
            untagEid(peg_rule_ref("integer")),
            untagEid(peg_rule_ref("string")),
            untagEid(peg_rule_ref("symbol-literal")),
            untagEid(peg_rule_ref("clause-quote")),
            untagEid(peg_rule_ref("query-var")),
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

    // query-var = "?" identifier  (Phase 7 datalog surface). Evaluates to a
    // symbol whose name carries the leading "?" — `findAll:where:` reads that
    // prefix to classify a clause term as a query variable. The CST span IS the
    // "?name" text.
    {
        const parts = [_]u64{
            untagEid(peg_literal("?")),
            untagEid(peg_rule_ref("identifier")),
        };
        defineRule(grammar, "query-var", peg_seq(&parts));
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

    // block-param = ":" (destructure-pattern | identifier)
    // A destructure-pattern param `:#[a b]` binds the elements of a vector arg —
    // desugared to a fresh temp param + leading `let`s in the body (Phase 16
    // deferred nicety). `destructure-pattern` is defined later in the same
    // grammar build; rule-refs resolve by interned name at parse time, so the
    // forward reference is fine.
    {
        const alt = [_]u64{
            untagEid(peg_rule_ref("destructure-pattern")),
            untagEid(peg_rule_ref("identifier")),
        };
        const parts = [_]u64{
            untagEid(peg_literal(":")),
            untagEid(peg_choice(&alt)),
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

    // binary-op = op-char op-char? op-char?   — one to three operator chars, so
    // 2-char ops (`<=`/`>=`) and the 3-char `<=>` spaceship (Phase 17.3) tokenize
    // as one selector. The optional chars only match another op-char, so `[a + b]`
    // (space after `+`) still reads a single `+`. The CST byte-span IS the selector.
    {
        const op_ranges = [_]u8{ '+', '+', '-', '-', '*', '*', '/', '/', '<', '<', '>', '>', '=', '=', ',', ',', '&', '&', '|', '|' };
        const op_one = peg_charset(&op_ranges);
        const op_two = peg_charset(&op_ranges);
        const op_three = peg_charset(&op_ranges);
        const op_parts = [_]u64{ untagEid(op_one), untagEid(peg_opt(op_two)), untagEid(peg_opt(op_three)) };
        defineRule(grammar, "binary-op", peg_seq(&op_parts));
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

    // -- Phase 7 datalog quote sugar: clause-list -------------------------
    // clause-quote = "#(" ws (bracket-keyword ws)* ")"
    // Each `[?e attr: ?v]` clause is QUOTED — lowered to a constant clause-vector
    // `#[?e #attr ?v]` (terms as values, NOT evaluated as a send), so the whole
    // `#(…)` is the clause-list value `[db findAll:where:]` consumes. Predicates /
    // #not / #or stay in the explicit `#[ … ]` vector form.
    {
        const elem_parts = [_]u64{
            untagEid(peg_rule_ref("bracket-keyword")),
            untagEid(peg_rule_ref("ws")),
        };
        const elem = peg_seq(&elem_parts);
        const parts = [_]u64{
            untagEid(peg_literal("#(")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_star(elem)),
            untagEid(peg_literal(")")),
        };
        defineRule(grammar, "clause-quote", peg_seq(&parts));
    }

    // -- Phase 16 Tier C: map literal -------------------------------------
    // map-entry   = kw-suffix ws expression
    // map-literal = "{" ws (map-entry ws)* "}"   (keyword-keyed, space-sep)
    {
        const me_parts = [_]u64{
            untagEid(peg_rule_ref("kw-suffix")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("expression")),
        };
        defineRule(grammar, "map-entry", peg_seq(&me_parts));
        const entry_parts = [_]u64{
            untagEid(peg_rule_ref("map-entry")),
            untagEid(peg_rule_ref("ws")),
        };
        const entry = peg_seq(&entry_parts);
        const parts = [_]u64{
            untagEid(peg_literal("{")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_star(entry)),
            untagEid(peg_literal("}")),
        };
        defineRule(grammar, "map-literal", peg_seq(&parts));
    }

    // -- Phase 16: |> pipe operator ---------------------------------------
    // Thread-as-receiver (Clojure ->). `x |> with: 1 |> select: even`
    // desugars to `[[x with: 1] select: even]`. The piped message's keyword
    // args are pipe-primary (non-pipe) so the chain stays LEFT-associative
    // (the arg doesn't swallow the next `|>`).
    {
        const alts = [_]u64{
            untagEid(peg_rule_ref("bracket-form")),
            untagEid(peg_rule_ref("vector-literal")),
            untagEid(peg_rule_ref("map-literal")),
            untagEid(peg_rule_ref("atom")),
        };
        defineRule(grammar, "pipe-primary", peg_choice(&alts));
    }
    {
        // pipe-kw-arg = ws kw-suffix ws pipe-primary
        const parts = [_]u64{
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("kw-suffix")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("pipe-primary")),
        };
        defineRule(grammar, "pipe-kw-arg", peg_seq(&parts));
    }
    {
        // pipe-tail = ws "|>" ws (pipe-kw-arg+ | identifier)
        const msg_alts = [_]u64{
            untagEid(peg_plus(peg_rule_ref("pipe-kw-arg"))),
            untagEid(peg_rule_ref("identifier")),
        };
        const parts = [_]u64{
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_literal("|>")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_choice(&msg_alts)),
        };
        defineRule(grammar, "pipe-tail", peg_seq(&parts));
    }
    {
        // pipe-expr = pipe-primary pipe-tail+
        const parts = [_]u64{
            untagEid(peg_rule_ref("pipe-primary")),
            untagEid(peg_plus(peg_rule_ref("pipe-tail"))),
        };
        defineRule(grammar, "pipe-expr", peg_seq(&parts));
    }

    // expression = pipe-expr | bracket-form | vector-literal | atom
    // (pipe-expr first; it needs ≥1 `|>`, else backtracks to the others)
    {
        const alts = [_]u64{
            untagEid(peg_rule_ref("pipe-expr")),
            untagEid(peg_rule_ref("bracket-form")),
            untagEid(peg_rule_ref("vector-literal")),
            untagEid(peg_rule_ref("map-literal")),
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

    // -- Phase 16 deferred: let destructuring ----------------------------
    // destructure-pattern = "#[" ws (identifier ws)* "]"
    // let-destructure     = kw-let ws destructure-pattern ws ":=" ws expression
    {
        const id_parts = [_]u64{
            untagEid(peg_rule_ref("identifier")),
            untagEid(peg_rule_ref("ws")),
        };
        const id = peg_seq(&id_parts);
        const pat_parts = [_]u64{
            untagEid(peg_literal("#[")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_star(id)),
            untagEid(peg_literal("]")),
        };
        defineRule(grammar, "destructure-pattern", peg_seq(&pat_parts));
        const parts = [_]u64{
            untagEid(peg_rule_ref("kw-let")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("destructure-pattern")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_literal(":=")),
            untagEid(peg_rule_ref("ws")),
            untagEid(peg_rule_ref("expression")),
        };
        defineRule(grammar, "let-destructure", peg_seq(&parts));
    }

    // top-form = let-destructure | let-statement | return-statement | assignment | expression
    {
        const alts = [_]u64{
            untagEid(peg_rule_ref("let-destructure")),
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
/// Phase 16 — build one piped send (`recv |> <tail>`): keyword send when the
/// tail holds pipe-kw-arg children, else unary send from its identifier.
fn desugarPipeTail(recv_ast: u64, tail_eid: u64) u64 {
    const api = g_api.?;
    const span = cstByteRange(tail_eid);
    var sel_buf: [128]u8 = undefined;
    var sel_len: usize = 0;
    var first_arg_ast: u64 = 0;
    var last_arg_ast: u64 = 0;
    var saw_kw = false;
    var c = cstFirstChild(tail_eid);
    while (c != 0) : (c = cstNextSibling(c)) {
        if (cstRuleName(c) != rn_pipe_kw_arg) continue;
        saw_kw = true;
        var ac = cstFirstChild(c);
        var arg_ast: u64 = 0;
        while (ac != 0) : (ac = cstNextSibling(ac)) {
            const acrn = cstRuleName(ac);
            if (acrn == rn_kw_suffix) {
                var sc = cstFirstChild(ac);
                while (sc != 0) : (sc = cstNextSibling(sc)) {
                    if (cstRuleName(sc) == rn_identifier) {
                        const ids = cstByteRange(sc);
                        const nb = g_source[ids.start..ids.end];
                        if (sel_len + nb.len + 1 < sel_buf.len) {
                            @memcpy(sel_buf[sel_len .. sel_len + nb.len], nb);
                            sel_len += nb.len;
                            sel_buf[sel_len] = ':';
                            sel_len += 1;
                        }
                        break;
                    }
                }
            } else if (acrn == rn_pipe_primary) {
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
    if (saw_kw) {
        const sel_eid = api.intern(sel_buf[0..sel_len].ptr, sel_len);
        const send = newAstNode(AST_KEYWORD_SEND_VT, span.start, span.end);
        _ = api.assert_fact(send, ATTR_AST_RECEIVER, mkEid(recv_ast), M_USER);
        _ = api.assert_fact(send, ATTR_AST_SELECTOR, mkEid(sel_eid), M_USER);
        if (first_arg_ast != 0) _ = api.assert_fact(send, ATTR_AST_FIRST_ARG, mkEid(first_arg_ast), M_USER);
        return send;
    }
    // Unary: the lone identifier child is the selector.
    c = cstFirstChild(tail_eid);
    while (c != 0) : (c = cstNextSibling(c)) {
        if (cstRuleName(c) == rn_identifier) {
            const ids = cstByteRange(c);
            const nb = g_source[ids.start..ids.end];
            const sel_eid = api.intern(nb.ptr, nb.len);
            const send = newAstNode(AST_UNARY_SEND_VT, span.start, span.end);
            _ = api.assert_fact(send, ATTR_AST_RECEIVER, mkEid(recv_ast), M_USER);
            _ = api.assert_fact(send, ATTR_AST_SELECTOR, mkEid(sel_eid), M_USER);
            return send;
        }
    }
    return recv_ast;
}

/// Evaluate a quote-term CST to its CONSTANT Ref (datalog clause sugar). Drills
/// through wrapper rules (expression→atom→…) to the leaf term: `?var`/`#sym`/bare
/// identifier → a symbol; string/integer/float → that literal value.
fn quoteTermValue(cst_eid: u64) Ref {
    const api = g_api.?;
    var c = cst_eid;
    var guard: u32 = 0;
    while (c != 0 and guard < 32) : (guard += 1) {
        const rn = cstRuleName(c);
        const span = cstByteRange(c);
        if (rn == rn_query_var or rn == rn_identifier) {
            const b = g_source[span.start..span.end]; // ?name or bare name → symbol
            return mkEid(api.intern(b.ptr, b.len));
        }
        if (rn == rn_symbol_literal) {
            const b = g_source[span.start + 1 .. span.end]; // strip leading '#'
            return mkEid(api.intern(b.ptr, b.len));
        }
        if (rn == rn_string) {
            return decodeStringLiteral(g_source[span.start + 1 .. span.end - 1]);
        }
        if (rn == rn_integer) {
            var n: i64 = 0;
            for (g_source[span.start..span.end]) |ch| n = n * 10 + (ch - '0');
            return mkSmallInt(n);
        }
        if (rn == rn_neg_integer) {
            // span includes the leading '-'; parseInt handles the sign.
            return mkSmallInt(std.fmt.parseInt(i64, g_source[span.start..span.end], 10) catch 0);
        }
        if (rn == rn_float_literal or rn == rn_neg_float) {
            return api.make_float(std.fmt.parseFloat(f64, g_source[span.start..span.end]) catch 0.0);
        }
        // Wrapper rule — descend to the first non-ws child.
        var child = cstFirstChild(c);
        while (child != 0 and cstRuleName(child) == rn_ws) child = cstNextSibling(child);
        c = child;
    }
    return NIL;
}

/// Lower a quoted single-keyword clause `[?e attr: ?v]` to a constant clause
/// vector `#[ ?e-value  #attr  ?v-value ]`. The attribute is the selector name
/// without its trailing colon, as a symbol.
fn quoteKeywordClause(kw_cst: u64) Ref {
    const api = g_api.?;
    var recv: Ref = NIL;
    var attr: Ref = NIL;
    var arg: Ref = NIL;
    var c = cstFirstChild(kw_cst);
    while (c != 0) : (c = cstNextSibling(c)) {
        const crn = cstRuleName(c);
        if (crn == rn_expression and recv == NIL) {
            recv = quoteTermValue(c); // receiver = the entity term
        } else if (crn == rn_kw_arg) {
            // kw-arg children: ws, kw-suffix (the attr name + ':'), ws, expression.
            var ac = cstFirstChild(c);
            while (ac != 0) : (ac = cstNextSibling(ac)) {
                const acrn = cstRuleName(ac);
                if (acrn == rn_kw_suffix) {
                    var sc = cstFirstChild(ac);
                    while (sc != 0) : (sc = cstNextSibling(sc)) {
                        if (cstRuleName(sc) == rn_identifier) {
                            const sp = cstByteRange(sc);
                            const b = g_source[sp.start..sp.end];
                            attr = mkEid(api.intern(b.ptr, b.len));
                            break;
                        }
                    }
                } else if (acrn == rn_expression) {
                    arg = quoteTermValue(ac);
                }
            }
        }
    }
    var parts = [3]Ref{ recv, attr, arg };
    return api.make_rrb(&parts, 3);
}

/// Build the `let <id_k> := [tmp at: k]` chain for a `destructure-pattern` CST
/// node, binding each pattern identifier to the k-th element of `tmp_eid`.
/// Returns the chain head/tail (linked via `:next-sibling`), or {0,0} if empty.
/// Shared by `let #[a b] := expr` and block-param destructuring `[:#[a b]| …]`.
fn buildDestructureLets(pattern_cst: u64, tmp_eid: u64, start: usize, end: usize) struct { first: u64, last: u64 } {
    const api = g_api.?;
    const at_sel = api.intern("at:".ptr, 3);
    var first: u64 = 0;
    var last: u64 = 0;
    var k: i64 = 0;
    var pc = cstFirstChild(pattern_cst);
    while (pc != 0) : (pc = cstNextSibling(pc)) {
        if (cstRuleName(pc) != rn_identifier) continue;
        const id_span = cstByteRange(pc);
        const nb = g_source[id_span.start..id_span.end];
        const name_eid = api.intern(nb.ptr, nb.len);
        const tmp_ref = newAstNode(AST_VAR_REF_VT, start, end);
        _ = api.assert_fact(tmp_ref, ATTR_AST_NAME, mkEid(tmp_eid), M_USER);
        const idx_lit = newAstNode(AST_LITERAL_VT, start, end);
        _ = api.assert_fact(idx_lit, ATTR_AST_VALUE, mkSmallInt(k), M_USER);
        const send = newAstNode(AST_KEYWORD_SEND_VT, start, end);
        _ = api.assert_fact(send, ATTR_AST_RECEIVER, mkEid(tmp_ref), M_USER);
        _ = api.assert_fact(send, ATTR_AST_SELECTOR, mkEid(at_sel), M_USER);
        _ = api.assert_fact(send, ATTR_AST_FIRST_ARG, mkEid(idx_lit), M_USER);
        const binding = newAstNode(AST_LET_VT, start, end);
        _ = api.assert_fact(binding, ATTR_AST_NAME, mkEid(name_eid), M_USER);
        _ = api.assert_fact(binding, ATTR_AST_LET_INIT, mkEid(send), M_USER);
        if (first == 0) {
            first = binding;
        } else {
            _ = api.assert_fact(last, ATTR_AST_NEXT_SIBLING, mkEid(binding), M_USER);
        }
        last = binding;
        k += 1;
    }
    return .{ .first = first, .last = last };
}

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
    if (rn == rn_neg_integer) {
        // span includes the leading '-'; parseInt reads the sign.
        const n = std.fmt.parseInt(i64, g_source[span.start..span.end], 10) catch 0;
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, mkSmallInt(n), M_USER);
        return ast;
    }
    if (rn == rn_float_literal or rn == rn_neg_float) {
        const bytes = g_source[span.start..span.end];
        const f = std.fmt.parseFloat(f64, bytes) catch 0.0;
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, api.make_float(f), M_USER);
        return ast;
    }
    if (rn == rn_string) {
        // Strip surrounding quotes — span.start+1 .. span.end-1 — and decode
        // backslash escapes (\n \t \r \\ \" ; unknown \x → x literal).
        const bytes = g_source[span.start + 1 .. span.end - 1];
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, decodeStringLiteral(bytes), M_USER);
        return ast;
    }
    if (rn == rn_kw_nil) {
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        // nil literal: store no value (absent → evalLiteral returns NIL; a NIL
        // value is forbidden by Date discipline — asserting it panics in debug).
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
        // nil literal: store no value (absent → evalLiteral returns NIL; a NIL
        // value is forbidden by Date discipline — asserting it panics in debug).
        return ast;
    }
    if (rn == rn_query_var) {
        // Span = "?name". Intern the WHOLE span (keeping the "?") so the symbol's
        // name carries the prefix; `findAll:where:` classifies it as a variable.
        const bytes = g_source[span.start..span.end];
        const name_eid = api.intern(bytes.ptr, bytes.len);
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, mkEid(name_eid), M_USER);
        return ast;
    }
    if (rn == rn_clause_quote) {
        // Quoted clause-list. Each `[?e attr: ?v]` keyword clause → a constant
        // clause-vector `#[?e #attr ?v]`; the whole `#(…)` is a literal vector of
        // them (terms are all constants/`?vars`, so it builds at lower time —
        // same pattern as a string literal storing its decoded Ref in :ast-value).
        var outer = api.make_rrb(@ptrCast(&[_]Ref{}), 0);
        var c = cstFirstChild(cst_eid);
        while (c != 0) : (c = cstNextSibling(c)) {
            if (cstRuleName(c) != rn_bracket_keyword) continue;
            outer = api.rrb_with(outer, quoteKeywordClause(c));
        }
        const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
        _ = api.assert_fact(ast, ATTR_AST_VALUE, outer, M_USER);
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
        // nil literal: store no value (absent → evalLiteral returns NIL; a NIL
        // value is forbidden by Date discipline — asserting it panics in debug).
        return ast;
    }
    if (rn == rn_block_with_params) {
        // Children: ws + block-params + ws + stmt-list. Each block-param is
        // either `:name` (identifier) or `:#[a b]` (destructure-pattern). A
        // destructured param becomes a fresh temp param + `let a := [tmp at: 0]`
        // bindings prepended to the body — reusing the let-destructure machinery,
        // so the compiler/evaluator need no param-pattern awareness.
        var c = cstFirstChild(cst_eid);
        var first_param_eid: u64 = 0;
        var last_param_eid: u64 = 0;
        var body_ast: u64 = 0;
        var dt_first: u64 = 0; // head of the prepended destructure-`let` chain
        var dt_last: u64 = 0;
        while (c != 0) : (c = cstNextSibling(c)) {
            const crn = cstRuleName(c);
            if (crn == rn_block_params) {
                var p = cstFirstChild(c);
                while (p != 0) : (p = cstNextSibling(p)) {
                    if (cstRuleName(p) != rn_block_param) continue;
                    // The param's payload is a destructure-pattern OR an identifier.
                    var param_name_eid: u64 = 0;
                    var ic = cstFirstChild(p);
                    while (ic != 0) : (ic = cstNextSibling(ic)) {
                        const icrn = cstRuleName(ic);
                        if (icrn == rn_destructure_pattern) {
                            // Fresh temp param name; bind its elements in the body.
                            var nbuf: [24]u8 = undefined;
                            const tn = std.fmt.bufPrint(&nbuf, "__bp{d}", .{g_destructure_counter}) catch "__bp";
                            g_destructure_counter += 1;
                            param_name_eid = api.intern(tn.ptr, tn.len);
                            const lets = buildDestructureLets(ic, param_name_eid, span.start, span.end);
                            if (lets.first != 0) {
                                if (dt_first == 0) {
                                    dt_first = lets.first;
                                } else {
                                    _ = api.assert_fact(dt_last, ATTR_AST_NEXT_SIBLING, mkEid(lets.first), M_USER);
                                }
                                dt_last = lets.last;
                            }
                            break;
                        } else if (icrn == rn_identifier) {
                            const id_span = cstByteRange(ic);
                            const bytes = g_source[id_span.start..id_span.end];
                            param_name_eid = api.intern(bytes.ptr, bytes.len);
                            break;
                        }
                    }
                    if (param_name_eid == 0) continue;
                    const pe = api.alloc_eid();
                    _ = api.assert_fact(pe, ATTR_AST_NAME, mkEid(param_name_eid), M_USER);
                    if (first_param_eid == 0) {
                        first_param_eid = pe;
                    } else {
                        _ = api.assert_fact(last_param_eid, ATTR_AST_NEXT_SIBLING, mkEid(pe), M_USER);
                    }
                    last_param_eid = pe;
                }
            } else if (crn == rn_stmt_list) {
                body_ast = desugar(c);
            }
        }
        // Splice the destructure `let`s ahead of the body's statements. If the
        // body is itself a STMT_SEQ, flatten its statement chain after the lets;
        // otherwise treat the single body expr as the trailing statement.
        if (dt_first != 0) {
            var body_first = body_ast;
            if (body_ast != 0 and api.query_eavt(body_ast, ATTR_VTABLE) == mkEid(AST_STMT_SEQ_VT)) {
                body_first = api.query_eavt(body_ast, ATTR_AST_STMT_FIRST) >> 3; // eid payload
            }
            if (body_first != 0) _ = api.assert_fact(dt_last, ATTR_AST_NEXT_SIBLING, mkEid(body_first), M_USER);
            const seq = newAstNode(AST_STMT_SEQ_VT, span.start, span.end);
            _ = api.assert_fact(seq, ATTR_AST_STMT_FIRST, mkEid(dt_first), M_USER);
            body_ast = seq;
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
    if (rn == rn_let_destructure) {
        // `let #[a b c] := expr` → a STMT_SEQ: bind a fresh temp to expr (once),
        // then `let <id_k> := [tmp at: k]` for each pattern name in order.
        var c = cstFirstChild(cst_eid);
        var pattern_eid: u64 = 0;
        var init_ast: u64 = 0;
        while (c != 0) : (c = cstNextSibling(c)) {
            const crn = cstRuleName(c);
            if (crn == rn_destructure_pattern) {
                pattern_eid = c;
            } else if (crn == rn_expression) {
                init_ast = desugar(c);
            }
        }
        // Fresh temp name (gensym) to evaluate the source exactly once.
        var nbuf: [24]u8 = undefined;
        const tmp_name = std.fmt.bufPrint(&nbuf, "__dt{d}", .{g_destructure_counter}) catch "__dt";
        g_destructure_counter += 1;
        const tmp_eid = api.intern(tmp_name.ptr, tmp_name.len);

        const tmp_let = newAstNode(AST_LET_VT, span.start, span.end);
        _ = api.assert_fact(tmp_let, ATTR_AST_NAME, mkEid(tmp_eid), M_USER);
        if (init_ast != 0) _ = api.assert_fact(tmp_let, ATTR_AST_LET_INIT, mkEid(init_ast), M_USER);

        if (pattern_eid != 0) {
            const lets = buildDestructureLets(pattern_eid, tmp_eid, span.start, span.end);
            if (lets.first != 0) _ = api.assert_fact(tmp_let, ATTR_AST_NEXT_SIBLING, mkEid(lets.first), M_USER);
        }
        const seq = newAstNode(AST_STMT_SEQ_VT, span.start, span.end);
        _ = api.assert_fact(seq, ATTR_AST_STMT_FIRST, mkEid(tmp_let), M_USER);
        return seq;
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
    if (rn == rn_pipe_expr) {
        // pipe-primary, then pipe-tail+. Fold left: each tail wraps the
        // accumulated receiver in a unary/keyword send.
        var c = cstFirstChild(cst_eid);
        var recv_ast: u64 = 0;
        while (c != 0) : (c = cstNextSibling(c)) {
            const crn = cstRuleName(c);
            if (crn == rn_ws) continue;
            if (crn == rn_pipe_primary) {
                recv_ast = desugar(c);
                c = cstNextSibling(c);
                break;
            }
        }
        while (c != 0) : (c = cstNextSibling(c)) {
            if (cstRuleName(c) != rn_pipe_tail) continue;
            recv_ast = desugarPipeTail(recv_ast, c);
        }
        return recv_ast;
    }
    if (rn == rn_atom or rn == rn_expression or rn == rn_bracket_form or rn == rn_start or rn == rn_top_form or rn == rn_pipe_primary) {
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
        // nil literal: store no value (absent → evalLiteral returns NIL; a NIL
        // value is forbidden by Date discipline — asserting it panics in debug).
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

    if (rn == rn_map_literal) {
        // {k: v ...} desugars to a left-nested at:put: chain over an empty map
        // literal: [[ {} at: #k put: v] at: #k2 put: v2]. Pure sends + literals
        // → works in tree-walker AND bytecode.
        const empty_ast = newAstNode(AST_LITERAL_VT, span.start, span.start);
        _ = api.assert_fact(empty_ast, ATTR_AST_VALUE, api.make_map(), M_USER);
        const at_put_sel = api.intern("at:put:".ptr, 7);
        var recv_ast = empty_ast;
        var c = cstFirstChild(cst_eid);
        while (c != 0) : (c = cstNextSibling(c)) {
            if (cstRuleName(c) != rn_map_entry) continue;
            // map-entry children: kw-suffix, ws, expression.
            var key_eid: u64 = 0;
            var val_ast: u64 = 0;
            var ec = cstFirstChild(c);
            while (ec != 0) : (ec = cstNextSibling(ec)) {
                const ecrn = cstRuleName(ec);
                if (ecrn == rn_kw_suffix) {
                    var sc = cstFirstChild(ec);
                    while (sc != 0) : (sc = cstNextSibling(sc)) {
                        if (cstRuleName(sc) == rn_identifier) {
                            const ids = cstByteRange(sc);
                            const nb = g_source[ids.start..ids.end];
                            key_eid = api.intern(nb.ptr, nb.len);
                            break;
                        }
                    }
                } else if (ecrn == rn_expression) {
                    val_ast = desugar(ec);
                }
            }
            if (key_eid == 0 or val_ast == 0) continue;
            const key_ast = newAstNode(AST_LITERAL_VT, span.start, span.start);
            _ = api.assert_fact(key_ast, ATTR_AST_VALUE, mkEid(key_eid), M_USER);
            const send = newAstNode(AST_KEYWORD_SEND_VT, span.start, span.end);
            _ = api.assert_fact(send, ATTR_AST_RECEIVER, mkEid(recv_ast), M_USER);
            _ = api.assert_fact(send, ATTR_AST_SELECTOR, mkEid(at_put_sel), M_USER);
            _ = api.assert_fact(send, ATTR_AST_FIRST_ARG, mkEid(key_ast), M_USER);
            _ = api.assert_fact(key_ast, ATTR_AST_NEXT_SIBLING, mkEid(val_ast), M_USER);
            recv_ast = send;
        }
        return recv_ast;
    }

    // Unknown rule — fallback to a nil literal (no value asserted → NIL).
    const ast = newAstNode(AST_LITERAL_VT, span.start, span.end);
    return ast;
}

fn cacheRuleNames() void {
    const api = g_api.?;
    rn_integer = api.intern("integer".ptr, 7);
    rn_neg_integer = api.intern("neg-integer".ptr, 11);
    rn_neg_float = api.intern("neg-float".ptr, 9);
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
    rn_query_var = api.intern("query-var".ptr, 9);
    rn_clause_quote = api.intern("clause-quote".ptr, 12);
    rn_block_bare = api.intern("block-bare".ptr, 10);
    rn_block_param = api.intern("block-param".ptr, 11);
    rn_block_params = api.intern("block-params".ptr, 12);
    rn_block_with_params = api.intern("block-with-params".ptr, 17);
    rn_bracket_binary = api.intern("bracket-binary".ptr, 14);
    rn_vector_literal = api.intern("vector-literal".ptr, 14);
    rn_pipe_primary = api.intern("pipe-primary".ptr, 12);
    rn_pipe_kw_arg = api.intern("pipe-kw-arg".ptr, 11);
    rn_pipe_tail = api.intern("pipe-tail".ptr, 9);
    rn_pipe_expr = api.intern("pipe-expr".ptr, 9);
    rn_map_literal = api.intern("map-literal".ptr, 11);
    rn_map_entry = api.intern("map-entry".ptr, 9);
    rn_let_destructure = api.intern("let-destructure".ptr, 15);
    rn_destructure_pattern = api.intern("destructure-pattern".ptr, 19);
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
// Cluster ① — selectors the top-level-mode compiler lowers free vars /
// outer `let`/`:=` to, dispatched against SCOPE_VT entities.
var sel_scope_lookup: u64 = 0;
var sel_scope_bind: u64 = 0;
var sel_scope_set: u64 = 0;

const EvalMode = enum { bytecode, treewalk };
/// Cluster ① — top-level eval strategy. `.bytecode` (default) compiles
/// top-level/REPL/script statements AND user methods to bytecode; `.treewalk`
/// forces the AST tree-walker for both — an inspectable fallback for debugging
/// a suspected miscompile or comparing the two paths. Session-only (not imaged);
/// toggled by the REPL `:eval-mode` command.
var g_eval_mode: EvalMode = .bytecode;
const TRUE_VT = ke.TRUE_VT;
const FALSE_VT = ke.FALSE_VT;

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

const SCOPE_VT = ke.SCOPE_VT;
const BINDING_VT = ke.BINDING_VT;
const ATTR_SCOPE_PARENT = ke.ATTR_SCOPE_PARENT;
const ATTR_SCOPE_SELF = ke.ATTR_SCOPE_SELF;
const ATTR_SCOPE_FIRST_BINDING = ke.ATTR_SCOPE_FIRST_BINDING;
const ATTR_SCOPE_TOP_LEVEL = ke.ATTR_SCOPE_TOP_LEVEL;
const ATTR_BIND_NAME = ke.ATTR_BIND_NAME;
const ATTR_BIND_VALUE = ke.ATTR_BIND_VALUE;
const ATTR_BIND_NEXT = ke.ATTR_BIND_NEXT;

/// Mirror of kernel_eids.TOP_SCOPE_EID (185) — stable well-known eid for the
/// top-level REPL scope so persisted bindings survive save/load.
const TOP_SCOPE_EID = ke.TOP_SCOPE_EID;

/// Mirror of kernel_eids.GLOBALS_SCOPE_EID (186) — parent of TOP_SCOPE; holds
/// built-in global bindings (e.g. `Module`). Kept out of the top scope so
/// `:reset` and image `:load` (which touch only TOP_SCOPE's binding chain)
/// don't disturb globals.
const GLOBALS_SCOPE_EID = ke.GLOBALS_SCOPE_EID;

/// Mirror of kernel_eids.MODULE_PROTO (15) — the `Module` prototype object the
/// FFI surface installs `load:` on. Bound to the global name `Module`.
const MODULE_PROTO_EID = ke.MODULE_PROTO;

/// Mirrors of kernel_eids — the `language` prototype (file-in) + filed-in-unit
/// attributes. lang owns the `fileIn:` methods; the kernel just reserves eids.
const LANGUAGE_PROTO_VT = ke.LANGUAGE_PROTO_VT;
const LANGUAGE_PROTO_EID = ke.LANGUAGE_PROTO;
const ATTR_FILED_IN_PATH = ke.ATTR_FILED_IN_PATH;
const ATTR_FILED_IN_SOURCE = ke.ATTR_FILED_IN_SOURCE;

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
    // lookupInScope returns the bound value (possibly nil) or null when the
    // name is genuinely unbound. Result model: unbound → reported error, not a
    // silent nil indistinguishable from a nil-valued binding.
    if (lookupInScope(scope_ref >> 3, name_eid)) |v| return v;
    const nm = api.symbol_name(name_eid);
    const kind = api.intern("undefinedVariable".ptr, 17);
    if (nm != NIL and isString(nm)) return mkRuntimeErrorFmt(kind, "undefined variable: {s}", .{stringBytes(nm)});
    return mkRuntimeErrorKind(kind, "undefined variable");
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
//
// Phase 17.7 type-mismatch hardening: a SmallInt op never `untagSmallInt`s a
// non-int argument (that reinterprets the tagged bits as a garbage integer).
// Integer args take the integer path (overflow → `#overflow` ERROR_VT, not a
// safe-build panic / silent high-bit drop); Float args promote to f64 (so mixed
// `1 + 2.5` works); anything else → `#typeMismatch` ERROR_VT (or `false` for
// `=`, `nil` for `<=>` — those are "incomparable", not errors).

fn typeMismatchError() Ref {
    return mkRuntimeErrorKind(g_api.?.intern("typeMismatch".ptr, 12), "arithmetic on a non-number");
}
fn overflowError() Ref {
    return mkRuntimeErrorKind(g_api.?.intern("overflow".ptr, 8), "integer overflow");
}
fn isNumberRef(r: Ref) bool {
    return isSmallInt(r) or isFloatRef(r);
}
fn boolRef(b: bool) Ref {
    return if (b) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}

fn smallintNegateImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    return mkSmallIntChecked(-untagSmallInt(s.receiver)) orelse overflowError();
}

fn smallintSquaredImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const n = untagSmallInt(s.receiver);
    const r = @mulWithOverflow(n, n);
    if (r[1] != 0) return overflowError();
    return mkSmallIntChecked(r[0]) orelse overflowError();
}

fn smallintSuccImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    return mkSmallIntChecked(untagSmallInt(s.receiver) + 1) orelse overflowError();
}

fn smallintPlusImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const a_ = untagSmallInt(s.receiver);
    if (isSmallInt(args[0])) {
        // Both operands are 61-bit, so the i64 sum can't overflow — only the
        // SmallInt range can.
        return mkSmallIntChecked(a_ + untagSmallInt(args[0])) orelse overflowError();
    }
    if (isFloatRef(args[0])) return g_api.?.make_float(@as(f64, @floatFromInt(a_)) + floatRefValue(args[0]));
    return typeMismatchError();
}

fn smallintTimesImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const a_ = untagSmallInt(s.receiver);
    if (isSmallInt(args[0])) {
        const r = @mulWithOverflow(a_, untagSmallInt(args[0]));
        if (r[1] != 0) return overflowError();
        return mkSmallIntChecked(r[0]) orelse overflowError();
    }
    if (isFloatRef(args[0])) return g_api.?.make_float(@as(f64, @floatFromInt(a_)) * floatRefValue(args[0]));
    return typeMismatchError();
}

fn smallintMinusImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const a_ = untagSmallInt(s.receiver);
    if (isSmallInt(args[0])) return mkSmallIntChecked(a_ - untagSmallInt(args[0])) orelse overflowError();
    if (isFloatRef(args[0])) return g_api.?.make_float(@as(f64, @floatFromInt(a_)) - floatRefValue(args[0]));
    return typeMismatchError();
}
fn smallintDivImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const a_ = untagSmallInt(s.receiver);
    if (isSmallInt(args[0])) {
        const b_ = untagSmallInt(args[0]);
        if (b_ == 0) return mkRuntimeErrorKind(g_api.?.intern("zeroDivide".ptr, 10), "divide by zero");
        return mkSmallIntChecked(@divTrunc(a_, b_)) orelse overflowError();
    }
    if (isFloatRef(args[0])) {
        const b = floatRefValue(args[0]);
        if (b == 0.0) return mkRuntimeErrorKind(g_api.?.intern("zeroDivide".ptr, 10), "divide by zero");
        return g_api.?.make_float(@as(f64, @floatFromInt(a_)) / b);
    }
    return typeMismatchError();
}
fn smallintEqImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const a_ = untagSmallInt(s.receiver);
    if (isSmallInt(args[0])) return boolRef(a_ == untagSmallInt(args[0]));
    if (isFloatRef(args[0])) return boolRef(@as(f64, @floatFromInt(a_)) == floatRefValue(args[0]));
    return mkEid(FALSE_EID); // `1 = #foo` is false, not an error
}
fn smallintLtImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const a_ = untagSmallInt(s.receiver);
    if (isSmallInt(args[0])) return boolRef(a_ < untagSmallInt(args[0]));
    if (isFloatRef(args[0])) return boolRef(@as(f64, @floatFromInt(a_)) < floatRefValue(args[0]));
    return typeMismatchError();
}
fn smallintGtImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const a_ = untagSmallInt(s.receiver);
    if (isSmallInt(args[0])) return boolRef(a_ > untagSmallInt(args[0]));
    if (isFloatRef(args[0])) return boolRef(@as(f64, @floatFromInt(a_)) > floatRefValue(args[0]));
    return typeMismatchError();
}
/// Three-way compare (Phase 17.3) — `[a <=> b]` / `[a compare: b]` → -1/0/1.
/// nil for a non-number argument (incomparable). Float args compare numerically.
fn smallintCompareImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const a = untagSmallInt(s.receiver);
    if (isSmallInt(args[0])) {
        const b = untagSmallInt(args[0]);
        return mkSmallInt(if (a < b) -1 else if (a > b) 1 else 0);
    }
    if (isFloatRef(args[0])) {
        const af: f64 = @floatFromInt(a);
        const b = floatRefValue(args[0]);
        return mkSmallInt(if (af < b) -1 else if (af > b) 1 else 0);
    }
    return NIL;
}

fn smallintAsFloatImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const api = g_api.?;
    return api.make_float(@floatFromInt(untagSmallInt(s.receiver)));
}

// -- SmallInt bitwise + shift (2026-06-17) --------------------------------
// Integer-only: a Float / non-number argument is a `#typeMismatch` (bit ops on
// a non-integer are meaningless — unlike `+`, there is no Float promotion).
// Results are range-checked with `mkSmallIntChecked` like arithmetic, so a
// bit-set that escapes the 61-bit payload is a `#overflow`, never a silent
// high-bit drop. NOTE: `&`/`|` stay boolean-only (TRUE_VT/FALSE_VT); bitwise
// gets distinct names so the int-vs-bool meaning is never ambiguous. These are
// the substrate for FFI flag sets (`O_NONBLOCK bitOr: O_CLOEXEC`), `htons`/
// `htonl` byteswap, and unpacking struct bit-fields after a width peek.

fn shiftAmountError() Ref {
    return mkRuntimeErrorKind(g_api.?.intern("shiftAmount".ptr, 11), "shift count out of range 0..60");
}

fn smallintBitAndImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    if (!isSmallInt(args[0])) return typeMismatchError();
    return mkSmallIntChecked(untagSmallInt(s.receiver) & untagSmallInt(args[0])) orelse overflowError();
}
fn smallintBitOrImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    if (!isSmallInt(args[0])) return typeMismatchError();
    return mkSmallIntChecked(untagSmallInt(s.receiver) | untagSmallInt(args[0])) orelse overflowError();
}
fn smallintBitXorImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    if (!isSmallInt(args[0])) return typeMismatchError();
    return mkSmallIntChecked(untagSmallInt(s.receiver) ^ untagSmallInt(args[0])) orelse overflowError();
}
fn smallintBitNotImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    // ~x = -x-1, always within the 61-bit range when x is.
    return mkSmallIntChecked(~untagSmallInt(s.receiver)) orelse overflowError();
}
fn smallintShlImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    if (!isSmallInt(args[0])) return typeMismatchError();
    const b = untagSmallInt(args[0]);
    if (b < 0 or b > 60) return shiftAmountError();
    const r = std.math.shl(i64, untagSmallInt(s.receiver), @as(u6, @intCast(b)));
    // shl can push bits past the 61-bit payload → caught as overflow.
    return mkSmallIntChecked(r) orelse overflowError();
}
fn smallintShrImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    if (!isSmallInt(args[0])) return typeMismatchError();
    const b = untagSmallInt(args[0]);
    if (b < 0 or b > 60) return shiftAmountError();
    // Arithmetic shift (signed) — result magnitude only shrinks, always fits.
    return mkSmallInt(std.math.shr(i64, untagSmallInt(s.receiver), @as(u6, @intCast(b))));
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
    if (!isNumberRef(args[0])) return typeMismatchError();
    return g_api.?.make_float(floatRefValue(s.receiver) + argAsF64(args[0]));
}
fn floatMinusImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    if (!isNumberRef(args[0])) return typeMismatchError();
    return g_api.?.make_float(floatRefValue(s.receiver) - argAsF64(args[0]));
}
fn floatTimesImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    if (!isNumberRef(args[0])) return typeMismatchError();
    return g_api.?.make_float(floatRefValue(s.receiver) * argAsF64(args[0]));
}
fn floatDivImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    if (!isNumberRef(args[0])) return typeMismatchError();
    const b_ = argAsF64(args[0]);
    if (b_ == 0.0) return mkRuntimeErrorKind(g_api.?.intern("zeroDivide".ptr, 10), "divide by zero");
    return g_api.?.make_float(floatRefValue(s.receiver) / b_);
}
fn floatEqImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    if (!isNumberRef(args[0])) return mkEid(FALSE_EID); // `2.5 = #foo` is false
    return if (floatRefValue(s.receiver) == argAsF64(args[0])) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}
fn floatLtImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    if (!isNumberRef(args[0])) return typeMismatchError();
    return if (floatRefValue(s.receiver) < argAsF64(args[0])) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}
fn floatGtImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    if (!isNumberRef(args[0])) return typeMismatchError();
    return if (floatRefValue(s.receiver) > argAsF64(args[0])) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}

/// `[2.5 <=> x]` / `[2.5 compare: x]` — three-way compare → -1 / 0 / 1 SmallInt
/// (nil if the arg is not a number). SmallInt args promote to f64 (Phase 17.3).
fn floatCompareImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    if (!isNumberRef(args[0])) return NIL;
    const a = floatRefValue(s.receiver);
    const b = argAsF64(args[0]);
    return mkSmallInt(if (a < b) -1 else if (a > b) 1 else 0);
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
    // If the rhs errored, propagate it rather than binding the error.
    if (isErrorValue(new_val)) return new_val;
    if (mutateBinding(scope_ref >> 3, name_ref >> 3, new_val)) {
        return new_val;
    }
    const nm = api.symbol_name(name_ref >> 3);
    const kind = api.intern("undefinedAssignment".ptr, 19);
    if (nm != NIL and isString(nm)) return mkRuntimeErrorFmt(kind, "assignment to undefined variable: {s} (declare with `let` first)", .{stringBytes(nm)});
    return mkRuntimeErrorKind(kind, "assignment to undefined variable");
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
        // Result model: an error halts the sequence (later statements don't run)
        // and propagates out as the sequence's value.
        if (isErrorValue(last_val)) return last_val;
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

// -- Cluster ① — SCOPE_VT methods backing top-level compiled var access -----
// The top-level-mode compiler lowers a free variable / outermost `let`/`:=`
// into sends against the scope entity: `[scope lookup: #name]`,
// `[scope bind: #name value: v]`, `[scope set: #name value: v]`. These mirror
// the tree-walker's evalVarRef / evalLet / evalAssign exactly (same scope
// walk, same error values) so a compiled top-level line behaves identically
// to a tree-walked one. Receiver = the scope entity (mkEid); args are the
// name symbol (and value).

fn scopeLookupImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    const api = g_api.?;
    if (n_args < 1 or !isEid(s.receiver) or !isEid(args[0])) return NIL;
    const name_eid = args[0] >> 3;
    if (lookupInScope(s.receiver >> 3, name_eid)) |v| return v;
    const nm = api.symbol_name(name_eid);
    const kind = api.intern("undefinedVariable".ptr, 17);
    if (nm != NIL and isString(nm)) return mkRuntimeErrorFmt(kind, "undefined variable: {s}", .{stringBytes(nm)});
    return mkRuntimeErrorKind(kind, "undefined variable");
}

fn scopeBindValueImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (n_args < 2 or !isEid(s.receiver) or !isEid(args[0])) return NIL;
    // Matches evalLet: bind whatever the init produced (an error value
    // included) and return it.
    pushScopeBinding(s.receiver >> 3, args[0] >> 3, args[1]);
    return args[1];
}

fn scopeSetValueImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    const api = g_api.?;
    if (n_args < 2 or !isEid(s.receiver) or !isEid(args[0])) return NIL;
    const value = args[1];
    // Matches evalAssign: propagate an error rhs rather than storing it.
    if (isErrorValue(value)) return value;
    if (mutateBinding(s.receiver >> 3, args[0] >> 3, value)) return value;
    const nm = api.symbol_name(args[0] >> 3);
    const kind = api.intern("undefinedAssignment".ptr, 19);
    if (nm != NIL and isString(nm)) return mkRuntimeErrorFmt(kind, "assignment to undefined variable: {s} (declare with `let` first)", .{stringBytes(nm)});
    return mkRuntimeErrorKind(kind, "assignment to undefined variable");
}

/// Cluster ① — evaluate a parsed top-level statement. In `.bytecode` mode,
/// compile + run it on the VM (the kernel lowers free vars / outer `let`/`:=`
/// to the SCOPE_VT sends above); fall back to the tree-walker when the
/// statement can't be compiled (unsupported AST kind, etc.) or eval-mode is
/// `.treewalk`. A compile failure happens before any execution, so the
/// fallback re-run is side-effect-clean.
fn evalTopLevelStmt(ast_ref: Ref) Ref {
    const api = g_api.?;
    if (g_eval_mode == .bytecode and isEid(ast_ref)) {
        var ok: bool = false;
        const result = api.eval_top_level(ast_ref >> 3, g_top_scope_eid, sel_scope_lookup, sel_scope_bind, sel_scope_set, &ok);
        if (ok) return result;
    }
    return api.send1(ast_ref, sel_eval, mkEid(g_top_scope_eid));
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

    // Install a proper `:bytecode-stream` method (NOT the raw closure value) so
    // prune's Root 2 can root it. Two shapes of block arg:
    //  1. Compiled top-level (Cluster ①): the block arrives as a *bytecode*
    //     closure (MAKE_BLOCK). `method_eid_of_bytecode_closure` extracts its
    //     already-compiled method eid → install that directly.
    //  2. Tree-walker closure (uncompiled block): `extractBlockAstEidForCompile`
    //     reads its wrapped AST_BLOCK_VT → `compile_block_as_method`.
    // (Installing the raw closure — the old fallback when neither applied — made
    // an un-rootable method that prune dropped, masked by a stale dispatch
    // cache; see ROADMAP "15.A-AST". The eval-mode toggle no longer gates this:
    // methods always compile so they stay prune-rootable.)
    const direct = api.method_eid_of_bytecode_closure(args[1] >> 3);
    if (direct != 0) {
        _ = api.assert_fact(vt_eid, sel_eid, mkEid(direct), M_USER);
        return s.receiver;
    }
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
    // Pass an ERROR_VT diagnostic through unchanged; otherwise desugar the CST.
    // (CST nodes now carry CST_NODE_VT, so the check must be ERROR_VT-specific,
    // not "has any vtable".)
    const vt = api.query_eavt(cst_eid, ATTR_VTABLE);
    if (isEid(vt) and untagEid(vt) == ERROR_VT) return cst_result;
    return mkEid(desugar(cst_eid));
}

fn isString(r: Ref) bool {
    return (r & 0b111) == 0b100;
}

/// Phase 17 — decode a string-literal body (quotes already stripped): turn
/// `\n \t \r \\ \"` into their bytes, `\0` into NUL; an unknown `\x` becomes a
/// literal `x`. Fast path (no backslash, or body > 4096) copies raw.
fn decodeStringLiteral(bytes: []const u8) Ref {
    const api = g_api.?;
    if (bytes.len > 4096 or std.mem.indexOfScalar(u8, bytes, '\\') == null) {
        return api.make_string(bytes.ptr, bytes.len);
    }
    var buf: [4096]u8 = undefined;
    var o: usize = 0;
    var i: usize = 0;
    while (i < bytes.len) : (i += 1) {
        if (bytes[i] == '\\' and i + 1 < bytes.len) {
            i += 1;
            buf[o] = switch (bytes[i]) {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                '0' => 0,
                else => bytes[i], // \\ \" and unknown → the char itself
            };
        } else {
            buf[o] = bytes[i];
        }
        o += 1;
    }
    return api.make_string(buf[0..o].ptr, o);
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

    // Cluster ① — SCOPE_VT methods the top-level-mode compiler targets for
    // free-var reads (`lookup:`) and outer `let`/`:=` (`bind:value:`/`set:value:`).
    sel_scope_lookup = api.intern("lookup:".ptr, 7);
    sel_scope_bind = api.intern("bind:value:".ptr, 11);
    sel_scope_set = api.intern("set:value:".ptr, 10);
    _ = api.assert_fact(SCOPE_VT, sel_scope_lookup, mkEid(api.make_closure(@ptrCast(&scopeLookupImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SCOPE_VT, sel_scope_bind, mkEid(api.make_closure(@ptrCast(&scopeBindValueImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SCOPE_VT, sel_scope_set, mkEid(api.make_closure(@ptrCast(&scopeSetValueImpl), NIL)), g_module_eid);
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
    // `[language evalLine: aString]` — evaluate one REPL line exactly as the
    // shell's stdin loop does (parse top-form, eval, print `=> value` / error,
    // and lang `:` commands like :eval-mode/:reset). Lets a birnam-authored REPL
    // reuse the real eval+print instead of reinventing it. Returns nil.
    _ = api.assert_fact(LANGUAGE_PROTO_VT, api.intern("evalLine:".ptr, 9), mkEid(api.make_closure(@ptrCast(&evalLineImpl), NIL)), g_module_eid);
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
        if (c == '"') { // string literal — skip body, honoring \" / \\ escapes
            i += 1;
            while (i < src.len and src[i] != '"') {
                if (src[i] == '\\' and i + 1 < src.len) i += 2 else i += 1;
            }
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
            const val = evalTopLevelStmt(ast);
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

        // Reclaim this form's run-and-discard garbage (its CST, AST, and the
        // top-level wrapper method tree) before moving on. Threshold-gated, so
        // it only actually compacts every ~30k new entities — bounding PEAK
        // memory during a large file-in instead of carrying every form's dead
        // tree to the end. The VM is quiescent here (the form fully evaluated),
        // the same boundary the REPL prunes at. No-op under retention/snapshots.
        api.maybe_prune();
    }

    // Commit: bind FFI modules declared by this unit. Atomic mode only reaches
    // here when errors == 0.
    resolvePendingModules();

    // Load boundary: one forced compaction. The inter-form `maybe_prune` above
    // bounded PEAK during the load; this collapses the surviving program's
    // per-fact history to a single basis and sweeps the final sub-threshold
    // tail of run-and-discard garbage, minimizing the STEADY-STATE footprint the
    // loaded program then runs in. No-op under retention / live snapshots.
    api.prune_now();

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

// -- Phase 7 datalog surface: [db findAll:where:] -------------------------

/// If `ref` is a `?`-prefixed symbol (a query variable), return its name WITHOUT
/// the leading `?`; else null (it is a constant term).
fn queryVarName(ref: Ref) ?[]const u8 {
    if (!isEid(ref)) return null;
    const nm = g_api.?.symbol_name(ref >> 3);
    if (!isString(nm)) return null;
    const b = stringBytes(nm);
    if (b.len < 2 or b[0] != '?') return null;
    return b[1..];
}

/// If `ref` is a comparison-operator symbol (the middle term of a predicate
/// clause), return its query op code (0=eq,1=ne,2=lt,3=le,4=gt,5=ge); else null
/// (the clause is a triple). Named ops — `#<` won't parse (`<` is not an
/// identifier char), so predicates read `#[?a #lt ?b]`.
fn predicateOp(ref: Ref) ?u32 {
    if (!isEid(ref)) return null;
    const nm = g_api.?.symbol_name(ref >> 3);
    if (!isString(nm)) return null;
    const b = stringBytes(nm);
    const ops = [_]struct { n: []const u8, op: u32 }{
        .{ .n = "eq", .op = 0 }, .{ .n = "ne", .op = 1 },
        .{ .n = "lt", .op = 2 }, .{ .n = "le", .op = 3 },
        .{ .n = "gt", .op = 4 }, .{ .n = "ge", .op = 5 },
    };
    for (ops) |o| {
        if (std.mem.eql(u8, b, o.n)) return o.op;
    }
    return null;
}

/// True iff `ref` is a symbol with exactly name `name` (a reserved control word).
fn symbolIs(ref: Ref, name: []const u8) bool {
    if (!isEid(ref)) return false;
    const nm = g_api.?.symbol_name(ref >> 3);
    if (!isString(nm)) return false;
    return std.mem.eql(u8, stringBytes(nm), name);
}

/// If `ref` is an aggregate-op symbol (`#count`/`#sum`/`#min`/`#max`/`#avg`/
/// `#collect`), return its kind code (count=0,sum=1,min=2,max=3,avg=4,collect=5).
fn aggOp(ref: Ref) ?u32 {
    if (!isEid(ref)) return null;
    const nm = g_api.?.symbol_name(ref >> 3);
    if (!isString(nm)) return null;
    const b = stringBytes(nm);
    const ks = [_]struct { n: []const u8, k: u32 }{
        .{ .n = "count", .k = 0 }, .{ .n = "sum", .k = 1 },
        .{ .n = "min", .k = 2 },   .{ .n = "max", .k = 3 },
        .{ .n = "avg", .k = 4 },   .{ .n = "collect", .k = 5 },
    };
    for (ks) |o| if (std.mem.eql(u8, b, o.n)) return o.k;
    return null;
}

/// Encode one clause term for the query builder: a `?var` → (kind=1, var idx);
/// any other value → (kind=0, the constant Ref).
fn encodeTerm(q: ?*anyopaque, ref: Ref, kind: *u32, val: *u64) void {
    if (queryVarName(ref)) |nm| {
        kind.* = 1;
        val.* = g_api.?.query_variable(q, nm.ptr, nm.len);
    } else {
        kind.* = 0;
        val.* = ref;
    }
}

/// Add one `where` element to `q`: a triple/predicate clause, a negation
/// `#[#not …]`, or a disjunction `#[#or <branch> …]` (each branch a single clause
/// or a `#[ #[c1] #[c2] ]` clause-list). Recursive — branches may nest not/or.
/// Returns an error Ref on malformed input, else null.
fn addWhereElement(q: ?*anyopaque, elem: Ref) ?Ref {
    const api = g_api.?;
    if (isVectorRef(elem) and api.rrb_len(elem) >= 2 and symbolIs(api.rrb_nth(elem, 0), "not")) {
        api.query_begin_negation(q);
        var k: u64 = 1;
        while (k < api.rrb_len(elem)) : (k += 1) {
            if (addWhereElement(q, api.rrb_nth(elem, k))) |err| return err;
        }
        api.query_end_negation(q);
        return null;
    }
    if (isVectorRef(elem) and api.rrb_len(elem) >= 2 and symbolIs(api.rrb_nth(elem, 0), "or")) {
        api.query_begin_disjunction(q);
        var k: u64 = 1;
        while (k < api.rrb_len(elem)) : (k += 1) {
            if (addBranch(q, api.rrb_nth(elem, k))) |err| return err;
        }
        api.query_end_disjunction(q);
        return null;
    }
    if (isVectorRef(elem) and api.rrb_len(elem) >= 2 and symbolIs(api.rrb_nth(elem, 0), "rule")) {
        return addRuleInvocation(q, elem);
    }
    return addClause(q, elem);
}

/// Add a rule-invocation clause `#[#rule #name ?a ?b …]` to `q`: marker `#rule`,
/// the rule-name `#symbol`, then positional argument terms (`?var`s or
/// constants, bound to the rule's params by position). Returns an error Ref on
/// malformed input, else null. (Phase 7.1 rule surface.)
fn addRuleInvocation(q: ?*anyopaque, elem: Ref) ?Ref {
    const api = g_api.?;
    const len = api.rrb_len(elem);
    const name_sym = api.rrb_nth(elem, 1);
    if (!isEid(name_sym)) return mkRuntimeError("findAll: #rule name must be a #symbol");
    const nargs = len - 2;
    if (nargs == 0) return mkRuntimeError("findAll: #rule invocation needs at least one argument");
    if (nargs > 16) return mkRuntimeError("findAll: #rule invocation arg limit is 16");
    var kinds: [16]u32 = undefined;
    var vals: [16]u64 = undefined;
    var k: u64 = 0;
    while (k < nargs) : (k += 1) {
        encodeTerm(q, api.rrb_nth(elem, k + 2), &kinds[k], &vals[k]);
    }
    api.query_add_rule_invocation(q, name_sym >> 3, @intCast(nargs), &kinds, &vals);
    return null;
}

/// One disjunction branch: a single clause `#[e a v]`, or a clause-list
/// `#[ #[c1] #[c2] ]` (detected by the first sub-element being a vector).
fn addBranch(q: ?*anyopaque, branch: Ref) ?Ref {
    const api = g_api.?;
    if (!isVectorRef(branch) or api.rrb_len(branch) == 0)
        return mkRuntimeError("findAll: #or branch must be a clause or clause-list");
    api.query_begin_branch(q);
    if (isVectorRef(api.rrb_nth(branch, 0))) {
        var k: u64 = 0;
        while (k < api.rrb_len(branch)) : (k += 1) {
            if (addWhereElement(q, api.rrb_nth(branch, k))) |err| return err;
        }
    } else {
        if (addWhereElement(q, branch)) |err| return err;
    }
    api.query_end_branch(q);
    return null;
}

/// Add one `#[e a v]` triple or `#[lhs op rhs]` predicate clause to `q`. Returns
/// an error Ref on a malformed clause, else null.
fn addClause(q: ?*anyopaque, clause: Ref) ?Ref {
    const api = g_api.?;
    if (!isVectorRef(clause) or api.rrb_len(clause) != 3)
        return mkRuntimeError("findAll: each clause must be #[e a v] or #[lhs op rhs]");
    var lk: u32 = 0;
    var lv: u64 = 0;
    var mk: u32 = 0;
    var mv: u64 = 0;
    var rk: u32 = 0;
    var rv: u64 = 0;
    const mid = api.rrb_nth(clause, 1);
    if (predicateOp(mid)) |op| {
        encodeTerm(q, api.rrb_nth(clause, 0), &lk, &lv);
        encodeTerm(q, api.rrb_nth(clause, 2), &rk, &rv);
        api.query_add_predicate(q, op, lk, lv, rk, rv);
    } else {
        encodeTerm(q, api.rrb_nth(clause, 0), &lk, &lv);
        encodeTerm(q, mid, &mk, &mv);
        encodeTerm(q, api.rrb_nth(clause, 2), &rk, &rv);
        api.query_add_triple(q, lk, lv, mk, mv, rk, rv);
    }
    return null;
}

/// History window for a query: none, or pinned via `asOf:`/`since:`/`between:and:`.
const TimeMod = union(enum) { none, asof: u64, since: u64, between: [2]u64 };

/// Core of `findAll:where:[asOf:/since:/between:and:]` (+ `[snap find:where:]`).
/// `find_ref` is either a single `?var` (result = a flat vector of its bindings)
/// or a `#[?a ?b …]` vector of `?var`s (result = a vector of tuple-vectors, one
/// column per find var, in order). `where` is a vector of clause-vectors — a
/// triple `#[e a v]`, or a predicate `#[lhs #lt rhs]` (named op symbol in the
/// middle). `tmod` pins the history window. Set semantics.
fn runDatalogQuery(find_ref: Ref, where: Ref, tmod: TimeMod) Ref {
    const api = g_api.?;
    const q = api.make_query() orelse return mkRuntimeError("findAll: out of memory");
    defer api.free_query(q);
    // Resolve any `#[#rule …]` invocations in the where clauses against the
    // global rule registry (harmless when no rules are used).
    api.query_use_global_rules(q);

    // Find spec — a single `?var` (flat result), or a vector whose elements are
    // each a `?var` or an aggregate `#[#count ?v]` (tuple/group-by rows).
    const multi = isVectorRef(find_ref);
    var nfind: u64 = 1;
    if (multi) {
        nfind = api.rrb_len(find_ref);
        if (nfind == 0) return mkRuntimeError("findAll: empty find vector");
        var fi: u64 = 0;
        while (fi < nfind) : (fi += 1) {
            const elem = api.rrb_nth(find_ref, fi);
            if (isVectorRef(elem)) {
                // Aggregate find element: `#[#count ?v]`.
                if (api.rrb_len(elem) != 2) return mkRuntimeError("findAll: aggregate must be #[#op ?var]");
                const op = aggOp(api.rrb_nth(elem, 0)) orelse
                    return mkRuntimeError("findAll: unknown aggregate op (use #count/#sum/#min/#max/#avg/#collect)");
                const nm = queryVarName(api.rrb_nth(elem, 1)) orelse
                    return mkRuntimeError("findAll: aggregate argument must be a ?variable");
                api.query_add_find_aggregate(q, op, api.query_variable(q, nm.ptr, nm.len));
            } else {
                const nm = queryVarName(elem) orelse
                    return mkRuntimeError("findAll: find vector elements must be ?variables or #[#op ?var]");
                api.query_add_find(q, api.query_variable(q, nm.ptr, nm.len));
            }
        }
    } else {
        const nm = queryVarName(find_ref) orelse
            return mkRuntimeError("findAll: argument must be a ?variable or a vector");
        api.query_add_find(q, api.query_variable(q, nm.ptr, nm.len));
    }

    // Clauses — each a triple `#[e a v]`, a predicate `#[lhs #lt rhs]`, a negation
    // `#[#not …]`, or a disjunction `#[#or <branch> …]`.
    const nclauses = api.rrb_len(where);
    var ci: u64 = 0;
    while (ci < nclauses) : (ci += 1) {
        if (addWhereElement(q, api.rrb_nth(where, ci))) |err| return err;
    }

    switch (tmod) {
        .none => {},
        .asof => |t| api.query_set_asof(q, t),
        .since => |t| api.query_set_since(q, t),
        .between => |b| api.query_set_between(q, b[0], b[1]),
    }

    const res = api.query_execute(q) orelse return mkRuntimeError("findAll: execute failed");
    defer api.free_result(res);

    var out = api.make_rrb(@ptrCast(&[_]Ref{}), 0);
    const rows = api.result_count(res);
    var ri: u64 = 0;
    while (ri < rows) : (ri += 1) {
        if (multi) {
            var tuple = api.make_rrb(@ptrCast(&[_]Ref{}), 0);
            var col: u64 = 0;
            while (col < nfind) : (col += 1) {
                tuple = api.rrb_with(tuple, api.result_at(res, ri, @intCast(col)));
            }
            out = api.rrb_with(out, tuple);
        } else {
            out = api.rrb_with(out, api.result_at(res, ri, 0));
        }
    }
    return out;
}

/// `[db findAll: ?v where: #[ #[e a v] … ]]` — conjunctive datalog query. `?v` is
/// a single `?var` or a `#[?a ?b …]` vector (→ tuple rows).
fn dbFindAllWhereImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = s;
    if (n < 2) return mkRuntimeError("findAll:where: expects a find spec and a clause vector");
    return runDatalogQuery(args[0], args[1], .none);
}

/// `[db findAll: ?v where: #[…] asOf: tx]` — pinned to the view as of `tx`
/// (a SmallInt basis, e.g. `[[db now] basisT]`).
fn dbFindAllWhereAsOfImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = s;
    if (n < 3) return mkRuntimeError("findAll:where:asOf: expects find, clauses, tx");
    return runDatalogQuery(args[0], args[1], .{ .asof = @intCast(untagSmallInt(args[2])) });
}

/// `[db findAll: ?v where: #[…] since: tx]` — only facts changed AFTER `tx`.
fn dbFindAllWhereSinceImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = s;
    if (n < 3) return mkRuntimeError("findAll:where:since: expects find, clauses, tx");
    return runDatalogQuery(args[0], args[1], .{ .since = @intCast(untagSmallInt(args[2])) });
}

/// `[db findAll: ?v where: #[…] between: from and: to]` — facts in the tx range.
fn dbFindAllWhereBetweenAndImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = s;
    if (n < 4) return mkRuntimeError("findAll:where:between:and: expects find, clauses, from, to");
    const from: u64 = @intCast(untagSmallInt(args[2]));
    const to: u64 = @intCast(untagSmallInt(args[3]));
    return runDatalogQuery(args[0], args[1], .{ .between = .{ from, to } });
}

/// `[aSnapshot find: ?v where: #[…]]` — the same conjunctive query pinned to the
/// snapshot's basis (datalog-over-snapshot via the time axis; 15.R).
fn snapFindWhereImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    if (n < 2) return mkRuntimeError("find:where: expects a find spec and a clause vector");
    const basis = g_api.?.snapshot_basis(s.receiver);
    if (!isSmallInt(basis)) return mkRuntimeError("find:where: receiver is not a snapshot");
    return runDatalogQuery(args[0], args[1], .{ .asof = @intCast(untagSmallInt(basis)) });
}

/// `[db defineRule: #name params: #[?a ?b] body: #[ <clauses> ]]` — define (or
/// add a definitional-disjunction alternative for) a datalog rule. The body is a
/// where-clause vector: triples, predicates, `#not`/`#or`, and recursive
/// `#[#rule #name …]` invocations. Params claim variable indices 0..n-1 so a
/// caller's invocation args bind to them by position. Returns the rule name.
/// (Phase 7.1 rule surface.)
fn dbDefineRuleImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = s;
    const api = g_api.?;
    if (n < 3) return mkRuntimeError("defineRule:params:body: expects a name, a params vector, and a body vector");
    const name_sym = args[0];
    if (!isEid(name_sym)) return mkRuntimeError("defineRule: rule name must be a #symbol");
    const params = args[1];
    if (!isVectorRef(params)) return mkRuntimeError("defineRule: params must be a #[?a …] vector");
    const body = args[2];
    if (!isVectorRef(body)) return mkRuntimeError("defineRule: body must be a clause vector");

    const q = api.make_query() orelse return mkRuntimeError("defineRule: out of memory");
    defer api.free_query(q);
    // A rule body may invoke other rules (incl. itself) — resolve via registry.
    api.query_use_global_rules(q);

    // Register params first so they take variable indices 0..n_params-1.
    const nparams = api.rrb_len(params);
    if (nparams == 0) return mkRuntimeError("defineRule: at least one parameter is required");
    var pi: u64 = 0;
    while (pi < nparams) : (pi += 1) {
        const nm = queryVarName(api.rrb_nth(params, pi)) orelse
            return mkRuntimeError("defineRule: params must all be ?variables");
        _ = api.query_variable(q, nm.ptr, nm.len);
    }

    // Body clauses become the rule body (snapshotted by define_rule).
    const nclauses = api.rrb_len(body);
    var ci: u64 = 0;
    while (ci < nclauses) : (ci += 1) {
        if (addWhereElement(q, api.rrb_nth(body, ci))) |err| return err;
    }

    // max_var is computed kernel-side from the built query (var_count - 1).
    if (!api.define_rule(q, name_sym >> 3, @intCast(nparams), 0))
        return mkRuntimeError("defineRule: rule registry unavailable");
    return name_sym;
}

/// `[db defineQuery: #name find: ?v where: #[ <clauses> ]]` — store a complete
/// query (its find spec + where clauses, both plain Refs since `?vars` are just
/// symbols) under `#name`, runnable later by `[db run: #name]`. Re-defining a
/// name overwrites (`:one` cardinality). Returns the name. (Phase 7.3 saved
/// queries — name resolution from the surface.)
fn dbDefineQueryImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = s;
    const api = g_api.?;
    if (n < 3) return mkRuntimeError("defineQuery:find:where: expects a name, a find spec, and a clause vector");
    const name_sym = args[0];
    if (!isEid(name_sym)) return mkRuntimeError("defineQuery: query name must be a #symbol");
    if (!isVectorRef(args[2])) return mkRuntimeError("defineQuery: where clauses must be a vector");
    const find_attr = api.intern("saved-query-find".ptr, 16);
    const where_attr = api.intern("saved-query-where".ptr, 17);
    _ = api.assert_fact(name_sym >> 3, find_attr, args[1], g_module_eid);
    _ = api.assert_fact(name_sym >> 3, where_attr, args[2], g_module_eid);
    return name_sym;
}

/// `[db run: #name]` — execute the saved query stored under `#name`. An undefined
/// name → an `#undefinedQuery` ERROR_VT. (Phase 7.3 saved queries.)
fn dbRunImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    const name_sym = args[0];
    if (!isEid(name_sym)) return mkRuntimeError("run: query name must be a #symbol");
    const find_attr = api.intern("saved-query-find".ptr, 16);
    const where_attr = api.intern("saved-query-where".ptr, 17);
    const find_ref = api.query_eavt(name_sym >> 3, find_attr);
    const where_ref = api.query_eavt(name_sym >> 3, where_attr);
    if (find_ref == NIL or where_ref == NIL)
        return mkRuntimeErrorKind(api.intern("undefinedQuery".ptr, 14), "run: no saved query under that name");
    return runDatalogQuery(find_ref, where_ref, .none);
}

/// True iff `r` is an ERROR_VT entity (a propagating error value).
fn isErrorRef(r: Ref) bool {
    if (!isEid(r)) return false;
    return g_api.?.query_eavt(r >> 3, ATTR_VTABLE) == mkEid(ERROR_VT);
}

/// Is `attr_sym` (a #symbol Ref) a REVERSE attribute — name starts with `_`
/// (Datomic `:_attr` convention)? If so, returns the forward attribute's eid via
/// `out_fwd`; else returns false. `#_likes` over entity E finds the entities that
/// assert `likes == E`.
fn reverseAttrFwd(attr_sym: Ref, out_fwd: *u64) bool {
    const api = g_api.?;
    if (!isEid(attr_sym)) return false;
    const name_ref = api.symbol_name(attr_sym >> 3);
    if (!isString(name_ref)) return false;
    const name = stringBytes(name_ref);
    if (name.len < 2 or name[0] != '_') return false;
    out_fwd.* = api.intern(name[1..].ptr, name[1..].len);
    return true;
}

/// Pull the eid-valued ref `v` through `sub` — one entity, or (for a `:many` ref
/// whose `currentValue` is a vector) a vector of child maps. Non-entity → nil.
fn pullRefValue(v: Ref, sub: Ref) Ref {
    const api = g_api.?;
    if (isVectorRef(v)) {
        const n = api.rrb_len(v);
        var empty: [0]Ref = .{};
        var items: Ref = api.make_rrb(&empty, 0);
        var k: u64 = 0;
        while (k < n) : (k += 1) {
            const el = api.rrb_nth(v, k);
            if (!isEid(el) or el == NIL) continue;
            const child = pullEntity(el >> 3, sub);
            if (isErrorRef(child)) return child;
            items = api.rrb_with(items, child);
        }
        return items;
    }
    if (isEid(v) and v != NIL) return pullEntity(v >> 3, sub);
    return NIL;
}

/// Recursively materialize an entity per a pull spec into an ASSOC_VT map.
/// Spec elements:
///   `#attr`              → `map[#attr] = currentValue(entity, attr)` (a vector
///                          for a `:many` attr; nil if absent).
///   `#_attr`             → reverse ref: `map[#_attr] = vector of the entity eids
///                          that assert `attr == entity`.
///   `#[#refattr <spec>]` → follow the forward ref(s) and pull `<spec>` from each
///                          (a `:many` ref yields a vector of child maps).
///   `#[#_attr <spec>]`   → follow reverse refs, pull `<spec>` from each referrer
///                          (always a vector).
/// (Phase 7.3 pull — flat + nested ref + reverse + `:many` ref nesting.)
fn pullEntity(entity_eid: u64, spec: Ref) Ref {
    const api = g_api.?;
    if (!isVectorRef(spec)) return mkRuntimeError("pull: spec must be a vector of #attr / #[#attr <spec>]");
    var map = api.make_map();
    const nspec = api.rrb_len(spec);
    var i: u64 = 0;
    while (i < nspec) : (i += 1) {
        const elem = api.rrb_nth(spec, i);
        if (isVectorRef(elem)) {
            if (api.rrb_len(elem) != 2) return mkRuntimeError("pull: a nested spec must be #[#attr <spec>]");
            const attr_sym = api.rrb_nth(elem, 0);
            if (!isEid(attr_sym)) return mkRuntimeError("pull: attribute must be a #symbol");
            const sub = api.rrb_nth(elem, 1);
            var fwd: u64 = 0;
            var child: Ref = NIL;
            if (reverseAttrFwd(attr_sym, &fwd)) {
                // Reverse ref → pull each referrer; result is always a vector.
                const refs = api.reverse_refs(fwd, entity_eid);
                child = pullRefValue(refs, sub);
            } else {
                child = pullRefValue(api.query_eavt(entity_eid, attr_sym >> 3), sub);
            }
            if (isErrorRef(child)) return child;
            map = api.map_put(map, attr_sym, child);
        } else if (isEid(elem)) {
            var fwd: u64 = 0;
            if (reverseAttrFwd(elem, &fwd)) {
                map = api.map_put(map, elem, api.reverse_refs(fwd, entity_eid));
            } else {
                map = api.map_put(map, elem, api.query_eavt(entity_eid, elem >> 3));
            }
        } else {
            return mkRuntimeError("pull: spec elements must be #attr or #[#attr <spec>]");
        }
    }
    return map;
}

/// `[db pull: anEntity spec: #[ #name #[#delegate #[#name]] … ]]` — materialize an
/// entity (e.g. a `findAll` result eid or a `#symbol`) into a nested map. Compose
/// with `collect:` over a query result to pull every match. (Phase 7.3.)
fn dbPullSpecImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = s;
    if (n < 2) return mkRuntimeError("pull:spec: expects an entity and a spec vector");
    const entity = args[0];
    if (!isEid(entity)) return mkRuntimeError("pull: entity must be an eid (a query result or #symbol)");
    return pullEntity(entity >> 3, args[1]);
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
        .{ .name = "findAll:where:", .impl = @ptrCast(&dbFindAllWhereImpl) },
        .{ .name = "findAll:where:asOf:", .impl = @ptrCast(&dbFindAllWhereAsOfImpl) },
        .{ .name = "findAll:where:since:", .impl = @ptrCast(&dbFindAllWhereSinceImpl) },
        .{ .name = "findAll:where:between:and:", .impl = @ptrCast(&dbFindAllWhereBetweenAndImpl) },
        .{ .name = "defineRule:params:body:", .impl = @ptrCast(&dbDefineRuleImpl) },
        .{ .name = "defineQuery:find:where:", .impl = @ptrCast(&dbDefineQueryImpl) },
        .{ .name = "run:", .impl = @ptrCast(&dbRunImpl) },
        .{ .name = "pull:spec:", .impl = @ptrCast(&dbPullSpecImpl) },
    };
    for (db_methods) |m| {
        const sel = api.intern(m.name.ptr, m.name.len);
        _ = api.assert_fact(DB_PROTO_VT, sel, mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }
    const snap_methods = [_]Pair{
        .{ .name = "basisT", .impl = @ptrCast(&snapBasisImpl) },
        .{ .name = "get:attr:", .impl = @ptrCast(&snapGetAttrImpl) },
        .{ .name = "release", .impl = @ptrCast(&snapReleaseImpl) },
        .{ .name = "find:where:", .impl = @ptrCast(&snapFindWhereImpl) },
    };
    for (snap_methods) |m| {
        const sel = api.intern(m.name.ptr, m.name.len);
        _ = api.assert_fact(SNAPSHOT_VT, sel, mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }

    pushGlobalBinding(GLOBALS_SCOPE_EID, api.intern("db".ptr, 2), mkEid(DB_PROTO));
}

// -- Phase 17.8: I/O surface (File + System) ------------------------------
//
// File does data read/write directly via std.fs from the lang dylib (no new
// ABI field — same as fileIn:). Failures return ERROR_VT (kinded) so they
// propagate + are catchable with on:do:; pair with `ensure:` for cleanup.
// System does stdout/stderr (direct fd write), stdin readLine, and arguments
// (argv, captured by the shell and exposed via the appended IdApi ABI).

/// Map a Zig file-system error to a birnam error kind symbol.
fn ioErrorKind(e: anyerror) []const u8 {
    return switch (e) {
        error.FileNotFound => "fileNotFound",
        error.AccessDenied => "accessDenied",
        else => "ioError",
    };
}

/// `[File read: path]` → file contents as a string, or a kinded ERROR_VT.
fn fileReadImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    if (!isString(args[0])) return mkRuntimeErrorKind(api.intern("ioError".ptr, 7), "File read: path must be a string");
    const path = stringBytes(args[0]);
    const bytes = std.fs.cwd().readFileAlloc(std.heap.page_allocator, path, 64 << 20) catch |e|
        return mkRuntimeErrorFmt(api.intern(ioErrorKind(e).ptr, ioErrorKind(e).len), "File read: {s}: {s}", .{ path, @errorName(e) });
    defer std.heap.page_allocator.free(bytes);
    return api.make_string(bytes.ptr, bytes.len);
}

/// `[File write: contents to: path]` → nil on success, or a kinded ERROR_VT.
/// Creates or truncates `path`.
fn fileWriteImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    if (!isString(args[0]) or !isString(args[1])) return mkRuntimeErrorKind(api.intern("ioError".ptr, 7), "File write:to: expects (string, string)");
    const data = stringBytes(args[0]);
    const path = stringBytes(args[1]);
    std.fs.cwd().writeFile(.{ .sub_path = path, .data = data }) catch |e|
        return mkRuntimeErrorFmt(api.intern(ioErrorKind(e).ptr, ioErrorKind(e).len), "File write: {s}: {s}", .{ path, @errorName(e) });
    return NIL;
}

/// `[File append: contents to: path]` → nil on success, or a kinded ERROR_VT.
/// Creates `path` if absent; appends otherwise.
fn fileAppendImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    if (!isString(args[0]) or !isString(args[1])) return mkRuntimeErrorKind(api.intern("ioError".ptr, 7), "File append:to: expects (string, string)");
    const data = stringBytes(args[0]);
    const path = stringBytes(args[1]);
    const f = std.fs.cwd().createFile(path, .{ .truncate = false }) catch |e|
        return mkRuntimeErrorFmt(api.intern(ioErrorKind(e).ptr, ioErrorKind(e).len), "File append: {s}: {s}", .{ path, @errorName(e) });
    defer f.close();
    f.seekFromEnd(0) catch |e| return mkRuntimeErrorFmt(api.intern("ioError".ptr, 7), "File append: {s}: {s}", .{ path, @errorName(e) });
    f.writeAll(data) catch |e| return mkRuntimeErrorFmt(api.intern("ioError".ptr, 7), "File append: {s}: {s}", .{ path, @errorName(e) });
    return NIL;
}

/// `[File exists: path]` → true / false (never errors).
fn fileExistsImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    if (!isString(args[0])) return mkEid(FALSE_EID);
    std.fs.cwd().access(stringBytes(args[0]), .{}) catch return mkEid(FALSE_EID);
    return mkEid(TRUE_EID);
}

/// `[File delete: path]` → nil on success, or a kinded ERROR_VT.
fn fileDeleteImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    if (!isString(args[0])) return mkRuntimeErrorKind(api.intern("ioError".ptr, 7), "File delete: path must be a string");
    const path = stringBytes(args[0]);
    std.fs.cwd().deleteFile(path) catch |e|
        return mkRuntimeErrorFmt(api.intern(ioErrorKind(e).ptr, ioErrorKind(e).len), "File delete: {s}: {s}", .{ path, @errorName(e) });
    return NIL;
}

/// `[File makeDir: path]` → nil on success, or a kinded ERROR_VT. Creates any
/// missing intermediate directories (mkdir -p). No-op if it already exists.
fn fileMakeDirImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    if (!isString(args[0])) return mkRuntimeErrorKind(api.intern("ioError".ptr, 7), "File makeDir: path must be a string");
    const path = stringBytes(args[0]);
    std.fs.cwd().makePath(path) catch |e|
        return mkRuntimeErrorFmt(api.intern(ioErrorKind(e).ptr, ioErrorKind(e).len), "File makeDir: {s}: {s}", .{ path, @errorName(e) });
    return NIL;
}

/// `[File removeDir: path]` → nil on success, or a kinded ERROR_VT. Removes an
/// EMPTY directory (errors if non-empty — deliberately not recursive).
fn fileRemoveDirImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    if (!isString(args[0])) return mkRuntimeErrorKind(api.intern("ioError".ptr, 7), "File removeDir: path must be a string");
    const path = stringBytes(args[0]);
    std.fs.cwd().deleteDir(path) catch |e|
        return mkRuntimeErrorFmt(api.intern(ioErrorKind(e).ptr, ioErrorKind(e).len), "File removeDir: {s}: {s}", .{ path, @errorName(e) });
    return NIL;
}

/// `[File isDir: path]` → true / false (never errors; false if absent or a file).
fn fileIsDirImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    if (!isString(args[0])) return mkEid(FALSE_EID);
    const st = std.fs.cwd().statFile(stringBytes(args[0])) catch return mkEid(FALSE_EID);
    return if (st.kind == .directory) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}

/// `[File list: path]` → a vector of entry-name strings (one level, unsorted), or
/// a kinded ERROR_VT. `.` / `..` are excluded by the iterator.
fn fileListImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    if (!isString(args[0])) return mkRuntimeErrorKind(api.intern("ioError".ptr, 7), "File list: path must be a string");
    const path = stringBytes(args[0]);
    var dir = std.fs.cwd().openDir(path, .{ .iterate = true }) catch |e|
        return mkRuntimeErrorFmt(api.intern(ioErrorKind(e).ptr, ioErrorKind(e).len), "File list: {s}: {s}", .{ path, @errorName(e) });
    defer dir.close();
    const alloc = std.heap.page_allocator;
    var items = std.ArrayList(Ref){};
    defer items.deinit(alloc);
    var it = dir.iterate();
    while (it.next() catch null) |entry| {
        items.append(alloc, api.make_string(entry.name.ptr, entry.name.len)) catch break;
    }
    return api.make_rrb(items.items.ptr, items.items.len);
}

/// `[System print: x]` → write x's displayString to stdout (no newline);
/// returns nil. Unlike `print` (which goes to the prefixed REPL log = stderr),
/// this is clean stdout for program output.
fn systemPrintImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    systemWrite(std.fs.File.stdout(), args[0], false);
    return NIL;
}
/// `[System println: x]` → like print: plus a trailing newline.
fn systemPrintlnImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    systemWrite(std.fs.File.stdout(), args[0], true);
    return NIL;
}
/// `[System error: x]` → write x's displayString + newline to stderr.
fn systemErrorImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    systemWrite(std.fs.File.stderr(), args[0], true);
    return NIL;
}

/// Render `v` to its displayString and write the bytes to `file` (optionally
/// with a trailing newline). Strings write their raw bytes; other values use
/// the canonical formatter.
fn systemWrite(file: std.fs.File, v: Ref, newline: bool) void {
    const w = file.deprecatedWriter();
    if (isString(v)) {
        w.writeAll(stringBytes(v)) catch {};
    } else {
        var buf: [4096]u8 = undefined;
        const len = appendRef(&buf, 0, v);
        w.writeAll(buf[0..len]) catch {};
    }
    if (newline) w.writeAll("\n") catch {};
}

/// `[System readLine]` → next line from stdin (newline stripped) as a string,
/// or nil at EOF. Reads fd 0 directly; in the interactive REPL this consumes
/// the next input line.
fn systemReadLineImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, args, n };
    const api = g_api.?;
    var buf: [4096]u8 = undefined;
    const r = std.fs.File.stdin().deprecatedReader();
    const line = r.readUntilDelimiterOrEof(&buf, '\n') catch return NIL;
    if (line) |bytes| return api.make_string(bytes.ptr, bytes.len);
    return NIL; // EOF
}

/// `[System arguments]` → a vector of the program's command-line arguments (the
/// tokens after `--` on the birnam command line), as strings.
fn systemArgumentsImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, args, n };
    const api = g_api.?;
    const count = api.arg_count();
    var items: [256]Ref = undefined;
    var i: usize = 0;
    while (i < count and i < items.len) : (i += 1) {
        var len: usize = 0;
        const p = api.arg_at(i, &len);
        items[i] = api.make_string(p, len);
    }
    return api.make_rrb(&items, i);
}

/// Install the `File` and `System` global prototypes (CODE provenance — rebuilt
/// each boot, never persisted).
fn installIoSurface() void {
    const api = g_api.?;
    const Pair = struct { name: []const u8, impl: *const anyopaque };

    _ = api.assert_fact(FILE_PROTO, ATTR_VTABLE, mkEid(FILE_PROTO_VT), g_module_eid);
    _ = api.assert_fact(FILE_PROTO_VT, ATTR_DELEGATE, mkEid(OBJECT_VT), g_module_eid);
    const file_methods = [_]Pair{
        .{ .name = "read:", .impl = @ptrCast(&fileReadImpl) },
        .{ .name = "write:to:", .impl = @ptrCast(&fileWriteImpl) },
        .{ .name = "append:to:", .impl = @ptrCast(&fileAppendImpl) },
        .{ .name = "exists:", .impl = @ptrCast(&fileExistsImpl) },
        .{ .name = "delete:", .impl = @ptrCast(&fileDeleteImpl) },
        .{ .name = "makeDir:", .impl = @ptrCast(&fileMakeDirImpl) },
        .{ .name = "removeDir:", .impl = @ptrCast(&fileRemoveDirImpl) },
        .{ .name = "isDir:", .impl = @ptrCast(&fileIsDirImpl) },
        .{ .name = "list:", .impl = @ptrCast(&fileListImpl) },
    };
    for (file_methods) |m| {
        _ = api.assert_fact(FILE_PROTO_VT, api.intern(m.name.ptr, m.name.len), mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }
    pushGlobalBinding(GLOBALS_SCOPE_EID, api.intern("File".ptr, 4), mkEid(FILE_PROTO));

    _ = api.assert_fact(SYSTEM_PROTO, ATTR_VTABLE, mkEid(SYSTEM_PROTO_VT), g_module_eid);
    _ = api.assert_fact(SYSTEM_PROTO_VT, ATTR_DELEGATE, mkEid(OBJECT_VT), g_module_eid);
    const system_methods = [_]Pair{
        .{ .name = "print:", .impl = @ptrCast(&systemPrintImpl) },
        .{ .name = "println:", .impl = @ptrCast(&systemPrintlnImpl) },
        .{ .name = "error:", .impl = @ptrCast(&systemErrorImpl) },
        .{ .name = "readLine", .impl = @ptrCast(&systemReadLineImpl) },
        .{ .name = "arguments", .impl = @ptrCast(&systemArgumentsImpl) },
    };
    for (system_methods) |m| {
        _ = api.assert_fact(SYSTEM_PROTO_VT, api.intern(m.name.ptr, m.name.len), mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }
    pushGlobalBinding(GLOBALS_SCOPE_EID, api.intern("System".ptr, 6), mkEid(SYSTEM_PROTO));
}

// -- Phase 16 Tier E: reference cells, metadata, applicative --------------

/// `db cell: v` / `ref: v` / `atom: v` — mint a reference cell. A cell IS an
/// entity carrying `[cell :vtable CELL_VT]` + `[cell :cell-value v]`; returned
/// as an EID Ref so its messages dispatch to CELL_VT. (Single-transactor
/// subsumes Clojure's atom/ref/agent split — all three names mint the same
/// thing.) nil-as-initial is refused: the store forbids a NIL value (Date),
/// so seed with a real value and use absence channels for "empty".
fn dbCellImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    if (isNil(args[0])) return mkRuntimeError("cell cannot be seeded with nil (Date: store forbids NIL value)");
    const e = api.alloc_eid();
    _ = api.assert_fact(e, ATTR_VTABLE, mkEid(CELL_VT), M_USER);
    _ = api.assert_fact(e, ATTR_CELL_VALUE, args[0], M_USER);
    return mkEid(e);
}

/// `db transact: aBlock` — coordinated change. Single-transactor: the block's
/// sets run sequentially with no interleaving, so the group is atomic by
/// construction (no STM). Returns the block's value.
fn dbTransactImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    return g_api.?.send0(args[0], sel_value_g);
}

/// `db entity` — mint a fresh entity (an OBJECT_VT-tagged eid) to hang facts on
/// with `assert:attr:value:`. The general counterpart to `cell:` (which pins a
/// single `:cell-value`): a plain entity any datalog clause can match.
fn dbEntityImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, args, n };
    const api = g_api.?;
    const e = api.alloc_eid();
    _ = api.assert_fact(e, ATTR_VTABLE, mkEid(OBJECT_VT), M_USER);
    return mkEid(e);
}

/// `db assert: e attr: #a value: v` — assert one fact `[e a v]`. `e` is an
/// entity (from `db entity`/`db cell:`), `a` a symbol naming the attribute, `v`
/// any non-nil value (use symbols for values you intend to query by — string
/// literals have distinct identities and won't match as clause constants).
/// Returns `e` so assertions chain. The birnam-level write path the datalog
/// query/rule/pull surface was missing.
fn dbAssertImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    const e = args[0];
    const a = args[1];
    const v = args[2];
    if (!isEid(e)) return mkRuntimeError("db assert:attr:value: — entity must be an entity");
    if (!isEid(a)) return mkRuntimeError("db assert:attr:value: — attr must be a symbol");
    if (isNil(v)) return mkRuntimeError("db assert:attr:value: — value cannot be nil (store forbids NIL)");
    _ = api.assert_fact(untagEid(e), untagEid(a), v, M_USER);
    return e;
}

/// `[cell value]` — current value (a cell always holds one — no-NULL).
fn cellValueImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return g_api.?.query_eavt(untagEid(s.receiver), ATTR_CELL_VALUE);
}

/// `[cell set: v]` — assert a new value (new tx); returns the cell (chainable).
fn cellSetImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    if (isNil(args[0])) return mkRuntimeError("cell cannot hold nil (Date: store forbids NIL value)");
    _ = api.assert_fact(untagEid(s.receiver), ATTR_CELL_VALUE, args[0], M_USER);
    return s.receiver;
}

/// `[cell asOf: tx]` — value as of the end of tx `tx` (time travel, 15.R).
fn cellAsOfImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return g_api.?.current_value_as_of(untagEid(s.receiver), ATTR_CELL_VALUE, @intCast(untagSmallInt(args[0])));
}

/// `[cell history]` — vector of every value held, oldest→newest (15.R history).
fn cellHistoryImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return g_api.?.cell_history(untagEid(s.receiver), ATTR_CELL_VALUE);
}

/// `[obj withMeta: m]` — attach metadata (just another fact). Returns obj.
/// Only entities carry meta; immediates (SmallInt etc.) answer self unchanged.
fn withMetaImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    if (!isEid(s.receiver) or isNil(args[0])) return s.receiver;
    _ = g_api.?.assert_fact(untagEid(s.receiver), ATTR_META, args[0], M_USER);
    return s.receiver;
}

/// `[obj meta]` — metadata, or nil if none / not an entity.
fn metaImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    if (!isEid(s.receiver)) return NIL;
    return g_api.?.query_eavt(untagEid(s.receiver), ATTR_META);
}

/// `aVector zip: bVector` — vector of `#[a_i b_i]` pairs, length = min(len).
/// The independent-combination primitive (17.6): pairs positions with no
/// cross-dependency.
fn vecZipImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    const a = s.receiver;
    const b = args[0];
    const la = api.rrb_len(a);
    const lb = api.rrb_len(b);
    const m: u64 = if (la < lb) la else lb;
    var result = api.make_rrb(@ptrCast(&[_]Ref{}), 0);
    var i: u64 = 0;
    while (i < m) : (i += 1) {
        var pair = [2]Ref{ api.rrb_nth(a, i), api.rrb_nth(b, i) };
        result = api.rrb_with(result, api.make_rrb(&pair, 2));
    }
    return result;
}

/// `aVector whenAll: aBlock` — if every element is non-nil (all "succeeded"),
/// apply the 1-arg block to the whole vector; else nil. The applicative
/// "combine independent results only when all present" verb (17.6).
fn vecWhenAllImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    const v = s.receiver;
    const len = api.rrb_len(v);
    var i: u64 = 0;
    while (i < len) : (i += 1) {
        if (isNil(api.rrb_nth(v, i))) return NIL;
    }
    return api.send1(args[0], sel_value_of_g, v);
}

/// `aVector reverse` — a new vector, elements in reverse order. Plumbing for
/// transducer composition order (and generally useful).
fn vecReverseImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    const api = g_api.?;
    const v = s.receiver;
    const len = api.rrb_len(v);
    var result = api.make_rrb(@ptrCast(&[_]Ref{}), 0);
    var i: u64 = len;
    while (i > 0) {
        i -= 1;
        result = api.rrb_with(result, api.rrb_nth(v, i));
    }
    return result;
}

/// Install Tier E: cell constructors + transact: on DB_PROTO_VT, cell methods
/// on CELL_VT, metadata on OBJECT_VT, zip:/whenAll:/reverse on RRB_VT, and the
/// `Xf` transducer factory. Phase 16 Tier E.
fn installCellSurface() void {
    const api = g_api.?;
    const Pair = struct { name: []const u8, impl: *const anyopaque };

    const db_methods = [_]Pair{
        .{ .name = "cell:", .impl = @ptrCast(&dbCellImpl) },
        .{ .name = "ref:", .impl = @ptrCast(&dbCellImpl) },
        .{ .name = "atom:", .impl = @ptrCast(&dbCellImpl) },
        .{ .name = "transact:", .impl = @ptrCast(&dbTransactImpl) },
        .{ .name = "entity", .impl = @ptrCast(&dbEntityImpl) },
        .{ .name = "assert:attr:value:", .impl = @ptrCast(&dbAssertImpl) },
    };
    for (db_methods) |m| {
        const sel = api.intern(m.name.ptr, m.name.len);
        _ = api.assert_fact(DB_PROTO_VT, sel, mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }

    const cell_methods = [_]Pair{
        .{ .name = "value", .impl = @ptrCast(&cellValueImpl) },
        .{ .name = "set:", .impl = @ptrCast(&cellSetImpl) },
        .{ .name = "asOf:", .impl = @ptrCast(&cellAsOfImpl) },
        .{ .name = "history", .impl = @ptrCast(&cellHistoryImpl) },
    };
    for (cell_methods) |m| {
        const sel = api.intern(m.name.ptr, m.name.len);
        _ = api.assert_fact(CELL_VT, sel, mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }

    // Metadata — on OBJECT_VT so every entity-shaped value answers it.
    _ = api.assert_fact(OBJECT_VT, api.intern("withMeta:".ptr, 9), mkEid(api.make_closure(@ptrCast(&withMetaImpl), NIL)), g_module_eid);
    _ = api.assert_fact(OBJECT_VT, api.intern("meta".ptr, 4), mkEid(api.make_closure(@ptrCast(&metaImpl), NIL)), g_module_eid);

    // Applicative independent-combination + reverse on RRB_VT.
    _ = api.assert_fact(RRB_VT, api.intern("zip:".ptr, 4), mkEid(api.make_closure(@ptrCast(&vecZipImpl), NIL)), g_module_eid);
    _ = api.assert_fact(RRB_VT, api.intern("whenAll:".ptr, 8), mkEid(api.make_closure(@ptrCast(&vecWhenAllImpl), NIL)), g_module_eid);
    _ = api.assert_fact(RRB_VT, api.intern("reverse".ptr, 7), mkEid(api.make_closure(@ptrCast(&vecReverseImpl), NIL)), g_module_eid);

    // `Xf` transducer factory singleton.
    _ = api.assert_fact(XF_PROTO, ATTR_VTABLE, mkEid(XF_VT), g_module_eid);
    pushGlobalBinding(GLOBALS_SCOPE_EID, api.intern("Xf".ptr, 2), mkEid(XF_PROTO));
}

// -- Phase 17: stdlib — printing, equality/comparison, sort ---------------

var sel_eq_g: u64 = 0;
var sel_lt_g: u64 = 0;

/// `[x printString]` — render any value to a string Ref via the canonical
/// `appendRef` formatter (strings quoted, `#[..]` / `{k: v}` recursive).
fn printStringImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    var buf: [4096]u8 = undefined;
    const len = appendRef(&buf, 0, s.receiver);
    return g_api.?.make_string(buf[0..len].ptr, len);
}

/// `[x displayString]` — human form: a string renders WITHOUT quotes; every
/// other value matches printString.
fn displayStringImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    if (isString(s.receiver)) return s.receiver;
    var buf: [4096]u8 = undefined;
    const len = appendRef(&buf, 0, s.receiver);
    return g_api.?.make_string(buf[0..len].ptr, len);
}

/// Append `val`'s display form (strings unquoted) to `list`.
fn appendDisplay(list: *std.ArrayList(u8), alloc: std.mem.Allocator, val: Ref) void {
    if (isString(val)) {
        list.appendSlice(alloc, stringBytes(val)) catch {};
        return;
    }
    var buf: [4096]u8 = undefined;
    const len = appendRef(&buf, 0, val);
    list.appendSlice(alloc, buf[0..len]) catch {};
}

/// `["{} + {} = {}" format: #[2 3 5]]` → `"2 + 3 = 5"`. Each `{}` placeholder is
/// replaced positionally by the next vector element's displayString (strings
/// unquoted). `{{` / `}}` escape literal braces. Too few args → an `#argError`
/// ERROR_VT; extra args are ignored. Phase 17 string format.
fn strFormatImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    if (!isVectorRef(args[0])) return mkRuntimeErrorKind(api.intern("typeMismatch".ptr, 12), "format: argument must be a vector");
    const tmpl = stringBytes(s.receiver);
    const nargs = api.rrb_len(args[0]);
    const alloc = std.heap.page_allocator;
    var out = std.ArrayList(u8){};
    defer out.deinit(alloc);
    var arg_i: u64 = 0;
    var i: usize = 0;
    while (i < tmpl.len) {
        const c = tmpl[i];
        if (c == '{' and i + 1 < tmpl.len and tmpl[i + 1] == '{') {
            out.append(alloc, '{') catch {};
            i += 2;
        } else if (c == '}' and i + 1 < tmpl.len and tmpl[i + 1] == '}') {
            out.append(alloc, '}') catch {};
            i += 2;
        } else if (c == '{' and i + 1 < tmpl.len and tmpl[i + 1] == '}') {
            if (arg_i >= nargs) return mkRuntimeErrorFmt(api.intern("argError".ptr, 8), "format: not enough arguments for placeholder {d}", .{arg_i + 1});
            appendDisplay(&out, alloc, api.rrb_nth(args[0], arg_i));
            arg_i += 1;
            i += 2;
        } else {
            out.append(alloc, c) catch {};
            i += 1;
        }
    }
    return api.make_string(out.items.ptr, out.items.len);
}

/// `[x print]` — write displayString + newline to output; return self
/// (chainable, so intermediate values are visible inside code).
fn printImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    var buf: [4096]u8 = undefined;
    var len: usize = 0;
    if (isString(s.receiver)) {
        const b = stringBytes(s.receiver);
        len = @min(b.len, buf.len - 1);
        @memcpy(buf[0..len], b[0..len]);
    } else {
        len = appendRef(&buf, 0, s.receiver);
    }
    buf[@min(len, buf.len - 1)] = 0;
    if (g_log) |log| log(@ptrCast(&buf));
    return s.receiver;
}

/// `[x = y]` default — Ref identity (covers cells/eids/singletons). SmallInt /
/// String / Vector override with value semantics on their own vtables.
fn objEqImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return if (s.receiver == args[0]) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}

/// `[v = w]` for vectors — structural: same size and elementwise `=` (recursive
/// via the `=` selector, so nested vectors compare deeply).
fn vecEqImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    const b = args[0];
    if (!isVectorRef(b)) return mkEid(FALSE_EID);
    const la = api.rrb_len(s.receiver);
    if (la != api.rrb_len(b)) return mkEid(FALSE_EID);
    var i: u64 = 0;
    while (i < la) : (i += 1) {
        if (api.send1(api.rrb_nth(s.receiver, i), sel_eq_g, api.rrb_nth(b, i)) != mkEid(TRUE_EID)) {
            return mkEid(FALSE_EID);
        }
    }
    return mkEid(TRUE_EID);
}

/// `[a < b]` for strings — lexicographic (byte order).
fn strLtImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    if (!isString(args[0])) return mkEid(FALSE_EID);
    const order = std.mem.order(u8, stringBytes(s.receiver), stringBytes(args[0]));
    return if (order == .lt) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}
/// Three-way lexicographic compare (Phase 17.3) — `[a <=> b]` / `[a compare: b]`
/// → -1/0/1. nil for a non-string argument.
fn strCompareImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    if (!isString(args[0])) return NIL;
    const order = std.mem.order(u8, stringBytes(s.receiver), stringBytes(args[0]));
    return mkSmallInt(switch (order) {
        .lt => -1,
        .eq => 0,
        .gt => 1,
    });
}

/// `[s copyFrom: lo to: hi]` — substring `[lo, hi)` (byte indices, clamped).
fn strCopyFromToImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    const b = stringBytes(s.receiver);
    const raw_lo = untagSmallInt(args[0]);
    const raw_hi = untagSmallInt(args[1]);
    const lo: usize = if (raw_lo < 0) 0 else @min(@as(usize, @intCast(raw_lo)), b.len);
    const hi: usize = if (raw_hi < 0) 0 else @min(@as(usize, @intCast(raw_hi)), b.len);
    if (hi <= lo) return api.make_string(b.ptr, 0);
    return api.make_string(b[lo..hi].ptr, hi - lo);
}

const SortCtx = struct { key: Ref };
fn sortLessThan(ctx: SortCtx, a: Ref, b: Ref) bool {
    const api = g_api.?;
    var ka = a;
    var kb = b;
    if (ctx.key != NIL) {
        ka = api.send1(ctx.key, sel_value_of_g, a);
        kb = api.send1(ctx.key, sel_value_of_g, b);
    }
    return api.send1(ka, sel_lt_g, kb) == mkEid(TRUE_EID);
}

/// Shared sort core for `sort` (key=NIL) and `sortBy:` (key=block). Comparison
/// rides the `<` message — any element type answering `<` is sortable.
fn sortInto(receiver: Ref, key: Ref) Ref {
    const api = g_api.?;
    const len = api.rrb_len(receiver);
    if (len > 1024) return mkRuntimeError("sort: vector too large (>1024)");
    var buf: [1024]Ref = undefined;
    var i: u64 = 0;
    while (i < len) : (i += 1) buf[i] = api.rrb_nth(receiver, i);
    std.mem.sort(Ref, buf[0..len], SortCtx{ .key = key }, sortLessThan);
    var result = api.make_rrb(@ptrCast(&[_]Ref{}), 0);
    i = 0;
    while (i < len) : (i += 1) result = api.rrb_with(result, buf[i]);
    return result;
}

/// `[v sort]` — ascending, via `<` on elements.
fn vecSortImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return sortInto(s.receiver, NIL);
}

/// `[v sortBy: keyBlock]` — ascending by the 1-arg key block's result.
fn vecSortByImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return sortInto(s.receiver, args[0]);
}

// -- Phase 17 string batch: native impls (byte-level / alloc-loops) --------

fn smallintLeImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const a_ = untagSmallInt(s.receiver);
    if (isSmallInt(args[0])) return boolRef(a_ <= untagSmallInt(args[0]));
    if (isFloatRef(args[0])) return boolRef(@as(f64, @floatFromInt(a_)) <= floatRefValue(args[0]));
    return typeMismatchError();
}
fn smallintGeImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const a_ = untagSmallInt(s.receiver);
    if (isSmallInt(args[0])) return boolRef(a_ >= untagSmallInt(args[0]));
    if (isFloatRef(args[0])) return boolRef(@as(f64, @floatFromInt(a_)) >= floatRefValue(args[0]));
    return typeMismatchError();
}

/// `[s indexOf: sub ifAbsent: blk]` — byte index, or the block's value (no-NULL).
fn strIndexOfIfAbsentImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const idx = g_api.?.string_index_of(s.receiver, args[0]);
    if (idx < 0) return g_api.?.send0(args[1], sel_value_g);
    return mkSmallInt(idx);
}
/// `[s indexOf: sub]` — byte index, raises (domain error) when absent.
fn strIndexOfImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const idx = g_api.?.string_index_of(s.receiver, args[0]);
    if (idx < 0) return mkRuntimeError("indexOf: substring not found");
    return mkSmallInt(idx);
}
fn strAsIntegerImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return g_api.?.string_to_int(s.receiver);
}
fn strUpperImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return g_api.?.string_case(s.receiver, 0);
}
fn strLowerImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return g_api.?.string_case(s.receiver, 1);
}
fn strTrimImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return g_api.?.string_trim(s.receiver, 0);
}
fn strTrimLeftImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return g_api.?.string_trim(s.receiver, 1);
}
fn strTrimRightImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return g_api.?.string_trim(s.receiver, 2);
}
fn strSplitImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return g_api.?.string_split(s.receiver, args[0]);
}
fn strReverseImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return g_api.?.string_reverse(s.receiver);
}
/// `[aString asFloat]` — parse a decimal float; nil if not a number.
fn strAsFloatImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    if (!isString(s.receiver)) return NIL;
    const b = stringBytes(s.receiver);
    const f = std.fmt.parseFloat(f64, b) catch return NIL;
    return g_api.?.make_float(f);
}
/// `[aVectorOfStrings join: sep]` — concatenate with `sep` between elements.
fn vecJoinImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return g_api.?.string_join(s.receiver, args[0]);
}

/// Install Phase 17 stdlib: printString/displayString/print + asString on
/// OBJECT_VT (+ immediates), default `=`, vector `=`/sort/sortBy:, string
/// `<`/copyFrom:to:.
fn installStdlib() void {
    const api = g_api.?;
    sel_eq_g = api.intern("=".ptr, 1);
    sel_lt_g = api.intern("<".ptr, 1);
    const Pair = struct { name: []const u8, impl: *const anyopaque };

    // Printing + asString on every value (OBJECT_VT) and the non-delegating
    // immediates (SMALLINT_VT / TRUE_VT / FALSE_VT / NIL_VT).
    const print_methods = [_]Pair{
        .{ .name = "printString", .impl = @ptrCast(&printStringImpl) },
        .{ .name = "displayString", .impl = @ptrCast(&displayStringImpl) },
        .{ .name = "asString", .impl = @ptrCast(&displayStringImpl) },
        .{ .name = "print", .impl = @ptrCast(&printImpl) },
    };
    for ([_]u64{ OBJECT_VT, SMALLINT_VT, TRUE_VT, FALSE_VT, NIL_VT_EID }) |vt| {
        for (print_methods) |m| {
            _ = api.assert_fact(vt, api.intern(m.name.ptr, m.name.len), mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
        }
    }

    // Default identity `=` on OBJECT_VT + non-delegating immediates that lack
    // their own (SmallInt/String have value `=` already; TRUE/FALSE/NIL use
    // identity, which is correct — they are singletons).
    for ([_]u64{ OBJECT_VT, TRUE_VT, FALSE_VT, NIL_VT_EID }) |vt| {
        _ = api.assert_fact(vt, sel_eq_g, mkEid(api.make_closure(@ptrCast(&objEqImpl), NIL)), g_module_eid);
    }

    // Vector value-equality + sorting.
    _ = api.assert_fact(RRB_VT, sel_eq_g, mkEid(api.make_closure(@ptrCast(&vecEqImpl), NIL)), g_module_eid);
    _ = api.assert_fact(RRB_VT, api.intern("sort".ptr, 4), mkEid(api.make_closure(@ptrCast(&vecSortImpl), NIL)), g_module_eid);
    _ = api.assert_fact(RRB_VT, api.intern("sortBy:".ptr, 7), mkEid(api.make_closure(@ptrCast(&vecSortByImpl), NIL)), g_module_eid);

    // String comparison + substring.
    _ = api.assert_fact(STRING_VT, sel_lt_g, mkEid(api.make_closure(@ptrCast(&strLtImpl), NIL)), g_module_eid);
    _ = api.assert_fact(STRING_VT, api.intern("copyFrom:to:".ptr, 12), mkEid(api.make_closure(@ptrCast(&strCopyFromToImpl), NIL)), g_module_eid);
    _ = api.assert_fact(STRING_VT, api.intern("format:".ptr, 7), mkEid(api.make_closure(@ptrCast(&strFormatImpl), NIL)), g_module_eid);

    // SmallInt `<=` / `>=` (completeness — had only `<`/`>`/`=`).
    _ = api.assert_fact(SMALLINT_VT, api.intern("<=".ptr, 2), mkEid(api.make_closure(@ptrCast(&smallintLeImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, api.intern(">=".ptr, 2), mkEid(api.make_closure(@ptrCast(&smallintGeImpl), NIL)), g_module_eid);

    // SmallInt bitwise + shift (2026-06-17). Named keyword selectors keep the
    // int-vs-bool meaning unambiguous (`&`/`|` remain boolean on TRUE/FALSE_VT);
    // `<<`/`>>` infix are added as idiomatic shift sugar (they tokenize as
    // 2-char binary-ops and collide with nothing). Substrate for FFI flag sets,
    // byteswap, and bit-field unpacking.
    _ = api.assert_fact(SMALLINT_VT, api.intern("bitAnd:".ptr, 7), mkEid(api.make_closure(@ptrCast(&smallintBitAndImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, api.intern("bitOr:".ptr, 6), mkEid(api.make_closure(@ptrCast(&smallintBitOrImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, api.intern("bitXor:".ptr, 7), mkEid(api.make_closure(@ptrCast(&smallintBitXorImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, api.intern("bitNot".ptr, 6), mkEid(api.make_closure(@ptrCast(&smallintBitNotImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, api.intern("shiftLeft:".ptr, 10), mkEid(api.make_closure(@ptrCast(&smallintShlImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, api.intern("shiftRight:".ptr, 11), mkEid(api.make_closure(@ptrCast(&smallintShrImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, api.intern("<<".ptr, 2), mkEid(api.make_closure(@ptrCast(&smallintShlImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, api.intern(">>".ptr, 2), mkEid(api.make_closure(@ptrCast(&smallintShrImpl), NIL)), g_module_eid);

    // Three-way compare (Phase 17.3) — both the `<=>` infix (3-char binary-op)
    // and the `compare:` keyword surface map to one impl, on SmallInt + String.
    const cmp_spaceship = api.intern("<=>".ptr, 3);
    const cmp_keyword = api.intern("compare:".ptr, 8);
    _ = api.assert_fact(SMALLINT_VT, cmp_spaceship, mkEid(api.make_closure(@ptrCast(&smallintCompareImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, cmp_keyword, mkEid(api.make_closure(@ptrCast(&smallintCompareImpl), NIL)), g_module_eid);
    _ = api.assert_fact(STRING_VT, cmp_spaceship, mkEid(api.make_closure(@ptrCast(&strCompareImpl), NIL)), g_module_eid);
    _ = api.assert_fact(STRING_VT, cmp_keyword, mkEid(api.make_closure(@ptrCast(&strCompareImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FLOAT_VT, cmp_spaceship, mkEid(api.make_closure(@ptrCast(&floatCompareImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FLOAT_VT, cmp_keyword, mkEid(api.make_closure(@ptrCast(&floatCompareImpl), NIL)), g_module_eid);

    // Phase 17 string batch — native (byte-level). Birnam-composed string
    // methods (startsWith:/endsWith:/includes:/replace:with:/lines/words/<=,>,>=)
    // live in installPrelude over these + the byte primitives.
    const str_native = [_]Pair{
        .{ .name = "indexOf:ifAbsent:", .impl = @ptrCast(&strIndexOfIfAbsentImpl) },
        .{ .name = "indexOf:", .impl = @ptrCast(&strIndexOfImpl) },
        .{ .name = "asInteger", .impl = @ptrCast(&strAsIntegerImpl) },
        .{ .name = "asNumber", .impl = @ptrCast(&strAsIntegerImpl) },
        .{ .name = "asFloat", .impl = @ptrCast(&strAsFloatImpl) },
        .{ .name = "asUppercase", .impl = @ptrCast(&strUpperImpl) },
        .{ .name = "asLowercase", .impl = @ptrCast(&strLowerImpl) },
        .{ .name = "trim", .impl = @ptrCast(&strTrimImpl) },
        .{ .name = "trimLeft", .impl = @ptrCast(&strTrimLeftImpl) },
        .{ .name = "trimRight", .impl = @ptrCast(&strTrimRightImpl) },
        .{ .name = "split:", .impl = @ptrCast(&strSplitImpl) },
        .{ .name = "reverse", .impl = @ptrCast(&strReverseImpl) },
    };
    for (str_native) |m| {
        _ = api.assert_fact(STRING_VT, api.intern(m.name.ptr, m.name.len), mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }
    // join: lives on vectors (receiver = vector of strings).
    _ = api.assert_fact(RRB_VT, api.intern("join:".ptr, 5), mkEid(api.make_closure(@ptrCast(&vecJoinImpl), NIL)), g_module_eid);
}

// -- Phase 16 Tier A: persistent vector (RRB_VT) methods ------------------

/// Build a runtime ERROR_VT diagnostic carrying `msg`. The no-NULL "raise"
/// surface of the Result error model: errors are first-class values that
/// propagate through sends (ERROR_VT's `doesNotUnderstand:` returns self) and
/// halt statement sequences, until caught by `on:do:` or reported at the top.
fn mkRuntimeError(msg: []const u8) Ref {
    return mkRuntimeErrorKind(0, msg);
}

/// Like `mkRuntimeError` but tags the error with a `kind` symbol eid (0 = no
/// kind). The kind drives `on:do:` matching and shows in reports.
fn mkRuntimeErrorKind(kind_sel: u64, msg: []const u8) Ref {
    const api = g_api.?;
    const e = api.alloc_eid();
    _ = api.assert_fact(e, ATTR_VTABLE, mkEid(ERROR_VT), M_USER);
    _ = api.assert_fact(e, ATTR_ERROR_MESSAGE, api.make_string(msg.ptr, msg.len), M_USER);
    if (kind_sel != 0) _ = api.assert_fact(e, ATTR_ERROR_KIND, mkEid(kind_sel), M_USER);
    return mkEid(e);
}

/// `mkRuntimeError` with a `std.fmt`-formatted message (truncated to 192 bytes).
fn mkRuntimeErrorFmt(kind_sel: u64, comptime fmt: []const u8, fmt_args: anytype) Ref {
    var buf: [192]u8 = undefined;
    const msg = std.fmt.bufPrint(&buf, fmt, fmt_args) catch "error (message too long)";
    return mkRuntimeErrorKind(kind_sel, msg);
}

/// True if `r` is an ERROR_VT value (the Result error sentinel).
fn isErrorValue(r: Ref) bool {
    if (!isEid(r)) return false;
    return g_api.?.query_eavt(r >> 3, ATTR_VTABLE) == mkEid(ERROR_VT);
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
    while (i < len) : (i += 1) {
        acc = api.send2(block, sel_value_value_g, acc, api.rrb_nth(s.receiver, i));
        if (api.is_reduced(acc)) return api.reduced_value(acc); // early-out
    }
    return acc;
}

fn objReducedImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return g_api.?.make_reduced(s.receiver);
}

// -- Phase 16 deferred: integer ranges (RANGE_VT) -------------------------

fn smallintToImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return g_api.?.make_range(untagSmallInt(s.receiver), untagSmallInt(a[0]), 1);
}
fn smallintToByImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return g_api.?.make_range(untagSmallInt(s.receiver), untagSmallInt(a[0]), untagSmallInt(a[1]));
}
fn rangeSizeImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return mkSmallInt(@intCast(g_api.?.range_len(s.receiver)));
}
fn rangeAtImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    const i = untagSmallInt(a[0]);
    const len: i64 = @intCast(api.range_len(s.receiver));
    if (i < 0 or i >= len) return mkRuntimeError("range index out of bounds");
    return api.range_nth(s.receiver, @intCast(i));
}
fn rangeDoImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    const len = api.range_len(s.receiver);
    var i: u64 = 0;
    while (i < len) : (i += 1) _ = api.send1(a[0], sel_value_of_g, api.range_nth(s.receiver, i));
    return s.receiver;
}
fn rangeInjectIntoImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    var acc = a[0];
    const block = a[1];
    const len = api.range_len(s.receiver);
    var i: u64 = 0;
    while (i < len) : (i += 1) {
        acc = api.send2(block, sel_value_value_g, acc, api.range_nth(s.receiver, i));
        if (api.is_reduced(acc)) return api.reduced_value(acc);
    }
    return acc;
}

// -- Phase 16 deferred: string ops (STRING_VT) ----------------------------

fn strSizeImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return mkSmallInt(@intCast(g_api.?.string_len(s.receiver)));
}
fn strAtImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    const i = untagSmallInt(a[0]);
    const len: i64 = @intCast(api.string_len(s.receiver));
    if (i < 0 or i >= len) return mkRuntimeError("string index out of bounds");
    return api.string_byte_at(s.receiver, @intCast(i)); // byte as SmallInt
}
fn strConcatImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return g_api.?.string_concat(s.receiver, a[0]);
}
fn strEqImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return if (g_api.?.string_eq(s.receiver, a[0])) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}
fn strAsSymbolImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return g_api.?.string_as_symbol(s.receiver);
}
fn strDoImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    const len = api.string_len(s.receiver);
    var i: u64 = 0;
    while (i < len) : (i += 1) _ = api.send1(a[0], sel_value_of_g, api.string_byte_at(s.receiver, i));
    return s.receiver;
}
fn strInjectIntoImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    var acc = a[0];
    const block = a[1];
    const len = api.string_len(s.receiver);
    var i: u64 = 0;
    while (i < len) : (i += 1) {
        acc = api.send2(block, sel_value_value_g, acc, api.string_byte_at(s.receiver, i));
        if (api.is_reduced(acc)) return api.reduced_value(acc);
    }
    return acc;
}

// Map fold over VALUES — gives maps the generic seq protocol (collect:/select:).
fn mapInjectIntoImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    var acc = a[0];
    const block = a[1];
    const vals = api.map_values(s.receiver);
    const len = api.rrb_len(vals);
    var i: u64 = 0;
    while (i < len) : (i += 1) {
        acc = api.send2(block, sel_value_value_g, acc, api.rrb_nth(vals, i));
        if (api.is_reduced(acc)) return api.reduced_value(acc);
    }
    return acc;
}

// -- Phase 16 Tier C: map (ASSOC_VT) methods ------------------------------

fn mapAtPutImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return g_api.?.map_put(s.receiver, args[0], args[1]);
}
fn mapAtImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    if (api.map_has(s.receiver, args[0])) return api.map_get(s.receiver, args[0]);
    return mkRuntimeError("map key not found");
}
fn mapAtIfAbsentImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    if (api.map_has(s.receiver, args[0])) return api.map_get(s.receiver, args[0]);
    return api.send0(args[1], sel_value_g);
}
fn mapIncludesKeyImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return if (g_api.?.map_has(s.receiver, args[0])) mkEid(TRUE_EID) else mkEid(FALSE_EID);
}
fn mapSizeImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return mkSmallInt(@intCast(g_api.?.map_len(s.receiver)));
}
fn mapKeysImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return g_api.?.map_keys(s.receiver);
}
fn mapValuesImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ args, n };
    return g_api.?.map_values(s.receiver);
}
fn mapMergeImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return g_api.?.map_merge(s.receiver, args[0]);
}
fn mapDoImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    const vals = api.map_values(s.receiver); // iterate VALUES (Smalltalk Dict do:)
    const len = api.rrb_len(vals);
    var i: u64 = 0;
    while (i < len) : (i += 1) _ = api.send1(args[0], sel_value_of_g, api.rrb_nth(vals, i));
    return s.receiver;
}

// -- Phase 16 Tier D: bool / nil ergonomics -------------------------------

fn nilIsNilImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    return mkEid(TRUE_EID);
}
fn objIsNilImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    return mkEid(FALSE_EID);
}
fn nilIfNilImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    return g_api.?.send0(a[0], sel_value_g); // eval the ifNil block
}
fn objIfNilImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return s.receiver; // non-nil → unchanged
}
fn nilIfNotNilImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    return NIL;
}
fn objIfNotNilImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return g_api.?.send1(a[0], sel_value_of_g, s.receiver); // pass self to block
}
fn trueNotImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    return mkEid(FALSE_EID);
}
fn falseNotImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    return mkEid(TRUE_EID);
}
fn trueAndImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    return g_api.?.send0(a[0], sel_value_g); // short-circuit: eval rhs block
}
fn falseAndImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    return mkEid(FALSE_EID);
}
fn trueOrImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    return mkEid(TRUE_EID);
}
fn falseOrImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    return g_api.?.send0(a[0], sel_value_g);
}
// Eager `&` / `|` (Phase 17.5) — the argument is an already-evaluated value, not
// a block (that's the difference from short-circuit `and:`/`or:`). Smalltalk
// semantics: `true & x` = x, `false & x` = false, `true | x` = true, `false | x` = x.
fn trueAmpImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    return a[0];
}
fn falseAmpImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    return mkEid(FALSE_EID);
}
fn truePipeImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    return mkEid(TRUE_EID);
}
fn falsePipeImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    return a[0];
}

// -- Phase 16 Tier D: number protocol -------------------------------------

fn smallintModImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const a_ = untagSmallInt(s.receiver);
    if (isSmallInt(a[0])) {
        const d = untagSmallInt(a[0]);
        if (d == 0) return mkRuntimeErrorKind(g_api.?.intern("zeroDivide".ptr, 10), "divide by zero");
        // `@mod` result has magnitude < |d| ≤ SMALLINT_MAX, so it always fits.
        return mkSmallInt(@mod(a_, d));
    }
    if (isFloatRef(a[0])) {
        const d = floatRefValue(a[0]);
        if (d == 0.0) return mkRuntimeErrorKind(g_api.?.intern("zeroDivide".ptr, 10), "divide by zero");
        return g_api.?.make_float(@mod(@as(f64, @floatFromInt(a_)), d));
    }
    return typeMismatchError();
}
fn floatSqrtImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return g_api.?.make_float(@sqrt(floatRefValue(s.receiver)));
}
fn floatFloorImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return g_api.?.make_float(@floor(floatRefValue(s.receiver)));
}
fn floatCeilImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return g_api.?.make_float(@ceil(floatRefValue(s.receiver)));
}
fn floatRoundImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return g_api.?.make_float(@round(floatRefValue(s.receiver)));
}

// -- Phase 17.9 stdlib extras: Math / Random / Time / OS / String-utils -----

/// `[n abs]` — magnitude, preserving int-ness for SmallInt.
fn absImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    if (isFloatRef(s.receiver)) return g_api.?.make_float(@abs(floatRefValue(s.receiver)));
    const v = untagSmallInt(s.receiver);
    return mkSmallInt(if (v < 0) -v else v);
}
/// Int-or-float ref → f64 (`floatRefValue` only handles boxed floats).
fn numF(r: Ref) f64 {
    return if (isFloatRef(r)) floatRefValue(r) else @floatFromInt(untagSmallInt(r));
}
/// `[a min: b]` / `[a max: b]` — the smaller/larger of two numbers, returning
/// the original ref (so an int stays an int).
fn minKwImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    if (!isNumberRef(a[0])) return typeMismatchError();
    return if (numF(a[0]) < numF(s.receiver)) a[0] else s.receiver;
}
fn maxKwImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    if (!isNumberRef(a[0])) return typeMismatchError();
    return if (numF(a[0]) > numF(s.receiver)) a[0] else s.receiver;
}
/// `[base pow: exp]` — `base ** exp` as a float.
fn powKwImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    if (!isNumberRef(a[0])) return typeMismatchError();
    return g_api.?.make_float(std.math.pow(f64, numF(s.receiver), numF(a[0])));
}

// -- Random (global Xoshiro PRNG) -------------------------------------------

var g_prng: ?std.Random.DefaultPrng = null;
fn prng() *std.Random.DefaultPrng {
    if (g_prng == null) g_prng = std.Random.DefaultPrng.init(@bitCast(std.time.milliTimestamp()));
    return &g_prng.?;
}
/// `[Random seed: n]` — reseed deterministically (reproducible runs).
fn randSeedImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    if (!isSmallInt(a[0])) return typeMismatchError();
    g_prng = std.Random.DefaultPrng.init(@bitCast(untagSmallInt(a[0])));
    return s.receiver;
}
/// `[Random next]` — a float in [0, 1).
fn randNextImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    return g_api.?.make_float(prng().random().float(f64));
}
/// `[Random nextInt: n]` — an int in [0, n).
fn randNextIntImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    if (!isSmallInt(a[0])) return typeMismatchError();
    const hi = untagSmallInt(a[0]);
    if (hi <= 0) return mkRuntimeError("Random nextInt: needs a positive bound");
    return mkSmallInt(prng().random().intRangeLessThan(i64, 0, hi));
}
/// `[Random between: lo and: hi]` — an int in [lo, hi] (inclusive).
fn randBetweenImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    if (!isSmallInt(a[0]) or !isSmallInt(a[1])) return typeMismatchError();
    const lo = untagSmallInt(a[0]);
    const hi = untagSmallInt(a[1]);
    if (hi < lo) return mkRuntimeError("Random between:and: — hi < lo");
    return mkSmallInt(prng().random().intRangeAtMost(i64, lo, hi));
}

// -- Time (wall clock) ------------------------------------------------------

/// `[Time now]` — Unix time in whole seconds.
fn timeNowImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    return mkSmallInt(std.time.timestamp());
}
/// `[Time millis]` — Unix time in milliseconds.
fn timeMillisImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    return mkSmallInt(std.time.milliTimestamp());
}
/// `[Time sleep: ms]` — block the process for `ms` milliseconds.
fn timeSleepImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    if (!isSmallInt(a[0])) return typeMismatchError();
    const ms = untagSmallInt(a[0]);
    if (ms > 0) std.Thread.sleep(@as(u64, @intCast(ms)) * std.time.ns_per_ms);
    return s.receiver;
}

// -- OS / process (on System) -----------------------------------------------

// libc bits not surfaced by zig 0.15's std.c.
extern "c" fn setenv(name: [*:0]const u8, value: [*:0]const u8, overwrite: c_int) c_int;
extern "c" fn system(command: [*:0]const u8) c_int;

var g_cstr: [4096]u8 = undefined;
/// Copy a birnam string into the shared NUL-terminated scratch buffer.
fn cstr(r: Ref) ?[:0]const u8 {
    if (!isString(r)) return null;
    const b = stringBytes(r);
    if (b.len >= g_cstr.len) return null;
    @memcpy(g_cstr[0..b.len], b);
    g_cstr[b.len] = 0;
    return g_cstr[0..b.len :0];
}
/// `[System getEnv: name]` — environment value as a string, or nil if unset.
fn sysGetEnvImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const name = cstr(a[0]) orelse return NIL;
    const val = std.c.getenv(name.ptr) orelse return NIL;
    const slice = std.mem.span(val);
    return g_api.?.make_string(slice.ptr, slice.len);
}
/// `[System setEnv: name to: value]` — set an environment variable. Returns self.
fn sysSetEnvImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    if (!isString(a[0]) or !isString(a[1])) return typeMismatchError();
    // Two separate NUL-terminated copies (cstr shares one buffer).
    var nbuf: [1024]u8 = undefined;
    var vbuf: [4096]u8 = undefined;
    const nm = stringBytes(a[0]);
    const vl = stringBytes(a[1]);
    if (nm.len >= nbuf.len or vl.len >= vbuf.len) return mkRuntimeError("System setEnv: name/value too long");
    @memcpy(nbuf[0..nm.len], nm);
    nbuf[nm.len] = 0;
    @memcpy(vbuf[0..vl.len], vl);
    vbuf[vl.len] = 0;
    _ = setenv(nbuf[0..nm.len :0].ptr, vbuf[0..vl.len :0].ptr, 1);
    return s.receiver;
}
/// `[System exit: code]` — terminate the process with `code`.
fn sysExitImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const code: u8 = if (isSmallInt(a[0])) @truncate(@as(u64, @bitCast(untagSmallInt(a[0])))) else 0;
    std.process.exit(code);
}
/// `[System run: cmd]` — run `cmd` via the system shell; answer its exit code.
fn sysRunImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const cmd = cstr(a[0]) orelse return typeMismatchError();
    const status = system(cmd.ptr);
    // POSIX wait status → exit code (low 8 bits of status >> 8 on normal exit).
    return mkSmallInt(@intCast(@divTrunc(status, 256) & 0xff));
}

// -- String-utils (on STRING_VT) --------------------------------------------

var g_strbuf: [65536]u8 = undefined;
/// `[s repeat: n]` — `s` concatenated `n` times.
fn strRepeatImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    if (!isSmallInt(a[0])) return typeMismatchError();
    const count = untagSmallInt(a[0]);
    const b = stringBytes(s.receiver);
    if (count <= 0) return g_api.?.make_string(b.ptr, 0);
    const total = b.len * @as(usize, @intCast(count));
    if (total > g_strbuf.len) return mkRuntimeError("repeat: result too large");
    var o: usize = 0;
    var i: i64 = 0;
    while (i < count) : (i += 1) {
        @memcpy(g_strbuf[o .. o + b.len], b);
        o += b.len;
    }
    return g_api.?.make_string(g_strbuf[0..total].ptr, total);
}
/// Pad `s` to `width` with spaces; `left` = pad on the left (right-align).
fn padTo(s_ref: Ref, width_ref: Ref, left: bool) Ref {
    if (!isSmallInt(width_ref)) return typeMismatchError();
    const b = stringBytes(s_ref);
    const w: usize = blk: {
        const wv = untagSmallInt(width_ref);
        break :blk if (wv < 0) 0 else @intCast(wv);
    };
    if (b.len >= w or w > g_strbuf.len) return g_api.?.make_string(b.ptr, b.len);
    const pad = w - b.len;
    if (left) {
        @memset(g_strbuf[0..pad], ' ');
        @memcpy(g_strbuf[pad..w], b);
    } else {
        @memcpy(g_strbuf[0..b.len], b);
        @memset(g_strbuf[b.len..w], ' ');
    }
    return g_api.?.make_string(g_strbuf[0..w].ptr, w);
}
fn strPadLeftImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return padTo(s.receiver, a[0], true);
}
fn strPadRightImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return padTo(s.receiver, a[0], false);
}
/// `[s center: width]` — center `s` in `width` columns, space-padded.
fn strCenterImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    if (!isSmallInt(a[0])) return typeMismatchError();
    const b = stringBytes(s.receiver);
    const w: usize = blk: {
        const wv = untagSmallInt(a[0]);
        break :blk if (wv < 0) 0 else @intCast(wv);
    };
    if (b.len >= w or w > g_strbuf.len) return g_api.?.make_string(b.ptr, b.len);
    const lead = (w - b.len) / 2;
    @memset(g_strbuf[0..lead], ' ');
    @memcpy(g_strbuf[lead .. lead + b.len], b);
    @memset(g_strbuf[lead + b.len .. w], ' ');
    return g_api.?.make_string(g_strbuf[0..w].ptr, w);
}
fn asciiUpper(c: u8) u8 {
    return if (c >= 'a' and c <= 'z') c - 32 else c;
}
/// `[s capitalize]` — uppercase the first letter, rest unchanged.
fn strCapitalizeImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    const b = stringBytes(s.receiver);
    if (b.len == 0 or b.len > g_strbuf.len) return g_api.?.make_string(b.ptr, b.len);
    @memcpy(g_strbuf[0..b.len], b);
    g_strbuf[0] = asciiUpper(g_strbuf[0]);
    return g_api.?.make_string(g_strbuf[0..b.len].ptr, b.len);
}
/// `[s titleCase]` — uppercase the first letter of every space-separated word.
fn strTitleCaseImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    const b = stringBytes(s.receiver);
    if (b.len > g_strbuf.len) return g_api.?.make_string(b.ptr, b.len);
    @memcpy(g_strbuf[0..b.len], b);
    var at_word_start = true;
    for (g_strbuf[0..b.len]) |*c| {
        if (c.* == ' ' or c.* == '\t' or c.* == '\n') {
            at_word_start = true;
        } else {
            if (at_word_start) c.* = asciiUpper(c.*);
            at_word_start = false;
        }
    }
    return g_api.?.make_string(g_strbuf[0..b.len].ptr, b.len);
}

/// Install the Math / Random / Time / OS / String-utils surface (Phase 17.9).
fn installStdlibExtras() void {
    const api = g_api.?;
    const Pair = struct { name: []const u8, impl: *const anyopaque };

    // Math on both numeric vts.
    const math_methods = [_]Pair{
        .{ .name = "abs", .impl = @ptrCast(&absImpl) },
        .{ .name = "min:", .impl = @ptrCast(&minKwImpl) },
        .{ .name = "max:", .impl = @ptrCast(&maxKwImpl) },
        .{ .name = "pow:", .impl = @ptrCast(&powKwImpl) },
    };
    for ([_]u64{ SMALLINT_VT, FLOAT_VT }) |vt| {
        for (math_methods) |m| {
            _ = api.assert_fact(vt, api.intern(m.name.ptr, m.name.len), mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
        }
    }

    // String-utils on STRING_VT.
    const str_methods = [_]Pair{
        .{ .name = "repeat:", .impl = @ptrCast(&strRepeatImpl) },
        .{ .name = "padLeft:", .impl = @ptrCast(&strPadLeftImpl) },
        .{ .name = "padRight:", .impl = @ptrCast(&strPadRightImpl) },
        .{ .name = "center:", .impl = @ptrCast(&strCenterImpl) },
        .{ .name = "capitalize", .impl = @ptrCast(&strCapitalizeImpl) },
        .{ .name = "titleCase", .impl = @ptrCast(&strTitleCaseImpl) },
    };
    for (str_methods) |m| {
        _ = api.assert_fact(STRING_VT, api.intern(m.name.ptr, m.name.len), mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }

    // OS / process methods on the existing System prototype.
    const os_methods = [_]Pair{
        .{ .name = "getEnv:", .impl = @ptrCast(&sysGetEnvImpl) },
        .{ .name = "setEnv:to:", .impl = @ptrCast(&sysSetEnvImpl) },
        .{ .name = "exit:", .impl = @ptrCast(&sysExitImpl) },
        .{ .name = "run:", .impl = @ptrCast(&sysRunImpl) },
    };
    for (os_methods) |m| {
        _ = api.assert_fact(SYSTEM_PROTO_VT, api.intern(m.name.ptr, m.name.len), mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }

    // Time prototype.
    _ = api.assert_fact(TIME_PROTO, ATTR_VTABLE, mkEid(TIME_PROTO_VT), g_module_eid);
    _ = api.assert_fact(TIME_PROTO_VT, ATTR_DELEGATE, mkEid(OBJECT_VT), g_module_eid);
    const time_methods = [_]Pair{
        .{ .name = "now", .impl = @ptrCast(&timeNowImpl) },
        .{ .name = "millis", .impl = @ptrCast(&timeMillisImpl) },
        .{ .name = "sleep:", .impl = @ptrCast(&timeSleepImpl) },
    };
    for (time_methods) |m| {
        _ = api.assert_fact(TIME_PROTO_VT, api.intern(m.name.ptr, m.name.len), mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }
    pushGlobalBinding(GLOBALS_SCOPE_EID, api.intern("Time".ptr, 4), mkEid(TIME_PROTO));

    // Random prototype.
    _ = api.assert_fact(RANDOM_PROTO, ATTR_VTABLE, mkEid(RANDOM_PROTO_VT), g_module_eid);
    _ = api.assert_fact(RANDOM_PROTO_VT, ATTR_DELEGATE, mkEid(OBJECT_VT), g_module_eid);
    const rand_methods = [_]Pair{
        .{ .name = "seed:", .impl = @ptrCast(&randSeedImpl) },
        .{ .name = "next", .impl = @ptrCast(&randNextImpl) },
        .{ .name = "nextInt:", .impl = @ptrCast(&randNextIntImpl) },
        .{ .name = "between:and:", .impl = @ptrCast(&randBetweenImpl) },
    };
    for (rand_methods) |m| {
        _ = api.assert_fact(RANDOM_PROTO_VT, api.intern(m.name.ptr, m.name.len), mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }
    pushGlobalBinding(GLOBALS_SCOPE_EID, api.intern("Random".ptr, 6), mkEid(RANDOM_PROTO));
}

// -- Phase 17.10: JSON parser via PEG ---------------------------------------

const JSON_PROTO_VT = ke.JSON_PROTO_VT;
const JSON_PROTO = ke.JSON_PROTO;

var g_json_grammar: u64 = 0;
var sel_at_put_g: u64 = 0;
// Cached JSON rule-name eids for CST dispatch.
var jrn_value: u64 = 0;
var jrn_object: u64 = 0;
var jrn_array: u64 = 0;
var jrn_string: u64 = 0;
var jrn_number: u64 = 0;
var jrn_true: u64 = 0;
var jrn_false: u64 = 0;
var jrn_null: u64 = 0;
var jrn_pair: u64 = 0;
var jrn_members: u64 = 0;
var jrn_elements: u64 = 0;

fn isEidRef(r: Ref) bool {
    return (r & 0b111) == 0b010;
}

/// Build the JSON grammar (a separate grammar object; rule-refs are the only
/// patterns that create CST nodes, so lowering dispatches on rule name).
fn buildJsonGrammar() void {
    const api = g_api.?;
    const grammar = api.send0(mkEid(PEG_EID), sel_new_grammar);
    g_json_grammar = untagEid(grammar);

    const ws = [_]u8{ ' ', ' ', '\t', '\t', '\n', '\n', '\r', '\r' };
    defineRule(grammar, "ws", peg_star(peg_charset(&ws)));

    const digit = [_]u8{ '0', '9' };
    {
        const dq = [_]u8{ '"', '"' };
        const bs = [_]u8{ '\\', '\\' };
        const esc_parts = [_]u64{ untagEid(peg_charset(&bs)), untagEid(peg_any()) };
        const esc = peg_seq(&esc_parts);
        const norm_parts = [_]u64{ untagEid(peg_not(peg_charset(&dq))), untagEid(peg_any()) };
        const norm = peg_seq(&norm_parts);
        const ch_alts = [_]u64{ untagEid(esc), untagEid(norm) };
        const ch = peg_choice(&ch_alts);
        const parts = [_]u64{ untagEid(peg_literal("\"")), untagEid(peg_star(ch)), untagEid(peg_literal("\"")) };
        defineRule(grammar, "string", peg_seq(&parts));
    }
    {
        const neg = peg_opt(peg_literal("-"));
        const int = peg_plus(peg_charset(&digit));
        const frac_parts = [_]u64{ untagEid(peg_literal(".")), untagEid(peg_plus(peg_charset(&digit))) };
        const frac = peg_opt(peg_seq(&frac_parts));
        const e_ch = [_]u8{ 'e', 'e', 'E', 'E' };
        const sign = [_]u8{ '+', '+', '-', '-' };
        const exp_parts = [_]u64{ untagEid(peg_charset(&e_ch)), untagEid(peg_opt(peg_charset(&sign))), untagEid(peg_plus(peg_charset(&digit))) };
        const exp = peg_opt(peg_seq(&exp_parts));
        const parts = [_]u64{ untagEid(neg), untagEid(int), untagEid(frac), untagEid(exp) };
        defineRule(grammar, "number", peg_seq(&parts));
    }
    defineRule(grammar, "true", peg_literal("true"));
    defineRule(grammar, "false", peg_literal("false"));
    defineRule(grammar, "null", peg_literal("null"));
    {
        const alts = [_]u64{ untagEid(peg_rule_ref("object")), untagEid(peg_rule_ref("array")), untagEid(peg_rule_ref("string")), untagEid(peg_rule_ref("number")), untagEid(peg_rule_ref("true")), untagEid(peg_rule_ref("false")), untagEid(peg_rule_ref("null")) };
        defineRule(grammar, "value", peg_choice(&alts));
    }
    {
        const parts = [_]u64{ untagEid(peg_rule_ref("string")), untagEid(peg_rule_ref("ws")), untagEid(peg_literal(":")), untagEid(peg_rule_ref("ws")), untagEid(peg_rule_ref("value")) };
        defineRule(grammar, "pair", peg_seq(&parts));
    }
    {
        const rep_parts = [_]u64{ untagEid(peg_rule_ref("ws")), untagEid(peg_literal(",")), untagEid(peg_rule_ref("ws")), untagEid(peg_rule_ref("pair")) };
        const rep = peg_star(peg_seq(&rep_parts));
        const parts = [_]u64{ untagEid(peg_rule_ref("pair")), untagEid(rep) };
        defineRule(grammar, "members", peg_seq(&parts));
    }
    {
        const parts = [_]u64{ untagEid(peg_literal("{")), untagEid(peg_rule_ref("ws")), untagEid(peg_opt(peg_rule_ref("members"))), untagEid(peg_rule_ref("ws")), untagEid(peg_literal("}")) };
        defineRule(grammar, "object", peg_seq(&parts));
    }
    {
        const rep_parts = [_]u64{ untagEid(peg_rule_ref("ws")), untagEid(peg_literal(",")), untagEid(peg_rule_ref("ws")), untagEid(peg_rule_ref("value")) };
        const rep = peg_star(peg_seq(&rep_parts));
        const parts = [_]u64{ untagEid(peg_rule_ref("value")), untagEid(rep) };
        defineRule(grammar, "elements", peg_seq(&parts));
    }
    {
        const parts = [_]u64{ untagEid(peg_literal("[")), untagEid(peg_rule_ref("ws")), untagEid(peg_opt(peg_rule_ref("elements"))), untagEid(peg_rule_ref("ws")), untagEid(peg_literal("]")) };
        defineRule(grammar, "array", peg_seq(&parts));
    }
    {
        const parts = [_]u64{ untagEid(peg_rule_ref("ws")), untagEid(peg_rule_ref("value")), untagEid(peg_rule_ref("ws")) };
        defineRule(grammar, "doc", peg_seq(&parts));
    }

    jrn_value = api.intern("value".ptr, 5);
    jrn_object = api.intern("object".ptr, 6);
    jrn_array = api.intern("array".ptr, 5);
    jrn_string = api.intern("string".ptr, 6);
    jrn_number = api.intern("number".ptr, 6);
    jrn_true = api.intern("true".ptr, 4);
    jrn_false = api.intern("false".ptr, 5);
    jrn_null = api.intern("null".ptr, 4);
    jrn_pair = api.intern("pair".ptr, 4);
    jrn_members = api.intern("members".ptr, 7);
    jrn_elements = api.intern("elements".ptr, 8);
    sel_at_put_g = api.intern("at:put:".ptr, 7);

    // Install the `Json` global prototype.
    _ = api.assert_fact(JSON_PROTO, ATTR_VTABLE, mkEid(JSON_PROTO_VT), g_module_eid);
    _ = api.assert_fact(JSON_PROTO_VT, ATTR_DELEGATE, mkEid(OBJECT_VT), g_module_eid);
    _ = api.assert_fact(JSON_PROTO_VT, api.intern("parse:".ptr, 6), mkEid(api.make_closure(@ptrCast(&jsonParseImpl), NIL)), g_module_eid);
    _ = api.assert_fact(JSON_PROTO_VT, api.intern("stringify:".ptr, 10), mkEid(api.make_closure(@ptrCast(&jsonStringifyImpl), NIL)), g_module_eid);
    pushGlobalBinding(GLOBALS_SCOPE_EID, api.intern("Json".ptr, 4), mkEid(JSON_PROTO));
}

/// First child of `node` whose rule name is `rule`, or 0.
fn cstChildByRule(node: u64, rule: u64) u64 {
    var c = cstFirstChild(node);
    while (c != 0) : (c = cstNextSibling(c)) {
        if (cstRuleName(c) == rule) return c;
    }
    return 0;
}

/// Lower a JSON CST node (a named rule node) to a birnam value.
fn jsonLower(node: u64) Ref {
    const api = g_api.?;
    const rn = cstRuleName(node);
    if (rn == jrn_value) {
        var c = cstFirstChild(node);
        while (c != 0) : (c = cstNextSibling(c)) {
            if (cstRuleName(c) != 0) return jsonLower(c);
        }
        return NIL;
    } else if (rn == jrn_object) {
        var map = api.make_map();
        const members = cstChildByRule(node, jrn_members);
        if (members != 0) {
            var p = cstFirstChild(members);
            while (p != 0) : (p = cstNextSibling(p)) {
                if (cstRuleName(p) != jrn_pair) continue;
                const ks = cstChildByRule(p, jrn_string);
                const vv = cstChildByRule(p, jrn_value);
                if (ks == 0 or vv == 0) continue;
                const sp = cstByteRange(ks);
                const key = if (sp.end > sp.start + 1)
                    mkEid(api.intern(g_source[sp.start + 1 .. sp.end - 1].ptr, sp.end - sp.start - 2))
                else
                    mkEid(api.intern("".ptr, 0));
                map = api.send2(map, sel_at_put_g, key, jsonLower(vv));
            }
        }
        return map;
    } else if (rn == jrn_array) {
        var vec = api.make_rrb(@ptrCast(&[_]Ref{}), 0);
        const elems = cstChildByRule(node, jrn_elements);
        if (elems != 0) {
            var v = cstFirstChild(elems);
            while (v != 0) : (v = cstNextSibling(v)) {
                if (cstRuleName(v) != jrn_value) continue;
                vec = api.rrb_with(vec, jsonLower(v));
            }
        }
        return vec;
    } else if (rn == jrn_string) {
        const sp = cstByteRange(node);
        if (sp.end <= sp.start + 1) return api.make_string("".ptr, 0);
        return decodeStringLiteral(g_source[sp.start + 1 .. sp.end - 1]);
    } else if (rn == jrn_number) {
        const sp = cstByteRange(node);
        const bytes = g_source[sp.start..sp.end];
        if (std.mem.indexOfAny(u8, bytes, ".eE") != null) {
            return api.make_float(std.fmt.parseFloat(f64, bytes) catch 0.0);
        }
        return mkSmallInt(std.fmt.parseInt(i64, bytes, 10) catch 0);
    } else if (rn == jrn_true) {
        return mkEid(TRUE_EID);
    } else if (rn == jrn_false) {
        return mkEid(FALSE_EID);
    } else if (rn == jrn_null) {
        return NIL;
    }
    return NIL;
}

/// `[Json parse: aString]` — parse JSON text into birnam maps/vectors/strings/
/// numbers/bool/nil. Returns an ERROR_VT on a syntax error.
fn jsonParseImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    if (!isString(a[0])) return mkRuntimeError("Json parse: expects a string");
    const src = stringBytes(a[0]);
    g_source = src;
    const src_dup = api.make_string(src.ptr, src.len);
    const rule = api.make_string("doc".ptr, 3);
    const cst = api.send2(mkEid(g_json_grammar), internSel("parse:from:"), src_dup, rule);
    if (!isEidRef(cst)) return mkRuntimeError("Json parse: failed");
    const cst_eid = untagEid(cst);
    const vt = api.query_eavt(cst_eid, ATTR_VTABLE);
    if (isEidRef(vt) and untagEid(vt) == ERROR_VT) return cst;
    const val = cstChildByRule(cst_eid, jrn_value);
    if (val == 0) return NIL;
    return jsonLower(val);
}

// JSON stringify — recursive serializer into the shared g_strbuf.
fn jsonAppend(pos: *usize, bytes: []const u8) bool {
    if (pos.* + bytes.len > g_strbuf.len) return false;
    @memcpy(g_strbuf[pos.* .. pos.* + bytes.len], bytes);
    pos.* += bytes.len;
    return true;
}
fn jsonWriteStr(pos: *usize, str: []const u8) bool {
    if (!jsonAppend(pos, "\"")) return false;
    for (str) |c| {
        const esc: ?[]const u8 = switch (c) {
            '"' => "\\\"",
            '\\' => "\\\\",
            '\n' => "\\n",
            '\r' => "\\r",
            '\t' => "\\t",
            else => null,
        };
        if (esc) |e| {
            if (!jsonAppend(pos, e)) return false;
        } else {
            if (pos.* >= g_strbuf.len) return false;
            g_strbuf[pos.*] = c;
            pos.* += 1;
        }
    }
    return jsonAppend(pos, "\"");
}
fn jsonWriteVal(pos: *usize, v: Ref) bool {
    const api = g_api.?;
    if (isNil(v)) return jsonAppend(pos, "null");
    if (v == mkEid(TRUE_EID)) return jsonAppend(pos, "true");
    if (v == mkEid(FALSE_EID)) return jsonAppend(pos, "false");
    if (isSmallInt(v)) {
        var b: [24]u8 = undefined;
        return jsonAppend(pos, std.fmt.bufPrint(&b, "{d}", .{untagSmallInt(v)}) catch return false);
    }
    if (isFloatRef(v)) {
        var b: [32]u8 = undefined;
        return jsonAppend(pos, std.fmt.bufPrint(&b, "{d}", .{floatRefValue(v)}) catch return false);
    }
    if (isString(v)) return jsonWriteStr(pos, stringBytes(v));
    if (isVectorRef(v)) {
        if (!jsonAppend(pos, "[")) return false;
        const len = api.rrb_len(v);
        var i: u64 = 0;
        while (i < len) : (i += 1) {
            if (i > 0 and !jsonAppend(pos, ",")) return false;
            if (!jsonWriteVal(pos, api.rrb_nth(v, i))) return false;
        }
        return jsonAppend(pos, "]");
    }
    if (isMapRef(v)) {
        if (!jsonAppend(pos, "{")) return false;
        const keys = api.map_keys(v);
        const len = api.rrb_len(keys);
        var i: u64 = 0;
        while (i < len) : (i += 1) {
            if (i > 0 and !jsonAppend(pos, ",")) return false;
            const k = api.rrb_nth(keys, i);
            if (isString(k)) {
                if (!jsonWriteStr(pos, stringBytes(k))) return false;
            } else if (isEidRef(k)) {
                const nm = api.symbol_name(untagEid(k));
                if (!jsonWriteStr(pos, if (isString(nm)) stringBytes(nm) else "?")) return false;
            } else if (!jsonWriteStr(pos, "?")) return false;
            if (!jsonAppend(pos, ":")) return false;
            if (!jsonWriteVal(pos, api.map_get(v, k))) return false;
        }
        return jsonAppend(pos, "}");
    }
    return jsonAppend(pos, "null");
}
/// `[Json stringify: value]` — birnam value → compact JSON text. Map keys are
/// symbols or strings; other value types serialize structurally.
fn jsonStringifyImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    var pos: usize = 0;
    if (!jsonWriteVal(&pos, a[0])) return mkRuntimeError("Json stringify: result too large");
    return g_api.?.make_string(g_strbuf[0..pos].ptr, pos);
}

// -- Phase 17.10: YAML (flat subset) via PEG --------------------------------
//
// Scope: ONE level of block structure (block mapping OR block sequence of
// scalars/flow), plus inline flow `[a, b]` / `{k: v}`. Real YAML is
// indentation-sensitive and not cleanly PEG-expressible; this covers the 80%
// that configs use. Documented limit.

const YAML_PROTO_VT = ke.YAML_PROTO_VT;
const YAML_PROTO = ke.YAML_PROTO;

var g_yaml_grammar: u64 = 0;
var yrn_mapping: u64 = 0;
var yrn_sequence: u64 = 0;
var yrn_pair: u64 = 0;
var yrn_item: u64 = 0;
var yrn_value: u64 = 0;
var yrn_flowseq: u64 = 0;
var yrn_flowmap: u64 = 0;
var yrn_fmpair: u64 = 0;
var yrn_key: u64 = 0;
var yrn_quoted: u64 = 0;
var yrn_number: u64 = 0;
var yrn_bareword: u64 = 0;

fn buildYamlGrammar() void {
    const api = g_api.?;
    sel_at_put_g = api.intern("at:put:".ptr, 7); // shared lowering selector (was set by buildJsonGrammar)
    const grammar = api.send0(mkEid(PEG_EID), sel_new_grammar);
    g_yaml_grammar = untagEid(grammar);

    const hsp = [_]u8{ ' ', ' ', '\t', '\t' }; // horizontal space
    const nlc = [_]u8{ '\n', '\n', '\r', '\r' };
    const digit = [_]u8{ '0', '9' };
    defineRule(grammar, "y-hsp", peg_star(peg_charset(&hsp)));
    defineRule(grammar, "y-nl", peg_plus(peg_charset(&nlc))); // one or more line breaks

    // y-quoted = '"' ( '\' any / !'"' any )* '"'
    {
        const dq = [_]u8{ '"', '"' };
        const bs = [_]u8{ '\\', '\\' };
        const esc_p = [_]u64{ untagEid(peg_charset(&bs)), untagEid(peg_any()) };
        const norm_p = [_]u64{ untagEid(peg_not(peg_charset(&dq))), untagEid(peg_any()) };
        const ch_alts = [_]u64{ untagEid(peg_seq(&esc_p)), untagEid(peg_seq(&norm_p)) };
        const parts = [_]u64{ untagEid(peg_literal("\"")), untagEid(peg_star(peg_choice(&ch_alts))), untagEid(peg_literal("\"")) };
        defineRule(grammar, "y-quoted", peg_seq(&parts));
    }
    // y-number = "-"? digit+ ("." digit+)?
    {
        const frac_p = [_]u64{ untagEid(peg_literal(".")), untagEid(peg_plus(peg_charset(&digit))) };
        const parts = [_]u64{ untagEid(peg_opt(peg_literal("-"))), untagEid(peg_plus(peg_charset(&digit))), untagEid(peg_opt(peg_seq(&frac_p))) };
        defineRule(grammar, "y-number", peg_seq(&parts));
    }
    // y-bareword = (! [stop] any)+   stop = nl , ] } (value context)
    {
        const stop = [_]u8{ '\n', '\n', '\r', '\r', ',', ',', ']', ']', '}', '}' };
        const step = [_]u64{ untagEid(peg_not(peg_charset(&stop))), untagEid(peg_any()) };
        defineRule(grammar, "y-bareword", peg_plus(peg_seq(&step)));
    }
    // y-key = y-quoted / (! [: nl , } ] any)+   (key context also stops at ':')
    {
        const kstop = [_]u8{ ':', ':', '\n', '\n', '\r', '\r', ',', ',', '}', '}' };
        const step = [_]u64{ untagEid(peg_not(peg_charset(&kstop))), untagEid(peg_any()) };
        const bare = peg_plus(peg_seq(&step));
        const alts = [_]u64{ untagEid(peg_rule_ref("y-quoted")), untagEid(bare) };
        defineRule(grammar, "y-key", peg_choice(&alts));
    }
    // y-value = y-flowseq / y-flowmap / y-quoted / y-number / y-bareword
    {
        const alts = [_]u64{ untagEid(peg_rule_ref("y-flowseq")), untagEid(peg_rule_ref("y-flowmap")), untagEid(peg_rule_ref("y-quoted")), untagEid(peg_rule_ref("y-number")), untagEid(peg_rule_ref("y-bareword")) };
        defineRule(grammar, "y-value", peg_choice(&alts));
    }
    // y-flowseq = "[" hsp ( y-value (hsp "," hsp y-value)* )? hsp "]"
    {
        const rep = [_]u64{ untagEid(peg_rule_ref("y-hsp")), untagEid(peg_literal(",")), untagEid(peg_rule_ref("y-hsp")), untagEid(peg_rule_ref("y-value")) };
        const tail = peg_star(peg_seq(&rep));
        const inner = [_]u64{ untagEid(peg_rule_ref("y-value")), untagEid(tail) };
        const parts = [_]u64{ untagEid(peg_literal("[")), untagEid(peg_rule_ref("y-hsp")), untagEid(peg_opt(peg_seq(&inner))), untagEid(peg_rule_ref("y-hsp")), untagEid(peg_literal("]")) };
        defineRule(grammar, "y-flowseq", peg_seq(&parts));
    }
    // y-fmpair = y-key hsp ":" hsp y-value
    {
        const parts = [_]u64{ untagEid(peg_rule_ref("y-key")), untagEid(peg_rule_ref("y-hsp")), untagEid(peg_literal(":")), untagEid(peg_rule_ref("y-hsp")), untagEid(peg_rule_ref("y-value")) };
        defineRule(grammar, "y-fmpair", peg_seq(&parts));
    }
    // y-flowmap = "{" hsp ( y-fmpair (hsp "," hsp y-fmpair)* )? hsp "}"
    {
        const rep = [_]u64{ untagEid(peg_rule_ref("y-hsp")), untagEid(peg_literal(",")), untagEid(peg_rule_ref("y-hsp")), untagEid(peg_rule_ref("y-fmpair")) };
        const tail = peg_star(peg_seq(&rep));
        const inner = [_]u64{ untagEid(peg_rule_ref("y-fmpair")), untagEid(tail) };
        const parts = [_]u64{ untagEid(peg_literal("{")), untagEid(peg_rule_ref("y-hsp")), untagEid(peg_opt(peg_seq(&inner))), untagEid(peg_rule_ref("y-hsp")), untagEid(peg_literal("}")) };
        defineRule(grammar, "y-flowmap", peg_seq(&parts));
    }
    // y-pair = hsp y-key hsp ":" hsp y-value
    {
        const parts = [_]u64{ untagEid(peg_rule_ref("y-hsp")), untagEid(peg_rule_ref("y-key")), untagEid(peg_rule_ref("y-hsp")), untagEid(peg_literal(":")), untagEid(peg_rule_ref("y-hsp")), untagEid(peg_rule_ref("y-value")) };
        defineRule(grammar, "y-pair", peg_seq(&parts));
    }
    // y-mapping = y-pair (nl y-pair)*
    {
        const rep = [_]u64{ untagEid(peg_rule_ref("y-nl")), untagEid(peg_rule_ref("y-pair")) };
        const parts = [_]u64{ untagEid(peg_rule_ref("y-pair")), untagEid(peg_star(peg_seq(&rep))) };
        defineRule(grammar, "y-mapping", peg_seq(&parts));
    }
    // y-item = hsp "-" hsp y-value
    {
        const parts = [_]u64{ untagEid(peg_rule_ref("y-hsp")), untagEid(peg_literal("-")), untagEid(peg_rule_ref("y-hsp")), untagEid(peg_rule_ref("y-value")) };
        defineRule(grammar, "y-item", peg_seq(&parts));
    }
    // y-sequence = y-item (nl y-item)*
    {
        const rep = [_]u64{ untagEid(peg_rule_ref("y-nl")), untagEid(peg_rule_ref("y-item")) };
        const parts = [_]u64{ untagEid(peg_rule_ref("y-item")), untagEid(peg_star(peg_seq(&rep))) };
        defineRule(grammar, "y-sequence", peg_seq(&parts));
    }
    // y-doc = nl? (y-sequence / y-mapping) nl?
    {
        const body = [_]u64{ untagEid(peg_rule_ref("y-sequence")), untagEid(peg_rule_ref("y-mapping")) };
        const parts = [_]u64{ untagEid(peg_opt(peg_rule_ref("y-nl"))), untagEid(peg_choice(&body)), untagEid(peg_opt(peg_rule_ref("y-nl"))) };
        defineRule(grammar, "y-doc", peg_seq(&parts));
    }

    yrn_mapping = api.intern("y-mapping".ptr, 9);
    yrn_sequence = api.intern("y-sequence".ptr, 10);
    yrn_pair = api.intern("y-pair".ptr, 6);
    yrn_item = api.intern("y-item".ptr, 6);
    yrn_value = api.intern("y-value".ptr, 7);
    yrn_flowseq = api.intern("y-flowseq".ptr, 9);
    yrn_flowmap = api.intern("y-flowmap".ptr, 9);
    yrn_fmpair = api.intern("y-fmpair".ptr, 8);
    yrn_key = api.intern("y-key".ptr, 5);
    yrn_quoted = api.intern("y-quoted".ptr, 8);
    yrn_number = api.intern("y-number".ptr, 8);
    yrn_bareword = api.intern("y-bareword".ptr, 10);

    _ = api.assert_fact(YAML_PROTO, ATTR_VTABLE, mkEid(YAML_PROTO_VT), g_module_eid);
    _ = api.assert_fact(YAML_PROTO_VT, ATTR_DELEGATE, mkEid(OBJECT_VT), g_module_eid);
    _ = api.assert_fact(YAML_PROTO_VT, api.intern("parse:".ptr, 6), mkEid(api.make_closure(@ptrCast(&yamlParseImpl), NIL)), g_module_eid);
    pushGlobalBinding(GLOBALS_SCOPE_EID, api.intern("Yaml".ptr, 4), mkEid(YAML_PROTO));
}

/// Trim trailing horizontal space from a byte range.
fn rtrimHsp(b: []const u8) []const u8 {
    var end = b.len;
    while (end > 0 and (b[end - 1] == ' ' or b[end - 1] == '\t')) end -= 1;
    return b[0..end];
}

/// A scalar's birnam value: quoted→string, number→int/float, bareword→
/// true/false→bool, null/~/empty→nil, else string.
fn yamlScalar(node: u64) Ref {
    const api = g_api.?;
    const rn = cstRuleName(node);
    const sp = cstByteRange(node);
    if (rn == yrn_quoted) {
        if (sp.end <= sp.start + 1) return api.make_string("".ptr, 0);
        return decodeStringLiteral(g_source[sp.start + 1 .. sp.end - 1]);
    }
    if (rn == yrn_number) {
        const bytes = g_source[sp.start..sp.end];
        if (std.mem.indexOfScalar(u8, bytes, '.') != null) {
            return api.make_float(std.fmt.parseFloat(f64, bytes) catch 0.0);
        }
        return mkSmallInt(std.fmt.parseInt(i64, bytes, 10) catch 0);
    }
    // bareword
    const raw = rtrimHsp(g_source[sp.start..sp.end]);
    if (std.mem.eql(u8, raw, "true")) return mkEid(TRUE_EID);
    if (std.mem.eql(u8, raw, "false")) return mkEid(FALSE_EID);
    if (raw.len == 0 or std.mem.eql(u8, raw, "null") or std.mem.eql(u8, raw, "~")) return NIL;
    return api.make_string(raw.ptr, raw.len);
}

/// Lower a y-value node (its single matched child) to a birnam value.
fn yamlLowerValue(vnode: u64) Ref {
    const api = g_api.?;
    var c = cstFirstChild(vnode);
    while (c != 0) : (c = cstNextSibling(c)) {
        const rn = cstRuleName(c);
        if (rn == 0) continue;
        if (rn == yrn_flowseq) {
            var vec = api.make_rrb(@ptrCast(&[_]Ref{}), 0);
            var e = cstFirstChild(c);
            while (e != 0) : (e = cstNextSibling(e)) {
                if (cstRuleName(e) == yrn_value) vec = api.rrb_with(vec, yamlLowerValue(e));
            }
            return vec;
        }
        if (rn == yrn_flowmap) {
            var map = api.make_map();
            var p = cstFirstChild(c);
            while (p != 0) : (p = cstNextSibling(p)) {
                if (cstRuleName(p) != yrn_fmpair) continue;
                map = api.send2(map, sel_at_put_g, yamlKey(p), yamlPairValue(p));
            }
            return map;
        }
        // scalar (quoted / number / bareword)
        return yamlScalar(c);
    }
    return NIL;
}

/// The key of a pair node → an interned symbol (quoted body or trimmed bareword).
fn yamlKey(pair: u64) Ref {
    const api = g_api.?;
    const k = cstChildByRule(pair, yrn_key);
    if (k == 0) return NIL;
    // y-key is a choice: a y-quoted child node, or a bareword span on y-key itself.
    const q = cstChildByRule(k, yrn_quoted);
    if (q != 0) {
        const sp = cstByteRange(q);
        if (sp.end <= sp.start + 1) return mkEid(api.intern("".ptr, 0));
        const inner = g_source[sp.start + 1 .. sp.end - 1];
        return mkEid(api.intern(inner.ptr, inner.len));
    }
    const sp = cstByteRange(k);
    const raw = rtrimHsp(g_source[sp.start..sp.end]);
    return mkEid(api.intern(raw.ptr, raw.len));
}
fn yamlPairValue(pair: u64) Ref {
    const v = cstChildByRule(pair, yrn_value);
    return if (v != 0) yamlLowerValue(v) else NIL;
}

/// `[Yaml parse: aString]` — parse a flat-subset YAML document to birnam data.
fn yamlParseImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    if (!isString(a[0])) return mkRuntimeError("Yaml parse: expects a string");
    const src = stringBytes(a[0]);
    g_source = src;
    const src_dup = api.make_string(src.ptr, src.len);
    const rule = api.make_string("y-doc".ptr, 5);
    const cst = api.send2(mkEid(g_yaml_grammar), internSel("parse:from:"), src_dup, rule);
    if (!isEidRef(cst)) return mkRuntimeError("Yaml parse: failed");
    const cst_eid = untagEid(cst);
    const vt = api.query_eavt(cst_eid, ATTR_VTABLE);
    if (isEidRef(vt) and untagEid(vt) == ERROR_VT) return cst;
    // doc child: a mapping or a sequence.
    const m = cstChildByRule(cst_eid, yrn_mapping);
    if (m != 0) {
        var map = api.make_map();
        var p = cstFirstChild(m);
        while (p != 0) : (p = cstNextSibling(p)) {
            if (cstRuleName(p) != yrn_pair) continue;
            map = api.send2(map, sel_at_put_g, yamlKey(p), yamlPairValue(p));
        }
        return map;
    }
    const sq = cstChildByRule(cst_eid, yrn_sequence);
    if (sq != 0) {
        var vec = api.make_rrb(@ptrCast(&[_]Ref{}), 0);
        var it = cstFirstChild(sq);
        while (it != 0) : (it = cstNextSibling(it)) {
            if (cstRuleName(it) != yrn_item) continue;
            vec = api.rrb_with(vec, yamlPairValueOfItem(it));
        }
        return vec;
    }
    return NIL;
}
fn yamlPairValueOfItem(item: u64) Ref {
    const v = cstChildByRule(item, yrn_value);
    return if (v != 0) yamlLowerValue(v) else NIL;
}

// -- Phase 17.11: CST navigation — author parsers in birnam over PEG --------
//
// `[grammar parse:from:]` returns a CST whose nodes are tagged CST_NODE_VT.
// These accessors let a birnam program walk that tree and lower it: combine
// `ruleName` (a symbol) to dispatch, `children` to recurse, and `start`/`end`
// (byte offsets) with the source's `copyFrom:to:` to read a node's text.

/// `[node ruleName]` — the matched rule's name as a STRING (so it compares
/// with `=`; bare symbols lack a value protocol). NIL for an unnamed node.
fn cstRuleNameImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    const rn = g_api.?.query_eavt(untagEid(s.receiver), ATTR_AST_RULE_NAME);
    if (!isEid(rn)) return NIL;
    return g_api.?.symbol_name(untagEid(rn));
}
fn cstStartImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return g_api.?.query_eavt(untagEid(s.receiver), ATTR_AST_BYTE_START);
}
fn cstEndImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return g_api.?.query_eavt(untagEid(s.receiver), ATTR_AST_BYTE_END);
}
fn cstFirstChildImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return g_api.?.query_eavt(untagEid(s.receiver), ATTR_AST_FIRST_CHILD);
}
fn cstNextSiblingImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return g_api.?.query_eavt(untagEid(s.receiver), ATTR_AST_NEXT_SIBLING);
}
/// `[node children]` — vector of child nodes (left to right).
fn cstChildrenImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    const api = g_api.?;
    var vec = api.make_rrb(@ptrCast(&[_]Ref{}), 0);
    var c = cstFirstChild(untagEid(s.receiver));
    while (c != 0) : (c = cstNextSibling(c)) {
        vec = api.rrb_with(vec, mkEid(c));
    }
    return vec;
}

/// `[peg seqOf: aVector]` — sequence pattern from a vector of sub-patterns.
/// Birnam-friendly counterpart to the variadic `seq:` (keyword sends are
/// fixed-arity, so a single vector arg is how birnam passes a list).
fn pegSeqOfImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    if (!isVectorRef(a[0])) return mkRuntimeError("peg seqOf: expects a vector");
    const len = api.rrb_len(a[0]);
    var buf: [256]u64 = undefined;
    if (len > buf.len) return mkRuntimeError("peg seqOf: too many sub-patterns");
    var i: u64 = 0;
    while (i < len) : (i += 1) buf[@intCast(i)] = untagEid(api.rrb_nth(a[0], i));
    return mkEid(api.peg_mk_seq(&buf, @intCast(len)));
}
/// `[peg choiceOf: aVector]` — ordered-choice pattern from a vector.
fn pegChoiceOfImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    if (!isVectorRef(a[0])) return mkRuntimeError("peg choiceOf: expects a vector");
    const len = api.rrb_len(a[0]);
    var buf: [256]u64 = undefined;
    if (len > buf.len) return mkRuntimeError("peg choiceOf: too many alternatives");
    var i: u64 = 0;
    while (i < len) : (i += 1) buf[@intCast(i)] = untagEid(api.rrb_nth(a[0], i));
    return mkEid(api.peg_mk_choice(&buf, @intCast(len)));
}
/// `[peg anyOf: aString]` — charset matching any single byte in the string.
fn pegAnyOfImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    if (!isString(a[0])) return mkRuntimeError("peg anyOf: expects a string");
    const b = stringBytes(a[0]);
    var buf: [512]u8 = undefined;
    if (b.len * 2 > buf.len) return mkRuntimeError("peg anyOf: too many chars");
    for (b, 0..) |c, i| {
        buf[2 * i] = c;
        buf[2 * i + 1] = c;
    }
    return mkEid(api.peg_mk_charset(&buf, b.len * 2));
}
/// `[peg range: loString to: hiString]` — charset for the inclusive byte range
/// of the first bytes of `lo`/`hi` (e.g. `[peg range: "0" to: "9"]`).
fn pegRangeToImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    if (!isString(a[0]) or !isString(a[1])) return mkRuntimeError("peg range:to: expects strings");
    const lo = stringBytes(a[0]);
    const hi = stringBytes(a[1]);
    if (lo.len == 0 or hi.len == 0) return mkRuntimeError("peg range:to: empty bound");
    const buf = [_]u8{ lo[0], hi[0] };
    return mkEid(api.peg_mk_charset(&buf, 2));
}

// -- Phase 17.12: symbol value protocol -------------------------------------

/// `[#a = #b]` — identity (interned symbols share identity).
fn symbolEqImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    return mkEid(if (s.receiver == a[0]) TRUE_EID else FALSE_EID);
}
/// `[#foo printString]` — the symbol with a leading `#` (e.g. "#foo").
fn symbolPrintStringImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    const api = g_api.?;
    const nm = api.symbol_name(untagEid(s.receiver));
    if (!isString(nm)) return api.make_string("#".ptr, 1);
    const b = stringBytes(nm);
    var buf: [1024]u8 = undefined;
    if (b.len + 1 > buf.len) return nm;
    buf[0] = '#';
    @memcpy(buf[1 .. 1 + b.len], b);
    return api.make_string(buf[0 .. 1 + b.len].ptr, 1 + b.len);
}
/// `[#foo asString]` — the bare name ("foo"), no `#`.
fn symbolAsStringImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    const nm = g_api.?.symbol_name(untagEid(s.receiver));
    return if (isString(nm)) nm else g_api.?.make_string("".ptr, 0);
}
/// `[#foo asSymbol]` — itself.
fn symbolAsSymbolImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return s.receiver;
}

// -- Phase 17.12: prototype creation from birnam ----------------------------

/// `[obj vtable]` — the receiver's vtable (so birnam can install methods on it).
fn objVtableImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return g_api.?.query_eavt(untagEid(s.receiver), ATTR_VTABLE);
}
/// `[Object new]` — a fresh prototype: an entity with its OWN vtable (delegating
/// OBJECT_VT). Install methods via `[[it vtable] method: #sel do: block]`. This
/// is how birnam authors a prototype (e.g. a parser's `Json`/`Yaml`).
fn objectNewImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    const api = g_api.?;
    // The instance and its per-object vtable are runtime DATA, not lang-module
    // CODE — tag their structural facts M_USER (like the method facts that
    // `method:do:` installs) so they persist in an image. Under g_module_eid
    // these rode the lang module's eid (CODE), so the image dropped them and an
    // image-loaded object had no vtable/delegate chain → dispatch returned nil.
    const vt = api.alloc_eid();
    _ = api.assert_fact(vt, ATTR_VTABLE, mkEid(VTABLE_VT), M_USER);
    _ = api.assert_fact(vt, ATTR_DELEGATE, mkEid(OBJECT_VT), M_USER);
    const obj = api.alloc_eid();
    _ = api.assert_fact(obj, ATTR_VTABLE, mkEid(vt), M_USER);
    return mkEid(obj);
}

// -- Phase 17.12: type predicates -------------------------------------------
// One impl per predicate, checking the receiver's actual tag/vtable, installed
// on every value vtable so any value answers (e.g. `[3 isString]` → false).

fn tpBool(b: bool) Ref {
    return mkEid(if (b) TRUE_EID else FALSE_EID);
}
fn isStringQImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return tpBool(isString(s.receiver));
}
fn isNumberQImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return tpBool(isSmallInt(s.receiver) or isFloatRef(s.receiver));
}
fn isVectorQImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return tpBool(isVectorRef(s.receiver));
}
fn isMapQImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return tpBool(isMapRef(s.receiver));
}

fn installTypePredicates() void {
    const api = g_api.?;
    const Pair = struct { name: []const u8, impl: *const anyopaque };
    const preds = [_]Pair{
        .{ .name = "isString", .impl = @ptrCast(&isStringQImpl) },
        .{ .name = "isNumber", .impl = @ptrCast(&isNumberQImpl) },
        .{ .name = "isVector", .impl = @ptrCast(&isVectorQImpl) },
        .{ .name = "isMap", .impl = @ptrCast(&isMapQImpl) },
    };
    const vts = [_]u64{ OBJECT_VT, SMALLINT_VT, FLOAT_VT, STRING_VT, RRB_VT, ASSOC_VT, NIL_VT_EID, TRUE_VT, FALSE_VT, SYMBOL_VT };
    for (vts) |vt| {
        for (preds) |p| {
            _ = api.assert_fact(vt, api.intern(p.name.ptr, p.name.len), mkEid(api.make_closure(p.impl, NIL)), g_module_eid);
        }
    }
}

fn installObjectSurface() void {
    const api = g_api.?;
    // A general `vtable` accessor on every object.
    _ = api.assert_fact(OBJECT_VT, api.intern("vtable".ptr, 6), mkEid(api.make_closure(@ptrCast(&objVtableImpl), NIL)), g_module_eid);
    // `Object` global with `new` — birnam-level prototype creation.
    const obj_vt = api.alloc_eid();
    _ = api.assert_fact(obj_vt, ATTR_VTABLE, mkEid(VTABLE_VT), g_module_eid);
    _ = api.assert_fact(obj_vt, ATTR_DELEGATE, mkEid(OBJECT_VT), g_module_eid);
    _ = api.assert_fact(obj_vt, api.intern("new".ptr, 3), mkEid(api.make_closure(@ptrCast(&objectNewImpl), NIL)), g_module_eid);
    const obj_proto = api.alloc_eid();
    _ = api.assert_fact(obj_proto, ATTR_VTABLE, mkEid(obj_vt), g_module_eid);
    pushGlobalBinding(GLOBALS_SCOPE_EID, api.intern("Object".ptr, 6), mkEid(obj_proto));
}

fn installSymbolSurface() void {
    const api = g_api.?;
    _ = api.assert_fact(SYMBOL_VT, ATTR_DELEGATE, mkEid(OBJECT_VT), g_module_eid);
    _ = api.assert_fact(SYMBOL_VT, api.intern("=".ptr, 1), mkEid(api.make_closure(@ptrCast(&symbolEqImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SYMBOL_VT, api.intern("printString".ptr, 11), mkEid(api.make_closure(@ptrCast(&symbolPrintStringImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SYMBOL_VT, api.intern("displayString".ptr, 13), mkEid(api.make_closure(@ptrCast(&symbolAsStringImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SYMBOL_VT, api.intern("asString".ptr, 8), mkEid(api.make_closure(@ptrCast(&symbolAsStringImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SYMBOL_VT, api.intern("asSymbol".ptr, 8), mkEid(api.make_closure(@ptrCast(&symbolAsSymbolImpl), NIL)), g_module_eid);
}

fn installCstSurface() void {
    const api = g_api.?;
    // Expose the PEG combinator singleton so birnam can build grammars:
    // `[peg newGrammar]`, `[peg literal: "x"]`, `[peg seqOf: #[…]]`, etc., then
    // `[g defineRule: name as: pat]` and `[g parse: src from: rule]`.
    pushGlobalBinding(GLOBALS_SCOPE_EID, api.intern("peg".ptr, 3), mkEid(PEG_EID));
    // Birnam-friendly (vector/string) combinators alongside the variadic ones.
    _ = api.assert_fact(PEG_EID, ATTR_VTABLE, mkEid(PEG_VT), g_module_eid);
    _ = api.assert_fact(PEG_VT, api.intern("seqOf:".ptr, 6), mkEid(api.make_closure(@ptrCast(&pegSeqOfImpl), NIL)), g_module_eid);
    _ = api.assert_fact(PEG_VT, api.intern("choiceOf:".ptr, 9), mkEid(api.make_closure(@ptrCast(&pegChoiceOfImpl), NIL)), g_module_eid);
    _ = api.assert_fact(PEG_VT, api.intern("anyOf:".ptr, 6), mkEid(api.make_closure(@ptrCast(&pegAnyOfImpl), NIL)), g_module_eid);
    _ = api.assert_fact(PEG_VT, api.intern("range:to:".ptr, 9), mkEid(api.make_closure(@ptrCast(&pegRangeToImpl), NIL)), g_module_eid);
    _ = api.assert_fact(CST_NODE_VT, ATTR_DELEGATE, mkEid(OBJECT_VT), g_module_eid);
    const Pair = struct { name: []const u8, impl: *const anyopaque };
    const methods = [_]Pair{
        .{ .name = "ruleName", .impl = @ptrCast(&cstRuleNameImpl) },
        .{ .name = "start", .impl = @ptrCast(&cstStartImpl) },
        .{ .name = "end", .impl = @ptrCast(&cstEndImpl) },
        .{ .name = "firstChild", .impl = @ptrCast(&cstFirstChildImpl) },
        .{ .name = "nextSibling", .impl = @ptrCast(&cstNextSiblingImpl) },
        .{ .name = "children", .impl = @ptrCast(&cstChildrenImpl) },
    };
    for (methods) |m| {
        _ = api.assert_fact(CST_NODE_VT, api.intern(m.name.ptr, m.name.len), mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }
}

fn installNumberProtocol() void {
    const api = g_api.?;
    _ = api.assert_fact(SMALLINT_VT, api.intern("mod:".ptr, 4), mkEid(api.make_closure(@ptrCast(&smallintModImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FLOAT_VT, api.intern("sqrt".ptr, 4), mkEid(api.make_closure(@ptrCast(&floatSqrtImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FLOAT_VT, api.intern("floor".ptr, 5), mkEid(api.make_closure(@ptrCast(&floatFloorImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FLOAT_VT, api.intern("ceil".ptr, 4), mkEid(api.make_closure(@ptrCast(&floatCeilImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FLOAT_VT, api.intern("round".ptr, 5), mkEid(api.make_closure(@ptrCast(&floatRoundImpl), NIL)), g_module_eid);
}

/// Phase 16 Tier D — default doesNotUnderstand:. `a[0]` = the missing selector
/// as a symbol. Returns an ERROR_VT diagnostic naming the selector.
fn dnuImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, n };
    const api = g_api.?;
    const sel_ref = a[0];
    const nm = if (isEid(sel_ref)) api.symbol_name(sel_ref >> 3) else NIL;
    var buf: [160]u8 = undefined;
    const msg = if (nm != NIL and isString(nm))
        (std.fmt.bufPrint(&buf, "does not understand: #{s}", .{stringBytes(nm)}) catch "does not understand")
    else
        "does not understand (unknown selector)";
    return mkRuntimeError(msg);
}

// -- Result error model: ERROR_VT propagation + inspectors -----------------

/// ERROR_VT's `doesNotUnderstand:` — return the error itself (the receiver) for
/// any unhandled selector, so an error propagates through a message chain
/// (`[[x foo] bar]` yields the original error) instead of becoming a new
/// "does not understand" error. This is receiver-side Result propagation done
/// purely through message dispatch — no send-loop special case.
fn errorDnuImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    return s.receiver;
}

/// `[err message]` → the error's message string (or nil).
fn errorMessageImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    if (!isEid(s.receiver)) return NIL;
    return g_api.?.query_eavt(s.receiver >> 3, ATTR_ERROR_MESSAGE);
}

/// `[err kind]` → the error's kind symbol (or nil).
fn errorKindImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    if (!isEid(s.receiver)) return NIL;
    return g_api.?.query_eavt(s.receiver >> 3, ATTR_ERROR_KIND);
}

/// `[x isError]` → true on ERROR_VT, false everywhere else.
fn isErrorTrueImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    return mkEid(TRUE_EID);
}
fn isErrorFalseImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, a, n };
    return mkEid(FALSE_EID);
}

/// Install the Result error surface. Always-kept tier (works in pure-VM AOT):
/// errors are produced by kernel primitives, so propagation must survive
/// --strip=malleable.
fn installErrors() void {
    const api = g_api.?;
    // Propagation: any unhandled message to an error returns the error.
    _ = api.assert_fact(ERROR_VT, SEL_DNU, mkEid(api.make_closure(@ptrCast(&errorDnuImpl), NIL)), g_module_eid);
    // Inspectors on ERROR_VT.
    const sel_message = api.intern("message".ptr, 7);
    const sel_kind = api.intern("kind".ptr, 4);
    const sel_is_error = api.intern("isError".ptr, 7);
    _ = api.assert_fact(ERROR_VT, sel_message, mkEid(api.make_closure(@ptrCast(&errorMessageImpl), NIL)), g_module_eid);
    _ = api.assert_fact(ERROR_VT, sel_kind, mkEid(api.make_closure(@ptrCast(&errorKindImpl), NIL)), g_module_eid);
    _ = api.assert_fact(ERROR_VT, sel_is_error, mkEid(api.make_closure(@ptrCast(&isErrorTrueImpl), NIL)), g_module_eid);
    // isError = false for everything else (delegating types via OBJECT_VT +
    // non-delegating immediates).
    for ([_]u64{ OBJECT_VT, SMALLINT_VT, TRUE_VT, FALSE_VT }) |vt| {
        _ = api.assert_fact(vt, sel_is_error, mkEid(api.make_closure(@ptrCast(&isErrorFalseImpl), NIL)), g_module_eid);
    }
    // Catch surface: `on:do:` + `ensure:` on blocks (CLOSURE_VT). `#Error` wildcard.
    g_sel_error_any = api.intern("Error".ptr, 5);
    const sel_on_do = api.intern("on:do:".ptr, 6);
    _ = api.assert_fact(CLOSURE_VT, sel_on_do, mkEid(api.make_closure(@ptrCast(&closureOnDoImpl), NIL)), g_module_eid);
    _ = api.assert_fact(CLOSURE_VT, api.intern("ensure:".ptr, 7), mkEid(api.make_closure(@ptrCast(&closureEnsureImpl), NIL)), g_module_eid);
    // Raise surface: `[msg error]` / `[msg error: #kind]` on strings.
    _ = api.assert_fact(STRING_VT, api.intern("error".ptr, 5), mkEid(api.make_closure(@ptrCast(&stringErrorImpl), NIL)), g_module_eid);
    _ = api.assert_fact(STRING_VT, api.intern("error:".ptr, 6), mkEid(api.make_closure(@ptrCast(&stringErrorKindImpl), NIL)), g_module_eid);
}

/// `#Error` (or `#any`) catch-all kind for `on:do:`. Cached at install.
var g_sel_error_any: u64 = 0;

/// True if a handler watching `want` (a kind symbol Ref) should catch an error
/// whose kind is `got` (a kind symbol Ref, possibly NIL for unkinded errors).
/// `#Error`/`#any` is the wildcard that catches every error.
fn errorKindMatches(want: Ref, got: Ref) bool {
    if (isEid(want) and ((want >> 3) == g_sel_error_any)) return true;
    if (got == NIL) return false; // unkinded error: only the wildcard catches it
    return want == got;
}

/// `[protectedBlock ensure: cleanupBlock]` — run the protected block, then ALWAYS
/// run the cleanup block (whether the protected block yielded a value or an
/// error), and return the protected block's result. The Result-model answer to
/// resource safety; pairs with `withFile:`/`withAlloc:` brackets.
fn closureEnsureImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    const result = api.send0(s.receiver, sel_value_g);
    _ = api.send0(args[0], sel_value_g); // cleanup runs regardless; its value is discarded
    return result;
}

/// `[msg error]` — raise: build an ERROR_VT carrying the receiver string. The
/// user-facing companion to primitive-raised errors; propagates + is catchable.
fn stringErrorImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ a, n };
    if (!isString(s.receiver)) return mkRuntimeError("error");
    return mkRuntimeError(stringBytes(s.receiver));
}

/// `[msg error: #kind]` — raise with an explicit kind symbol (for `on:do:`).
fn stringErrorKindImpl(s: *Send, a: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const msg = if (isString(s.receiver)) stringBytes(s.receiver) else "error";
    const kind: u64 = if (isEid(a[0])) a[0] >> 3 else 0;
    return mkRuntimeErrorKind(kind, msg);
}

/// `[protectedBlock on: #kind do: handlerBlock]` — run the protected block; if
/// it yields an error matching `#kind` (or `#Error` wildcard), invoke the
/// handler with the error (`handler value: error`) and return its result. A
/// non-error result, or an error of a different kind, passes through unchanged
/// (an unmatched error keeps propagating — Result model).
fn closureOnDoImpl(s: *Send, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = n;
    const api = g_api.?;
    const result = api.send0(s.receiver, sel_value_g);
    if (!isErrorValue(result)) return result;
    const got = api.query_eavt(result >> 3, ATTR_ERROR_KIND);
    if (errorKindMatches(args[0], got)) {
        return api.send1(args[1], sel_value_of_g, result);
    }
    return result;
}

fn installBoolNil() void {
    const api = g_api.?;
    // Tier D — default doesNotUnderstand: on OBJECT_VT (delegating types) + the
    // non-delegating immediates. Unknown selectors → graceful ERROR_VT.
    for ([_]u64{ OBJECT_VT, SMALLINT_VT, TRUE_VT, FALSE_VT }) |vt| {
        _ = api.assert_fact(vt, SEL_DNU, mkEid(api.make_closure(@ptrCast(&dnuImpl), NIL)), g_module_eid);
    }
    const isnil = api.intern("isNil".ptr, 5);
    const ifnil = api.intern("ifNil:".ptr, 6);
    const ifnotnil = api.intern("ifNotNil:".ptr, 9);
    // Default (not-nil): OBJECT_VT (covers delegating types) + the
    // non-delegating immediates SMALLINT_VT / TRUE_VT / FALSE_VT.
    const reduced_sel = api.intern("reduced".ptr, 7);
    const default_vts = [_]u64{ OBJECT_VT, SMALLINT_VT, TRUE_VT, FALSE_VT };
    for (default_vts) |vt| {
        _ = api.assert_fact(vt, isnil, mkEid(api.make_closure(@ptrCast(&objIsNilImpl), NIL)), g_module_eid);
        _ = api.assert_fact(vt, ifnil, mkEid(api.make_closure(@ptrCast(&objIfNilImpl), NIL)), g_module_eid);
        _ = api.assert_fact(vt, ifnotnil, mkEid(api.make_closure(@ptrCast(&objIfNotNilImpl), NIL)), g_module_eid);
        _ = api.assert_fact(vt, reduced_sel, mkEid(api.make_closure(@ptrCast(&objReducedImpl), NIL)), g_module_eid);
    }
    // nil overrides on NIL_VT.
    _ = api.assert_fact(NIL_VT_EID, isnil, mkEid(api.make_closure(@ptrCast(&nilIsNilImpl), NIL)), g_module_eid);
    _ = api.assert_fact(NIL_VT_EID, ifnil, mkEid(api.make_closure(@ptrCast(&nilIfNilImpl), NIL)), g_module_eid);
    _ = api.assert_fact(NIL_VT_EID, ifnotnil, mkEid(api.make_closure(@ptrCast(&nilIfNotNilImpl), NIL)), g_module_eid);
    // Boolean: not / and: / or: (and:/or: take a block — short-circuit).
    const not_sel = api.intern("not".ptr, 3);
    const and_sel = api.intern("and:".ptr, 4);
    const or_sel = api.intern("or:".ptr, 3);
    _ = api.assert_fact(TRUE_VT, not_sel, mkEid(api.make_closure(@ptrCast(&trueNotImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FALSE_VT, not_sel, mkEid(api.make_closure(@ptrCast(&falseNotImpl), NIL)), g_module_eid);
    _ = api.assert_fact(TRUE_VT, and_sel, mkEid(api.make_closure(@ptrCast(&trueAndImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FALSE_VT, and_sel, mkEid(api.make_closure(@ptrCast(&falseAndImpl), NIL)), g_module_eid);
    _ = api.assert_fact(TRUE_VT, or_sel, mkEid(api.make_closure(@ptrCast(&trueOrImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FALSE_VT, or_sel, mkEid(api.make_closure(@ptrCast(&falseOrImpl), NIL)), g_module_eid);
    // Eager `&` / `|` (Phase 17.5). `|` now tokenizes as a binary-op char; the
    // block-param bar (`[:x | …]` / `[| …]`) and `|>` pipe are matched by their
    // own rules ahead of bracket-binary, so `[a | b]` only means eager-or.
    const amp_sel = api.intern("&".ptr, 1);
    const pipe_sel = api.intern("|".ptr, 1);
    _ = api.assert_fact(TRUE_VT, amp_sel, mkEid(api.make_closure(@ptrCast(&trueAmpImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FALSE_VT, amp_sel, mkEid(api.make_closure(@ptrCast(&falseAmpImpl), NIL)), g_module_eid);
    _ = api.assert_fact(TRUE_VT, pipe_sel, mkEid(api.make_closure(@ptrCast(&truePipeImpl), NIL)), g_module_eid);
    _ = api.assert_fact(FALSE_VT, pipe_sel, mkEid(api.make_closure(@ptrCast(&falsePipeImpl), NIL)), g_module_eid);
}

fn installCollections() void {
    const api = g_api.?;
    const M = struct { name: []const u8, impl: *const anyopaque };
    const vec_methods = [_]M{
        .{ .name = "with:", .impl = @ptrCast(&vecWithImpl) },
        .{ .name = "size", .impl = @ptrCast(&vecSizeImpl) },
        .{ .name = "at:", .impl = @ptrCast(&vecAtImpl) },
        .{ .name = "at:ifAbsent:", .impl = @ptrCast(&vecAtIfAbsentImpl) },
        .{ .name = "do:", .impl = @ptrCast(&vecDoImpl) },
        .{ .name = "inject:into:", .impl = @ptrCast(&vecInjectIntoImpl) },
    };
    for (vec_methods) |m| {
        const sel = api.intern(m.name.ptr, m.name.len);
        _ = api.assert_fact(RRB_VT, sel, mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }
    const map_methods = [_]M{
        .{ .name = "at:put:", .impl = @ptrCast(&mapAtPutImpl) },
        .{ .name = "at:", .impl = @ptrCast(&mapAtImpl) },
        .{ .name = "at:ifAbsent:", .impl = @ptrCast(&mapAtIfAbsentImpl) },
        .{ .name = "includesKey:", .impl = @ptrCast(&mapIncludesKeyImpl) },
        .{ .name = "size", .impl = @ptrCast(&mapSizeImpl) },
        .{ .name = "keys", .impl = @ptrCast(&mapKeysImpl) },
        .{ .name = "values", .impl = @ptrCast(&mapValuesImpl) },
        .{ .name = "merge:", .impl = @ptrCast(&mapMergeImpl) },
        .{ .name = "do:", .impl = @ptrCast(&mapDoImpl) },
    };
    for (map_methods) |m| {
        const sel = api.intern(m.name.ptr, m.name.len);
        _ = api.assert_fact(ASSOC_VT, sel, mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }
    // Maps fold over values → inherit the generic seq protocol (OBJECT_VT).
    _ = api.assert_fact(ASSOC_VT, api.intern("inject:into:".ptr, 12), mkEid(api.make_closure(@ptrCast(&mapInjectIntoImpl), NIL)), g_module_eid);

    // Ranges: to:/to:by: constructors on SmallInt; the seq protocol on RANGE_VT.
    _ = api.assert_fact(SMALLINT_VT, api.intern("to:".ptr, 3), mkEid(api.make_closure(@ptrCast(&smallintToImpl), NIL)), g_module_eid);
    _ = api.assert_fact(SMALLINT_VT, api.intern("to:by:".ptr, 6), mkEid(api.make_closure(@ptrCast(&smallintToByImpl), NIL)), g_module_eid);
    const range_methods = [_]M{
        .{ .name = "size", .impl = @ptrCast(&rangeSizeImpl) },
        .{ .name = "at:", .impl = @ptrCast(&rangeAtImpl) },
        .{ .name = "do:", .impl = @ptrCast(&rangeDoImpl) },
        .{ .name = "inject:into:", .impl = @ptrCast(&rangeInjectIntoImpl) },
    };
    for (range_methods) |m| {
        const sel = api.intern(m.name.ptr, m.name.len);
        _ = api.assert_fact(RANGE_VT, sel, mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }
    // Strings: byte-seq protocol + concat/eq/asSymbol. do:/inject:into: give
    // strings the generic OBJECT_VT seq protocol (collect: → vector of bytes).
    const str_methods = [_]M{
        .{ .name = "size", .impl = @ptrCast(&strSizeImpl) },
        .{ .name = "at:", .impl = @ptrCast(&strAtImpl) },
        .{ .name = ",", .impl = @ptrCast(&strConcatImpl) },
        .{ .name = "=", .impl = @ptrCast(&strEqImpl) },
        .{ .name = "asSymbol", .impl = @ptrCast(&strAsSymbolImpl) },
        .{ .name = "do:", .impl = @ptrCast(&strDoImpl) },
        .{ .name = "inject:into:", .impl = @ptrCast(&strInjectIntoImpl) },
    };
    for (str_methods) |m| {
        const sel = api.intern(m.name.ptr, m.name.len);
        _ = api.assert_fact(STRING_VT, sel, mkEid(api.make_closure(m.impl, NIL)), g_module_eid);
    }
}

/// Phase 16 Tier B — install a method whose BODY is birnam source, compiled to
/// bytecode. The receiver-vt + selector are given by Zig (no `vtable` reflection
/// needed in source); the body self-hosts over the Tier A primitives. `src` is
/// a block literal `[:params | body]` (or `[| body]` for unary). Best-effort:
/// a parse/compile failure logs and skips (the prelude is vetted source).
fn installSourceMethod(vt: u64, sel_name: []const u8, src: []const u8) void {
    // W1 Stage 3b.2b — booting from a full image: the prelude/stdlib bytecode +
    // bindings are in the image (replayed after this module loads), so skip the
    // (expensive) parse+compile. Native primitives still install above; only the
    // birnam-source methods are deferred to the replay.
    if (g_ctx) |ctx| {
        if (ctx.boot_from_image) return;
    }
    const api = g_api.?;
    const src_ref = api.make_string(src.ptr, src.len);
    const ast = api.send1(mkEid(g_module_eid), sel_parse_expr_g, src_ref);
    if (!isEid(ast)) {
        if (g_log) |log| log("[lang] prelude parse failed");
        return;
    }
    // 15.A-AST: prelude methods are CODE (reconstructed each boot, like the
    // install fact below) — compile the method ENTITY under the module eid so
    // its provenance matches its install. Otherwise the method is M_USER while
    // its install is CODE: imaged-but-reconstructed (bloat) and unrootable by
    // prune via its install.
    const method_eid = api.compile_block_as_method_in(ast >> 3, g_module_eid);
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
    // Seq unification: the derived protocol lives on OBJECT_VT, written once
    // over `self inject:into:`. ANY type providing inject:into: (vectors,
    // ranges, maps-over-values, future strings) inherits collect:/select:/
    // count/detect:/asVector via delegation. Results materialize as vectors.
    installSourceMethod(OBJECT_VT, "collect:", "[:f | [self inject: #[] into: [:acc :x | [acc with: [f value: x]]]]]");
    installSourceMethod(OBJECT_VT, "select:", "[:p | [self inject: #[] into: [:acc :x | [[p value: x] ifTrue: [| [acc with: x]] ifFalse: [| acc]]]]]");
    installSourceMethod(OBJECT_VT, "count", "[| [self inject: 0 into: [:n :x | [n + 1]]]]");
    installSourceMethod(OBJECT_VT, "detect:", "[:pred | [self inject: nil into: [:acc :x | [[pred value: x] ifTrue: [| [x reduced]] ifFalse: [| acc]]]]]");
    installSourceMethod(OBJECT_VT, "asVector", "[| [self inject: #[] into: [:acc :x | [acc with: x]]]]");
    // Vector-specific concat (result is a vector; needs `with:` on the accumulator).
    installSourceMethod(RRB_VT, ",", "[:other | [other inject: self into: [:acc :x | [acc with: x]]]]");
    // Tier D — numeric comparison helpers, self-hosted over `<` + bool/nil.
    installSourceMethod(SMALLINT_VT, "min:", "[:other | [[self < other] ifTrue: [| self] ifFalse: [| other]]]");
    installSourceMethod(SMALLINT_VT, "max:", "[:other | [[self < other] ifTrue: [| other] ifFalse: [| self]]]");
    installSourceMethod(SMALLINT_VT, "between:and:", "[:lo :hi | [[[self < lo] not] and: [| [[hi < self] not]]]]");
    // Tier D — abs/even/odd, self-hosted over `<`/negate/mod:/`=`.
    installSourceMethod(SMALLINT_VT, "abs", "[| [[self < 0] ifTrue: [| [self negate]] ifFalse: [| self]]]");
    installSourceMethod(SMALLINT_VT, "even", "[| [[self mod: 2] = 0]]");
    installSourceMethod(SMALLINT_VT, "odd", "[| [[self mod: 2] = 1]]");

    // -- Tier E: applicative + transducers (self-hosted) -----------------
    // validate: — error-ACCUMULATING (vs short-circuit). `check` answers nil
    // for OK, an error value for failure; result = vector of ALL failures
    // (empty = valid). Pairs with schema-as-facts (9.5.2).
    installSourceMethod(OBJECT_VT, "validate:", "[:check | [self inject: #[] into: [:acc :x | let r := [check value: x] ; [[r isNil] ifTrue: [| acc] ifFalse: [| [acc with: r]]]]]]");
    // Transducers — a transducer is an rf→rf' block. The factory `Xf`
    // constructs them; `then:` composes (data flows self-then-other);
    // `transduce:` runs one eagerly with `with:` as the base reducer (single
    // fold, no intermediate vectors). `take:` uses a cell counter + `reduced`
    // early-out. comp deferred per ROADMAP DNA — `then:` is the 2-ary compose.
    installSourceMethod(XF_VT, "map:", "[:f | [:rf | [:acc :x | [rf value: acc value: [f value: x]]]]]");
    installSourceMethod(XF_VT, "filter:", "[:p | [:rf | [:acc :x | [[p value: x] ifTrue: [| [rf value: acc value: x]] ifFalse: [| acc]]]]]");
    installSourceMethod(XF_VT, "take:", "[:nn | [:rf | [:acc :x | [[[acc size] < nn] ifTrue: [| [rf value: acc value: x]] ifFalse: [| [acc reduced]]]]]]");
    installSourceMethod(CLOSURE_VT, "then:", "[:other | [:rf | [self value: [other value: rf]]]]");
    installSourceMethod(OBJECT_VT, "transduce:", "[:xf | [self inject: #[] into: [xf value: [:acc :x | [acc with: x]]]]]");

    // -- Phase 17 string batch: composed over byte primitives + natives ----
    // isEmpty is generic (any sized seq).
    installSourceMethod(OBJECT_VT, "isEmpty", "[| [[self size] = 0]]");
    installSourceMethod(STRING_VT, "startsWith:", "[:p | [[[p size] > [self size]] ifTrue: [| false] ifFalse: [| [[self copyFrom: 0 to: [p size]] = p]]]]");
    installSourceMethod(STRING_VT, "endsWith:", "[:p | [[[p size] > [self size]] ifTrue: [| false] ifFalse: [| [[self copyFrom: [[self size] - [p size]] to: [self size]] = p]]]]");
    installSourceMethod(STRING_VT, "includes:", "[:sub | [[[self indexOf: sub ifAbsent: [| [1 negate]]] < 0] not]]");
    // String ordering over `<` + `=` (SmallInt-style triad).
    installSourceMethod(STRING_VT, "<=", "[:o | [[self < o] or: [| [self = o]]]]");
    installSourceMethod(STRING_VT, ">", "[:o | [o < self]]");
    installSourceMethod(STRING_VT, ">=", "[:o | [[o < self] or: [| [self = o]]]]");
    // replace:with: = split on old, join with new. lines/words derive from split:.
    installSourceMethod(STRING_VT, "replace:with:", "[:old :new | [[self split: old] join: new]]");
    installSourceMethod(STRING_VT, "lines", "[| [self split: \"\\n\"]]");
    installSourceMethod(STRING_VT, "words", "[| [[self split: \" \"] select: [:w | [[[w size] = 0] not]]]]");
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
    // 3-/4-arg block invocation — closureValueImpl is arity-agnostic. Lets FFI
    // callbacks with 3 or 4 C parameters dispatch a birnam block (Phase 19.2
    // arity>2 finish); also usable directly as `[blk value: a value: b value: c]`.
    _ = api.assert_fact(CLOSURE_VT, api.intern("value:value:value:".ptr, 18), mkEid(api.make_closure(@ptrCast(&closureValueImpl), NIL)), g_module_eid);
    _ = api.assert_fact(CLOSURE_VT, api.intern("value:value:value:value:".ptr, 24), mkEid(api.make_closure(@ptrCast(&closureValueImpl), NIL)), g_module_eid);

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

    installCollections(); // Phase 16 Tier A/C — vector + map methods
    installErrors(); // Result error model — ERROR_VT propagation + inspectors
    installBoolNil(); // Phase 16 Tier D — bool/nil ergonomics
    installNumberProtocol(); // Phase 16 Tier D — mod: + Float sqrt/floor/ceil/round
}

/// Full method install (dylib / non-AOT path) — every tier.
fn installMethods() void {
    installEvalMethods();
    installPrimitives();
    installMethodDo();
    installParseSurface();
}

const LANG_VT = ke.LANG_VT;

// -- Phase 10.5 REPL handler -----------------------------------------------

const ERROR_VT = ke.ERROR_VT;
const NIL_VT_EID = ke.NIL_VT;
const FLOAT_VT = ke.FLOAT_VT;

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

/// `[language evalLine: aString]` — NUL-terminate the line and run it through
/// the same `replHandler` the shell drives, so a birnam-authored REPL gets
/// identical parse/eval/print/error behaviour. Returns nil.
fn evalLineImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = n_args;
    const arg = args[0];
    if (!isString(arg)) return NIL;
    const bytes = stringBytes(arg);
    var buf: [4096]u8 = undefined;
    if (bytes.len >= buf.len) return NIL; // over-long line: ignore (REPL safety)
    @memcpy(buf[0..bytes.len], bytes);
    buf[bytes.len] = 0;
    replHandler(@ptrCast(&buf));
    return NIL;
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
        // Cluster ① — `:eval-mode [bytecode|treewalk]` toggles the top-level /
        // method eval strategy for this session. Bare `:eval-mode` prints it.
        if (std.mem.startsWith(u8, line, ":eval-mode")) {
            const arg = std.mem.trim(u8, line[":eval-mode".len..], " \t");
            if (arg.len == 0) {
                if (g_log) |log| log(if (g_eval_mode == .bytecode) "[lang] eval-mode: bytecode" else "[lang] eval-mode: treewalk");
            } else if (std.mem.eql(u8, arg, "bytecode")) {
                g_eval_mode = .bytecode;
                if (g_log) |log| log("[lang] eval-mode → bytecode");
            } else if (std.mem.eql(u8, arg, "treewalk")) {
                g_eval_mode = .treewalk;
                if (g_log) |log| log("[lang] eval-mode → treewalk");
            } else {
                if (g_log) |log| log("[lang] usage: :eval-mode [bytecode|treewalk]");
            }
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
    // Parse error → ERROR_VT diagnostic; report its message.
    const vt = api.query_eavt(ast_eid, ATTR_VTABLE);
    if (vt == mkEid(ERROR_VT)) {
        reportError(ast_ref, "parse error");
        return;
    }
    const val = evalTopLevelStmt(ast_ref);
    // Line-commit: bind any FFI modules/wraps declared on this line (deferred
    // dlopen → resolve here for interactive eager feel). No-op without FFI.
    resolvePendingModules();
    // Result model: an uncaught error surfaces as a distinct error line, not as
    // an ordinary `=> value`.
    if (isErrorValue(val)) {
        reportError(val, "error");
        return;
    }
    if (g_log) |log| {
        var buf: [256]u8 = undefined;
        const m = formatValue(&buf, val) catch return;
        log(m);
    }
}

/// Emit an uncaught error to the REPL/file-in log as `! <label>: <message>`.
fn reportError(err_ref: Ref, label: []const u8) void {
    const log = g_log orelse return;
    const api = g_api.?;
    var buf: [256]u8 = undefined;
    const msg_ref = if (isEid(err_ref)) api.query_eavt(err_ref >> 3, ATTR_ERROR_MESSAGE) else NIL;
    const line = if (msg_ref != NIL and isString(msg_ref))
        (std.fmt.bufPrintZ(&buf, "[lang] ! {s}: {s}", .{ label, stringBytes(msg_ref) }) catch return)
    else
        (std.fmt.bufPrintZ(&buf, "[lang] ! {s}", .{label}) catch return);
    log(line);
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

fn isMapRef(r: Ref) bool {
    if ((r & 0b111) != 0) return false;
    if (r == 0) return false;
    const vt_ptr: *const Ref = @ptrFromInt(r - @sizeOf(Ref));
    return vt_ptr.* == mkEid(ASSOC_VT);
}

fn isRangeRef(r: Ref) bool {
    if ((r & 0b111) != 0) return false;
    if (r == 0) return false;
    const vt_ptr: *const Ref = @ptrFromInt(r - @sizeOf(Ref));
    return vt_ptr.* == mkEid(RANGE_VT);
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
    if (isRangeRef(val)) {
        const len = g_api.?.range_len(val);
        if (len == 0) return off + (std.fmt.bufPrint(buf[off..], "(empty range)", .{}) catch return off).len;
        var o = off + (std.fmt.bufPrint(buf[off..], "(", .{}) catch return off).len;
        o = appendRef(buf, o, g_api.?.range_nth(val, 0));
        o += (std.fmt.bufPrint(buf[o..], " to: ", .{}) catch return o).len;
        o = appendRef(buf, o, g_api.?.range_nth(val, len - 1));
        return o + (std.fmt.bufPrint(buf[o..], ")", .{}) catch return o).len;
    }
    if (isMapRef(val)) {
        var o = off + (std.fmt.bufPrint(buf[off..], "{{", .{}) catch return off).len;
        const ks = g_api.?.map_keys(val);
        const vs = g_api.?.map_values(val);
        const len = g_api.?.rrb_len(ks);
        var i: u64 = 0;
        while (i < len) : (i += 1) {
            if (i > 0) o += (std.fmt.bufPrint(buf[o..], " ", .{}) catch return o).len;
            // Map key: render a keyword bare (`name:`), else generic.
            const k = g_api.?.rrb_nth(ks, i);
            if (isEid(k)) {
                const nm = g_api.?.symbol_name(k >> 3);
                if (nm != NIL and isString(nm)) {
                    o += (std.fmt.bufPrint(buf[o..], "{s}: ", .{stringBytes(nm)}) catch return o).len;
                } else {
                    o = appendRef(buf, o, k);
                    o += (std.fmt.bufPrint(buf[o..], ": ", .{}) catch return o).len;
                }
            } else {
                o = appendRef(buf, o, k);
                o += (std.fmt.bufPrint(buf[o..], ": ", .{}) catch return o).len;
            }
            o = appendRef(buf, o, g_api.?.rrb_nth(vs, i));
        }
        return o + (std.fmt.bufPrint(buf[o..], "}}", .{}) catch return o).len;
    }
    if (isEid(val)) {
        const e = val >> 3;
        if (e == TRUE_EID) return off + (std.fmt.bufPrint(buf[off..], "true", .{}) catch return off).len;
        if (e == FALSE_EID) return off + (std.fmt.bufPrint(buf[off..], "false", .{}) catch return off).len;
        const em = g_api.?.query_eavt(e, ATTR_ERROR_MESSAGE);
        if (em != NIL and isString(em)) return off + (std.fmt.bufPrint(buf[off..], "error: {s}", .{stringBytes(em)}) catch return off).len;
        const nm = g_api.?.symbol_name(e);
        if (nm != NIL and isString(nm)) return off + (std.fmt.bufPrint(buf[off..], "#{s}", .{stringBytes(nm)}) catch return off).len;
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
    // W1 Stage 2b part 2 (docs/19c): hand the kernel this process's tree-walker
    // block shim so the full-image loader can rebuild durable `let f := [block]`
    // REPL globals (CLOSURE_VT { method = blockEvalShim, data = OBJECT_VT{ast,
    // scope} }) at their original eid from their persisted (ast, scope) facts.
    api.register_treewalk_block_shim(@ptrCast(&blockEvalShim));
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
    installIoSurface(); // Phase 17.8 — File + System I/O
    installCellSurface(); // Phase 16 Tier E — cells/metadata/applicative/Xf
    installStdlib(); // Phase 17 — printing, equality/comparison, sort
    installStdlibExtras(); // Phase 17.9 — Math/Random/Time/OS/String-utils
    installCstSurface(); // Phase 17.11 — birnam-navigable parse trees
    installSymbolSurface(); // Phase 17.12 — symbol =/printString/asString
    installObjectSurface(); // Phase 17.12 — Object new + vtable accessor
    installTypePredicates(); // Phase 17.12 — isString/isNumber/isVector/isMap
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
    // JSON is now birnam-authored (the `json` dylib files in json.birnam); the
    // zig buildJsonGrammar is retained but no longer wired. YAML stays zig.
    buildYamlGrammar(); // Phase 17.10 — YAML flat-subset parser
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
