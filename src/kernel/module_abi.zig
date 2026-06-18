//! Shared module-side ABI definitions — THE single source of truth.
//!
//! A birnam module dylib talks to the kernel through a C-ABI `IdApi` table.
//! Zig has no cross-dylib type *imports* (nominal types are per-module), but
//! the table is a pure C-ABI `extern struct`, so one definition compiled into
//! each dylib yields an identical layout. This file holds that one definition;
//! the kernel (`id_api.zig`) re-exports `IdApi`/`ShellContext`/`FactCallback`
//! from here, and every module imports it instead of hand-copying the struct.
//!
//! Because the kernel itself compiles its populate code against this `IdApi`,
//! a missing/renamed field is a kernel build error — this file can never go
//! silently stale (the failure mode the old hand-copies had).
//!
//! ABI discipline (Phase 9.5.5): new IdApi fields append at the END, never
//! mid-struct, so already-compiled modules reading a shorter prefix stay
//! layout-compatible.

const std = @import("std");

pub const Ref = u64;
pub const NIL: Ref = 0b010;

/// Tag a raw entity id as an EID-tagged Ref (tag bits 0b010).
pub inline fn mkEid(e: u64) Ref {
    return (e << 3) | 0b010;
}

// -- Well-known kernel eids a wrapper may need (mirror of kernel_eids) --------
pub const MODULE_PROTO: u64 = 15;
pub const LANGUAGE_PROTO: u64 = 192;

pub const ShellContext = extern struct {
    log: *const fn ([*:0]const u8) callconv(.c) void,
    set_repl_handler: *const fn (?*const fn ([*:0]const u8) callconv(.c) void) callconv(.c) void,
};

/// Callback invoked per (e, a, v) triple by facts_of_module.
pub const FactCallback = fn (e: u64, a: u64, v: Ref, ctx: ?*anyopaque) callconv(.c) void;

