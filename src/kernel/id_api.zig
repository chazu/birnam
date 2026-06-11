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
const arenas = @import("arenas.zig");
const module_abi = @import("module_abi.zig");

pub const Ref = ref_mod.Ref;

// -- Shell side of the bridge ------------------------------------------------
//
// The ABI struct types live in module_abi.zig (the single source of truth that
// modules also import). Re-exported here so the kernel populate code below — and
// existing `id_api.IdApi` / `id_api.ShellContext` consumers — keep working. The
// kernel compiling its populate against `module_abi.IdApi` is what prevents the
// shared mirror from ever going stale.

pub const ShellContext = module_abi.ShellContext;

// -- IdApi -------------------------------------------------------------------

pub const IdApi = module_abi.IdApi;

/// Callback invoked per (e, a, v) triple by facts_of_module.
pub const FactCallback = module_abi.FactCallback;

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

        .save_query = &cSaveQuery,
        .run_query = &cRunQuery,

        .query_add_pred_call = &cQueryAddPredCall,
        .query_add_fn_call = &cQueryAddFnCall,
        .register_pred = &cRegisterPred,
        .register_fn = &cRegisterFn,

        .promote_eid = &cPromote_8,
        .demote_eid = &cDemote_8,
        .hotness_of = &cHotnessOf,
        .is_materialized = &cIsMaterialized,
        .ffi_enter = &cFfiEnter,
        .ffi_exit = &cFfiExit,

        .query_execute = &cQueryExecute,
        .result_count = &cResultCount,
        .result_arity = &cResultArity,
        .result_at = &cResultAt,
        .free_result = &cFreeResult,

        .peg_install_send_api = &cPegInstallSendApi,
        .peg_mk_seq = &cPegMkSeq,
        .peg_mk_choice = &cPegMkChoice,
        .peg_mk_charset = &cPegMkCharset,

        .sendN = &cSendN,
        .object_at_eid = &cObjectAtEid,
        .make_float = &cMakeFloat,
        .compile_block_as_method = &cCompileBlockAsMethod,
        .snapshot_now = &cSnapshotNow,
        .snapshot_as_of = &cSnapshotAsOf,
        .snapshot_get = &cSnapshotGet,
        .snapshot_basis = &cSnapshotBasis,
        .snapshot_release = &cSnapshotRelease,
        .set_retention = &cSetRetention,
        .excise_entity = &cExciseEntity,
        .make_rrb = &cMakeRrb,
        .rrb_with = &cRrbWith,
        .rrb_len = &cRrbLen,
        .rrb_nth = &cRrbNth,
        .make_map = &cMakeMap,
        .map_put = &cMapPut,
        .map_get = &cMapGet,
        .map_has = &cMapHas,
        .map_len = &cMapLen,
        .map_keys = &cMapKeys,
        .map_values = &cMapValues,
        .map_merge = &cMapMerge,
        .symbol_name = &cSymbolName,
        .make_reduced = &cMakeReduced,
        .is_reduced = &cIsReduced,
        .reduced_value = &cReducedValue,
        .make_range = &cMakeRange,
        .range_len = &cRangeLen,
        .range_nth = &cRangeNth,
        .string_len = &cStringLen,
        .string_byte_at = &cStringByteAt,
        .string_concat = &cStringConcat,
        .string_eq = &cStringEq,
        .string_as_symbol = &cStringAsSymbol,
        .current_value_as_of = &cCurrentValueAsOf,
        .cell_history = &cCellHistory,
        .basis_now = &cBasisNow,
        .string_index_of = &cStringIndexOf,
        .string_to_int = &cStringToInt,
        .string_case = &cStringCase,
        .string_trim = &cStringTrim,
        .string_split = &cStringSplit,
        .string_join = &cStringJoin,
        .string_reverse = &cStringReverse,
    };
}

