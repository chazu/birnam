//! A tiny reloadable module — proves arbitrary (non-id) state survives a hot
//! reload. `module_init(api, state)` threads an opaque `*Counter`: `null` on the
//! first load (allocate fresh), the SAME pointer on every reload. The module's
//! *code* re-runs each load; its *data* persists in the threaded state. Nothing
//! here touches libid's internals — it only holds the IdApi *type* and an opaque
//! pointer, so the floor's reflectivity is untouched.

const std = @import("std");
const idapi = @import("idapi.zig");

const Counter = struct { count: u64 };

pub export fn module_init(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = api; // a real module would register objects via the api; this one is pure state.
    const c: *Counter = if (state) |s|
        @ptrCast(@alignCast(s))
    else
        (std.heap.c_allocator.create(Counter) catch @panic("oom"));
    if (state == null) c.count = 0;
    c.count += 1;
    std.debug.print("[counter] init: count={d} ({s})\n", .{
        c.count,
        if (state == null) "fresh" else "preserved across reload",
    });
    return c;
}
