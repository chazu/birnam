const std = @import("std");
const obj = @import("object.zig");
const mcache = @import("mcache.zig");
const vtable_ops = @import("vtable.zig");
const Oop = obj.Oop;

/// The send struct, passed as first arg to all methods.
/// Matches libid.c's `struct __send`.
pub const Send = extern struct {
    selector: Oop,
    n_args: c_int,
    receiver: Oop,
    state: Oop,
    closure: ?*obj.Closure,
};

/// The universal method signature: fn(*Send, ...) Oop
/// In Zig we model the common 0-extra-args case.
pub const Imp = *const fn (*Send) callconv(.c) Oop;
pub const Imp1 = *const fn (*Send, Oop) callconv(.c) Oop;
pub const Imp2 = *const fn (*Send, Oop, Oop) callconv(.c) Oop;
pub const Imp3 = *const fn (*Send, Oop, Oop, Oop) callconv(.c) Oop;

// Selectors needed for dispatch (set during bootstrap)
var s_lookup: Oop = null;
var vtable_vt: Oop = null;

pub fn initDispatch(lookup_sel: Oop, vt_vt: Oop) void {
    s_lookup = lookup_sel;
    vtable_vt = vt_vt;
}

/// Recursion guard — when > 0, use direct vtable_lookup instead of send.
var recursion_guard: u32 = 0;

fn castMethod(comptime T: type, raw: ?*const anyopaque) ?T {
    const ptr = raw orelse return null;
    return @ptrCast(@alignCast(ptr));
}

/// Bind: resolve selector on receiver, returning the closure.
/// This is the core dispatch function matching _libid_bindv.
pub fn bind(send: *Send) ?*obj.Closure {
    const receiver = send.receiver;
    const selector = send.selector;

    const vtable = if (receiver) |r| obj.vtableOf(r) else null;
    if (vtable == null) return null;

    // Check method cache
    if (mcache.lookup(vtable, selector)) |cached| {
        send.state = receiver;
        send.closure = cached;
        return cached;
    }

    // Look up method — recursion guard breaks infinite regress
    var assoc: Oop = null;
    if (recursion_guard > 0) {
        // Direct lookup (no send)
        assoc = vtable_ops.vtableLookup(vtable, selector);
    } else {
        // send(vt, s_lookup, selector) — the normal path
        recursion_guard += 1;
        var lookup_send = Send{
            .selector = s_lookup,
            .n_args = 2,
            .receiver = vtable,
            .state = null,
            .closure = null,
        };
        const lookup_closure = bind(&lookup_send) orelse {
            recursion_guard -= 1;
            return null;
        };
        // Call the lookup method
        const method = castMethod(Imp1, lookup_closure.method) orelse {
            recursion_guard -= 1;
            return null;
        };
        assoc = method(&lookup_send, selector);
        recursion_guard -= 1;
    }

    if (assoc == null) return null;

    // Extract closure from assoc.value
    const assoc_s = obj.asAssoc(assoc) orelse return null;
    const closure = obj.asClosure(assoc_s.value) orelse return null;

    // Cache it
    mcache.store(vtable, selector, closure);

    send.state = receiver;
    send.closure = closure;
    return closure;
}

// -- Convenience send wrappers ----------------------------------------------

/// send(receiver, selector) → 0-arg message
pub fn send0(receiver: Oop, selector: Oop) Oop {
    var s = Send{
        .selector = selector,
        .n_args = 1,
        .receiver = receiver,
        .state = null,
        .closure = null,
    };
    const closure = bind(&s) orelse return null;
    const method = castMethod(Imp, closure.method) orelse return null;
    return method(&s);
}

/// send(receiver, selector, arg1)
pub fn send1(receiver: Oop, selector: Oop, arg1: Oop) Oop {
    var s = Send{
        .selector = selector,
        .n_args = 2,
        .receiver = receiver,
        .state = null,
        .closure = null,
    };
    const closure = bind(&s) orelse return null;
    const method = castMethod(Imp1, closure.method) orelse return null;
    return method(&s, arg1);
}

/// send(receiver, selector, arg1, arg2)
pub fn send2(receiver: Oop, selector: Oop, arg1: Oop, arg2: Oop) Oop {
    var s = Send{
        .selector = selector,
        .n_args = 3,
        .receiver = receiver,
        .state = null,
        .closure = null,
    };
    const closure = bind(&s) orelse return null;
    const method = castMethod(Imp2, closure.method) orelse return null;
    return method(&s, arg1, arg2);
}

/// send(receiver, selector, arg1, arg2, arg3)
pub fn send3(receiver: Oop, selector: Oop, arg1: Oop, arg2: Oop, arg3: Oop) Oop {
    var s = Send{
        .selector = selector,
        .n_args = 4,
        .receiver = receiver,
        .state = null,
        .closure = null,
    };
    const closure = bind(&s) orelse return null;
    const method = castMethod(Imp3, closure.method) orelse return null;
    return method(&s, arg1, arg2, arg3);
}