fn cStringIndexOf(hay: Ref, needle: Ref) callconv(.c) i64 {
    if (!strings.isString(hay) or !strings.isString(needle)) return -1;
    const h = strings.bytes(hay);
    const n = strings.bytes(needle);
    if (n.len == 0) return 0;
    const idx = std.mem.indexOf(u8, h, n) orelse return -1;
    return @intCast(idx);
}
fn cStringToInt(r: Ref) callconv(.c) Ref {
    if (!strings.isString(r)) return ref_mod.NIL;
    const b = strings.bytes(r);
    const v = std.fmt.parseInt(i64, b, 10) catch return ref_mod.NIL;
    return ref_mod.mkSmallInt(v) orelse ref_mod.NIL;
}
fn cStringCase(r: Ref, mode: u32) callconv(.c) Ref {
    if (!strings.isString(r)) return ref_mod.NIL;
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const b = strings.bytes(r);
    const buf = a.alloc(u8, b.len) catch return ref_mod.NIL;
    defer a.free(buf);
    for (b, 0..) |c, i| buf[i] = if (mode == 0) std.ascii.toUpper(c) else std.ascii.toLower(c);
    return strings.mkString(a, buf) catch ref_mod.NIL;
}
fn cStringTrim(r: Ref, mode: u32) callconv(.c) Ref {
    if (!strings.isString(r)) return ref_mod.NIL;
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const ws = " \t\r\n";
    const b = strings.bytes(r);
    const out = switch (mode) {
        1 => std.mem.trimLeft(u8, b, ws),
        2 => std.mem.trimRight(u8, b, ws),
        else => std.mem.trim(u8, b, ws),
    };
    return strings.mkString(a, out) catch ref_mod.NIL;
}
fn cStringSplit(hay: Ref, sep: Ref) callconv(.c) Ref {
    if (!strings.isString(hay) or !strings.isString(sep)) return ref_mod.NIL;
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const h = strings.bytes(hay);
    const s = strings.bytes(sep);
    var parts = std.ArrayList(Ref){};
    defer parts.deinit(a);
    if (s.len == 0) {
        parts.append(a, strings.mkString(a, h) catch return ref_mod.NIL) catch return ref_mod.NIL;
    } else {
        var it = std.mem.splitSequence(u8, h, s);
        while (it.next()) |piece| {
            parts.append(a, strings.mkString(a, piece) catch return ref_mod.NIL) catch return ref_mod.NIL;
        }
    }
    return boxed.mkRrb(a, parts.items) catch ref_mod.NIL;
}
fn cStringJoin(vec: Ref, sep: Ref) callconv(.c) Ref {
    if (!strings.isString(sep)) return ref_mod.NIL;
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const s = strings.bytes(sep);
    const len = boxed.rrbLen(vec);
    var buf = std.ArrayList(u8){};
    defer buf.deinit(a);
    var i: usize = 0;
    while (i < len) : (i += 1) {
        if (i > 0) buf.appendSlice(a, s) catch return ref_mod.NIL;
        const e = boxed.rrbNth(vec, i);
        if (strings.isString(e)) buf.appendSlice(a, strings.bytes(e)) catch return ref_mod.NIL;
    }
    return strings.mkString(a, buf.items) catch ref_mod.NIL;
}
fn cStringReverse(r: Ref) callconv(.c) Ref {
    if (!strings.isString(r)) return ref_mod.NIL;
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const b = strings.bytes(r);
    const buf = a.alloc(u8, b.len) catch return ref_mod.NIL;
    defer a.free(buf);
    for (b, 0..) |c, i| buf[b.len - 1 - i] = c;
    return strings.mkString(a, buf) catch ref_mod.NIL;
}

fn cCurrentValueAsOf(e: u64, a: u64, as_of_tx: u64) callconv(.c) Ref {
    const s = send_mod.storePtr() orelse return ref_mod.NIL;
    return s.currentValueAsOf(e, a, as_of_tx) orelse ref_mod.NIL;
}
fn cCellHistory(e: u64, a: u64) callconv(.c) Ref {
    const s = send_mod.storePtr() orelse return ref_mod.NIL;
    return s.valueHistory(e, a) catch ref_mod.NIL;
}
fn cBasisNow() callconv(.c) u64 {
    const s = send_mod.storePtr() orelse return 0;
    return s.currentBasisT();
}

