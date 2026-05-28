const obj = @import("object.zig");
const bootstrap = @import("bootstrap.zig");
const selector = @import("selector.zig");
const send_mod = @import("send.zig");
const vtable_ops = @import("vtable.zig");

/// C ABI struct exposing the id object system to other modules.
/// All pointers are ?*anyopaque for cross-dylib compatibility.
pub const IdApi = extern struct {
    // Bootstrap vtables
    vtable_vt: ?*anyopaque,
    object_vt: ?*anyopaque,
    vector_vt: ?*anyopaque,
    selector_vt: ?*anyopaque,
    closure_vt: ?*anyopaque,
    assoc_vt: ?*anyopaque,

    // Key selectors
    s_lookup: ?*anyopaque,
    s_methodAt_put_with: ?*anyopaque,
    s_delegated: ?*anyopaque,
    s_vtable: ?*anyopaque,
    s_init: ?*anyopaque,

    // Functions
    intern: *const fn ([*:0]const u8) callconv(.c) ?*anyopaque,
    alloc: *const fn (?*anyopaque, usize) callconv(.c) ?*anyopaque,
    send0: *const fn (?*anyopaque, ?*anyopaque) callconv(.c) ?*anyopaque,
    send1: *const fn (?*anyopaque, ?*anyopaque, ?*anyopaque) callconv(.c) ?*anyopaque,
    send2: *const fn (?*anyopaque, ?*anyopaque, ?*anyopaque, ?*anyopaque) callconv(.c) ?*anyopaque,
    send3: *const fn (?*anyopaque, ?*anyopaque, ?*anyopaque, ?*anyopaque, ?*anyopaque) callconv(.c) ?*anyopaque,
    vtable_delegated: *const fn (?*anyopaque) callconv(.c) ?*anyopaque,
    vtable_method_at_put_with: *const fn (?*anyopaque, ?*anyopaque, ?*const anyopaque, ?*anyopaque) callconv(.c) ?*anyopaque,
};

pub var id_api: IdApi = undefined;

/// Populate the IdApi struct from bootstrap globals. Call after bootstrap.init().
pub fn populate() void {
    id_api = .{
        .vtable_vt = @ptrCast(bootstrap.vtable_vt),
        .object_vt = @ptrCast(bootstrap.object_vt),
        .vector_vt = @ptrCast(bootstrap.vector_vt),
        .selector_vt = @ptrCast(bootstrap.selector_vt),
        .closure_vt = @ptrCast(bootstrap.closure_vt),
        .assoc_vt = @ptrCast(bootstrap.assoc_vt),

        .s_lookup = @ptrCast(bootstrap.s_lookup),
        .s_methodAt_put_with = @ptrCast(bootstrap.s_methodAt_put_with),
        .s_delegated = @ptrCast(bootstrap.s_delegated),
        .s_vtable = @ptrCast(bootstrap.s_vtable),
        .s_init = @ptrCast(bootstrap.s_init),

        .intern = &cIntern,
        .alloc = &cAlloc,
        .send0 = &cSend0,
        .send1 = &cSend1,
        .send2 = &cSend2,
        .send3 = &cSend3,
        .vtable_delegated = &cVtableDelegated,
        .vtable_method_at_put_with = &cVtableMethodAtPutWith,
    };
}

// -- C-callable wrappers ----------------------------------------------------

fn cIntern(name: [*:0]const u8) callconv(.c) ?*anyopaque {
    return @ptrCast(selector.intern(name));
}

fn cAlloc(vt: ?*anyopaque, size: usize) callconv(.c) ?*anyopaque {
    const oop: obj.Oop = @ptrCast(@alignCast(vt));
    return @ptrCast(obj.alloc(oop, size));
}

fn cSend0(receiver: ?*anyopaque, sel: ?*anyopaque) callconv(.c) ?*anyopaque {
    return @ptrCast(send_mod.send0(@ptrCast(@alignCast(receiver)), @ptrCast(@alignCast(sel))));
}

fn cSend1(receiver: ?*anyopaque, sel: ?*anyopaque, arg1: ?*anyopaque) callconv(.c) ?*anyopaque {
    return @ptrCast(send_mod.send1(
        @ptrCast(@alignCast(receiver)),
        @ptrCast(@alignCast(sel)),
        @ptrCast(@alignCast(arg1)),
    ));
}

fn cSend2(receiver: ?*anyopaque, sel: ?*anyopaque, arg1: ?*anyopaque, arg2: ?*anyopaque) callconv(.c) ?*anyopaque {
    return @ptrCast(send_mod.send2(
        @ptrCast(@alignCast(receiver)),
        @ptrCast(@alignCast(sel)),
        @ptrCast(@alignCast(arg1)),
        @ptrCast(@alignCast(arg2)),
    ));
}

fn cSend3(receiver: ?*anyopaque, sel: ?*anyopaque, arg1: ?*anyopaque, arg2: ?*anyopaque, arg3: ?*anyopaque) callconv(.c) ?*anyopaque {
    return @ptrCast(send_mod.send3(
        @ptrCast(@alignCast(receiver)),
        @ptrCast(@alignCast(sel)),
        @ptrCast(@alignCast(arg1)),
        @ptrCast(@alignCast(arg2)),
        @ptrCast(@alignCast(arg3)),
    ));
}

fn cVtableDelegated(parent: ?*anyopaque) callconv(.c) ?*anyopaque {
    return @ptrCast(vtable_ops.vtableDelegated(@ptrCast(@alignCast(parent))));
}

fn cVtableMethodAtPutWith(vt: ?*anyopaque, sel: ?*anyopaque, method: ?*const anyopaque, data: ?*anyopaque) callconv(.c) ?*anyopaque {
    return @ptrCast(vtable_ops.vtableMethodAtPutWith(
        @ptrCast(@alignCast(vt)),
        @ptrCast(@alignCast(sel)),
        method,
        @ptrCast(@alignCast(data)),
    ));
}
