const std = @import("std");
const obj = @import("object.zig");
const vtable_ops = @import("vtable.zig");
const selector = @import("selector.zig");
const send_mod = @import("send.zig");
const mcache = @import("mcache.zig");
const Oop = obj.Oop;

// -- Exported bootstrap objects (accessible after init) ---------------------

pub var vtable_vt: Oop = null;
pub var object_vt: Oop = null;
pub var vector_vt: Oop = null;
pub var selector_vt: Oop = null;
pub var closure_vt: Oop = null;
pub var assoc_vt: Oop = null;

pub var object_proto: Oop = null;
pub var vtable_proto: Oop = null;
pub var vector_proto: Oop = null;
pub var selector_proto: Oop = null;
pub var closure_proto: Oop = null;
pub var assoc_proto: Oop = null;

// Interned selectors
pub var s_lookup: Oop = null;
pub var s_methodAt_put_with: Oop = null;
pub var s_allocate: Oop = null;
pub var s_delegated: Oop = null;
pub var s_vtable: Oop = null;
pub var s_delegate: Oop = null;
pub var s_delegated_obj: Oop = null;
pub var s_init: Oop = null;
pub var s_intern: Oop = null;

/// Initialize the id object system. Follows _libid_init sequence from libid.c.
pub fn init() bool {
    // ---- Phase 1: Raw allocation (no send, no interning yet) ----

    // 1. Create vtable_vt — the metacircular fixed point
    vtable_vt = allocVtableRaw(null);
    if (vtable_vt == null) return false;
    // Point vtable_vt's vtable at itself
    obj.setVtable(@ptrCast(@alignCast(obj.asVtable(vtable_vt).?)), vtable_vt);

    // 2. Create vector_vt so we can properly type binding vectors
    vector_vt = allocVtableRaw(null);
    if (vector_vt == null) return false;

    // Now set vector vtable on existing binding vectors
    vtable_ops.setVectorVt(vector_vt);
    fixupBindingVectors(vtable_vt);
    fixupBindingVectors(vector_vt);

    // 3. Create object_vt — root of delegation
    object_vt = allocVtableRaw(null);
    if (object_vt == null) return false;

    // Wire delegation
    setDelegate(vtable_vt, object_vt);
    setDelegate(vector_vt, object_vt);

    // 4. Create selector_Table and object_Table (vtables used as storage)
    const selector_table = allocVtableRaw(null);
    if (selector_table == null) return false;
    const object_table = allocVtableRaw(null);
    if (object_table == null) return false;

    // 5. Create prototype objects
    object_proto = obj.toOop(obj.alloc(object_vt, 0) orelse return false);
    vtable_proto = obj.toOop(obj.alloc(vtable_vt, @sizeOf(obj.Vtable)) orelse return false);

    // Init vtable_proto's fields
    if (obj.asVtable(vtable_proto)) |vp| {
        vp.tally = 0;
        vp.bindings = null;
        vp.delegate = null;
    }

    vector_proto = obj.toOop(obj.alloc(vector_vt, @sizeOf(obj.Vector)) orelse return false);
    if (obj.asVector(vector_proto)) |vp| {
        vp.size = 0;
    }

    // 6. Create assoc, selector, closure prototypes via delegation
    //    (like libid.c's _object___delegated)
    assoc_vt = vtable_ops.vtableDelegated(object_vt);
    if (assoc_vt == null) return false;
    assoc_proto = obj.toOop(obj.alloc(assoc_vt, @sizeOf(obj.Assoc)) orelse return false);

    selector_vt = vtable_ops.vtableDelegated(object_vt);
    if (selector_vt == null) return false;
    selector_proto = obj.toOop(obj.alloc(selector_vt, @sizeOf(obj.Selector)) orelse return false);

    closure_vt = vtable_ops.vtableDelegated(object_vt);
    if (closure_vt == null) return false;
    closure_proto = obj.toOop(obj.alloc(closure_vt, @sizeOf(obj.Closure)) orelse return false);

    // Wire up vtable_ops with type vtables it needs for alloc
    vtable_ops.setAssocVt(assoc_vt);
    vtable_ops.setClosureVt(closure_vt);

    // ---- Phase 2: Selector interning (direct, no send) ----

    selector.init(selector_vt, selector_proto, selector_table);

    s_methodAt_put_with = selector.intern("methodAt:put:with:");
    s_lookup = selector.intern("lookup:");
    s_allocate = selector.intern("allocate:");
    s_delegated = selector.intern("delegated");
    s_vtable = selector.intern("_vtable");
    s_delegate = selector.intern("_delegate");
    s_delegated_obj = selector.intern("_delegated");
    s_init = selector.intern("init");
    s_intern = selector.intern("_intern:");

    if (s_methodAt_put_with == null or s_lookup == null) return false;

    // ---- Phase 3: Install bootstrap methods directly (no send) ----

    // methodAt:put:with: on vtable_vt (needed first so we can install more)
    _ = vtable_ops.vtableMethodAtPutWith(vtable_vt, s_methodAt_put_with, @ptrCast(&vtable_ops.methodAtPutWithMethod), null);

    // lookup: on vtable_vt
    _ = vtable_ops.vtableMethodAtPutWith(vtable_vt, s_lookup, @ptrCast(&vtable_ops.lookupMethod), null);

    // allocate: on vtable_vt
    _ = vtable_ops.vtableMethodAtPutWith(vtable_vt, s_allocate, @ptrCast(&vtable_ops.allocateMethod), null);

    // delegated on vtable_vt
    _ = vtable_ops.vtableMethodAtPutWith(vtable_vt, s_delegated, @ptrCast(&vtable_ops.delegatedMethod), null);

    // _vtable on object_vt
    _ = vtable_ops.vtableMethodAtPutWith(object_vt, s_vtable, @ptrCast(&vtable_ops.vtableAccessor), null);

    // _delegate on object_vt
    _ = vtable_ops.vtableMethodAtPutWith(object_vt, s_delegate, @ptrCast(&vtable_ops.delegateAccessor), null);

    // _delegated on object_vt
    _ = vtable_ops.vtableMethodAtPutWith(object_vt, s_delegated_obj, @ptrCast(&vtable_ops.objectDelegated), null);

    // init on object_vt
    _ = vtable_ops.vtableMethodAtPutWith(object_vt, s_init, @ptrCast(&vtable_ops.objectInit), null);

    // Also install lookup: on object_vt and selector_vt so delegation works
    _ = vtable_ops.vtableMethodAtPutWith(object_vt, s_lookup, @ptrCast(&vtable_ops.lookupMethod), null);

    // ---- Phase 4: Enable send dispatch ----

    send_mod.initDispatch(s_lookup, vtable_vt);

    // ---- Phase 5: Verify with a test send ----
    // send(vtable_vt, s_lookup, s_lookup) should return the assoc for lookup:
    const test_result = send_mod.send1(vtable_vt, s_lookup, s_lookup);
    if (test_result == null) return false;

    return true;
}

