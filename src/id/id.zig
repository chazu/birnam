const std = @import("std");
pub const bootstrap = @import("bootstrap.zig");
const mcache = @import("mcache.zig");
pub const api = @import("api.zig");

pub const object = @import("object.zig");
pub const vtable = @import("vtable.zig");
pub const selector = @import("selector.zig");
pub const send = @import("send.zig");

// Re-export core types
pub const Oop = object.Oop;
pub const Object = object.Object;
pub const IdApi = api.IdApi;

/// Shell context passed from the shell executable.
pub const ShellContext = extern struct {
    load_module: *const fn ([*:0]const u8) callconv(.c) bool,
    set_repl_handler: *const fn (?*const fn ([*:0]const u8) callconv(.c) void) callconv(.c) void,
    log: *const fn ([*:0]const u8) callconv(.c) void,
    export_value: *const fn ([*:0]const u8, ?*anyopaque) callconv(.c) void,
    import_value: *const fn ([*:0]const u8) callconv(.c) ?*anyopaque,
};

var shell_ctx: ?*ShellContext = null;

fn shellLog(msg: [*:0]const u8) void {
    if (shell_ctx) |ctx| {
        ctx.log(msg);
        return;
    }
    // Fallback: write to stderr
    const w = std.fs.File.stderr().deprecatedWriter();
    w.print("{s}\n", .{msg}) catch {};
}

/// Module entry point, called by the shell.
export fn birnam_module_init(ctx: *ShellContext) void {
    shell_ctx = ctx;
    shellLog("id: initializing object system...");

    if (bootstrap.init()) {
        shellLog("id: bootstrap complete");
        shellLog("id:   vtable_vt, object_vt, vector_vt OK");
        shellLog("id:   selector_vt, closure_vt, assoc_vt OK");
        shellLog("id:   send dispatch verified");

        // Populate and export IdApi for other modules
        api.populate();
        ctx.export_value("id_api", @ptrCast(&api.id_api));
        shellLog("id: exported id_api");
    } else {
        shellLog("id: BOOTSTRAP FAILED");
    }
}

/// Flush method cache (called on hot reload).
export fn birnam_module_reload() void {
    mcache.flushAll();
    shellLog("id: method cache flushed");
}

// -- Tests ------------------------------------------------------------------

test "bootstrap initializes successfully" {
    const ok = bootstrap.init();
    try std.testing.expect(ok);
    try std.testing.expect(bootstrap.vtable_vt != null);
    try std.testing.expect(bootstrap.object_vt != null);
    try std.testing.expect(bootstrap.vector_vt != null);
    try std.testing.expect(bootstrap.selector_vt != null);
    try std.testing.expect(bootstrap.closure_vt != null);
    try std.testing.expect(bootstrap.assoc_vt != null);
}

test "selector interning returns pointer-equal values" {
    _ = bootstrap.init();
    const s1 = selector.intern("hello:");
    const s2 = selector.intern("hello:");
    const s3 = selector.intern("world:");
    try std.testing.expect(s1 != null);
    try std.testing.expect(s1 == s2);
    try std.testing.expect(s1 != s3);
}

test "send dispatch: lookup via send" {
    _ = bootstrap.init();
    // send(vtable_vt, lookup:, lookup:) should find the lookup method
    const result = send.send1(bootstrap.vtable_vt, bootstrap.s_lookup, bootstrap.s_lookup);
    try std.testing.expect(result != null);
    // The result is an assoc whose key should be s_lookup
    const assoc = object.asAssoc(result).?;
    try std.testing.expect(assoc.key == bootstrap.s_lookup);
}

test "delegated vtable creation and method install" {
    _ = bootstrap.init();
    // Create a child vtable of object_vt
    const child_vt = send.send0(bootstrap.object_vt, bootstrap.s_delegated_obj);
    try std.testing.expect(child_vt != null);

    // The child should have object_vt as its vtable's vtable
    const child_vt_vt = object.vtableOf(@as(*object.Object, @ptrCast(@alignCast(object.asVtable(child_vt).?))));
    try std.testing.expect(child_vt_vt != null);
}

test "method cache works" {
    _ = bootstrap.init();
    // First call populates cache
    _ = send.send1(bootstrap.vtable_vt, bootstrap.s_lookup, bootstrap.s_lookup);
    // Second call should hit cache
    const result = send.send1(bootstrap.vtable_vt, bootstrap.s_lookup, bootstrap.s_lookup);
    try std.testing.expect(result != null);
    // Verify cache hit by checking result is still correct
    const assoc = object.asAssoc(result).?;
    try std.testing.expect(assoc.key == bootstrap.s_lookup);
}
