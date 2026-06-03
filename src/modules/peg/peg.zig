//! PEG dylib façade — Phase 10.0 step 4.
//!
//! The PEG engine lives in `src/kernel/peg.zig` (P10 deviation noted there).
//! This file is the dynamic-link artifact loaded at runtime via the
//! Phase 5 module manager. At module-init it calls
//! `api.peg_install_send_api(module_eid)` which dispatches into the
//! kernel-side install routine, installing `parse:from:` on GRAMMAR_VT.
//!
//! IdApi layout is duplicated and MUST stay layout-compatible with
//! src/kernel/id_api.zig (Phase 9.5.5 ABI discipline — new fields are
//! appended at the end of the struct, never inserted mid-struct).

const std = @import("std");

const Ref = u64;
const NIL: Ref = 0b010;

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

    // Phase 10.0 step 4 — appended.
    peg_install_send_api: *const fn (u64) callconv(.c) void,

    // Phase 10.1.b procedural escape hatch.
    peg_mk_seq: *const fn ([*]const u64, usize) callconv(.c) u64,
    peg_mk_choice: *const fn ([*]const u64, usize) callconv(.c) u64,
    peg_mk_charset: *const fn ([*]const u8, usize) callconv(.c) u64,

    // Phase 10.2.5 — variadic send.
    sendN: *const fn (Ref, u64, [*]const Ref, usize) callconv(.c) Ref,
};

// Phase 14 M4: export the C entry only for dylib builds. When statically
// `@import`ed into the AOT binary (build_opts.export_init = false), the
// caller invokes `moduleInitImpl` directly — avoids a duplicate
// `birnam_module_init` symbol across statically-linked modules.
const build_opts = @import("build_opts");

pub fn moduleInitImpl(
    ctx: *ShellContext,
    api: *const IdApi,
    module_eid: u64,
) callconv(.c) void {
    ctx.log("[peg] initializing");
    api.peg_install_send_api(module_eid);
    ctx.log("[peg] ready");
}

comptime {
    if (build_opts.export_init) @export(&moduleInitImpl, .{ .name = "birnam_module_init" });
}
