//! Phase 9: boxed immediate types stored as pointer-tagged Refs.
//!
//! `Float` and `FfiPtr` need more bits than a 61-bit immediate can hold
//! (full f64 mantissa; full 64-bit C address). Strategy: allocate a small
//! `[vt_ref][payload]` struct, pointer-tag its slot-0 address. Reads check
//! the vt at offset -1 to discriminate.
//!
//! Memory shape (both types):
//!   [vt_ref][slot_0]
//!   ^         ^
//!   -1        Ref points here
//!
//! For Float, slot_0 = f64 bit-pattern.
//! For FfiPtr, slot_0 = usize (raw foreign address).

const std = @import("std");
const ref_mod = @import("ref.zig");
const object = @import("object.zig");
const kernel_eids = @import("kernel_eids.zig");
const arenas = @import("arenas.zig");

pub const Ref = ref_mod.Ref;

/// Phase 15.1b: float/ffiptr box payloads route to the `boxes` arena when the
/// live registry is up, so they are GC-enumerable. `demote` frees the owning
/// materialized struct but NOT the separate box allocations its slots point at,
/// so boxes are pure leak until swept — and sweep needs the arena to enumerate
/// them. Falls back to `fallback` (kernel/test allocator) when no registry, so
/// bare unit tests stay on their own allocator and free locally.
fn boxAlloc(fallback: std.mem.Allocator) std.mem.Allocator {
    return if (arenas.ready()) arenas.boxes() else fallback;
}

// -- Float ------------------------------------------------------------------

/// Box an f64 into a pointer-tagged Ref. Caller owns the allocation
/// indirectly via the kernel allocator — typical use is from the kernel
/// arena that lasts the image lifetime.
pub fn mkFloat(allocator: std.mem.Allocator, value: f64) !Ref {
    const obj_ptr = try object.allocObject(boxAlloc(allocator), ref_mod.mkEid(kernel_eids.FLOAT_VT), 1);
    const bits: u64 = @bitCast(value);
    object.setSlot(obj_ptr, 0, bits);
    return object.toRef(obj_ptr);
}

pub fn isFloat(r: Ref) bool {
    if (!ref_mod.isPointer(r)) return false;
    const obj = object.fromRef(r);
    const vt = object.vtableOf(obj);
    if (!ref_mod.isEid(vt)) return false;
    return ref_mod.untagEid(vt) == kernel_eids.FLOAT_VT;
}

pub fn untagFloat(r: Ref) f64 {
    std.debug.assert(isFloat(r));
    const obj = object.fromRef(r);
    const bits = object.slotAt(obj, 0);
    return @bitCast(bits);
}

// -- FFI pointer ------------------------------------------------------------

/// FFI pointer layout (post-Phase 9.5.5):
///   slot -1 : vt_ref = mkEid(FFI_PTR_VT)
///   slot  0 : raw C address (u64)
///   slot  1 : owning box-EID (u64), or 0 if not associated with a
///             Module — unowned pointers can never be staled.
///
/// 2-slot layout costs 8 extra bytes per box vs the 1-slot Phase 9 shape;
/// in exchange `pushArg`'s stale check is a constant-time slot read plus
/// a single fact-store lookup.
pub fn mkFfiPtr(allocator: std.mem.Allocator, addr: usize) !Ref {
    const obj_ptr = try object.allocObject(boxAlloc(allocator), ref_mod.mkEid(kernel_eids.FFI_PTR_VT), 2);
    object.setSlot(obj_ptr, 0, @as(u64, @intCast(addr)));
    object.setSlot(obj_ptr, 1, 0); // unowned
    return object.toRef(obj_ptr);
}

/// Phase 9.5.5 variant — allocate the boxed pointer AND assert a
/// `[box_eid :ffi-owner <mod_eid>]` fact tying it to its owning Module.
/// The box-EID is stashed in slot 1 so `pushArg`'s stale check can hop
/// directly from a pointer Ref to its fact-store entity without a
/// reverse-lookup index.
pub fn mkFfiPtrOwned(
    allocator: std.mem.Allocator,
    store: *@import("fact_store.zig").FactStore,
    table: *@import("object_table.zig").ObjectTable,
    addr: usize,
    owner_mod_eid: u64,
) !Ref {
    const obj_ptr = try object.allocObject(boxAlloc(allocator), ref_mod.mkEid(kernel_eids.FFI_PTR_VT), 2);
    object.setSlot(obj_ptr, 0, @as(u64, @intCast(addr)));
    const box_eid = table.alloc();
    object.setSlot(obj_ptr, 1, box_eid);
    _ = try store.assert(box_eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.FFI_PTR_VT), kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(box_eid, kernel_eids.ATTR_FFI_OWNER, ref_mod.mkEid(owner_mod_eid), kernel_eids.M_BOOTSTRAP);
    try table.put(box_eid, object.toRef(obj_ptr));
    return object.toRef(obj_ptr);
}

/// Read slot 1 of an FFI-ptr box. Returns 0 if the box is unowned.
pub fn ffiPtrBoxEid(r: Ref) u64 {
    std.debug.assert(isFfiPtr(r));
    const obj = object.fromRef(r);
    return object.slotAt(obj, 1);
}

pub fn isFfiPtr(r: Ref) bool {
    if (!ref_mod.isPointer(r)) return false;
    const obj = object.fromRef(r);
    const vt = object.vtableOf(obj);
    if (!ref_mod.isEid(vt)) return false;
    return ref_mod.untagEid(vt) == kernel_eids.FFI_PTR_VT;
}

pub fn untagFfiPtr(r: Ref) usize {
    std.debug.assert(isFfiPtr(r));
    const obj = object.fromRef(r);
    return @as(usize, @intCast(object.slotAt(obj, 0)));
}

// -- RRB Ref ----------------------------------------------------------------
//
// Mirrors aggregate.aggCollect's libid-wrapper pattern so callers can build
// RRB-Refs outside the aggregate-query path. Used by Phase 9.5.2 :many
// currentValue materialization (ADR-9.5-D2).

const rrb = @import("rrb.zig");
pub const RrbRef = rrb.Rrb(Ref);

/// Build an RRB from a slice of Refs, wrap in a libid object with vt =
/// RRB_VT, return a pointer-tagged Ref. Insertion order = slice order.
pub fn mkRrb(allocator: std.mem.Allocator, values: []const Ref) !Ref {
    var r = RrbRef.empty(allocator);
    for (values) |v| r = try r.push(v);
    const r_ptr = try allocator.create(RrbRef);
    r_ptr.* = r;
    const obj_ptr = try object.allocObject(allocator, ref_mod.mkEid(kernel_eids.RRB_VT), 1);
    object.setSlot(obj_ptr, 0, ref_mod.mkPointer(@ptrCast(r_ptr)));
    return object.toRef(obj_ptr);
}

pub fn isRrbRef(r: Ref) bool {
    if (!ref_mod.isPointer(r)) return false;
    const obj = object.fromRef(r);
    const vt = object.vtableOf(obj);
    if (!ref_mod.isEid(vt)) return false;
    return ref_mod.untagEid(vt) == kernel_eids.RRB_VT;
}

pub fn untagRrbRef(r: Ref) *const RrbRef {
    std.debug.assert(isRrbRef(r));
    const obj = object.fromRef(r);
    const inner_ref = object.slotAt(obj, 0);
    std.debug.assert(ref_mod.isPointer(inner_ref));
    return @ptrCast(@alignCast(ref_mod.untagPointer(inner_ref)));
}
