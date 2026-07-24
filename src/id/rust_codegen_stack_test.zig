//! Exercise the Rust Codegen ↔ Rust Store callback contract without a Zig
//! Codegen export. This is deliberately a separate process from the mixed
//! runtime suite so the Store single-owner law is tested on both sides.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const lang = @import("lang.zig");
const idapi = @import("idapi.zig");

const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;

fn load(api: *idapi.IdApi, variable: []const u8) !void {
    const path = std.posix.getenv(variable) orelse return error.MissingDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.DlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.ModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);
}

fn setup() !*idapi.IdApi {
    const api = floor.api();
    try load(api, "BIRNAM_RUST_PEG_DYLIB");
    lang.bringUp(api);
    try load(api, "BIRNAM_RUST_TCC_DYLIB");
    try load(api, "BIRNAM_RUST_STORE_DYLIB");
    try load(api, "BIRNAM_RUST_CODEGEN_DYLIB");
    // Do not bring up Zig Codegen here. Rust Codegen must become the sole
    // Store callback owner in this process.
    return api;
}

fn probeAnswer(_: idapi.Oop, _: [*]const idapi.Oop, _: usize) callconv(.c) idapi.Oop {
    return @ptrFromInt(77);
}

test "Rust Codegen owns Rust Store callbacks after collection" {
    const api = try setup();
    const rust_codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const store = api.import("Store") orelse return error.MissingStore;

    // Rust-only hosts make callback ownership explicit after the complete
    // stack is selected. Mixed hosts omit this call and retain Zig Codegen's
    // already-registered Store owner.
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("enterRunning"), &.{}, 0));

    // The lifecycle transition installs Rust Codegen as the single Store
    // observer and raw authorizer. Force collection before invoking either callback: the
    // callback receiver and candidate registry must remain rooted by the Rust
    // provider/export environment, not by a transient Rust or Zig stack frame.
    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    _ = api.gcCollect();

    const integer_vt = api.vtableOf(api.mkSmallInt(3));
    const selector = api.intern("rustOnlyCallbackProbe");
    const method: idapi.Oop = @ptrCast(@constCast(&probeAnswer));

    // Running-phase mutation is denied by the Rust authorizer before either
    // dictionary or Store mirror changes.
    try std.testing.expectEqual(
        api.nilObj,
        api.send(store, api.intern("addMethod:on:with:"), &.{ selector, integer_vt, method }, 3),
    );
    try std.testing.expectEqual(
        null,
        api.send(store, api.intern("rawCurrentValue:attr:"), &.{ integer_vt, selector }, 2),
    );

    // Quiescence authorizes the same mutation; completion calls back into the
    // surviving Rust observer, then dispatch observes the exact installed fn.
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("beginQuiescence"), &.{}, 0));
    try std.testing.expectEqual(
        store,
        api.send(store, api.intern("addMethod:on:with:"), &.{ selector, integer_vt, method }, 3),
    );
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("endQuiescence"), &.{}, 0));
    try std.testing.expectEqual(@as(usize, 77), @intFromPtr(api.send(api.mkSmallInt(3), selector, &.{}, 0)));
}