fn cStringLen(r: Ref) callconv(.c) u64 {
    if (!strings.isString(r)) return 0;
    return @intCast(strings.bytes(r).len);
}
fn cStringByteAt(r: Ref, i: u64) callconv(.c) Ref {
    if (!strings.isString(r)) return ref_mod.NIL;
    const b = strings.bytes(r);
    if (i >= b.len) return ref_mod.NIL;
    return ref_mod.mkSmallInt(b[@intCast(i)]) orelse ref_mod.NIL;
}
fn cStringConcat(x: Ref, y: Ref) callconv(.c) Ref {
    if (!strings.isString(x) or !strings.isString(y)) return ref_mod.NIL;
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const bx = strings.bytes(x);
    const by = strings.bytes(y);
    const buf = a.alloc(u8, bx.len + by.len) catch return ref_mod.NIL;
    defer a.free(buf);
    @memcpy(buf[0..bx.len], bx);
    @memcpy(buf[bx.len..], by);
    return strings.mkString(a, buf) catch ref_mod.NIL;
}
fn cStringEq(x: Ref, y: Ref) callconv(.c) bool {
    if (!strings.isString(x) or !strings.isString(y)) return false;
    return std.mem.eql(u8, strings.bytes(x), strings.bytes(y));
}
fn cStringAsSymbol(r: Ref) callconv(.c) Ref {
    if (!strings.isString(r)) return ref_mod.NIL;
    const i = g_interner orelse return ref_mod.NIL;
    const s = send_mod.storePtr() orelse return ref_mod.NIL;
    const t = send_mod.tablePtr() orelse return ref_mod.NIL;
    const eid = i.intern(s, t, strings.bytes(r)) catch return ref_mod.NIL;
    return ref_mod.mkEid(eid);
}

fn cMakeRange(lo: i64, hi: i64, step: i64) callconv(.c) Ref {
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    return boxed.mkRange(a, lo, hi, step) catch ref_mod.NIL;
}
fn cRangeLen(r: Ref) callconv(.c) u64 {
    if (!boxed.isRange(r)) return 0;
    return @intCast(boxed.rangeLen(r));
}
fn cRangeNth(r: Ref, i: u64) callconv(.c) Ref {
    if (!boxed.isRange(r)) return ref_mod.NIL;
    if (i >= boxed.rangeLen(r)) return ref_mod.NIL;
    return boxed.rangeNth(r, @intCast(i));
}

fn cMakeReduced(val: Ref) callconv(.c) Ref {
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    return boxed.mkReduced(a, val) catch ref_mod.NIL;
}
fn cIsReduced(r: Ref) callconv(.c) bool {
    return boxed.isReduced(r);
}
fn cReducedValue(r: Ref) callconv(.c) Ref {
    if (!boxed.isReduced(r)) return r;
    return boxed.reducedValue(r);
}

fn cSymbolName(eid: u64) callconv(.c) Ref {
    const i = g_interner orelse return ref_mod.NIL;
    const name = i.nameOf(eid) orelse return ref_mod.NIL;
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    return strings.mkString(a, name) catch ref_mod.NIL;
}

// -- Phase 16 Tier C: map primitives ---------------------------------------

fn cMakeMap() callconv(.c) Ref {
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    return boxed.mkMap(a) catch ref_mod.NIL;
}
fn cMapPut(map: Ref, key: Ref, val: Ref) callconv(.c) Ref {
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    if (!boxed.isMapRef(map)) return ref_mod.NIL;
    return boxed.mapPut(a, map, key, val) catch ref_mod.NIL;
}
fn cMapGet(map: Ref, key: Ref) callconv(.c) Ref {
    if (!boxed.isMapRef(map)) return ref_mod.NIL;
    return boxed.mapGet(map, key) orelse ref_mod.NIL;
}
fn cMapHas(map: Ref, key: Ref) callconv(.c) bool {
    if (!boxed.isMapRef(map)) return false;
    return boxed.mapGet(map, key) != null;
}
fn cMapLen(map: Ref) callconv(.c) u64 {
    if (!boxed.isMapRef(map)) return 0;
    return @intCast(boxed.mapLen(map));
}
fn cMapKeys(map: Ref) callconv(.c) Ref {
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    if (!boxed.isMapRef(map)) return ref_mod.NIL;
    return boxed.mapKeys(a, map) catch ref_mod.NIL;
}
fn cMapValues(map: Ref) callconv(.c) Ref {
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    if (!boxed.isMapRef(map)) return ref_mod.NIL;
    return boxed.mapValues(a, map) catch ref_mod.NIL;
}
fn cMapMerge(x: Ref, y: Ref) callconv(.c) Ref {
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    if (!boxed.isMapRef(x) or !boxed.isMapRef(y)) return ref_mod.NIL;
    return boxed.mapMerge(a, x, y) catch ref_mod.NIL;
}

// -- Phase 16 Tier A: vector primitives ------------------------------------

