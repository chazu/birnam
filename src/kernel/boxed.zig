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

/// FFI pointer layout (post-Phase 9.5.5; slot 2 added in Phase 19.7):
///   slot -1 : vt_ref = mkEid(FFI_PTR_VT)
///   slot  0 : raw C address (u64)
///   slot  1 : owning box-EID (u64), or 0 if not associated with a
///             Module — unowned pointers can never be staled.
///   slot  2 : out-param width tag (u64), or 0 if this is not an out-param
///             "cell" (Phase 19.7). Non-zero only for `[Module outInt]` &c.;
///             tells `[cell value]` which width to peek. Inert otherwise.
///
/// All three slots are raw u64 (addr / eid / tag), never Refs, so the GC mark
/// walk does not scan them. `pushArg`'s stale check is a constant-time slot-1
/// read plus a single fact-store lookup.
pub fn mkFfiPtr(allocator: std.mem.Allocator, addr: usize) !Ref {
    const obj_ptr = try object.allocObject(boxAlloc(allocator), ref_mod.mkEid(kernel_eids.FFI_PTR_VT), 3);
    object.setSlot(obj_ptr, 0, @as(u64, @intCast(addr)));
    object.setSlot(obj_ptr, 1, 0); // unowned
    object.setSlot(obj_ptr, 2, 0); // not an out-param cell
    return object.toRef(obj_ptr);
}

/// Phase 19.7 — mint an out-param "cell": an FfiPtr that remembers a width tag
/// in slot 2 so `[cell value]` peeks the right C width without the caller
/// re-stating it. Always unowned (slot 1 = 0); freed via `[cell free]`.
pub fn mkFfiPtrCell(allocator: std.mem.Allocator, addr: usize, width_tag: u64) !Ref {
    const obj_ptr = try object.allocObject(boxAlloc(allocator), ref_mod.mkEid(kernel_eids.FFI_PTR_VT), 3);
    object.setSlot(obj_ptr, 0, @as(u64, @intCast(addr)));
    object.setSlot(obj_ptr, 1, 0);
    object.setSlot(obj_ptr, 2, width_tag);
    return object.toRef(obj_ptr);
}

/// Read the Phase 19.7 out-param width tag (slot 2). 0 = not a cell.
pub fn ffiPtrWidth(r: Ref) u64 {
    std.debug.assert(isFfiPtr(r));
    const obj = object.fromRef(r);
    return object.slotAt(obj, 2);
}

// -- FFI callback (Phase 19.2) ----------------------------------------------
//
// A callback box wraps a libffi closure: slot 0 is the C-callable code location
// (the function pointer C will call), slot 1 is a usize pointer to the heap
// `CallbackCtx` (the birnam closure + parsed sig + ffi_closure handle). Both
// raw u64, not Refs — so, like FfiPtr, the GC mark walk doesn't scan them.

pub fn mkCallback(allocator: std.mem.Allocator, code_addr: usize, ctx_ptr: usize) !Ref {
    const obj_ptr = try object.allocObject(boxAlloc(allocator), ref_mod.mkEid(kernel_eids.CALLBACK_VT), 2);
    object.setSlot(obj_ptr, 0, @as(u64, @intCast(code_addr)));
    object.setSlot(obj_ptr, 1, @as(u64, @intCast(ctx_ptr)));
    return object.toRef(obj_ptr);
}

pub fn isCallback(r: Ref) bool {
    if (!ref_mod.isPointer(r)) return false;
    const obj = object.fromRef(r);
    const vt = object.vtableOf(obj);
    if (!ref_mod.isEid(vt)) return false;
    return ref_mod.untagEid(vt) == kernel_eids.CALLBACK_VT;
}

/// The C-callable code location (slot 0) — what gets passed to a C function as a
/// function-pointer arg.
pub fn callbackCode(r: Ref) usize {
    std.debug.assert(isCallback(r));
    return @as(usize, @intCast(object.slotAt(object.fromRef(r), 0)));
}

