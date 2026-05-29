//! IdApi — C-ABI surface that the kernel exposes to dylibs.
//!
//! Each loaded dylib receives a pointer to this struct at init time.
//! All cross-dylib calls go through these function pointers; modules
//! never see `*FactStore` or `*ObjectTable` directly.
//!
//! Zig 0.15 caveat: nominal types are per-module, so the dylib's
//! corresponding declaration of `IdApi` is layout-compatible (extern
//! struct) but not type-identical. Modules cast accordingly.

const std = @import("std");
const ref_mod = @import("ref.zig");
const kernel_eids = @import("kernel_eids.zig");
const strings = @import("strings.zig");
const object = @import("object.zig");
const send_mod = @import("send.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");

pub const Ref = ref_mod.Ref;

// -- Shell side of the bridge ------------------------------------------------

pub const ShellContext = extern struct {
    log: *const fn ([*:0]const u8) callconv(.c) void,
    set_repl_handler: *const fn (?*const fn ([*:0]const u8) callconv(.c) void) callconv(.c) void,
};

// -- IdApi -------------------------------------------------------------------

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
};

/// Callback invoked per (e, a, v) triple by facts_of_module.
pub const FactCallback = fn (e: u64, a: u64, v: Ref, ctx: ?*anyopaque) callconv(.c) void;

// -- populate ----------------------------------------------------------------

pub fn populate() IdApi {
    return .{
        .nil_eid = kernel_eids.NIL,
        .vtable_vt_eid = kernel_eids.VTABLE_VT,
        .object_vt_eid = kernel_eids.OBJECT_VT,
        .selector_vt_eid = kernel_eids.SELECTOR_VT,
        .closure_vt_eid = kernel_eids.CLOSURE_VT,
        .nil_vt_eid = kernel_eids.NIL_VT,
        .sel_lookup = kernel_eids.SEL_LOOKUP,
        .sel_methodAtPutWith = kernel_eids.SEL_METHOD_AT_PUT_WITH,
        .sel_delegated = kernel_eids.SEL_DELEGATED,
        .sel_init = kernel_eids.SEL_INIT,
        .sel_allocate = kernel_eids.SEL_ALLOCATE,
        .sel_vtable = kernel_eids.SEL_VTABLE,
        .attr_vtable = kernel_eids.ATTR_VTABLE,
        .attr_delegate = kernel_eids.ATTR_DELEGATE,
        .attr_name = kernel_eids.ATTR_NAME,
        .attr_module = kernel_eids.ATTR_MODULE,

        .alloc_eid = &cAllocEid,
        .promote = &cPromote,
        .demote = &cDemote,
        .alloc_object = &cAllocObject,
        .object_to_ref = &cObjectToRef,
        .make_closure = &cMakeClosure,

        .intern = &cIntern,
        .make_string = &cMakeString,

        .assert_fact = &cAssertFact,
        .retract_fact = &cRetractFact,
        .query_eavt = &cQueryEavt,
        .facts_of_module = &cFactsOfModule,

        .send0 = &cSend0,
        .send1 = &cSend1,
        .send2 = &cSend2,
        .send3 = &cSend3,

        .slot_at = &cSlotAt,
        .set_slot = &cSetSlot,

        .make_query = &cMakeQuery,
        .free_query = &cFreeQuery,
        .query_variable = &cQueryVariable,
        .query_add_triple = &cQueryAddTriple,
        .query_add_predicate = &cQueryAddPredicate,
        .query_add_find = &cQueryAddFind,
        .query_set_asof = &cQuerySetAsof,
        .query_set_since = &cQuerySetSince,
        .query_set_between = &cQuerySetBetween,
        .query_set_in_module = &cQuerySetInModule,
        .query_set_max_iter = &cQuerySetMaxIter,
        .query_set_max_tuples = &cQuerySetMaxTuples,

        .query_begin_negation = &cQueryBeginNegation,
        .query_end_negation = &cQueryEndNegation,
        .query_begin_disjunction = &cQueryBeginDisjunction,
        .query_begin_branch = &cQueryBeginBranch,
        .query_end_branch = &cQueryEndBranch,
        .query_end_disjunction = &cQueryEndDisjunction,

        .query_add_rule_invocation = &cQueryAddRuleInvocation,
        .define_rule = &cDefineRule,
        .query_use_global_rules = &cQueryUseGlobalRules,

        .query_add_find_aggregate = &cQueryAddFindAggregate,
        .rrb_count = &cRrbCount,
        .rrb_at = &cRrbAt,

        .query_execute = &cQueryExecute,
        .result_count = &cResultCount,
        .result_arity = &cResultArity,
        .result_at = &cResultAt,
        .free_result = &cFreeResult,
    };
}