fn cMakeRrb(elems: [*]const Ref, n: usize) callconv(.c) Ref {
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    return boxed.mkRrb(a, elems[0..n]) catch ref_mod.NIL;
}
fn cRrbWith(vec: Ref, elem: Ref) callconv(.c) Ref {
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    if (!boxed.isRrbRef(vec)) return ref_mod.NIL;
    return boxed.rrbWith(a, vec, elem) catch ref_mod.NIL;
}
fn cRrbLen(vec: Ref) callconv(.c) u64 {
    if (!boxed.isRrbRef(vec)) return 0;
    return @intCast(boxed.rrbLen(vec));
}
fn cRrbNth(vec: Ref, idx: u64) callconv(.c) Ref {
    if (!boxed.isRrbRef(vec)) return ref_mod.NIL;
    if (idx >= boxed.rrbLen(vec)) return ref_mod.NIL;
    return boxed.rrbNth(vec, @intCast(idx));
}

// -- Phase 15.R R1d: snapshot object wrap/unwrap + db ops -------------------

/// Wrap a kernel `*Snapshot` in a SNAPSHOT_VT libid object `[vt][snap_ptr]`
/// (vt at -1, mirroring a closure) and register it in the object table.
/// Returns the object eid, or 0 on failure (snapshot freed by caller then).
fn makeSnapshotObj(snap: *fact_store.Snapshot) u64 {
    const a = send_mod.kernelAllocator() orelse return 0;
    const t = send_mod.tablePtr() orelse return 0;
    const total = 2 * @sizeOf(Ref);
    const buf = a.alignedAlloc(u8, .of(Ref), total) catch return 0;
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = ref_mod.mkEid(kernel_eids.SNAPSHOT_VT);
    const data_slot: *Ref = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    data_slot.* = ref_mod.mkPointer(@ptrCast(snap));
    const eid = t.alloc();
    t.put(eid, ref_mod.mkPointer(@ptrCast(data_slot))) catch return 0;
    return eid;
}

/// Recover the `*Snapshot` from a SNAPSHOT_VT object Ref (an eid or the payload
/// pointer). Returns null if `r` isn't a live snapshot object.
fn snapshotFromRef(r: Ref) ?*fact_store.Snapshot {
    var payload: Ref = r;
    if (ref_mod.isEid(r)) {
        const t = send_mod.tablePtr() orelse return null;
        payload = t.get(ref_mod.untagEid(r)) orelse return null;
    }
    if (!ref_mod.isPointer(payload)) return null;
    const data_slot: *Ref = @ptrCast(@alignCast(ref_mod.untagPointer(payload)));
    // vt sits one Ref before the payload slot.
    const vt_slot: *Ref = @ptrFromInt(@intFromPtr(data_slot) - @sizeOf(Ref));
    if (!ref_mod.isEid(vt_slot.*) or ref_mod.untagEid(vt_slot.*) != kernel_eids.SNAPSHOT_VT) return null;
    if (!ref_mod.isPointer(data_slot.*)) return null;
    return @ptrCast(@alignCast(ref_mod.untagPointer(data_slot.*)));
}

fn cSnapshotNow() callconv(.c) Ref {
    const s = send_mod.storePtr() orelse return ref_mod.NIL;
    const snap = s.snapshotNow() catch return ref_mod.NIL;
    const eid = makeSnapshotObj(snap);
    if (eid == 0) {
        snap.deinit(s.allocator);
        return ref_mod.NIL;
    }
    return ref_mod.mkEid(eid);
}

fn cSnapshotAsOf(basis: u64) callconv(.c) Ref {
    const s = send_mod.storePtr() orelse return ref_mod.NIL;
    const snap = s.snapshotAsOf(basis) catch return ref_mod.NIL;
    const eid = makeSnapshotObj(snap);
    if (eid == 0) {
        snap.deinit(s.allocator);
        return ref_mod.NIL;
    }
    return ref_mod.mkEid(eid);
}

fn cSnapshotGet(snap_ref: Ref, e: u64, a: u64) callconv(.c) Ref {
    const snap = snapshotFromRef(snap_ref) orelse return ref_mod.NIL;
    return snap.get(e, a) orelse ref_mod.NIL;
}

fn cSnapshotBasis(snap_ref: Ref) callconv(.c) Ref {
    const snap = snapshotFromRef(snap_ref) orelse return ref_mod.NIL;
    return ref_mod.mkSmallInt(@intCast(snap.basis_t)) orelse ref_mod.NIL;
}

