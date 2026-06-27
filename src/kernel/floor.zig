//! The struct-resident metaobject graph for FLOOR vtables (W7 Track B, B0).
//!
//! docs/30 §6 (per-layer primacy): the floor — the well-known kernel vtables and
//! their *kernel-time* delegation + methods — must be able to dispatch with no
//! facts in existence, so the fact layer is a true superimposition. This module
//! holds that graph in the vtable object's own slots (Piumarta-faithful), B0:
//! **dumb and fast** — a `parent` ref + a small inline open-addressed
//! `(selector → method)` array, no HAMT.
//!
//! Layout over `object.zig`'s uniform-Ref slots (allocated by `allocFloorVt`):
//!   slot 0        : parent Ref (NIL = top of the delegation chain)
//!   slot 1        : dict entry count (SmallInt Ref)
//!   slot 2 + 2i   : selector eid Ref (mkEid)   ┐ kernel-method dict
//!   slot 3 + 2i   : method Ref (mkEid closure)  ┘ linear scan, tiny N
//!
//! Identity: floor vtables are the materialized kernel vtables (eid < 256). A
//! fixed `[256]?ObjPtr` registry — populated only by `allocFloorVt` — is the
//! reliable "is this a floor-layout vtable" test (a bare eid-range check would
//! misread non-materialized kernel vtables like RANGE_VT that carry `:delegate`
//! facts but have no struct).
//!
//! `send.resolve` consults the fact store FIRST (a fact is an override/addition
//! over the frozen floor) and this struct graph as the fallback — so the floor's
//! kernel delegation + methods survive even with their facts retracted.

const std = @import("std");
const ref_mod = @import("ref.zig");
const object = @import("object.zig");
const kernel_eids = @import("kernel_eids.zig");

const Ref = ref_mod.Ref;
const ObjPtr = object.ObjPtr;

/// Max kernel-time methods on a single floor vtable. VTABLE_VT carries 3
/// (lookup: / delegated / methodAt:put:with:); OBJECT_VT 2. 6 leaves headroom.
pub const DICT_CAP: usize = 6;
pub const N_SLOTS: usize = 2 + 2 * DICT_CAP;

const SLOT_PARENT: usize = 0;
const SLOT_COUNT: usize = 1;

/// eid → materialized floor-vtable struct (kernel eids only, < 256). Single-image
/// global, mirroring the `send.g_store` convention; re-populated each boot.
var g_floor: [kernel_eids.USER_EID_START]?ObjPtr = .{null} ** kernel_eids.USER_EID_START;

/// Reset the registry (call at boot start, before allocating floor vtables).
pub fn resetRegistry() void {
    g_floor = .{null} ** kernel_eids.USER_EID_START;
}

pub fn register(eid: u64, obj: ObjPtr) void {
    if (eid < kernel_eids.USER_EID_START) g_floor[eid] = obj;
}

fn objFor(eid: u64) ?ObjPtr {
    if (eid >= kernel_eids.USER_EID_START) return null;
    return g_floor[eid];
}

/// Initialize the floor slots of a freshly allocated `N_SLOTS` vtable object.
pub fn initSlots(obj: ObjPtr) void {
    object.setSlot(obj, SLOT_PARENT, ref_mod.NIL);
    object.setSlot(obj, SLOT_COUNT, ref_mod.mkSmallInt(0).?);
}

fn setParent(obj: ObjPtr, parent: Ref) void {
    object.setSlot(obj, SLOT_PARENT, parent);
}

/// Mirror each floor vtable's `:delegate` fact into its struct parent slot.
/// Called at the identity-layer mount, after all delegation facts are asserted
/// (dual-write: the struct becomes the fallback for the fact). Absence of a
/// `:delegate` fact leaves the parent NIL — top of the chain (Date discipline).
pub fn syncParentsFromStore(store: anytype) void {
    var eid: u64 = 0;
    while (eid < kernel_eids.USER_EID_START) : (eid += 1) {
        const obj = g_floor[eid] orelse continue;
        if (store.currentValue(eid, kernel_eids.ATTR_DELEGATE)) |p| setParent(obj, p);
    }
}

fn putMethod(obj: ObjPtr, sel_eid: u64, method: Ref) void {
    const n: usize = @intCast(ref_mod.untagSmallInt(object.slotAt(obj, SLOT_COUNT)));
    var i: usize = 0;
    while (i < n) : (i += 1) {
        const s = object.slotAt(obj, 2 + 2 * i);
        if (ref_mod.isEid(s) and ref_mod.untagEid(s) == sel_eid) {
            object.setSlot(obj, 3 + 2 * i, method); // overwrite
            return;
        }
    }
    if (n >= DICT_CAP) @panic("floor.putMethod: kernel-method dict overflow (raise DICT_CAP)");
    object.setSlot(obj, 2 + 2 * n, ref_mod.mkEid(sel_eid));
    object.setSlot(obj, 3 + 2 * n, method);
    object.setSlot(obj, SLOT_COUNT, ref_mod.mkSmallInt(@intCast(n + 1)).?);
}

pub fn putMethodByEid(vt_eid: u64, sel_eid: u64, method: Ref) void {
    if (objFor(vt_eid)) |obj| putMethod(obj, sel_eid, method);
}

/// The struct-resident cold-path read for the floor: delegation parent +
/// kernel-method lookup. Returns null if `entity` is not a floor vtable, or has
/// no such edge (the caller then has its definitive answer — no fact for it).
pub fn resolve(entity: u64, attr: u64) ?Ref {
    const obj = objFor(entity) orelse return null;
    if (attr == kernel_eids.ATTR_DELEGATE) {
        const p = object.slotAt(obj, SLOT_PARENT);
        return if (ref_mod.isNil(p)) null else p;
    }
    // attr is a selector eid — scan the kernel-method dict.
    const n: usize = @intCast(ref_mod.untagSmallInt(object.slotAt(obj, SLOT_COUNT)));
    var i: usize = 0;
    while (i < n) : (i += 1) {
        const s = object.slotAt(obj, 2 + 2 * i);
        if (ref_mod.isEid(s) and ref_mod.untagEid(s) == attr) {
            return object.slotAt(obj, 3 + 2 * i);
        }
    }
    return null;
}