/// Pointer to the heap `CallbackCtx` (slot 1), 0 if already freed.
pub fn callbackCtx(r: Ref) usize {
    std.debug.assert(isCallback(r));
    return @as(usize, @intCast(object.slotAt(object.fromRef(r), 1)));
}

/// Clear the ctx pointer after `[cb free]` so a double free is a no-op.
pub fn clearCallbackCtx(r: Ref) void {
    std.debug.assert(isCallback(r));
    object.setSlot(object.fromRef(r), 1, 0);
}

// -- FFI struct instance (Phase 19.5) ---------------------------------------
//
// A struct instance is a box-arena object whose vtable is the per-type instance
// vtable (minted by `Module struct:fields:`) and whose slot 0 is the backing C
// address. Field getters/setters installed on that vtable peek/poke at field
// offsets through this address. slot 0 is a raw usize (not a Ref), so the GC
// mark walk doesn't scan it — same discipline as FfiPtr.

pub fn mkStructInstance(allocator: std.mem.Allocator, instance_vt: u64, backing_addr: usize) !Ref {
    const obj_ptr = try object.allocObject(boxAlloc(allocator), ref_mod.mkEid(instance_vt), 1);
    object.setSlot(obj_ptr, 0, @as(u64, @intCast(backing_addr)));
    return object.toRef(obj_ptr);
}

/// The backing C address (slot 0) of a struct instance.
pub fn structInstanceAddr(r: Ref) usize {
    return @as(usize, @intCast(object.slotAt(object.fromRef(r), 0)));
}

/// A struct TYPE object (STRUCT_TYPE_VT): slot 0 = the minted per-type instance
/// vtable eid, slot 1 = the struct's byte size. Both raw u64.
pub fn mkStructType(allocator: std.mem.Allocator, instance_vt: u64, size: usize) !Ref {
    const obj_ptr = try object.allocObject(boxAlloc(allocator), ref_mod.mkEid(kernel_eids.STRUCT_TYPE_VT), 2);
    object.setSlot(obj_ptr, 0, instance_vt);
    object.setSlot(obj_ptr, 1, @as(u64, @intCast(size)));
    return object.toRef(obj_ptr);
}

pub fn structTypeInstanceVt(r: Ref) u64 {
    return @as(u64, @intCast(object.slotAt(object.fromRef(r), 0)));
}

pub fn structTypeSize(r: Ref) usize {
    return @as(usize, @intCast(object.slotAt(object.fromRef(r), 1)));
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
    const obj_ptr = try object.allocObject(boxAlloc(allocator), ref_mod.mkEid(kernel_eids.FFI_PTR_VT), 3);
    object.setSlot(obj_ptr, 0, @as(u64, @intCast(addr)));
    const box_eid = table.alloc();
    object.setSlot(obj_ptr, 1, box_eid);
    object.setSlot(obj_ptr, 2, 0); // not an out-param cell
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
    // Bulk build (O(N) allocs, no path-copy garbage) so the result is fully
    // reclaimable by `freeRrb` — see rrb.fromSlice / Phase 15.P.
    const r = try RrbRef.fromSlice(allocator, values);
    return wrapRrb(allocator, r);
}

/// Wrap an already-built `RrbRef` in a libid object (vt = RRB_VT) + return a
/// pointer-tagged Ref. Shared by `mkRrb` and `rrbWith` (Phase 16).
pub fn wrapRrb(allocator: std.mem.Allocator, r: RrbRef) !Ref {
    const r_ptr = try allocator.create(RrbRef);
    r_ptr.* = r;
    const obj_ptr = try object.allocObject(allocator, ref_mod.mkEid(kernel_eids.RRB_VT), 1);
    object.setSlot(obj_ptr, 0, ref_mod.mkPointer(@ptrCast(r_ptr)));
    return object.toRef(obj_ptr);
}

