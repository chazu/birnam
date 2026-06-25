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
const vm_mod = @import("vm.zig");
const object = @import("object.zig");
const closure_mod = @import("closure.zig");
const mcache = @import("mcache.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const kernel_eids = @import("kernel_eids.zig");
const promotion = @import("promotion.zig");
const strings_mod = @import("strings.zig");
const dispatch_mod = @import("dispatch.zig");

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

/// Variadic Imp signature (Phase 10.2.5 — β option locked in
/// ADR-10-Send-Arity). Every method takes the Send frame plus a flat
/// argument buffer + count. `args[0..n_args]` are caller-managed; method
/// reads positionally. Zero-arg methods discard `args` and `n_args`.
pub const Imp = *const fn (*Send, [*]const Ref, usize) callconv(.c) Ref;

// -- Globals ---------------------------------------------------------------

var g_store: ?*fact_store.FactStore = null;
var g_table: ?*object_table.ObjectTable = null;
var g_allocator: ?std.mem.Allocator = null;

/// Recursion guard limit. Phase 7.5: consulted before invoking `:bind`
/// override. Above the threshold, bind unconditionally takes the
/// Zig-primitive path — this bounds the depth of override-of-override-of-bind
/// recursion. The counter itself lives in `Vm.recursion_guard` (Phase
/// 11.7.i — ADR-11 #7 per-thread state).
pub const BIND_RECURSION_LIMIT: u32 = 4;

/// Phase 7.5: fact-store mutate hook. Routes most attribute mutations to
/// the standard mcache per-sel flush, but for `:bind` mutations flushes
/// the entire cache — an override installed on a high vtable can affect
/// dispatch for every (vt, sel) pair beneath it.
pub fn onAttrMutate(a: u64) void {
    if (a == kernel_eids.ATTR_BIND) {
        mcache.clearAll();
    } else {
        mcache.flushSel(a);
    }
}

pub fn initDispatch(
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    allocator: std.mem.Allocator,
) void {
    g_store = store;
    g_table = table;
    g_allocator = allocator;
    mcache.clearAll();
    vm_mod.reset();
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

/// The resolution interface (W6 cut #3, ROADMAP D5.2): the one cold-path
/// operation dispatch performs is `get(entity, attribute)` over the fact store —
/// **`bind` *is* `get`.** Cold-path resolution reads the store *only* through
/// `resolve`, never `store.currentValue` directly, so the metacircular seam is a
/// single, auditable surface (grep `resolve(`). `mcache` (hot method lookups)
/// and the object table (materialized vtables, via `vtableEidOf`) are
/// **memoizations** of these gets; the seam is metacircular *by design*, not a
/// layering violation — D5.2 rejects re-architecting it behind a separate
/// projection (anti-Piumarta).
///
/// The **closed set** of (attribute) questions dispatch asks to resolve a send:
///   - `resolve(vt, selector)`          — method lookup `[vt sel ?method]`
///   - `resolve(vt, ATTR_DELEGATE)`     — delegation / inheritance walk
///   - `resolve(vt, ATTR_BIND)`         — custom `bind`-is-a-method override
///   - `resolve(receiver, ATTR_VTABLE)` — vtable resolution
///   - `resolve(receiver, ATTR_NAME)`   — symbol fallback (named ⇒ SYMBOL_VT)
///
/// Adding a new cold-path resolution read means it goes through here,
/// deliberately. (Method-*body* reads — bytecode/const-pool/captures — are a
/// separate concern, `dispatch.zig` cluster 2, memoized by the `Closure`.)
inline fn resolve(entity: u64, attr: u64) ?Ref {
    return g_store.?.currentValue(entity, attr);
}

/// Resolve the closure for `s.receiver` + `s.selector`. Returns null on
/// "method not found." Populates `s.closure` and `s.state` on success.
pub fn bind(s: *Send) ?*Closure {
    const vm = vm_mod.current();
    vm.recursion_guard += 1;
    defer vm.recursion_guard -= 1;

    const vt_eid = vtableEidOf(s.receiver) orelse return null;
    s.state = s.receiver;

    // Hot path
    if (mcache.lookup(vt_eid, s.selector)) |c| {
        s.closure = c;
        return c;
    }

    // Phase 7.5: try `:bind` override along vt's delegate chain. Skipped
    // when recursion guard is tripped so the override can call back into
    // bind without infinite recursion.
    if (vm.recursion_guard <= BIND_RECURSION_LIMIT) {
        if (tryBindOverride(s, vt_eid)) |c| {
            mcache.store(vt_eid, s.selector, c);
            s.closure = c;
            return c;
        }
    }

    // Cold path: walk vt + delegation chain.
    var cur_vt = vt_eid;
    while (cur_vt != kernel_eids.NIL) {
        if (resolve(cur_vt, s.selector)) |method_ref| {
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
        if (resolve(cur_vt, kernel_eids.ATTR_DELEGATE)) |delegate_ref| {
            if (!ref_mod.isEid(delegate_ref)) return null;
            cur_vt = ref_mod.untagEid(delegate_ref);
        } else {
            return null;
        }
    }
    return null;
}

/// Phase 7.5: walk vt's delegate chain looking for an `:bind` override
/// closure. Invoke it with the Send frame; expect closure-EID Ref or NIL.
/// NIL → fallthrough (let primitive lookup handle it). Pointer/other →
/// treat as NIL.
fn tryBindOverride(s: *Send, vt_eid: u64) ?*Closure {
    const table = g_table.?;
    var cur_vt = vt_eid;
    while (cur_vt != kernel_eids.NIL) {
        if (resolve(cur_vt, kernel_eids.ATTR_BIND)) |method_ref| {
            if (!ref_mod.isEid(method_ref)) return null;
            const override_eid = ref_mod.untagEid(method_ref);
            const override_ptr_ref = table.get(override_eid) orelse return null;
            if (!ref_mod.isPointer(override_ptr_ref)) return null;
            const override_c: *Closure = @ptrCast(@alignCast(ref_mod.untagPointer(override_ptr_ref)));
            const m: Imp = @ptrCast(@alignCast(override_c.method));
            var empty: [0]Ref = .{};
            const result = m(s, @ptrCast(&empty), 0);
            if (ref_mod.isNil(result)) return null;
            if (!ref_mod.isEid(result)) return null;
            const result_eid = ref_mod.untagEid(result);
            const result_ptr_ref = table.get(result_eid) orelse return null;
            if (!ref_mod.isPointer(result_ptr_ref)) return null;
            return @ptrCast(@alignCast(ref_mod.untagPointer(result_ptr_ref)));
        }
        if (resolve(cur_vt, kernel_eids.ATTR_DELEGATE)) |d| {
            if (!ref_mod.isEid(d)) return null;
            cur_vt = ref_mod.untagEid(d);
        } else return null;
    }
    return null;
}

/// Determine the vtable EID of a receiver Ref, by tag:
///   - pointer: read `obj[-1]`, which is an EID Ref by convention.
///   - eid: query `[receiver :vtable ?vt_eid]` in EAVT.
///   - smallint: kernel-fixed SMALLINT_VT (Phase 10.0 — arithmetic methods).
///   - keyword: future kernel-vtable EID (not in Phase 10 — return null).
pub fn vtableEidOf(r: Ref) ?u64 {
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
        const vt_ref = resolve(receiver_eid, kernel_eids.ATTR_VTABLE) orelse {
            // No explicit vtable: an interned NAMED entity (a symbol — `#foo`,
            // a selector) dispatches on SYMBOL_VT so it answers =/printString.
            if (resolve(receiver_eid, kernel_eids.ATTR_NAME) != null) return kernel_eids.SYMBOL_VT;
            return null;
        };
        if (!ref_mod.isEid(vt_ref)) return null;
        return ref_mod.untagEid(vt_ref);
    }
    if (ref_mod.isSmallInt(r)) return kernel_eids.SMALLINT_VT;
    if (strings_mod.isString(r)) return kernel_eids.STRING_VT; // Phase 16: strings dispatch
    // keyword, reserved — no kernel vt yet.
    return null;
}

// -- Convenience send wrappers --------------------------------------------

/// Core variadic dispatch. Binds, then invokes the resolved closure with
/// `args[0..n_args]`. send0..send3 are thin packers calling this.
pub fn sendN(receiver: Ref, sel: Selector, args: [*]const Ref, n_args: usize) Ref {
    var s = Send{
        .selector = sel,
        .receiver = receiver,
        .n_args = @intCast(n_args),
        .state = receiver,
        .closure = null,
    };
    const c = bind(&s) orelse {
        // Phase 16 Tier D — method not found: route to `doesNotUnderstand:`
        // with the missing selector as a symbol, IF a handler is installed.
        // Falls back to NIL (the historical miss result) when none exists, and
        // never recurses on a missing DNU itself.
        if (sel == kernel_eids.SEL_DNU) return ref_mod.NIL;
        var dnu = Send{
            .selector = kernel_eids.SEL_DNU,
            .receiver = receiver,
            .n_args = 1,
            .state = receiver,
            .closure = null,
        };
        const dc = bind(&dnu) orelse return ref_mod.NIL;
        const dm: Imp = @ptrCast(@alignCast(dc.method));
        var dargs: [1]Ref = .{ref_mod.mkEid(sel)};
        return dm(&dnu, &dargs, 1);
    };
    // Phase 12.8 — bytecode-method fast path. A compiled method resolves to a
    // BYTECODE_METHOD_VT closure `{&bytecodeDispatcherImpl, mkEid(method_eid)}`.
    // Recognize it at the bind boundary and enter `runMethod` DIRECTLY, lifting
    // the isEid branch out of `bytecodeDispatcherImpl` — no fn-ptr shim call, no
    // re-dispatch. Block closures (data = pointer, not eid) and native
    // primitives (`.method` ≠ dispatcher) keep the honest fn-ptr call below.
    if (c.method == @as(*const anyopaque, @ptrCast(&dispatch_mod.bytecodeDispatcherImpl)) and
        ref_mod.isEid(c.data))
    {
        const store = g_store orelse return ref_mod.NIL;
        const allocator = g_allocator orelse return ref_mod.NIL;
        return dispatch_mod.runMethod(allocator, store, receiver, ref_mod.untagEid(c.data), args[0..n_args], &.{}) catch ref_mod.NIL;
    }
    const m: Imp = @ptrCast(@alignCast(c.method));
    return m(&s, args, n_args);
}

pub fn send0(receiver: Ref, sel: Selector) Ref {
    var empty: [0]Ref = .{};
    return sendN(receiver, sel, @ptrCast(&empty), 0);
}

pub fn send1(receiver: Ref, sel: Selector, a1: Ref) Ref {
    var args: [1]Ref = .{a1};
    return sendN(receiver, sel, &args, 1);
}

pub fn send2(receiver: Ref, sel: Selector, a1: Ref, a2: Ref) Ref {
    var args: [2]Ref = .{ a1, a2 };
    return sendN(receiver, sel, &args, 2);
}

pub fn send3(receiver: Ref, sel: Selector, a1: Ref, a2: Ref, a3: Ref) Ref {
    var args: [3]Ref = .{ a1, a2, a3 };
    return sendN(receiver, sel, &args, 3);
}
