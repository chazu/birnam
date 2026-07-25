//! Cross-language smoke for the standalone Rust Error family.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;
const Identity = *const fn () callconv(.c) [*:0]const u8;

fn send(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8, args: []const Oop) Oop {
    return api.send(receiver, api.intern(selector), args.ptr, args.len);
}

fn loadModule(api: *idapi.IdApi, path: []const u8) !void {
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.ModuleDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.ModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);
}

fn stringEquals(api: *idapi.IdApi, object: Oop, expected: []const u8) !void {
    var length: usize = 0;
    const bytes = api.stringBytes(object, &length);
    try std.testing.expectEqualStrings(expected, bytes[0..length]);
}

test "Rust Error preserves the standalone family and DNU contract" {
    const api = floor.api();
    const rust_path = std.posix.getenv("BIRNAM_RUST_ERROR_DYLIB") orelse
        return error.MissingRustErrorDylib;
    const zig_path = std.posix.getenv("BIRNAM_ZIG_ERROR_DYLIB") orelse
        return error.MissingZigErrorDylib;
    const order = std.posix.getenv("BIRNAM_ERROR_PRELOAD_ORDER") orelse "rust-zig";
    if (!std.mem.eql(u8, order, "rust-zig") and !std.mem.eql(u8, order, "zig-rust")) {
        return error.InvalidErrorPreloadOrder;
    }
    if (std.mem.eql(u8, order, "zig-rust")) {
        try loadModule(api, zig_path);
    }

    const path_z = try std.heap.c_allocator.dupeZ(u8, rust_path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustErrorDlopenFailed;

    const raw_identity = api.dlsym(handle, "birnam_error_rust_identity") orelse
        return error.RustErrorIdentityMissing;
    const identity: Identity = @ptrCast(@alignCast(raw_identity));
    try std.testing.expectEqualStrings("rust-error-r2-core", std.mem.span(identity()));

    const raw_init = api.dlsym(handle, "module_init") orelse
        return error.RustErrorModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const error_vt = api.import("ErrorVt") orelse return error.ErrorVtMissing;
    const factory_vt = api.import("ErrorFactoryVt") orelse return error.ErrorFactoryVtMissing;
    const factory = api.import("Error") orelse return error.ErrorFactoryMissing;
    const probe = api.alloc(error_vt, 16) orelse return error.ErrorAllocationFailed;
    try std.testing.expectEqual(error_vt, api.vtableOf(probe));
    try std.testing.expectEqual(@as(usize, 16), api.objectPayloadSize(probe));
    try std.testing.expectEqual(idapi.slot_oop, api.slotKind(error_vt, 0));
    try std.testing.expectEqual(factory_vt, api.vtableOf(factory));

    const kind = api.intern("unitError");
    const message = api.mkString("unit failure", 12);
    const error_object = send(api, factory, "kind:message:", &.{ kind, message });
    try std.testing.expectEqual(error_vt, api.vtableOf(error_object));
    try std.testing.expectEqual(api.trueObj, send(api, error_object, "isError", &.{}));
    try std.testing.expectEqual(kind, send(api, error_object, "kind", &.{}));
    try std.testing.expectEqual(message, send(api, error_object, "message", &.{}));
    var propagated = error_object;
    for (0..5) |depth| {
        var selector_buf: [32]u8 = undefined;
        const selector = try std.fmt.bufPrintZ(&selector_buf, "propagate-{d}", .{depth});
        propagated = send(api, propagated, selector, &.{});
    }
    try std.testing.expectEqual(error_object, propagated);

    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    api.gcSetAuto(0);
    api.gcSetPrecise(1);
    defer {
        api.gcSetPrecise(0);
        api.gcSetAuto(1);
    }
    for (0..128) |_| {
        _ = api.mkString("rust-error-gc-churn", 19);
    }
    _ = api.gcCollect();
    try std.testing.expectEqual(error_vt, api.vtableOf(error_object));
    try std.testing.expectEqual(kind, send(api, error_object, "kind", &.{}));
    try std.testing.expectEqual(message, send(api, error_object, "message", &.{}));
    try std.testing.expectEqual(error_object, send(api, error_object, "anything", &.{}));

    try std.testing.expectEqual(api.falseObj, send(api, api.nilObj, "isError", &.{}));
    const missed = send(api, api.nilObj, "frobnicate", &.{});
    try std.testing.expectEqual(error_vt, api.vtableOf(missed));
    try std.testing.expectEqual(api.intern("doesNotUnderstand"), send(api, missed, "kind", &.{}));
    try stringEquals(api, send(api, missed, "message", &.{}), "does not understand #frobnicate");
    try std.testing.expectEqual(missed, send(api, missed, "stillMissing", &.{}));

    try loadModule(api, rust_path);
    try std.testing.expectEqual(error_vt, api.import("ErrorVt"));
    try std.testing.expectEqual(factory_vt, api.import("ErrorFactoryVt"));
    try std.testing.expectEqual(factory, api.import("Error"));

    if (std.mem.eql(u8, order, "rust-zig")) {
        try loadModule(api, zig_path);
    }
    try std.testing.expectEqual(error_vt, api.import("ErrorVt"));
    try std.testing.expectEqual(factory_vt, api.import("ErrorFactoryVt"));
    try std.testing.expectEqual(factory, api.import("Error"));
    try std.testing.expectEqual(error_vt, api.vtableOf(error_object));
    try std.testing.expectEqual(@as(usize, 16), api.objectPayloadSize(error_object));
}