/// Storage for the interner so the C wrappers can find it. Shell calls this
/// after bootstrap; modules see the API through `populate()` which closes
/// over this state.
var g_interner: ?*interner_mod.Interner = null;

pub fn setInterner(i: *interner_mod.Interner) void {
    g_interner = i;
}

// -- C wrappers --------------------------------------------------------------

fn cAllocEid() callconv(.c) u64 {
    const t = send_mod.tablePtr() orelse return 0;
    return t.alloc();
}

fn cPromote(eid: u64, ptr_ref: Ref) callconv(.c) void {
    const t = send_mod.tablePtr() orelse return;
    t.put(eid, ptr_ref) catch {};
}

fn cDemote(eid: u64) callconv(.c) void {
    const t = send_mod.tablePtr() orelse return;
    t.clear(eid) catch {};
}

fn cAllocObject(vt_ref: Ref, n_slots: usize) callconv(.c) ?*anyopaque {
    const a = send_mod.kernelAllocator() orelse return null;
    const obj = object.allocObject(a, vt_ref, n_slots) catch return null;
    return @ptrCast(obj);
}

fn cObjectToRef(p: ?*anyopaque) callconv(.c) Ref {
    const obj_ptr: object.ObjPtr = @ptrCast(@alignCast(p orelse return ref_mod.NIL));
    return object.toRef(obj_ptr);
}

const Closure = @import("closure.zig").Closure;

fn cMakeClosure(method_fn: ?*const anyopaque, data: Ref) callconv(.c) u64 {
    const a = send_mod.kernelAllocator() orelse return 0;
    const t = send_mod.tablePtr() orelse return 0;
    const fn_ptr = method_fn orelse return 0;
    // Closure has the libid-object layout: [vt_ref][method][data].
    const total = 3 * @sizeOf(Ref);
    const buf = a.alignedAlloc(u8, .of(Ref), total) catch return 0;
    @memset(buf, 0);
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = ref_mod.mkEid(kernel_eids.CLOSURE_VT);
    const c: *Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    c.method = fn_ptr;
    c.data = data;
    const eid = t.alloc();
    t.put(eid, ref_mod.mkPointer(@ptrCast(c))) catch return 0;
    return eid;
}

fn cIntern(name: [*]const u8, len: usize) callconv(.c) u64 {
    const i = g_interner orelse return 0;
    const s = send_mod.storePtr() orelse return 0;
    const t = send_mod.tablePtr() orelse return 0;
    const slice = name[0..len];
    return i.intern(s, t, slice) catch 0;
}

fn cMakeString(name: [*]const u8, len: usize) callconv(.c) Ref {
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    return strings.mkString(a, name[0..len]) catch ref_mod.NIL;
}

fn cAssertFact(e: u64, a: u64, v: Ref, m: u64) callconv(.c) u64 {
    const s = send_mod.storePtr() orelse return 0;
    return s.assert(e, a, v, m) catch 0;
}

fn cRetractFact(e: u64, a: u64, v: Ref, m: u64) callconv(.c) u64 {
    const s = send_mod.storePtr() orelse return 0;
    return s.retract(e, a, v, m) catch 0;
}

fn cQueryEavt(e: u64, a: u64) callconv(.c) Ref {
    const s = send_mod.storePtr() orelse return ref_mod.NIL;
    return s.currentValue(e, a) orelse ref_mod.NIL;
}

fn cSend0(receiver: Ref, sel: u64) callconv(.c) Ref {
    return send_mod.send0(receiver, sel);
}
fn cSend1(receiver: Ref, sel: u64, a1: Ref) callconv(.c) Ref {
    return send_mod.send1(receiver, sel, a1);
}
fn cSend2(receiver: Ref, sel: u64, a1: Ref, a2: Ref) callconv(.c) Ref {
    return send_mod.send2(receiver, sel, a1, a2);
}
fn cSend3(receiver: Ref, sel: u64, a1: Ref, a2: Ref, a3: Ref) callconv(.c) Ref {
    return send_mod.send3(receiver, sel, a1, a2, a3);
}

