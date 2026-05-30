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

pub const Ref = ref_mod.Ref;

// -- Float ------------------------------------------------------------------

/// Box an f64 into a pointer-tagged Ref. Caller owns the allocation
/// indirectly via the kernel allocator — typical use is from the kernel
/// arena that lasts the image lifetime.
pub fn mkFloat(allocator: std.mem.Allocator, value: f64) !Ref {
    const obj_ptr = try object.allocObject(allocator, ref_mod.mkEid(kernel_eids.FLOAT_VT), 1);
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

pub fn mkFfiPtr(allocator: std.mem.Allocator, addr: usize) !Ref {
    const obj_ptr = try object.allocObject(allocator, ref_mod.mkEid(kernel_eids.FFI_PTR_VT), 1);
    object.setSlot(obj_ptr, 0, @as(u64, @intCast(addr)));
    return object.toRef(obj_ptr);
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
