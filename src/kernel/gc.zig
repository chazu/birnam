//! Phase 15.2/15.3/15.5 — non-moving mark-sweep heap GC.
//!
//! Reclaims four arenas, non-moving (survivors stay put, Refs never rewritten):
//!   - `boxes`       — float / ffiptr boxes.
//!   - `strings`     — interner `:name` string boxes. SAFE to sweep because the
//!                     strings arena is fed ONLY by `interner.seed`/`intern`,
//!                     each of which immediately asserts `[eid :name box]` — so
//!                     every box here is a current fact value reached by R1 (and
//!                     captured strings are marked in scanWords). FFI/module
//!                     `make_string` (cMakeString) allocates off the kernel
//!                     allocator, NOT this arena, so module-held string boxes
//!                     are never swept.
//!   - `captures`    — BytecodeClosures (the redefine-/block-churn leak: when a
//!                     method is redefined or a scope dies, `:prune` clears the
//!                     closure's object-table slot but `demote` cannot free the
//!                     allocation — closures carry no layout — so it leaks here).
//!   - `closures_vt` — CLOSURE_VT wrappers `[vt][method][data]` (tree-walker
//!                     block closures via `cMakeClosure`, and bytecode-block
//!                     wrappers via `allocClosureEntity`). The wrapper's `data`
//!                     handle is followed in the mark so the inner captures-arena
//!                     BytecodeClosure (reachable ONLY through it) is pinned.
//!
//! Mark roots:
//!   R1 — fact `v`-column (every value; over-marks history → safe; under a
//!        current-only store this is the exact live set). Struct slots mirror
//!        current facts, so R1 also covers struct-slot boxes — no struct walk.
//!   R2 — owned ffiptr boxes parked in the object table (`:ffi-owner` ents).
//!   R3 — constant-pool RRB elements (recursed in place via the fact value).
//!   R4 — object-table slots: marks direct boxes and live closures. A marked
//!        closure is enqueued; its capture tail is then scanned (conservative
//!        word-scan) to reach captured boxes and nested closures. Only *live*
//!        closures are scanned, so a box captured solely by a dead closure is
//!        correctly reclaimed.
//!
//! Runs at a quiescent point (between REPL lines → empty value stack). Skipped
//! while `ffi_pause > 0` (an alien callee may hold a raw box address) and when
//! the registry is absent.