pub const IdApi = extern struct {
    // Well-known EIDs (snapshot of kernel_eids at populate-time).
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

    // Identity / allocation
    alloc_eid: *const fn () callconv(.c) u64,
    promote: *const fn (u64, Ref) callconv(.c) void,
    demote: *const fn (u64) callconv(.c) void,
    alloc_object: *const fn (Ref, usize) callconv(.c) ?*anyopaque,
    object_to_ref: *const fn (?*anyopaque) callconv(.c) Ref,
    /// Allocate a Closure with the given Zig fn pointer + data Ref, mint
    /// an EID, register the pointer Ref in the object table, return the
    /// EID. The returned EID is what modules assert as the value of a
    /// method fact: `[vt sel mkEid(closure_eid)]`.
    make_closure: *const fn (?*const anyopaque, Ref) callconv(.c) u64,

    // Strings / interning
    intern: *const fn ([*]const u8, usize) callconv(.c) u64,
    make_string: *const fn ([*]const u8, usize) callconv(.c) Ref,

    // Fact store
    assert_fact: *const fn (u64, u64, Ref, u64) callconv(.c) u64,
    retract_fact: *const fn (u64, u64, Ref, u64) callconv(.c) u64,
    query_eavt: *const fn (u64, u64) callconv(.c) Ref, // returns NIL if absent
    facts_of_module: *const fn (u64, *const FactCallback, ?*anyopaque) callconv(.c) usize,

    // Dispatch
    send0: *const fn (Ref, u64) callconv(.c) Ref,
    send1: *const fn (Ref, u64, Ref) callconv(.c) Ref,
    send2: *const fn (Ref, u64, Ref, Ref) callconv(.c) Ref,
    send3: *const fn (Ref, u64, Ref, Ref, Ref) callconv(.c) Ref,

    // Object slot ops
    slot_at: *const fn (?*anyopaque, usize) callconv(.c) Ref,
    set_slot: *const fn (?*anyopaque, usize, Ref) callconv(.c) void,

    // Query construction + execution (Phase 7).
    // Term encoding: kind 0 = constant (val=Ref), 1 = variable (val=var_idx),
    // 2 = wildcard (val ignored).
    make_query: *const fn () callconv(.c) ?*anyopaque,
    free_query: *const fn (?*anyopaque) callconv(.c) void,
    query_variable: *const fn (?*anyopaque, [*]const u8, usize) callconv(.c) u32,
    query_add_triple: *const fn (
        ?*anyopaque,
        u32, u64, // e
        u32, u64, // a
        u32, u64, // v
    ) callconv(.c) void,
    query_add_predicate: *const fn (
        ?*anyopaque,
        u32, // op (0=eq,1=ne,2=lt,3=le,4=gt,5=ge)
        u32, u64, // lhs
        u32, u64, // rhs
    ) callconv(.c) void,
    query_add_find: *const fn (?*anyopaque, u32) callconv(.c) void,
    query_set_asof: *const fn (?*anyopaque, u64) callconv(.c) void,
    query_set_in_module: *const fn (?*anyopaque, u64) callconv(.c) void,

    /// Execute the query. Returns a Result handle (raw pointer Ref). Caller
    /// must `free_result` when done.
    query_execute: *const fn (?*anyopaque) callconv(.c) ?*anyopaque,
    result_count: *const fn (?*anyopaque) callconv(.c) u64,
    result_arity: *const fn (?*anyopaque) callconv(.c) u32,
    result_at: *const fn (?*anyopaque, u64, u32) callconv(.c) Ref,
    free_result: *const fn (?*anyopaque) callconv(.c) void,

    // -- Phase 7.1 polish (added at end to preserve dylib ABI offsets) -----
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

    query_add_rule_invocation: *const fn (
        ?*anyopaque,
        u64,
        usize,
        [*]const u32,
        [*]const u64,
    ) callconv(.c) void,

    define_rule: *const fn (
        ?*anyopaque,
        u64,
        u32,
        u32,
    ) callconv(.c) bool,

    query_use_global_rules: *const fn (?*anyopaque) callconv(.c) void,

    // -- Phase 7.2: aggregation -----------------------------------------------
    // kind = AggregateKind: 0 count, 1 sum, 2 min, 3 max, 4 avg, 5 collect.
    query_add_find_aggregate: *const fn (?*anyopaque, u32, u32) callconv(.c) void,

    // RRB-Tree accessors for collect-aggregate results. `handle` is a raw
    // pointer (the value stashed in slot 0 of a libid-wrapped RRB object).
    rrb_count: *const fn (?*anyopaque) callconv(.c) u64,
    rrb_at: *const fn (?*anyopaque, u64) callconv(.c) Ref,

    // -- Phase 7.3: saved queries -------------------------------------------
    // Caller passes the Query handle; the kernel takes ownership until
    // explicit retract of the saved-query entity.
    save_query: *const fn ([*]const u8, usize, ?*anyopaque) callconv(.c) u64,
    /// Look up + execute a saved query by name. Returns a Result handle, or
    /// null if no saved query exists with that name.
    run_query: *const fn ([*]const u8, usize) callconv(.c) ?*anyopaque,

    // -- Phase 7.4: predicates + functions ---------------------------------
    /// Add a `[(name a b ...)]` predicate-call clause.
    query_add_pred_call: *const fn (
        ?*anyopaque,
        u64, // name_eid
        usize,
        [*]const u32,
        [*]const u64,
    ) callconv(.c) void,
    /// Add a `[(name a b ...) ?target]` function-call clause.
    query_add_fn_call: *const fn (
        ?*anyopaque,
        u64, // name_eid
        usize,
        [*]const u32,
        [*]const u64,
        u32, // target var idx
    ) callconv(.c) void,
    /// Register a native predicate by interned name. impl_ptr is a
    /// `*const fn ([*]const Ref, usize) callconv(.c) bool`.
    register_pred: *const fn (u64, ?*const anyopaque) callconv(.c) u64,
    /// Register a native function by interned name. impl_ptr is a
    /// `*const fn ([*]const Ref, usize) callconv(.c) Ref`.
    register_fn: *const fn (u64, ?*const anyopaque) callconv(.c) u64,

    // -- Phase 8: promotion / FFI pin --------------------------------------
    promote_eid: *const fn (u64) callconv(.c) bool,
    demote_eid: *const fn (u64) callconv(.c) void,
    hotness_of: *const fn (u64) callconv(.c) u32,
    is_materialized: *const fn (u64) callconv(.c) bool,
    ffi_enter: *const fn () callconv(.c) void,
    ffi_exit: *const fn () callconv(.c) void,

    // -- Phase 10.0 step 4: PEG send-dispatched surface install ------------
    /// Called by the PEG dylib at module-init to install `parse:from:` on
    /// GRAMMAR_VT (caches engine state in kernel/peg.zig module-static).
    /// The dylib never holds the engine pointer directly — caching lives
    /// in the kernel layer that already owns the engine code (see P10
    /// deviation note in peg.zig).
    peg_install_send_api: *const fn (u64) callconv(.c) void,

    // -- DEPRECATED (Phase 10.2.5 superseded these) ------------------------
    // Procedural escape hatch from Phase 10.1.b. Retained for ABI discipline
    // (do NOT remove fields mid-life — appended-only struct). New callers
    // MUST use sendN against PEG_VT's seq:/choice:/charset: methods. The
    // ADR-10-Send-Arity resolution notes the migration path; this section
    // becomes purgeable at the next image-format breaking change.
    peg_mk_seq: *const fn ([*]const u64, usize) callconv(.c) u64,
    peg_mk_choice: *const fn ([*]const u64, usize) callconv(.c) u64,
    peg_mk_charset: *const fn ([*]const u8, usize) callconv(.c) u64,

    /// Phase 10.2.5: variadic send. send0..send3 remain as thin packers
    /// for convenience; sendN is the canonical entry for arbitrary-arity
    /// dispatch from user-source compound keyword sends.
    sendN: *const fn (Ref, u64, [*]const Ref, usize) callconv(.c) Ref,

    /// Look up the object-table Ref stored at `eid`, or NIL if absent.
    /// Used by closure-as-receiver methods (block value/value:/value:value:)
    /// that need to recover the closure's pointer from its EID receiver.
    object_at_eid: *const fn (u64) callconv(.c) Ref,

    /// Allocate a boxed Float (vt = FLOAT_VT) and return its pointer Ref.
    /// Used by the lang dylib at parse time to pre-allocate float literals
    /// stored on AST_LITERAL_VT nodes per X5 lock.
    make_float: *const fn (f64) callconv(.c) Ref,

    /// Phase 11.2 — compile an AST_BLOCK_VT entity as a bytecode method.
    /// Block params become method args (locals[0..n]); block body
    /// becomes the method body. Returns the materialized method EID
    /// suitable for installation via `[vt sel mkEid(method_eid)]` (or
    /// 0 on error). The returned method's closure is wired so SEND_*
    /// dispatches into bytecode via `dispatch.bytecodeDispatcherImpl`.
    /// Append-only ABI: new field at end per Phase 9.5.5 discipline.
    compile_block_as_method: *const fn (block_ast_eid: u64) callconv(.c) u64,

    // -- Phase 15.R R1d: db-value language surface (append-only) ------------
    /// Capture a NOW snapshot; returns a SNAPSHOT_VT object Ref (mkEid), or NIL.
    snapshot_now: *const fn () callconv(.c) Ref,
    /// Capture a snapshot as of basis-T; returns a SNAPSHOT_VT Ref, or NIL when
    /// the history is no longer retained.
    snapshot_as_of: *const fn (u64) callconv(.c) Ref,
    /// Exact point read (e, a) through a snapshot Ref; NIL if absent / bad ref.
    snapshot_get: *const fn (Ref, u64, u64) callconv(.c) Ref,
    /// Snapshot basis-T as a SmallInt Ref.
    snapshot_basis: *const fn (Ref) callconv(.c) Ref,
    /// Release a snapshot (unpin its epoch + free the object). Idempotent-ish:
    /// a bad/already-released ref is a no-op.
    snapshot_release: *const fn (Ref) callconv(.c) void,
    /// Set retention policy: code 0=current_only, 1=keep_all, 2=keep_since(arg),
    /// 3=keep_last_n(arg).
    set_retention: *const fn (u32, u64) callconv(.c) void,
    /// Hard-delete an entity (current + history + archives). Returns true on
    /// success, false if refused (a live snapshot pins) or errored.
    excise_entity: *const fn (u64) callconv(.c) bool,

    // -- Phase 16 Tier A: persistent vectors (append-only) -----------------
    /// Build a vector (RRB Ref, vt RRB_VT) from `n` element Refs.
    make_rrb: *const fn ([*]const Ref, usize) callconv(.c) Ref,
    /// Immutable append → a NEW vector with `elem` at the end.
    rrb_with: *const fn (Ref, Ref) callconv(.c) Ref,
    /// Element count of a vector.
    rrb_len: *const fn (Ref) callconv(.c) u64,
    /// Element at `idx` (caller bounds-checks).
    rrb_nth: *const fn (Ref, u64) callconv(.c) Ref,

    // -- Phase 16 Tier C: persistent maps (append-only) --------------------
    /// Empty keyword-keyed map (ASSOC_VT Ref).
    make_map: *const fn () callconv(.c) Ref,
    /// Immutable put → a NEW map with `key`→`val`.
    map_put: *const fn (Ref, Ref, Ref) callconv(.c) Ref,
    /// Value for `key`, or NIL if absent (use `map_has` to disambiguate a
    /// stored nil from absence — no-NULL).
    map_get: *const fn (Ref, Ref) callconv(.c) Ref,
    /// Whether `key` is present.
    map_has: *const fn (Ref, Ref) callconv(.c) bool,
    /// Entry count.
    map_len: *const fn (Ref) callconv(.c) u64,
    /// Keys as a vector (RRB Ref).
    map_keys: *const fn (Ref) callconv(.c) Ref,
    /// Values as a vector (RRB Ref).
    map_values: *const fn (Ref) callconv(.c) Ref,
    /// Merge two maps → a NEW map (`b` overrides `a`).
    map_merge: *const fn (Ref, Ref) callconv(.c) Ref,

    /// Interned name of `eid` as a string Ref, or NIL if not interned. Lets the
    /// display layer render keyword/selector eids by name (Phase 16 Tier C).
    symbol_name: *const fn (u64) callconv(.c) Ref,

    // -- Phase 16 deferred: reduced early-out (append-only) -----------------
    make_reduced: *const fn (Ref) callconv(.c) Ref,
    is_reduced: *const fn (Ref) callconv(.c) bool,
    reduced_value: *const fn (Ref) callconv(.c) Ref,

    // -- Phase 16 deferred: integer ranges (append-only) -------------------
    make_range: *const fn (i64, i64, i64) callconv(.c) Ref,
    range_len: *const fn (Ref) callconv(.c) u64,
    range_nth: *const fn (Ref, u64) callconv(.c) Ref,

    // -- Phase 16 deferred: string ops (append-only) -----------------------
    string_len: *const fn (Ref) callconv(.c) u64,
    string_byte_at: *const fn (Ref, u64) callconv(.c) Ref,
    string_concat: *const fn (Ref, Ref) callconv(.c) Ref,
    string_eq: *const fn (Ref, Ref) callconv(.c) bool,
    string_as_symbol: *const fn (Ref) callconv(.c) Ref,

    // -- Phase 16 Tier E: reference cells (append-only) --------------------
    /// Value of (e, a) as of the end of tx `as_of_tx` (history-walked). NIL if
    /// absent at that time. Backs `[cell asOf: tx]`.
    current_value_as_of: *const fn (u64, u64, u64) callconv(.c) Ref,
    /// Every value (e, a) ever held, oldest→newest, as an RRB vector Ref.
    /// Retracts skipped. Backs `[cell history]`.
    cell_history: *const fn (u64, u64) callconv(.c) Ref,
    /// Current basis-T (durable time coordinate). Backs `db basisT` / cells.
    basis_now: *const fn () callconv(.c) u64,

    // -- Phase 17 string batch (append-only) -------------------------------
    /// Byte index of first occurrence of `needle` in `hay`, or -1 if absent
    /// (empty needle → 0). Backs `indexOf:`/`indexOf:ifAbsent:`.
    string_index_of: *const fn (Ref, Ref) callconv(.c) i64,
    /// Parse a base-10 integer (optional leading `-`) → SmallInt Ref, or NIL if
    /// not a clean integer. Backs `asInteger`/`asNumber`.
    string_to_int: *const fn (Ref) callconv(.c) Ref,
    /// ASCII upper/lower-cased copy. `mode` 0 = upper, 1 = lower.
    string_case: *const fn (Ref, u32) callconv(.c) Ref,
    /// Whitespace-trimmed copy. `mode` 0 = both ends, 1 = left, 2 = right.
    string_trim: *const fn (Ref, u32) callconv(.c) Ref,
    /// Split `hay` on `sep` → vector (RRB) of string Refs. Empty sep → the whole
    /// string as one element. Backs `split:`.
    string_split: *const fn (Ref, Ref) callconv(.c) Ref,
    /// Join a vector of strings with `sep` → one string. Backs `join:`.
    string_join: *const fn (Ref, Ref) callconv(.c) Ref,
    /// Byte-reversed copy. Backs `reverse`.
    string_reverse: *const fn (Ref) callconv(.c) Ref,
    /// Phase 17.8 I/O — command-line arguments (the tokens after `--` on the
    /// birnam command line), captured by the shell. Backs `System arguments`.
    /// `arg_count` = number of args; `arg_at(i, &len)` = pointer+length of arg i.
    /// (Appended at END per the append-only ABI discipline.)
    arg_count: *const fn () callconv(.c) usize,
    arg_at: *const fn (usize, *usize) callconv(.c) [*]const u8,

    /// Cluster ① (2026-06-15) — compile + run a top-level statement AST in
    /// "top-level mode" and return its result. Free variables and outermost
    /// `let`/`:=` lower to scope sends (`[scope <lookup_sel> #name]` etc.)
    /// against `scope_eid`, so REPL/script top-level lines run on the bytecode
    /// VM while keeping cross-line binding visibility. `ok_out` is set true on
    /// a successful compile+run (return value is the result, which MAY itself
    /// be an error value from the error model) and false if the statement
    /// could not be compiled (unsupported AST kind, too many locals, OOM) —
    /// the caller then falls back to the tree-walker. No side effects occur
    /// before a compile failure, so fallback is clean.
    /// (Appended at END per the append-only ABI discipline.)
    eval_top_level: *const fn (stmt_ast_eid: u64, scope_eid: u64, lookup_sel: u64, bind_sel: u64, set_sel: u64, ok_out: *bool) callconv(.c) Ref,

    /// 15.A-AST (2026-06-15) — like `compile_block_as_method`, but the caller
    /// chooses the provenance the method ENTITY (and its block children) are
    /// materialized under. Pass the caller's module eid for CODE methods
    /// (reconstructed each boot, not imaged, never swept — e.g. the stdlib
    /// prelude); pass M_USER for durable user definitions. The plain
    /// `compile_block_as_method` remains and defaults to M_USER.
    /// (Appended at END per the append-only ABI discipline.)
    compile_block_as_method_in: *const fn (block_ast_eid: u64, module_eid: u64) callconv(.c) u64,

    /// 15.A-AST (2026-06-16) — if `closure_eid` is a bytecode closure (direct
    /// compiled method or a MAKE_BLOCK block closure), return its underlying
    /// `:bytecode-stream` method eid; else 0. Lets `method:do:` install a proper
    /// rootable method from a compiled top-level block arg (Cluster ①) instead
    /// of a raw, prune-invisible closure. (Appended at END per ABI discipline.)
    method_eid_of_bytecode_closure: *const fn (closure_eid: u64) callconv(.c) u64,

    /// Phase 7.3 pull finish (2026-06-16) — reverse-ref walk. Returns an RRB
    /// vector of the entity eids (each `mkEid`-tagged) that currently assert
    /// `attr == mkEid(value_eid)` — i.e. the entities pointing AT `value_eid`
    /// through `attr` (AVET-backed). Empty vector if none. Backs reverse pull
    /// (`#[#_attr <spec>]`). (Appended at END per the append-only ABI discipline.)
    reverse_refs: *const fn (attr_eid: u64, value_eid: u64) callconv(.c) Ref,

    // -- Phase 17.13: bind a global from a module (append-only) --------------
    bind_global: *const fn (name_eid: u64, value: Ref) callconv(.c) void,
};

comptime {
    std.debug.assert(@sizeOf(IdApi) > 0);
}
