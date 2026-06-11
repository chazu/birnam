//! Query — Phase 7 dylib module.
//!
//! Exposes a `db` singleton with `execute:` taking a pre-built Query handle
//! (pointer Ref to kernel `*Query`). Returns a Result object whose `count`
//! and `at:` methods walk the kernel-side `Result`.
//!
//! Real bracket-array clause parsing in `where:` arg is deferred to
//! Phase 10 (language module). For Phase 7, callers (tests and other
//! modules) build the Query via the IdApi `make_query` + `query_add_*`
//! family, then send `[db execute: q_ref]`.
//!
//! IdApi + ShellContext layout duplicated — must stay layout-compatible
//! with src/kernel/id_api.zig.

const std = @import("std");

const abi = @import("../../kernel/module_abi.zig");
const Ref = abi.Ref;
const NIL = abi.NIL;

const mkEid = abi.mkEid;
fn mkPointer(p: ?*anyopaque) Ref {
    return @intFromPtr(p);
}
fn mkSmallInt(n: i64) Ref {
    return @as(u64, @bitCast(n << 3)) | 0b001;
}
fn untagSmallInt(r: Ref) i64 {
    return @as(i64, @bitCast(r)) >> 3;
}
fn untagPointer(r: Ref) ?*anyopaque {
    return @ptrFromInt(r);
}
fn isPointer(r: Ref) bool {
    return (r & 0b111) == 0b000;
}
fn isSmallInt(r: Ref) bool {
    return (r & 0b111) == 0b001;
}

const ShellContext = abi.ShellContext;

const FactCallback = abi.FactCallback;

const IdApi = abi.IdApi;

const RRB_VT_EID: u64 = 9;

const Send = extern struct {
    selector: u64,
    receiver: Ref,
    n_args: u8,
    state: Ref,
    closure: ?*anyopaque,
};

// -- Module-static state -----------------------------------------------------

var g_api: ?*const IdApi = null;
var g_log: ?*const fn ([*:0]const u8) callconv(.c) void = null;
var g_module_eid: u64 = 0;
var g_db_eid: u64 = 0;
var g_db_vt: u64 = 0;
var g_result_vt: u64 = 0;

// -- Methods on db -----------------------------------------------------------

/// `[db execute: query_handle_ref]` — runs the query, returns a result
/// object Ref whose count/at: methods walk the rows.
fn dbExecuteImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = n_args;
    const q_ref = args[0];
    const api = g_api orelse return NIL;
    if (!isPointer(q_ref)) return NIL;
    const q_ptr = untagPointer(q_ref) orelse return NIL;
    const handle = api.query_execute(q_ptr) orelse return NIL;

    // Wrap into a materialized object with vt = result_vt, 1 slot for handle.
    const obj = api.alloc_object(mkEid(g_result_vt), 1) orelse return NIL;
    api.set_slot(obj, 0, mkPointer(handle));
    return api.object_to_ref(obj);
}

// -- Methods on result_vt ----------------------------------------------------

fn resultCountImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const api = g_api orelse return NIL;
    if (!isPointer(s.receiver)) return NIL;
    const obj = untagPointer(s.receiver) orelse return NIL;
    const handle_ref = api.slot_at(obj, 0);
    if (!isPointer(handle_ref)) return NIL;
    const handle = untagPointer(handle_ref) orelse return NIL;
    const c = api.result_count(handle);
    return mkSmallInt(@intCast(c));
}

fn resultArityImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const api = g_api orelse return NIL;
    if (!isPointer(s.receiver)) return NIL;
    const obj = untagPointer(s.receiver) orelse return NIL;
    const handle_ref = api.slot_at(obj, 0);
    if (!isPointer(handle_ref)) return NIL;
    const handle = untagPointer(handle_ref) orelse return NIL;
    return mkSmallInt(@intCast(api.result_arity(handle)));
}

/// `[rrb count]` — wraps slot 0 (raw RRB pointer) and calls api.rrb_count.
fn rrbCountImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const api = g_api orelse return NIL;
    if (!isPointer(s.receiver)) return NIL;
    const obj = untagPointer(s.receiver) orelse return NIL;
    const handle_ref = api.slot_at(obj, 0);
    if (!isPointer(handle_ref)) return NIL;
    const handle = untagPointer(handle_ref) orelse return NIL;
    return mkSmallInt(@intCast(api.rrb_count(handle)));
}

