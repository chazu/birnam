//! Method cache.
//!
//! Direct-mapped 1024-entry cache. Key: `(vt_eid, sel_eid)`. Value: pointer
//! to materialized `Closure`. Invalidation is **per-selector**: any
//! `assert`/`retract` touching `(?, sel_eid, ?)` flushes every cache entry
//! with that selector. Coarser than per-(vt, sel) but simpler; revisit when
//! profiling demands (likely Phase 8 promotion/demotion traffic).
//!
//! Single-threaded image — no locks.

const std = @import("std");
const closure_mod = @import("closure.zig");

const Closure = closure_mod.Closure;

const SIZE: usize = 1024;
const MASK: u64 = SIZE - 1;

const Entry = struct {
    vt_eid: u64,
    sel_eid: u64,
    closure: ?*Closure,
    /// True if the entry is occupied. Empty entries return null from
    /// lookup even on accidental key match (vt_eid=0, sel_eid=0).
    occupied: bool,
};

var entries: [SIZE]Entry = [_]Entry{.{ .vt_eid = 0, .sel_eid = 0, .closure = null, .occupied = false }} ** SIZE;

fn slot(vt_eid: u64, sel_eid: u64) usize {
    // Cheap hash: xor + multiply, then mask. Spreads bits enough for a
    // 1024-entry direct-mapped cache.
    var h: u64 = vt_eid ^ (sel_eid *% 0x9E3779B97F4A7C15);
    h ^= h >> 32;
    return @intCast(h & MASK);
}

pub fn lookup(vt_eid: u64, sel_eid: u64) ?*Closure {
    const i = slot(vt_eid, sel_eid);
    const e = entries[i];
    if (e.occupied and e.vt_eid == vt_eid and e.sel_eid == sel_eid) return e.closure;
    return null;
}

pub fn store(vt_eid: u64, sel_eid: u64, closure: ?*Closure) void {
    const i = slot(vt_eid, sel_eid);
    entries[i] = .{ .vt_eid = vt_eid, .sel_eid = sel_eid, .closure = closure, .occupied = true };
}

/// Flush every entry whose selector matches. Called from FactStore when
/// `(?, sel_eid, ?)` is asserted or retracted. O(SIZE).
pub fn flushSel(sel_eid: u64) void {
    for (&entries) |*e| {
        if (e.occupied and e.sel_eid == sel_eid) {
            e.occupied = false;
            e.closure = null;
        }
    }
}

/// Clear the entire cache. Used by bootstrap and image reload.
pub fn clearAll() void {
    for (&entries) |*e| {
        e.occupied = false;
        e.closure = null;
        e.vt_eid = 0;
        e.sel_eid = 0;
    }
}
