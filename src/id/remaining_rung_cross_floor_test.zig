//! One backend-neutral Prelude corpus, run in separate Zig- and Rust-selected
//! processes. This is an R0 harness: it proves selection/observation plumbing,
//! not canonical Rust behavior for the later remaining rungs.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");
const prelude = @import("prelude.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;

fn send(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8, args: []const Oop) Oop {
    return api.send(receiver, api.intern(selector), args.ptr, args.len);
}

fn stringBytes(api: *idapi.IdApi, object: Oop) []const u8 {
    var len: usize = 0;
    return api.stringBytes(object, &len)[0..len];
}

fn installSelectedPrelude(api: *idapi.IdApi) !void {
    const implementation = std.posix.getenv("BIRNAM_PRELUDE_IMPL") orelse
        return error.MissingPreludeImplementation;
    if (std.mem.eql(u8, implementation, "zig")) {
        prelude.bringUp(api);
        return;
    }
    if (!std.mem.eql(u8, implementation, "rust")) return error.UnknownPreludeImplementation;
    const path = std.posix.getenv("BIRNAM_PRELUDE_DYLIB") orelse
        return error.MissingPreludeDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.PreludeDlopenFailed;
    const raw = api.dlsym(handle, "module_init") orelse return error.PreludeModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw));
    _ = init(api, null);
}

test "remaining rung Prelude corpus is identical across selected floors" {
    const api = floor.api();
    try installSelectedPrelude(api);

    const three = api.mkSmallInt(3);
    const five = api.mkSmallInt(5);
    try std.testing.expectEqual(@as(i64, 8), api.smallIntValue(send(api, three, "+", &.{five})));
    try std.testing.expectEqual(@as(i64, -2), api.smallIntValue(send(api, three, "-", &.{five})));
    try std.testing.expectEqual(api.trueObj, send(api, three, "<", &.{five}));
    try std.testing.expectEqualStrings("-42", stringBytes(api, send(api, api.mkSmallInt(-42), "printString", &.{})));
    try std.testing.expectEqualStrings("#r0Prelude", stringBytes(api, send(api, api.intern("r0Prelude"), "printString", &.{})));
    try std.testing.expectEqual(api.falseObj, send(api, api.trueObj, "not", &.{}));

    const system = api.import("System") orelse return error.SystemExportMissing;
    _ = system;
    const system_vt = api.import("SystemVt") orelse return error.SystemVtableMissing;
    const required = [_][]const u8{
        "print:", "println:", "readFile:", "writeFile:contents:", "getEnv:",
        // `load:` remains a deliberate R0.5 completion item: the current Rust
        // Prelude has no Codegen source-eval owner yet, so it is not in the
        // common parity subset.
        "loadModule:", "readLine", "gc", "liveObjects", "autoGc:", "preciseGc:",
    };
    for (required) |name| {
        var found = false;
        var index: usize = 0;
        while (api.vtableSelectorAt(system_vt, index)) |selector| : (index += 1) {
            if (std.mem.eql(u8, name, std.mem.span(api.symbolName(selector)))) {
                found = true;
                break;
            }
        }
        try std.testing.expect(found);
    }
}
