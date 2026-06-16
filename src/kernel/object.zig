//! Materialized object layout and the -1 vtable trick.
//!
//! Layout: `[vt_ref_at_minus_1][slot_0][slot_1]...[slot_{N-1}]`
//! The caller's `ObjPtr` points at `slot_0`; `vtableOf(obj)` reads the
//! slot one word before that.
//!
//! Design note: the `-1` slot holds an **EID Ref** (tag = 010), not a raw
//! pointer to the vtable's materialized struct. Reasons:
//!   - Cold-path dispatch needs the vtable EID to query EAVT; storing EID
//!     at -1 makes this direct (no reverse lookup table).
//!   - Mcache key is `(vt_eid, sel_eid)` — both u64. No pointer compare.
//!   - The materialized struct pointer lives in the object table at the
//!     EID, retrievable when needed for struct ops.
//!
//! This is a small departure from Piumarta's pointer-at-minus-1 — same
//! shape, EID instead of pointer. The "-1 trick" survives intact in
//! spirit.

const std = @import("std");
const ref_mod = @import("ref.zig");

pub const Ref = ref_mod.Ref;
pub const Object = opaque {};
pub const ObjPtr = *Object;

/// Allocate `n_slots` uniform-Ref slots, with the vtable Ref written at
/// offset -1. Returns a pointer to `slot[0]`.
///
/// `vt_ref` MUST be an EID-tagged Ref. Use `ref.mkEid(vt_eid)`.
pub fn allocObject(allocator: std.mem.Allocator, vt_ref: Ref, n_slots: usize) !ObjPtr {
    std.debug.assert(ref_mod.isEid(vt_ref));
    const total_bytes = @sizeOf(Ref) * (1 + n_slots);
    const buf = try allocator.alignedAlloc(u8, .of(Ref), total_bytes);
    // Zero-fill slots so reads of uninitialized slots are deterministic.
    @memset(buf, 0);
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = vt_ref;
    const obj_ptr: ObjPtr = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    return obj_ptr;
}

/// Free a libid object allocated by `allocObject` (base = `obj` - 1 word, size =
/// `1 + n_slots` words, alignment = Ref). Caller must pass the same `n_slots`
/// used at alloc. For non-arena objects whose lifetime is managed explicitly
/// (e.g. a swept method's constant-pool RRB box — Phase 15.P).
pub fn freeObject(allocator: std.mem.Allocator, obj: ObjPtr, n_slots: usize) void {
    const base_addr = @intFromPtr(obj) - @sizeOf(Ref);
    const total_bytes = @sizeOf(Ref) * (1 + n_slots);
    const buf: []align(@alignOf(Ref)) u8 = @as([*]align(@alignOf(Ref)) u8, @ptrFromInt(base_addr))[0..total_bytes];
    allocator.free(buf);
}

/// Read the vtable Ref stored one word before `obj`.
pub fn vtableOf(obj: ObjPtr) Ref {
    const obj_addr: usize = @intFromPtr(obj);
    const vt_slot: *const Ref = @ptrFromInt(obj_addr - @sizeOf(Ref));
    return vt_slot.*;
}

/// Overwrite the vtable Ref at offset -1.
pub fn setVtable(obj: ObjPtr, vt: Ref) void {
    std.debug.assert(ref_mod.isEid(vt));
    const obj_addr: usize = @intFromPtr(obj);
    const vt_slot: *Ref = @ptrFromInt(obj_addr - @sizeOf(Ref));
    vt_slot.* = vt;
}

/// Read slot `idx` (0-indexed; slot 0 is what `obj` points at).
pub fn slotAt(obj: ObjPtr, idx: usize) Ref {
    const obj_addr: usize = @intFromPtr(obj);
    const slot: *const Ref = @ptrFromInt(obj_addr + idx * @sizeOf(Ref));
    return slot.*;
}

/// Write slot `idx`.
pub fn setSlot(obj: ObjPtr, idx: usize, v: Ref) void {
    const obj_addr: usize = @intFromPtr(obj);
    const slot: *Ref = @ptrFromInt(obj_addr + idx * @sizeOf(Ref));
    slot.* = v;
}

/// Convenience: pack an `ObjPtr` as a raw-pointer `Ref` (tag = 000).
pub fn toRef(obj: ObjPtr) Ref {
    return ref_mod.mkPointer(@ptrCast(obj));
}

/// Inverse: unpack a pointer Ref into an `ObjPtr`. Caller verifies tag.
pub fn fromRef(r: Ref) ObjPtr {
    std.debug.assert(ref_mod.isPointer(r));
    return @ptrCast(@alignCast(ref_mod.untagPointer(r)));
}