/// Phase 16 — immutable append: a NEW vector = `vec` with `elem` at the end
/// (structural sharing). `vec` must be an RRB Ref.
pub fn rrbWith(allocator: std.mem.Allocator, vec: Ref, elem: Ref) !Ref {
    var r = untagRrbRef(vec).*;
    r = try r.push(elem);
    return wrapRrb(allocator, r);
}

/// Phase 16 — element count of an RRB Ref.
pub fn rrbLen(vec: Ref) usize {
    return untagRrbRef(vec).count();
}

/// Phase 16 — element at `idx` (caller bounds-checks; asserts in Debug).
pub fn rrbNth(vec: Ref, idx: usize) Ref {
    return untagRrbRef(vec).nth(idx);
}

// -- Phase 16 Tier C: persistent map (ASSOC_VT) --------------------------

const hamt = @import("hamt.zig");

/// Keyword-keyed persistent map VALUE. Keys are Refs by-value (a keyword
/// `#name` is an `mkEid` Ref); the HAMT key is the key Ref's bits.
pub const MapRef = hamt.Hamt(u64, Ref, hamt.U64Ctx);

pub fn wrapMap(allocator: std.mem.Allocator, m: MapRef) !Ref {
    const m_ptr = try allocator.create(MapRef);
    m_ptr.* = m;
    const obj_ptr = try object.allocObject(allocator, ref_mod.mkEid(kernel_eids.ASSOC_VT), 1);
    object.setSlot(obj_ptr, 0, ref_mod.mkPointer(@ptrCast(m_ptr)));
    return object.toRef(obj_ptr);
}

pub fn mkMap(allocator: std.mem.Allocator) !Ref {
    return wrapMap(allocator, MapRef.empty(allocator, .{}));
}

pub fn isMapRef(r: Ref) bool {
    if (!ref_mod.isPointer(r)) return false;
    const obj = object.fromRef(r);
    const vt = object.vtableOf(obj);
    if (!ref_mod.isEid(vt)) return false;
    return ref_mod.untagEid(vt) == kernel_eids.ASSOC_VT;
}

pub fn untagMapRef(r: Ref) *const MapRef {
    std.debug.assert(isMapRef(r));
    const obj = object.fromRef(r);
    const inner = object.slotAt(obj, 0);
    std.debug.assert(ref_mod.isPointer(inner));
    return @ptrCast(@alignCast(ref_mod.untagPointer(inner)));
}

/// Immutable put: a NEW map = `map` with `key` → `val` (structural sharing).
pub fn mapPut(allocator: std.mem.Allocator, map: Ref, key: Ref, val: Ref) !Ref {
    var m = untagMapRef(map).*;
    m = try m.assoc(key, val);
    return wrapMap(allocator, m);
}

pub fn mapGet(map: Ref, key: Ref) ?Ref {
    return untagMapRef(map).get(key);
}

pub fn mapLen(map: Ref) usize {
    return untagMapRef(map).count();
}

/// Collect a map's keys (true) or values (false) into a fresh RRB vector.
fn mapColumn(allocator: std.mem.Allocator, map: Ref, want_keys: bool) !Ref {
    var r = RrbRef.empty(allocator);
    var it = untagMapRef(map).iter();
    while (it.next()) |kv| r = try r.push(if (want_keys) kv.key else kv.value);
    return wrapRrb(allocator, r);
}
pub fn mapKeys(allocator: std.mem.Allocator, map: Ref) !Ref {
    return mapColumn(allocator, map, true);
}
pub fn mapValues(allocator: std.mem.Allocator, map: Ref) !Ref {
    return mapColumn(allocator, map, false);
}

/// Merge: `b`'s entries override `a`'s → a NEW map.
pub fn mapMerge(allocator: std.mem.Allocator, a: Ref, b: Ref) !Ref {
    var m = untagMapRef(a).*;
    var it = untagMapRef(b).iter();
    while (it.next()) |kv| m = try m.assoc(kv.key, kv.value);
    return wrapMap(allocator, m);
}