const std = @import("std");
const ref_mod = @import("ref.zig");
const object = @import("object.zig");
const boxed = @import("boxed.zig");
const strings = @import("strings.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const kernel_eids = @import("kernel_eids.zig");
const arenas = @import("arenas.zig");
const arena_mod = @import("arena.zig");
const vm_mod = @import("vm.zig");

const Ref = ref_mod.Ref;
const Arena = arena_mod.Arena;
const REF_SZ = @sizeOf(Ref);

pub const GcStats = struct {
    boxes_freed: usize = 0,
    boxes_kept: usize = 0,
    closures_freed: usize = 0,
    closures_kept: usize = 0,
    wrappers_freed: usize = 0,
    wrappers_kept: usize = 0,
    strings_freed: usize = 0,
    strings_kept: usize = 0,
    skipped: bool = false,
};

/// Marking context — the swept arenas plus a worklist of live-closure
/// allocation bases still to scan.
const Ctx = struct {
    boxes: *Arena,
    caps: ?*Arena,
    closures: ?*Arena, // CLOSURE_VT wrappers (tree-walker + bytecode blocks)
    strs: ?*Arena, // interner `:name` string boxes (every box here is a fact value)
    work: *std.ArrayList(usize),
    scratch: std.mem.Allocator,
};

// -- 15.6 auto-trigger ------------------------------------------------------
//
// Run the collector when the sweepable arenas have grown past a threshold
// since the last collection, at a caller-chosen quiescent point (the shell's
// REPL line boundary — empty value stack). Growth-gated, not absolute: a large
// but stable live set never re-triggers. `collectBoxes` itself re-checks
// `ffi_pause`, so a fired-but-skipped collection leaves the baseline untouched.

/// Cells of net growth across the swept arenas that arm an auto-collection.
pub const GC_GROWTH_THRESHOLD: usize = 50_000;

var last_live: usize = 0;

fn sweepableLive() usize {
    var n: usize = 0;
    if (arenas.arena(.boxes)) |a| n += a.liveCount();
    if (arenas.arena(.captures)) |a| n += a.liveCount();
    if (arenas.arena(.closures_vt)) |a| n += a.liveCount();
    if (arenas.arena(.strings)) |a| n += a.liveCount();
    return n;
}

/// Reset the growth baseline (call after `arenas.init`, so a fresh process
/// doesn't inherit a stale high-water mark).
pub fn resetAutoTrigger() void {
    last_live = 0;
}

/// Collect iff net arena growth since the last collection crossed the
/// threshold. Returns the stats when it ran (skipped stats if `ffi_pause`
/// blocked it), or null when below threshold. Updates the baseline to the
/// post-sweep live count only when a real collection happened.
pub fn maybeCollect(
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    scratch: std.mem.Allocator,
) ?GcStats {
    if (!arenas.ready()) return null;
    const live = sweepableLive();
    if (live <= last_live + GC_GROWTH_THRESHOLD) {
        if (live < last_live) last_live = live; // shrank (e.g. manual :gc) → track down
        return null;
    }
    const st = collectBoxes(store, table, scratch);
    if (!st.skipped) last_live = sweepableLive();
    return st;
}

pub fn collectBoxes(
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    scratch: std.mem.Allocator,
) GcStats {
    const boxes = arenas.arena(.boxes) orelse return .{ .skipped = true };
    if (vm_mod.current().ffi_pause > 0)
        return .{ .skipped = true, .boxes_kept = boxes.liveCount() };

    var work = std.ArrayList(usize){};
    defer work.deinit(scratch);
    var ctx = Ctx{
        .boxes = boxes,
        .caps = arenas.arena(.captures),
        .closures = arenas.arena(.closures_vt),
        .strs = arenas.arena(.strings),
        .work = &work,
        .scratch = scratch,
    };

    // -- Mark --------------------------------------------------------------
    // R1: every fact value.
    var facts = store.meatIter();
    while (facts.next()) |f| consider(&ctx, f.v);

    // R2: owned ffiptr boxes held directly in the object table.
    var ents = store.entitiesWith(kernel_eids.ATTR_FFI_OWNER);
    while (ents.next()) |e| {
        if (table.get(e.e)) |r| consider(&ctx, r);
    }

    // R4 seed: every live object-table slot (direct boxes + live closures).
    var ti = table.iterator();
    while (ti.next()) |kv| consider(&ctx, kv.value);

    // R4 drain: scan each newly-reached live closure's capture tail. Nested
    // closures reached here are enqueued in turn → fixpoint.
    while (work.pop()) |base| {
        if (ctx.caps) |c| {
            if (c.lenOf(base)) |len| scanWords(&ctx, base, len);
        }
    }

    // -- Sweep -------------------------------------------------------------
    var st = GcStats{};
    const boxes_before = boxes.liveCount();
    st.boxes_freed = boxes.sweep();
    st.boxes_kept = boxes_before - st.boxes_freed;
    if (ctx.caps) |c| {
        const caps_before = c.liveCount();
        st.closures_freed = c.sweep();
        st.closures_kept = caps_before - st.closures_freed;
    }
    if (ctx.closures) |cv| {
        const cv_before = cv.liveCount();
        st.wrappers_freed = cv.sweep();
        st.wrappers_kept = cv_before - st.wrappers_freed;
    }
    if (ctx.strs) |s| {
        const s_before = s.liveCount();
        st.strings_freed = s.sweep();
        st.strings_kept = s_before - st.strings_freed;
    }
    return st;
}

/// Typed mark of a Ref: box → mark; RRB → recurse elements; closure (a pointer
/// whose untagged address bases a captures-arena allocation) → mark + enqueue;
/// struct/string/eid/int → skip (covered by R1 or not a swept target).
const PTR_ADDR_MASK: u64 = ~@as(u64, 0b111);

fn consider(ctx: *Ctx, r: Ref) void {
    // String boxes (tag 100) are not "pointers" but ARE a swept arena. Every
    // box in the strings arena is an interner `:name` fact value, so reaching
    // one here (R1/R3/R4) marks it live; the rest sweep. Base = r & addr-mask
    // (StringHeader is the allocation base, see strings.encode).
    if (strings.isString(r)) {
        if (ctx.strs) |s| _ = s.mark(r & PTR_ADDR_MASK);
        return;
    }
    if (!ref_mod.isPointer(r)) return; // eids / ints / keywords
    const p = r & PTR_ADDR_MASK; // address bits only — avoids untagPointer's null assert
    if (p == 0) return; // a tag-000 zero word (e.g. a frame id), not a heap ref

    // A BytecodeClosure Ref points at the allocation base directly (no -1), so
    // its `p` IS a captures-arena base. Identify closures by address FIRST —
    // before any vt read — because reading p[-1] on a closure is out of bounds.
    if (ctx.caps) |c| {
        if (c.contains(p)) {
            if (c.mark(p)) ctx.work.append(ctx.scratch, p) catch {};
            return;
        }
    }

    // Now p is a libid-object payload (box / struct / RRB / CLOSURE_VT wrapper):
    // vt lives at p-1, safe to read.
    //
    // A CLOSURE_VT wrapper is `[vt][method][data]` with the Ref at the payload
    // (method slot, base+8). Its `data` is the actual capture handle — for a
    // bytecode block it is the captures-arena BytecodeClosure, which is ONLY
    // reachable through this wrapper. So we must follow `data` or that live
    // inner closure would be swept under a live wrapper (use-after-free).
    if (isClosureVt(r)) {
        const base = p - REF_SZ;
        if (ctx.closures) |cv| {
            if (cv.contains(base)) {
                if (!cv.mark(base)) return; // owned + already visited → break cycle
            }
            // owned+newly-marked, or wrapper not arena-owned (boot/compiler/ffi
            // closures, permanently kept): fall through and follow `data`.
        }
        const data = @as(*const Ref, @ptrFromInt(p + REF_SZ)).*;
        consider(ctx, data);
        return;
    }

    if (boxed.isFloat(r) or boxed.isFfiPtr(r)) {
        _ = ctx.boxes.mark(p - REF_SZ);
        return;
    }
    if (isRrb(r)) {
        const obj = object.fromRef(r);
        const slot0 = object.slotAt(obj, 0);
        if (!ref_mod.isPointer(slot0)) return;
        const rrb_ptr: *boxed.RrbRef = @ptrCast(@alignCast(ref_mod.untagPointer(slot0)));
        const n = rrb_ptr.count();
        var i: usize = 0;
        while (i < n) : (i += 1) consider(ctx, rrb_ptr.nth(i));
    }
    // else: struct (slots mirror facts → covered by R1) / other — skip.
}

fn isRrb(r: Ref) bool {
    const vt = object.vtableOf(object.fromRef(r));
    return ref_mod.isEid(vt) and ref_mod.untagEid(vt) == kernel_eids.RRB_VT;
}

fn isClosureVt(r: Ref) bool {
    const vt = object.vtableOf(object.fromRef(r));
    if (!ref_mod.isEid(vt)) return false;
    const vt_eid = ref_mod.untagEid(vt);
    // Phase 12.8 — a materialized bytecode method is BYTECODE_METHOD_VT but lives
    // in the same closures_vt arena with the same `[vt][method][data]` layout, so
    // it marks + follows `data` identically (its data is an eid → consider no-ops,
    // but the wrapper itself must be marked live or it is wrongly swept).
    return vt_eid == kernel_eids.CLOSURE_VT or vt_eid == kernel_eids.BYTECODE_METHOD_VT;
}

/// Conservatively scan a live closure's allocation words. Each word that looks
/// like a box pointer marks that box; each that bases a captures allocation
/// marks (and enqueues, once) that nested closure. Over-keeping is safe.
fn scanWords(ctx: *Ctx, base: usize, len: usize) void {
    var off: usize = 0;
    while (off + REF_SZ <= len) : (off += REF_SZ) {
        const w: u64 = @as(*const u64, @ptrFromInt(base + off)).*;
        // A captured string Ref (tag 100) isn't a "pointer" but is sweepable;
        // mark its box so a string held only by a live closure survives.
        if (strings.isString(w)) {
            if (ctx.strs) |s| _ = s.mark(w & PTR_ADDR_MASK);
            continue;
        }
        if (!ref_mod.isPointer(w)) continue;
        const payload = w & PTR_ADDR_MASK;
        if (payload == 0) continue;
        _ = ctx.boxes.mark(payload - REF_SZ); // box base (no-op if not a box)
        if (ctx.caps) |c| {
            if (c.mark(payload)) ctx.work.append(ctx.scratch, payload) catch {};
        }
    }
}