// -- Slot ops ---------------------------------------------------------------

fn cSlotAt(obj_ptr: ?*anyopaque, idx: usize) callconv(.c) Ref {
    const obj: object.ObjPtr = @ptrCast(@alignCast(obj_ptr orelse return ref_mod.NIL));
    return object.slotAt(obj, idx);
}

fn cSetSlot(obj_ptr: ?*anyopaque, idx: usize, v: Ref) callconv(.c) void {
    const obj: object.ObjPtr = @ptrCast(@alignCast(obj_ptr orelse return));
    object.setSlot(obj, idx, v);
}

// -- Query construction -----------------------------------------------------

const query = @import("query.zig");

fn cMakeQuery() callconv(.c) ?*anyopaque {
    const a = send_mod.kernelAllocator() orelse return null;
    const q = a.create(query.Query) catch return null;
    q.* = query.Query.init(a);
    return @ptrCast(q);
}

fn cFreeQuery(p: ?*anyopaque) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    const a = send_mod.kernelAllocator() orelse return;
    q.deinit();
    a.destroy(q);
}

fn cQueryVariable(p: ?*anyopaque, name: [*]const u8, len: usize) callconv(.c) u32 {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return 0));
    return q.variable(name[0..len]) catch 0;
}

fn termFromExtern(kind: u32, val: u64) query.Term {
    return switch (kind) {
        0 => .{ .constant = val },
        1 => .{ .variable = @intCast(val) },
        else => .wildcard,
    };
}

fn cQueryAddTriple(
    p: ?*anyopaque,
    ek: u32, ev: u64,
    ak: u32, av: u64,
    vk: u32, vv: u64,
) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    q.addTriple(termFromExtern(ek, ev), termFromExtern(ak, av), termFromExtern(vk, vv)) catch {};
}

fn cQueryAddPredicate(
    p: ?*anyopaque,
    op: u32,
    lk: u32, lv: u64,
    rk: u32, rv: u64,
) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    const pred_op: query.PredicateOp = switch (op) {
        0 => .eq,
        1 => .ne,
        2 => .lt,
        3 => .le,
        4 => .gt,
        5 => .ge,
        else => return,
    };
    q.addPredicate(pred_op, termFromExtern(lk, lv), termFromExtern(rk, rv)) catch {};
}

fn cQueryAddFind(p: ?*anyopaque, var_idx: u32) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    q.addFind(var_idx) catch {};
}

fn cQuerySetAsof(p: ?*anyopaque, tx: u64) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    q.setAsOf(tx);
}

fn cQuerySetSince(p: ?*anyopaque, tx: u64) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    q.setSince(tx);
}

fn cQuerySetBetween(p: ?*anyopaque, from: u64, to: u64) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    q.setBetween(from, to);
}

fn cQuerySetInModule(p: ?*anyopaque, m: u64) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    q.setInModule(m);
}

fn cQuerySetMaxIter(p: ?*anyopaque, n: u32) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    q.max_iter = n;
}

fn cQuerySetMaxTuples(p: ?*anyopaque, n: u64) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    q.max_tuples = n;
}

fn cQueryBeginNegation(p: ?*anyopaque) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    q.beginNegation() catch {};
}
fn cQueryEndNegation(p: ?*anyopaque) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    q.endNegation() catch {};
}
fn cQueryBeginDisjunction(p: ?*anyopaque) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    q.beginDisjunction() catch {};
}
fn cQueryBeginBranch(p: ?*anyopaque) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    q.beginBranch() catch {};
}
fn cQueryEndBranch(p: ?*anyopaque) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    q.endBranch() catch {};
}
fn cQueryEndDisjunction(p: ?*anyopaque) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    q.endDisjunction() catch {};
}

const rule_mod = @import("rule.zig");

/// Module-global rule registry. Initialized by the shell at boot; modules
/// add rules via `define_rule`.
var g_registry: ?*rule_mod.Registry = null;

pub fn setRegistry(r: *rule_mod.Registry) void {
    g_registry = r;
}