fn cSnapshotRelease(snap_ref: Ref) callconv(.c) void {
    const s = send_mod.storePtr() orelse return;
    const snap = snapshotFromRef(snap_ref) orelse return;
    snap.deinit(s.allocator); // unpin epoch + free the Snapshot
    // The SNAPSHOT_VT object pair itself leaks until value-heap GC reclaims it;
    // the epoch pin (the costly resource) is released here.
}

fn cSetRetention(code: u32, arg: u64) callconv(.c) void {
    const s = send_mod.storePtr() orelse return;
    s.setRetention(switch (code) {
        1 => .keep_all,
        2 => .{ .keep_since_basis = arg },
        3 => .{ .keep_last_n = arg },
        else => .current_only,
    });
}

fn cExciseEntity(eid: u64) callconv(.c) bool {
    const s = send_mod.storePtr() orelse return false;
    const a = send_mod.kernelAllocator() orelse return false;
    s.exciseEntity(eid, a) catch return false;
    return true;
}

const boxed = @import("boxed.zig");
const compiler_mod = @import("compiler.zig");
const kernel_eids_mod = @import("kernel_eids.zig");

fn cMakeFloat(value: f64) callconv(.c) Ref {
    const a = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    return boxed.mkFloat(a, value) catch ref_mod.NIL;
}

fn cSendN(receiver: Ref, sel: u64, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    return send_mod.sendN(receiver, sel, args, n_args);
}

fn cObjectAtEid(eid: u64) callconv(.c) Ref {
    const t = send_mod.tablePtr() orelse return ref_mod.NIL;
    return t.get(eid) orelse ref_mod.NIL;
}

fn cCompileBlockAsMethod(block_ast_eid: u64) callconv(.c) u64 {
    const allocator = send_mod.kernelAllocator() orelse return 0;
    const store = send_mod.storePtr() orelse return 0;
    const table = send_mod.tablePtr() orelse return 0;

    const vt_ref = store.currentValue(block_ast_eid, kernel_eids_mod.ATTR_VTABLE) orelse return 0;
    if (!ref_mod.isEid(vt_ref)) return 0;
    if (ref_mod.untagEid(vt_ref) != kernel_eids_mod.AST_BLOCK_VT) return 0;

    var m = compiler_mod.compileBlockAsMethod(allocator, store, block_ast_eid) catch return 0;
    defer m.deinit(allocator);
    // Phase 12.0 provenance split: user-compiled methods are runtime DATA
    // (persisted by the image), not module code. Tag under M_USER so the
    // snapshot captures them and module unload can't retract them.
    return compiler_mod.materialize(allocator, store, table, &m, kernel_eids_mod.M_USER) catch 0;
}

/// Storage for the interner so the C wrappers can find it. Shell calls this
/// after bootstrap; modules see the API through `populate()` which closes
/// over this state.
var g_interner: ?*interner_mod.Interner = null;

pub fn setInterner(i: *interner_mod.Interner) void {
    g_interner = i;
}

