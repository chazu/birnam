/// IdApi struct definition — mirrors src/id/api.zig's IdApi.
/// Defined locally so the lang module has no compile-time dependency on the id module.
/// At runtime, lang receives a pointer to the id module's actual IdApi via import_value.
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
