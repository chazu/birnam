//! Kernel bootstrap: brings up the id object model atop the fact store.
//!
//! Stages (VISION.md §Bootstrap, Phase 4 design):
//!   1. Allocate kernel vtable structs (zero user slots each).
//!   2. Wire vt_ref at offset -1 of every kernel vtable to mkEid(VTABLE_VT).
//!      vtable_vt's own -1 slot resolves to itself (fixed-point).
//!   3. Register every kernel-vtable pointer Ref in the object table.
//!   4. Assert each kernel vtable's `:vtable` fact (so bind's cold path
//!      can dispatch on EID-tagged refs to vtables).
//!   5. Assert `:delegate` facts for the kernel delegation chain.
//!      Absence of a `:delegate` fact = top of chain (Date discipline).
//!   6. Assert `[NIL :vtable NIL_VT]` so `send`-to-NIL is legal.
//!   7. For each kernel method (vt, sel, fn): allocate a Closure struct,
//!      mint EID, register, assert the method fact.
//!   8. initDispatch sets the send.zig globals so bind can run.

const std = @import("std");
const ref_mod = @import("ref.zig");
const object = @import("object.zig");
const closure_mod = @import("closure.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const kernel_eids = @import("kernel_eids.zig");
const send_mod = @import("send.zig");
const mcache = @import("mcache.zig");
const interner_mod = @import("interner.zig");
const rule_mod = @import("rule.zig");
const id_api_mod = @import("id_api.zig");

const Ref = ref_mod.Ref;
const Closure = closure_mod.Closure;
const Send = send_mod.Send;

/// Kernel handles, returned for caller convenience and testing.
pub const Kernel = struct {
    vtable_vt_ref: Ref,
    object_vt_ref: Ref,
    selector_vt_ref: Ref,
    closure_vt_ref: Ref,
    vector_vt_ref: Ref,
    assoc_vt_ref: Ref,
    nil_vt_ref: Ref,
};

/// Bootstrap the kernel. After this call:
///   - send / bind are wired and operational
///   - kernel vtables are materialized + registered
///   - kernel methods are installed and dispatchable
///   - kernel selectors carry `:name` facts so `intern("lookup:") -> SEL_LOOKUP`
/// Module-global rule registry. The shell creates one and the bootstrap
/// hands it to the IdApi layer. Modules add rules via `api.define_rule`.
pub fn boot(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
) !Kernel {
    // Phase 7.1: provision a kernel-side rule registry for IdApi.
    const registry = try allocator.create(rule_mod.Registry);
    registry.* = rule_mod.Registry.init(allocator);
    id_api_mod.setRegistry(registry);
    // Wire mcache invalidation hook into the fact store.
    store.on_attr_mutate = &mcache.flushSel;

    // -- Stage 1+2+3: allocate kernel vtables, set -1, register ---------
    const vt_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.VTABLE_VT, object.toRef(vt_vt_ptr));

    const obj_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.OBJECT_VT, object.toRef(obj_vt_ptr));

    const sel_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.SELECTOR_VT, object.toRef(sel_vt_ptr));

    const clo_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.CLOSURE_VT, object.toRef(clo_vt_ptr));

    const vec_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.VECTOR_VT, object.toRef(vec_vt_ptr));

    const ass_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.ASSOC_VT, object.toRef(ass_vt_ptr));

    const nil_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.NIL_VT, object.toRef(nil_vt_ptr));

    const rrb_vt_ptr = try allocVtStruct(allocator, kernel_eids.VTABLE_VT);
    try table.put(kernel_eids.RRB_VT, object.toRef(rrb_vt_ptr));

    // -- Stage 4: assert :vtable facts for every entity bind may dispatch on
    // Each kernel vtable's :vtable is VTABLE_VT (so sending a message to
    // mkEid(SOME_VT) lands in vtable_vt's method table).
    const m_boot = kernel_eids.M_BOOTSTRAP;
    const vtv = ref_mod.mkEid(kernel_eids.VTABLE_VT);
    _ = try store.assert(kernel_eids.VTABLE_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.OBJECT_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.SELECTOR_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.CLOSURE_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.VECTOR_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.ASSOC_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.NIL_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);
    _ = try store.assert(kernel_eids.RRB_VT, kernel_eids.ATTR_VTABLE, vtv, m_boot);

    // -- Stage 5: delegation chain ---------------------------------------
    // Top of chain (object_vt) has NO :delegate fact — Date: absence = no parent.
    // Everyone else delegates to object_vt.
    const ovt = ref_mod.mkEid(kernel_eids.OBJECT_VT);
    _ = try store.assert(kernel_eids.VTABLE_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.SELECTOR_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.CLOSURE_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.VECTOR_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.ASSOC_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.NIL_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);
    _ = try store.assert(kernel_eids.RRB_VT, kernel_eids.ATTR_DELEGATE, ovt, m_boot);

    // -- Stage 6: NIL's :vtable ------------------------------------------
    _ = try store.assert(kernel_eids.NIL, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.NIL_VT), m_boot);

    // -- Stage 7: install kernel methods ---------------------------------
    try installMethod(allocator, store, table, kernel_eids.OBJECT_VT, kernel_eids.SEL_VTABLE, @ptrCast(&objectVtableImpl));
    try installMethod(allocator, store, table, kernel_eids.OBJECT_VT, kernel_eids.SEL_INIT, @ptrCast(&objectInitImpl));
    try installMethod(allocator, store, table, kernel_eids.VTABLE_VT, kernel_eids.SEL_LOOKUP, @ptrCast(&vtableLookupImpl));
    try installMethod(allocator, store, table, kernel_eids.VTABLE_VT, kernel_eids.SEL_DELEGATED, @ptrCast(&vtableDelegatedImpl));
    try installMethod(allocator, store, table, kernel_eids.VTABLE_VT, kernel_eids.SEL_METHOD_AT_PUT_WITH, @ptrCast(&vtableMethodAtPutWithImpl));

    // -- Stage 8: seed selector names ------------------------------------
    try interner.seed(store, kernel_eids.SEL_LOOKUP, "lookup:");
    try interner.seed(store, kernel_eids.SEL_METHOD_AT_PUT_WITH, "methodAt:put:with:");
    try interner.seed(store, kernel_eids.SEL_DELEGATED, "delegated");
    try interner.seed(store, kernel_eids.SEL_INIT, "init");
    try interner.seed(store, kernel_eids.SEL_ALLOCATE, "allocate");
    try interner.seed(store, kernel_eids.SEL_VTABLE, "vtable");
    try interner.seed(store, kernel_eids.ATTR_SAVED_QUERY_NAME, "saved-query-name");
    try interner.seed(store, kernel_eids.ATTR_SAVED_QUERY_SPEC, "saved-query-spec");

    // -- Stage 9: enable dispatch ----------------------------------------
    send_mod.initDispatch(store, table, allocator);

    return .{
        .vtable_vt_ref = object.toRef(vt_vt_ptr),
        .object_vt_ref = object.toRef(obj_vt_ptr),
        .selector_vt_ref = object.toRef(sel_vt_ptr),
        .closure_vt_ref = object.toRef(clo_vt_ptr),
        .vector_vt_ref = object.toRef(vec_vt_ptr),
        .assoc_vt_ref = object.toRef(ass_vt_ptr),
        .nil_vt_ref = object.toRef(nil_vt_ptr),
    };
}