// -- Raw bootstrap helpers (no send, no interning) --------------------------

/// Allocate a raw vtable with an initial bindings vector. No vtable pointer
/// set on the vtable itself yet (caller must fix up).
fn allocVtableRaw(delegate: Oop) Oop {
    // For the very first vtables, vector_vt might not exist yet.
    // Use vtable_vt as a placeholder (or null for the very first).
    const vt_for_vec = if (vector_vt != null) vector_vt else vtable_vt;
    const vec_obj = obj.alloc(vt_for_vec, @sizeOf(obj.Vector) + @sizeOf(Oop) * 4) orelse return null;
    const vec = obj.asVector(obj.toOop(vec_obj)).?;
    vec.size = 4;

    const vt_obj = obj.alloc(if (vtable_vt != null) vtable_vt else null, @sizeOf(obj.Vtable)) orelse return null;
    const vt = obj.asVtable(obj.toOop(vt_obj)).?;
    vt.tally = 0;
    vt.bindings = obj.toOop(vec_obj);
    vt.delegate = delegate;

    return obj.toOop(vt_obj);
}

/// Set the delegate field of a vtable.
fn setDelegate(vtable: Oop, delegate: Oop) void {
    if (obj.asVtable(vtable)) |vt| {
        vt.delegate = delegate;
    }
}

/// Fix up the vtable pointer on a vtable's bindings vector to use vector_vt.
fn fixupBindingVectors(vtable: Oop) void {
    if (obj.asVtable(vtable)) |vt| {
        if (vt.bindings) |bindings| {
            obj.setVtable(@ptrCast(@alignCast(bindings)), vector_vt);
        }
    }
}
