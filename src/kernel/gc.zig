//! Phase 15.2/15.3 — non-moving mark-sweep heap GC over the boxes arena.
//!
//! Reclaims float / ffiptr boxes (the `boxes` arena) that no live structure
//! references. Non-moving (ROADMAP Phase 15 §Collector): survivors stay put,
//! Refs are never rewritten.
//!
//! A live box Ref hides in exactly four places; the mark covers all four:
//!   R1 — fact `v`-column. Every fact value (over-marks history → safe; under
//!        a current-only store — the Q2 gate — this is the exact live set).
//!        Materialized struct slots mirror their entity's current facts
//!        (promote builds slots from `attributesOf`), so R1 also covers
//!        struct-slot boxes — no struct walk needed.
//!   R2 — owned ffiptr boxes parked directly in the object table by
//!        `mkFfiPtrOwned` (reached via their `:ffi-owner` entities).
//!   R3 — constant-pool RRB elements (a float literal compiles into the pool).
//!        Reached via the `:constant-pool` fact value, recursed in place.
//!   R4 — closure capture tails (`let x := 3.14; [x]`). Conservative word-scan
//!        of the `captures` arena: any word that looks like a live box pointer
//!        is kept. Over-keeping is safe; a live box is never freed.
//!
//! Runs at a quiescent point (between REPL lines) so the value stack is empty.
//! Skipped while an FFI call is in flight (`ffi_pause > 0`) — an alien callee
//! may hold a raw box address. No-op when the arena registry is absent.

const std = @import("std");
const ref_mod = @import("ref.zig");
const object = @import("object.zig");
const boxed = @import("boxed.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const kernel_eids = @import("kernel_eids.zig");
const arenas = @import("arenas.zig");
const arena_mod = @import("arena.zig");
const vm_mod = @import("vm.zig");

const Ref = ref_mod.Ref;
const Arena = arena_mod.Arena;
const REF_SZ = @sizeOf(Ref);

pub const GcStats = struct { boxes_freed: usize, boxes_kept: usize, skipped: bool = false };

/// Mark-sweep the boxes arena. See module doc for the root set.
pub fn collectBoxes(store: *fact_store.FactStore, table: *object_table.ObjectTable) GcStats {
    const boxes = arenas.arena(.boxes) orelse return .{ .boxes_freed = 0, .boxes_kept = 0, .skipped = true };
    // Pin everything while alien code may hold a raw box address.
    if (vm_mod.current().ffi_pause > 0)
        return .{ .boxes_freed = 0, .boxes_kept = boxes.liveCount(), .skipped = true };

    // -- Mark --------------------------------------------------------------
    // R1: every fact value (current + historical → conservative).
    var facts = store.meatIter();
    while (facts.next()) |f| considerRef(boxes, f.v);

    // R2: owned ffiptr boxes held directly in the object table.
    var ents = store.entitiesWith(kernel_eids.ATTR_FFI_OWNER);
    while (ents.next()) |e| {
        if (table.get(e.e)) |r| considerRef(boxes, r);
    }

    // R4: conservative scan of closure capture allocations.
    if (arenas.arena(.captures)) |caps| {
        var ai = caps.iterate();
        while (ai.next()) |a| scanWords(boxes, a.base, a.len);
    }

    // -- Sweep -------------------------------------------------------------
    const before = boxes.liveCount();
    const freed = boxes.sweep();
    return .{ .boxes_freed = freed, .boxes_kept = before - freed };
}

/// Typed mark: mark a box; recurse RRB pool elements (R3). Anything else
/// (strings, eids, ints, struct/closure pointers) is handled elsewhere or
/// is not a boxes-arena target.
fn considerRef(boxes: *Arena, r: Ref) void {
    if (!ref_mod.isPointer(r)) return; // strings (tag 100) / eids / ints
    if (boxed.isFloat(r) or boxed.isFfiPtr(r)) {
        _ = boxes.mark(baseOf(r));
        return;
    }
    if (isRrb(r)) {
        const obj = object.fromRef(r);
        const slot0 = object.slotAt(obj, 0);
        if (!ref_mod.isPointer(slot0)) return;
        const rrb_ptr: *boxed.RrbRef = @ptrCast(@alignCast(ref_mod.untagPointer(slot0)));
        const n = rrb_ptr.count();
        var i: usize = 0;
        while (i < n) : (i += 1) considerRef(boxes, rrb_ptr.nth(i));
    }
}

/// A box / libid-object Ref points at slot 0 (payload); the allocation base —
/// what the arena records — is one Ref-word earlier (the vt slot at -1).
fn baseOf(r: Ref) usize {
    return @intFromPtr(ref_mod.untagPointer(r)) - REF_SZ;
}

fn isRrb(r: Ref) bool {
    const vt = object.vtableOf(object.fromRef(r));
    return ref_mod.isEid(vt) and ref_mod.untagEid(vt) == kernel_eids.RRB_VT;
}

/// Conservatively scan an allocation's words; mark any that look like a live
/// box pointer. Over-keeping is safe — never frees a live box.
fn scanWords(boxes: *Arena, base: usize, len: usize) void {
    var off: usize = 0;
    while (off + REF_SZ <= len) : (off += REF_SZ) {
        const w: u64 = @as(*const u64, @ptrFromInt(base + off)).*;
        if (!ref_mod.isPointer(w)) continue;
        _ = boxes.mark(@intFromPtr(ref_mod.untagPointer(w)) - REF_SZ);
    }
}