fn rrbAtImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const idx_ref = args[0];
    const api = g_api orelse return NIL;
    if (!isPointer(s.receiver)) return NIL;
    if (!isSmallInt(idx_ref)) return NIL;
    const obj = untagPointer(s.receiver) orelse return NIL;
    const handle_ref = api.slot_at(obj, 0);
    if (!isPointer(handle_ref)) return NIL;
    const handle = untagPointer(handle_ref) orelse return NIL;
    const idx = untagSmallInt(idx_ref);
    if (idx < 0) return NIL;
    return api.rrb_at(handle, @intCast(idx));
}

/// `[result at: <SmallInt row>]` — returns the Ref at row, column 0.
/// (Multi-column results: row*arity + col exposure is Phase 7 polish.)
fn resultAtImpl(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const idx_ref = args[0];
    const api = g_api orelse return NIL;
    if (!isPointer(s.receiver)) return NIL;
    const obj = untagPointer(s.receiver) orelse return NIL;
    const handle_ref = api.slot_at(obj, 0);
    if (!isPointer(handle_ref)) return NIL;
    const handle = untagPointer(handle_ref) orelse return NIL;
    if (!isSmallInt(idx_ref)) return NIL;
    const idx = untagSmallInt(idx_ref);
    if (idx < 0) return NIL;
    return api.result_at(handle, @intCast(idx), 0);
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
    ctx.log("[query] initializing");

    // Result vtable + methods.
    g_result_vt = api.alloc_eid();
    _ = api.assert_fact(g_result_vt, api.attr_vtable, mkEid(api.vtable_vt_eid), module_eid);
    _ = api.assert_fact(g_result_vt, api.attr_delegate, mkEid(api.object_vt_eid), module_eid);

    const sel_count = api.intern("count", 5);
    const sel_arity = api.intern("arity", 5);
    const sel_at = api.intern("at:", 3);

    _ = api.assert_fact(g_result_vt, sel_count, mkEid(api.make_closure(@ptrCast(&resultCountImpl), NIL)), module_eid);
    _ = api.assert_fact(g_result_vt, sel_arity, mkEid(api.make_closure(@ptrCast(&resultArityImpl), NIL)), module_eid);
    _ = api.assert_fact(g_result_vt, sel_at, mkEid(api.make_closure(@ptrCast(&resultAtImpl), NIL)), module_eid);

    // db vtable + methods.
    g_db_vt = api.alloc_eid();
    _ = api.assert_fact(g_db_vt, api.attr_vtable, mkEid(api.vtable_vt_eid), module_eid);
    _ = api.assert_fact(g_db_vt, api.attr_delegate, mkEid(api.object_vt_eid), module_eid);

    const sel_execute = api.intern("execute:", 8);
    _ = api.assert_fact(g_db_vt, sel_execute, mkEid(api.make_closure(@ptrCast(&dbExecuteImpl), NIL)), module_eid);

    // Install count + at: methods on the kernel-reserved RRB_VT so collect
    // aggregate results behave as sendable objects.
    _ = api.assert_fact(RRB_VT_EID, sel_count, mkEid(api.make_closure(@ptrCast(&rrbCountImpl), NIL)), module_eid);
    _ = api.assert_fact(RRB_VT_EID, sel_at, mkEid(api.make_closure(@ptrCast(&rrbAtImpl), NIL)), module_eid);

    // db singleton instance.
    g_db_eid = api.alloc_eid();
    _ = api.assert_fact(g_db_eid, api.attr_vtable, mkEid(g_db_vt), module_eid);

    // Export.
    const sel_db = api.intern("db", 2);
    _ = api.assert_fact(module_eid, sel_db, mkEid(g_db_eid), module_eid);

    const sel_result_vt = api.intern("result_vt", 9);
    _ = api.assert_fact(module_eid, sel_result_vt, mkEid(g_result_vt), module_eid);

    ctx.log("[query] ready");
}
