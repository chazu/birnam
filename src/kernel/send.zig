//! Send / bind: dispatch atop the fact store.
//!
//! Phase 4 design (locked):
//!   - `bind` is a Zig primitive that queries EAVT directly. Not metacircular.
//!     Phase 7.5 will introduce optional `[vt :bind <method>]` overrides.
//!   - Cold path on EID receiver: look up `[receiver :vtable ?vt_eid]`.
//!     NIL (and other future immediates) gets a `:vtable` fact asserted at
//!     bootstrap.
//!   - Delegation chain walked here, not in a `lookup:` method. (User-visible
//!     `lookup:` exists on `vtable_vt` for reflection; bind doesn't use it.)
//!   - Recursion guard scaffolded; unused in Phase 4 because bind never sends.
//!
//! Globals: `g_store` and `g_table` set at bootstrap. Single-threaded image.

const std = @import("std");
const ref_mod = @import("ref.zig");
const object = @import("object.zig");
const closure_mod = @import("closure.zig");
const mcache = @import("mcache.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const kernel_eids = @import("kernel_eids.zig");
const promotion = @import("promotion.zig");

pub const Ref = ref_mod.Ref;
pub const Closure = closure_mod.Closure;
pub const ObjPtr = object.ObjPtr;

/// Selector EID stored as a u64 (selectors are entities).
pub const Selector = u64;

/// Send frame, passed to every method body. Pointer-passed.
pub const Send = extern struct {
    selector: Selector,
    receiver: Ref,
    n_args: u8,
    state: Ref, // current "self" reference; may differ from receiver in super-sends
    closure: ?*Closure,
};

pub const Imp = *const fn (*Send) callconv(.c) Ref;
pub const Imp1 = *const fn (*Send, Ref) callconv(.c) Ref;
pub const Imp2 = *const fn (*Send, Ref, Ref) callconv(.c) Ref;
pub const Imp3 = *const fn (*Send, Ref, Ref, Ref) callconv(.c) Ref;

// -- Globals ---------------------------------------------------------------

var g_store: ?*fact_store.FactStore = null;
var g_table: ?*object_table.ObjectTable = null;
var g_allocator: ?std.mem.Allocator = null;

/// Recursion guard. Scaffolded for Phase 7.5 where bind becomes
/// redefinable. In Phase 4 it is bumped but never consulted.
var g_recursion: u32 = 0;

pub fn initDispatch(
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    allocator: std.mem.Allocator,
) void {
    g_store = store;
    g_table = table;
    g_allocator = allocator;
    mcache.clearAll();
    g_recursion = 0;
}

// Accessors for kernel methods that need fact-store / object-table / allocator.
pub fn storePtr() ?*fact_store.FactStore {
    return g_store;
}
pub fn tablePtr() ?*object_table.ObjectTable {
    return g_table;
}
pub fn kernelAllocator() ?std.mem.Allocator {
    return g_allocator;
}

// -- bind ------------------------------------------------------------------

/// Resolve the closure for `s.receiver` + `s.selector`. Returns null on
/// "method not found." Populates `s.closure` and `s.state` on success.
pub fn bind(s: *Send) ?*Closure {
    g_recursion += 1;
    defer g_recursion -= 1;

    const vt_eid = vtableEidOf(s.receiver) orelse return null;
    s.state = s.receiver;

    // Hot path
    if (mcache.lookup(vt_eid, s.selector)) |c| {
        s.closure = c;
        return c;
    }

    // Cold path: walk vt + delegation chain.
    var cur_vt = vt_eid;
    while (cur_vt != kernel_eids.NIL) {
        const store = g_store.?;
        if (store.currentValue(cur_vt, s.selector)) |method_ref| {
            // method_ref is mkEid(closure_eid).
            if (!ref_mod.isEid(method_ref)) return null; // shouldn't happen
            const closure_eid = ref_mod.untagEid(method_ref);
            const ptr_ref = (g_table.?).get(closure_eid) orelse return null;
            if (!ref_mod.isPointer(ptr_ref)) return null;
            const c: *Closure = @ptrCast(@alignCast(ref_mod.untagPointer(ptr_ref)));
            mcache.store(vt_eid, s.selector, c);
            s.closure = c;
            return c;
        }
        // No method here; walk to delegate.
        if (store.currentValue(cur_vt, kernel_eids.ATTR_DELEGATE)) |delegate_ref| {
            if (!ref_mod.isEid(delegate_ref)) return null;
            cur_vt = ref_mod.untagEid(delegate_ref);
        } else {
            return null;
        }
    }
    return null;
}

/// Determine the vtable EID of a receiver Ref, by tag:
///   - pointer: read `obj[-1]`, which is an EID Ref by convention.
///   - eid: query `[receiver :vtable ?vt_eid]` in EAVT.
///   - smallint/keyword: future kernel-vtable EIDs (not in Phase 4 — return null).
fn vtableEidOf(r: Ref) ?u64 {
    if (ref_mod.isPointer(r)) {
        const obj = object.fromRef(r);
        const vt_ref = object.vtableOf(obj);
        if (!ref_mod.isEid(vt_ref)) return null;
        return ref_mod.untagEid(vt_ref);
    }
    if (ref_mod.isEid(r)) {
        const receiver_eid = ref_mod.untagEid(r);
        // Phase 8 promotion shortcut: if the entity has been materialized,
        // its vtable Ref lives at slot -1 of the struct. Skip the EAVT hit.
        if (g_table) |table| {
            if (table.get(receiver_eid)) |ptr_ref| {
                if (ref_mod.isPointer(ptr_ref)) {
                    const obj = object.fromRef(ptr_ref);
                    const vt_ref = object.vtableOf(obj);
                    if (ref_mod.isEid(vt_ref)) return ref_mod.untagEid(vt_ref);
                }
            }
            // Cold: bump hotness, maybe auto-promote.
            table.bumpHotness(receiver_eid);
            promotion.maybeAutoPromote(receiver_eid);
        }
        const store = g_store.?;
        const vt_ref = store.currentValue(receiver_eid, kernel_eids.ATTR_VTABLE) orelse return null;
        if (!ref_mod.isEid(vt_ref)) return null;
        return ref_mod.untagEid(vt_ref);
    }
    // smallint, keyword, reserved — Phase 4 has no vtables for these yet.
    return null;
}

// -- Convenience send wrappers --------------------------------------------

pub fn send0(receiver: Ref, sel: Selector) Ref {
    var s = Send{
        .selector = sel,
        .receiver = receiver,
        .n_args = 0,
        .state = receiver,
        .closure = null,
    };
    const c = bind(&s) orelse return ref_mod.NIL;
    const m: Imp = @ptrCast(@alignCast(c.method));
    return m(&s);
}

pub fn send1(receiver: Ref, sel: Selector, a1: Ref) Ref {
    var s = Send{
        .selector = sel,
        .receiver = receiver,
        .n_args = 1,
        .state = receiver,
        .closure = null,
    };
    const c = bind(&s) orelse return ref_mod.NIL;
    const m: Imp1 = @ptrCast(@alignCast(c.method));
    return m(&s, a1);
}

pub fn send2(receiver: Ref, sel: Selector, a1: Ref, a2: Ref) Ref {
    var s = Send{
        .selector = sel,
        .receiver = receiver,
        .n_args = 2,
        .state = receiver,
        .closure = null,
    };
    const c = bind(&s) orelse return ref_mod.NIL;
    const m: Imp2 = @ptrCast(@alignCast(c.method));
    return m(&s, a1, a2);
}

pub fn send3(receiver: Ref, sel: Selector, a1: Ref, a2: Ref, a3: Ref) Ref {
    var s = Send{
        .selector = sel,
        .receiver = receiver,
        .n_args = 3,
        .state = receiver,
        .closure = null,
    };
    const c = bind(&s) orelse return ref_mod.NIL;
    const m: Imp3 = @ptrCast(@alignCast(c.method));
    return m(&s, a1, a2, a3);
}
