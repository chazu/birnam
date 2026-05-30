//! PEG — Phase 10.0 dylib module.
//!
//! Goal: pattern-as-facts PEG engine. Each PEG combinator is a fact-shaped
//! entity carrying `:pattern-kind <keyword>` + combinator-specific child /
//! literal facts. Grammar = entity holding a linked list of rule entities.
//! Parser walks the fact-shaped pattern recursively, with a per-parse
//! `AutoHashMap` packrat memo, and emits CST nodes as entities carrying
//! the kernel-reserved `:ast-*` attrs (Phase 10.0 kernel-EID block 91..98).
//!
//! Layered per Phase 10 decision P9: kernel-procedural API via IdApi
//! (deferred to Step 8 of the 10.0 roll-up), send-dispatched API surface
//! on `PEG_VT` / `GRAMMAR_VT` (this file).
//!
//! Scaffolding step (10.0 step 2): module-init wires the shell context,
//! the IdApi pointer, the module EID, and logs ready. No methods minted
//! yet — engine guts land in the next step. The dylib is meant to be
//! loaded after kernel boot, before the lang dylib (X1 ordering).
//!
//! IdApi + ShellContext layout duplicated — must stay layout-compatible
//! with src/kernel/id_api.zig.

const std = @import("std");

const Ref = u64;
const NIL: Ref = 0b010;

fn mkEid(eid: u64) Ref {
    return (eid << 3) | 0b010;
}

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
};

// -- Module-static state -----------------------------------------------------

var g_api: ?*const IdApi = null;
var g_log: ?*const fn ([*:0]const u8) callconv(.c) void = null;
var g_module_eid: u64 = 0;

// -- Module init -------------------------------------------------------------

export fn birnam_module_init(
    ctx: *ShellContext,
    api: *const IdApi,
    module_eid: u64,
) void {
    g_api = api;
    g_log = ctx.log;
    g_module_eid = module_eid;
    ctx.log("[peg] initializing");

    // Scaffolding only — engine + send-dispatched methods land in step 3+.
    // The kernel pre-materialized PEG_VT / GRAMMAR_VT in Stage 16 of boot;
    // this dylib will attach methods to them once the engine is in place.

    ctx.log("[peg] ready");
}
