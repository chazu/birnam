//! Cross-language smoke for the first Rust N7.1 primitive-prelude slice.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;
const Identity = *const fn () callconv(.c) [*:0]const u8;

fn send(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8, args: []const Oop) Oop {
    return api.send(receiver, api.intern(selector), args.ptr, args.len);
}

test "Rust Prelude N7.1 installs integer protocol through a dynamic rung" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_PRELUDE_DYLIB") orelse
        return error.MissingRustPreludeDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustPreludeDlopenFailed;

    const raw_identity = api.dlsym(handle, "birnam_prelude_rust_identity") orelse
        return error.RustPreludeIdentityMissing;
    const identity: Identity = @ptrCast(@alignCast(raw_identity));
    try std.testing.expectEqualStrings("rust-prelude-n7.1-integer", std.mem.span(identity()));

    const raw_init = api.dlsym(handle, "module_init") orelse
        return error.RustPreludeModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const three = api.mkSmallInt(3);
    const five = api.mkSmallInt(5);
    try std.testing.expectEqual(@as(i64, 8), api.smallIntValue(send(api, three, "+", &.{five})));
    try std.testing.expectEqual(@as(i64, -2), api.smallIntValue(send(api, three, "-", &.{five})));
    try std.testing.expectEqual(@as(i64, 15), api.smallIntValue(send(api, three, "*", &.{five})));
    try std.testing.expectEqual(@as(i64, 0), api.smallIntValue(send(api, three, "/", &.{five})));
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(send(api, api.mkSmallInt(13), "mod:", &.{five})));
    try std.testing.expectEqual(api.trueObj, send(api, three, "<", &.{five}));
    try std.testing.expectEqual(api.trueObj, send(api, five, ">=", &.{three}));
    try std.testing.expectEqual(api.trueObj, send(api, three, "=", &.{api.mkSmallInt(3)}));

    const rendered = send(api, api.mkSmallInt(-42), "printString", &.{});
    var rendered_len: usize = 0;
    const rendered_bytes = api.stringBytes(rendered, &rendered_len);
    try std.testing.expectEqualStrings("-42", rendered_bytes[0..rendered_len]);
    try std.testing.expect(api.isFloat(send(api, three, "asFloat", &.{})) != 0);
}