// -- Phase 16: `reduced` early-out sentinel -------------------------------

/// Wrap `val` in a REDUCED_VT box (1 slot). `inject:into:` returning one stops
/// the fold and unwraps it.
pub fn mkReduced(allocator: std.mem.Allocator, val: Ref) !Ref {
    const obj_ptr = try object.allocObject(allocator, ref_mod.mkEid(kernel_eids.REDUCED_VT), 1);
    object.setSlot(obj_ptr, 0, val);
    return object.toRef(obj_ptr);
}
pub fn isReduced(r: Ref) bool {
    if (!ref_mod.isPointer(r)) return false;
    const obj = object.fromRef(r);
    const vt = object.vtableOf(obj);
    if (!ref_mod.isEid(vt)) return false;
    return ref_mod.untagEid(vt) == kernel_eids.REDUCED_VT;
}
pub fn reducedValue(r: Ref) Ref {
    std.debug.assert(isReduced(r));
    return object.slotAt(object.fromRef(r), 0);
}

// -- Phase 16: integer range (RANGE_VT) -----------------------------------

/// Inclusive integer range `lo, lo+step, ... <= hi` (or `>= hi` for step<0).
/// A 3-slot box (lo, hi, step as SmallInts). A lazy seq — no materialization.
pub fn mkRange(allocator: std.mem.Allocator, lo: i64, hi: i64, step: i64) !Ref {
    const obj_ptr = try object.allocObject(allocator, ref_mod.mkEid(kernel_eids.RANGE_VT), 3);
    object.setSlot(obj_ptr, 0, ref_mod.mkSmallInt(lo) orelse ref_mod.NIL);
    object.setSlot(obj_ptr, 1, ref_mod.mkSmallInt(hi) orelse ref_mod.NIL);
    object.setSlot(obj_ptr, 2, ref_mod.mkSmallInt(step) orelse ref_mod.NIL);
    return object.toRef(obj_ptr);
}
pub fn isRange(r: Ref) bool {
    if (!ref_mod.isPointer(r)) return false;
    const obj = object.fromRef(r);
    const vt = object.vtableOf(obj);
    if (!ref_mod.isEid(vt)) return false;
    return ref_mod.untagEid(vt) == kernel_eids.RANGE_VT;
}
fn rangeSlot(r: Ref, i: usize) i64 {
    return ref_mod.untagSmallInt(object.slotAt(object.fromRef(r), i));
}
pub fn rangeLen(r: Ref) usize {
    const lo = rangeSlot(r, 0);
    const hi = rangeSlot(r, 1);
    const step = rangeSlot(r, 2);
    if (step == 0) return 0;
    if (step > 0) return if (hi >= lo) @intCast(@divFloor(hi - lo, step) + 1) else 0;
    return if (lo >= hi) @intCast(@divFloor(lo - hi, -step) + 1) else 0;
}
pub fn rangeNth(r: Ref, i: usize) Ref {
    return ref_mod.mkSmallInt(rangeSlot(r, 0) + @as(i64, @intCast(i)) * rangeSlot(r, 2)) orelse ref_mod.NIL;
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

/// Phase 15.P — fully reclaim an RRB allocated by `mkRrb`/`wrapRrb` on
/// `allocator`: the tree spine (`RrbRef.deinitTree`), the `RrbRef` struct, and
/// the libid object. SAFE ONLY for an unshared RRB (see `rrb.deinitTree`) — a
/// compiler constant pool qualifies. `r` must not be used afterward.
pub fn freeRrb(allocator: std.mem.Allocator, r: Ref) void {
    std.debug.assert(isRrbRef(r));
    const obj = object.fromRef(r);
    const rrb_ptr: *RrbRef = @constCast(untagRrbRef(r));
    rrb_ptr.deinitTree();
    allocator.destroy(rrb_ptr);
    object.freeObject(allocator, obj, 1); // RRB libid object has 1 slot
}