// -- Allocation helpers -------------------------------------------------------

fn allocVtStruct(allocator: std.mem.Allocator, vt_eid: u64) !object.ObjPtr {
    _ = vt_eid;
    // Every kernel vtable has 0 user slots — its content is fact-shaped.
    // -1 slot is set to mkEid(VTABLE_VT): all kernel vtables share the metavtable.
    return object.allocObject(allocator, ref_mod.mkEid(kernel_eids.VTABLE_VT), 0);
}

/// Allocate a Closure as a libid-style object: layout `[vt_ref][method][data]`,
/// where vt_ref = mkEid(CLOSURE_VT) at offset -1. Closure pointer is at offset 0.
fn allocClosure(allocator: std.mem.Allocator, method_fn: *const anyopaque, data_ref: Ref) !*Closure {
    const total = 3 * @sizeOf(Ref);
    const buf = try allocator.alignedAlloc(u8, .of(Ref), total);
    @memset(buf, 0);
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = ref_mod.mkEid(kernel_eids.CLOSURE_VT);
    const c: *Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    c.method = method_fn;
    c.data = data_ref;
    return c;
}

fn installMethod(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    vt_eid: u64,
    sel_eid: u64,
    method_fn: *const anyopaque,
) !void {
    const c = try allocClosure(allocator, method_fn, ref_mod.NIL);
    const closure_eid = table.alloc();
    try table.put(closure_eid, ref_mod.mkPointer(@ptrCast(c)));
    _ = try store.assert(vt_eid, sel_eid, ref_mod.mkEid(closure_eid), kernel_eids.M_BOOTSTRAP);
}