pub fn internerPtr() ?*interner_mod.Interner {
    return g_interner;
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
    // Tree-walker block closures are the redefine/block churn leak: route them
    // to the swept `closures_vt` arena when the registry is live (Phase 15.A).
    const a = if (arenas.ready()) arenas.closuresVt() else (send_mod.kernelAllocator() orelse return 0);
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

const saved_query_mod = @import("saved_query.zig");

fn cSaveQuery(name_ptr: [*]const u8, name_len: usize, q_ptr: ?*anyopaque) callconv(.c) u64 {
    const q: *query.Query = @ptrCast(@alignCast(q_ptr orelse return 0));
    const i = g_interner orelse return 0;
    const s = send_mod.storePtr() orelse return 0;
    const t = send_mod.tablePtr() orelse return 0;
    const name = name_ptr[0..name_len];
    const name_eid = i.intern(s, t, name) catch return 0;
    return saved_query_mod.saveQuery(s, t, name_eid, q) catch 0;
}

fn cRunQuery(name_ptr: [*]const u8, name_len: usize) callconv(.c) ?*anyopaque {
    const i = g_interner orelse return null;
    const s = send_mod.storePtr() orelse return null;
    const t = send_mod.tablePtr() orelse return null;
    const a = send_mod.kernelAllocator() orelse return null;
    const name = name_ptr[0..name_len];
    const name_eid = i.intern(s, t, name) catch return null;
    const q = saved_query_mod.lookupQuery(s, name_eid) orelse return null;
    const r = a.create(query.Result) catch return null;
    r.* = query.execute(q, s) catch {
        a.destroy(r);
        return null;
    };
    return @ptrCast(r);
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

// -- Phase 7.4: predicate / function clauses + registration -----------------

const predicate_mod = @import("predicate.zig");

fn cQueryAddPredCall(
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
    q.addPredCall(name_eid, terms) catch {};
}

fn cQueryAddFnCall(
    p: ?*anyopaque,
    name_eid: u64,
    n_args: usize,
    arg_kinds: [*]const u32,
    arg_vals: [*]const u64,
    target: u32,
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
    q.addFnCall(name_eid, terms, target) catch {};
}

fn cRegisterPred(name_eid: u64, impl_ptr: ?*const anyopaque) callconv(.c) u64 {
    const ip = impl_ptr orelse return 0;
    const s = send_mod.storePtr() orelse return 0;
    const t = send_mod.tablePtr() orelse return 0;
    const a = send_mod.kernelAllocator() orelse return 0;
    const f: predicate_mod.NativePredFn = @ptrCast(@alignCast(ip));
    return predicate_mod.definePredNative(a, s, t, name_eid, f) catch 0;
}

fn cRegisterFn(name_eid: u64, impl_ptr: ?*const anyopaque) callconv(.c) u64 {
    const ip = impl_ptr orelse return 0;
    const s = send_mod.storePtr() orelse return 0;
    const t = send_mod.tablePtr() orelse return 0;
    const a = send_mod.kernelAllocator() orelse return 0;
    const f: predicate_mod.NativeFnFn = @ptrCast(@alignCast(ip));
    return predicate_mod.defineFnNative(a, s, t, name_eid, f) catch 0;
}

// -- Phase 8: promotion + FFI pin -------------------------------------------

const promotion_mod = @import("promotion.zig");

fn cPromote_8(eid: u64) callconv(.c) bool {
    promotion_mod.promote(eid) catch return false;
    return true;
}

fn cDemote_8(eid: u64) callconv(.c) void {
    promotion_mod.demote(eid);
}

fn cHotnessOf(eid: u64) callconv(.c) u32 {
    const t = send_mod.tablePtr() orelse return 0;
    return t.hotnessOf(eid);
}

fn cIsMaterialized(eid: u64) callconv(.c) bool {
    return promotion_mod.isMaterialized(eid);
}

fn cFfiEnter() callconv(.c) void {
    promotion_mod.ffiEnter();
}

fn cFfiExit() callconv(.c) void {
    promotion_mod.ffiExit();
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

const peg_mod = @import("peg.zig");

fn cPegInstallSendApi(module_eid: u64) callconv(.c) void {
    const a = send_mod.kernelAllocator() orelse return;
    const s = send_mod.storePtr() orelse return;
    const t = send_mod.tablePtr() orelse return;
    const i = g_interner orelse return;
    peg_mod.installSendApi(a, s, t, i, module_eid) catch {};
}

fn cPegMkSeq(children: [*]const u64, count: usize) callconv(.c) u64 {
    const eng = peg_mod.installedEngine() orelse return 0;
    return peg_mod.mkSeq(eng, children[0..count]) catch 0;
}

fn cPegMkChoice(children: [*]const u64, count: usize) callconv(.c) u64 {
    const eng = peg_mod.installedEngine() orelse return 0;
    return peg_mod.mkChoice(eng, children[0..count]) catch 0;
}

/// Charset ranges encoded as flat u8 pairs: `[lo0, hi0, lo1, hi1, ...]`,
/// `count` = total length of the buffer (must be even).
fn cPegMkCharset(ranges: [*]const u8, count: usize) callconv(.c) u64 {
    if (count == 0 or (count & 1) != 0) return 0;
    const eng = peg_mod.installedEngine() orelse return 0;
    const allocator = send_mod.kernelAllocator() orelse return 0;
    const n_ranges = count / 2;
    const buf = allocator.alloc(peg_mod.Range, n_ranges) catch return 0;
    defer allocator.free(buf);
    for (0..n_ranges) |k| {
        buf[k] = .{ .lo = ranges[2 * k], .hi = ranges[2 * k + 1] };
    }
    return peg_mod.mkCharset(eng, buf) catch 0;
}