fn cQueryAddRuleInvocation(
    p: ?*anyopaque,
    name_eid: u64,
    n_args: usize,
    arg_kinds: [*]const u32,
    arg_vals: [*]const u64,
) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    const a = q.allocator;
    const terms = a.alloc(query.Term, n_args) catch return;
    for (0..n_args) |i| {
        terms[i] = switch (arg_kinds[i]) {
            0 => .{ .constant = arg_vals[i] },
            1 => .{ .variable = @intCast(arg_vals[i]) },
            else => .wildcard,
        };
    }
    q.addRuleInvocation(name_eid, terms) catch {};
}

fn cDefineRule(p: ?*anyopaque, name_eid: u64, n_params: u32, max_var: u32) callconv(.c) bool {
    const reg = g_registry orelse return false;
    const q: *query.Query = @ptrCast(@alignCast(p orelse return false));
    // Snapshot the where: clauses as the rule body. Caller still owns the
    // Query; freeing the Query later will only release its non-where state
    // because we transfer ownership of the clauses slice here.
    const body_slice = q.where.toOwnedSlice(q.allocator) catch return false;
    reg.define(name_eid, .{
        .n_params = n_params,
        .body = body_slice,
        .max_var = max_var,
    }) catch return false;
    return true;
}

fn cQueryUseGlobalRules(p: ?*anyopaque) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    if (g_registry) |r| q.setRules(r);
}

fn cQueryAddFindAggregate(p: ?*anyopaque, kind: u32, var_idx: u32) callconv(.c) void {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return));
    const k: query.AggregateKind = switch (kind) {
        0 => .count,
        1 => .sum,
        2 => .min,
        3 => .max,
        4 => .avg,
        5 => .collect,
        else => return,
    };
    q.addFindAggregate(k, var_idx) catch {};
}

const aggregate_mod = @import("aggregate.zig");

fn cRrbCount(handle: ?*anyopaque) callconv(.c) u64 {
    const r: *aggregate_mod.RrbRef = @ptrCast(@alignCast(handle orelse return 0));
    return r.count();
}

fn cRrbAt(handle: ?*anyopaque, idx: u64) callconv(.c) Ref {
    const r: *aggregate_mod.RrbRef = @ptrCast(@alignCast(handle orelse return ref_mod.NIL));
    if (idx >= r.count()) return ref_mod.NIL;
    return r.nth(idx);
}

fn cQueryExecute(p: ?*anyopaque) callconv(.c) ?*anyopaque {
    const q: *query.Query = @ptrCast(@alignCast(p orelse return null));
    const store = send_mod.storePtr() orelse return null;
    const a = send_mod.kernelAllocator() orelse return null;
    const r = a.create(query.Result) catch return null;
    r.* = query.execute(q, store) catch {
        a.destroy(r);
        return null;
    };
    return @ptrCast(r);
}

fn cResultCount(p: ?*anyopaque) callconv(.c) u64 {
    const r: *query.Result = @ptrCast(@alignCast(p orelse return 0));
    return r.count();
}

fn cResultArity(p: ?*anyopaque) callconv(.c) u32 {
    const r: *query.Result = @ptrCast(@alignCast(p orelse return 0));
    return r.arity;
}

fn cResultAt(p: ?*anyopaque, row: u64, col: u32) callconv(.c) Ref {
    const r: *query.Result = @ptrCast(@alignCast(p orelse return ref_mod.NIL));
    if (row >= r.count() or col >= r.arity) return ref_mod.NIL;
    return r.at(@intCast(row), col);
}

fn cFreeResult(p: ?*anyopaque) callconv(.c) void {
    const r: *query.Result = @ptrCast(@alignCast(p orelse return));
    const a = send_mod.kernelAllocator() orelse return;
    r.deinit();
    a.destroy(r);
}

fn cFactsOfModule(m: u64, cb: *const FactCallback, ctx: ?*anyopaque) callconv(.c) usize {
    const s = send_mod.storePtr() orelse return 0;
    const a = send_mod.kernelAllocator() orelse return 0;
    var it = s.factsOfModule(m, a) catch return 0;
    defer it.deinit();
    var n: usize = 0;
    while (it.next()) |triple| {
        cb(triple.e, triple.a, triple.v, ctx);
        n += 1;
    }
    return n;
}