// -- Kernel method implementations -------------------------------------------

/// `[obj vtable]` — return the receiver's vtable Ref.
/// Works for pointer receivers (reads -1) and EID receivers (queries :vtable).
fn objectVtableImpl(s: *Send) callconv(.c) Ref {
    if (ref_mod.isPointer(s.receiver)) {
        return object.vtableOf(object.fromRef(s.receiver));
    }
    if (ref_mod.isEid(s.receiver)) {
        // The caller already routed through bind, which proved a vt exists.
        // Re-derive it.
        const store = send_mod.storePtr() orelse return ref_mod.NIL;
        const e = ref_mod.untagEid(s.receiver);
        return store.currentValue(e, kernel_eids.ATTR_VTABLE) orelse ref_mod.NIL;
    }
    return ref_mod.NIL;
}

/// `[obj init]` — identity.
fn objectInitImpl(s: *Send) callconv(.c) Ref {
    return s.receiver;
}

/// `[vt lookup: sel]` — return the method Ref currently bound to `sel` on `vt`,
/// or NIL if absent. Walks the delegation chain, mirroring `bind`'s cold path.
fn vtableLookupImpl(s: *Send, sel_ref: Ref) callconv(.c) Ref {
    if (!ref_mod.isEid(s.receiver) or !ref_mod.isEid(sel_ref)) return ref_mod.NIL;
    const sel_eid = ref_mod.untagEid(sel_ref);
    var cur_vt = ref_mod.untagEid(s.receiver);
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    while (cur_vt != kernel_eids.NIL) {
        if (store.currentValue(cur_vt, sel_eid)) |m| return m;
        if (store.currentValue(cur_vt, kernel_eids.ATTR_DELEGATE)) |d| {
            if (!ref_mod.isEid(d)) return ref_mod.NIL;
            cur_vt = ref_mod.untagEid(d);
        } else return ref_mod.NIL;
    }
    return ref_mod.NIL;
}

/// `[vt delegated]` — allocate a fresh child vtable delegating to `vt`.
/// Returns the new vtable's EID Ref.
fn vtableDelegatedImpl(s: *Send) callconv(.c) Ref {
    if (!ref_mod.isEid(s.receiver)) return ref_mod.NIL;
    const parent_eid = ref_mod.untagEid(s.receiver);
    const allocator = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    const table = send_mod.tablePtr() orelse return ref_mod.NIL;

    const new_struct = allocVtStruct(allocator, 0) catch return ref_mod.NIL;
    const new_eid = table.alloc();
    table.put(new_eid, object.toRef(new_struct)) catch return ref_mod.NIL;

    // The child's :vtable is VTABLE_VT; its :delegate is the parent.
    _ = store.assert(new_eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.VTABLE_VT), kernel_eids.M_BOOTSTRAP) catch return ref_mod.NIL;
    _ = store.assert(new_eid, kernel_eids.ATTR_DELEGATE, ref_mod.mkEid(parent_eid), kernel_eids.M_BOOTSTRAP) catch return ref_mod.NIL;

    return ref_mod.mkEid(new_eid);
}

/// `[vt methodAt: sel put: method_ref with: data]` — install a method on `vt`.
/// `method_ref` must already be an EID Ref to a registered closure. `data` is
/// ignored in this Phase 4 stub (closures carry their own data at allocation).
fn vtableMethodAtPutWithImpl(s: *Send, sel_ref: Ref, method_ref: Ref, data_ref: Ref) callconv(.c) Ref {
    _ = data_ref;
    if (!ref_mod.isEid(s.receiver) or !ref_mod.isEid(sel_ref) or !ref_mod.isEid(method_ref)) return ref_mod.NIL;
    const vt_eid = ref_mod.untagEid(s.receiver);
    const sel_eid = ref_mod.untagEid(sel_ref);
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    _ = store.assert(vt_eid, sel_eid, method_ref, kernel_eids.M_BOOTSTRAP) catch return ref_mod.NIL;
    return method_ref;
}
