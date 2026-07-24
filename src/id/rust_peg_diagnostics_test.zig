//! Language-facing diagnostic parity for the explicit Rust PEG backend.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const peg = @import("peg.zig");
const lang = @import("lang.zig");
const err_mod = @import("error.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;

const malformed = [_][]const u8{
    "let := 30",
    "[n printString",
    "@#$%",
    "[justAVar]",
    "let n := 30 garbage",
};

fn bringPeg(api: *idapi.IdApi) !void {
    if (std.posix.getenv("BIRNAM_RUST_PEG_DYLIB")) |path| {
        const path_z = try std.heap.c_allocator.dupeZ(u8, path);
        defer std.heap.c_allocator.free(path_z);
        const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustPegDlopenFailed;
        const raw_init = api.dlsym(handle, "module_init") orelse return error.RustPegModuleInitMissing;
        const init: ModuleInit = @ptrCast(@alignCast(raw_init));
        _ = init(api, null);
    } else {
        peg.bringUp(api);
    }
}

fn bringLang(api: *idapi.IdApi) !void {
    if (std.posix.getenv("BIRNAM_RUST_LANG_DYLIB")) |path| {
        const path_z = try std.heap.c_allocator.dupeZ(u8, path);
        defer std.heap.c_allocator.free(path_z);
        const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustLangDlopenFailed;
        const raw_init = api.dlsym(handle, "module_init") orelse return error.RustLangModuleInitMissing;
        const init: ModuleInit = @ptrCast(@alignCast(raw_init));
        _ = init(api, null);
    } else {
        lang.bringUp(api);
    }
}

fn send0(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8) Oop {
    return api.send(receiver, api.intern(selector), &.{}, 0);
}

fn send1(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8, arg: Oop) Oop {
    return api.send(receiver, api.intern(selector), &.{arg}, 1);
}

fn expectedMessage(source: []const u8, offset: i64, buffer: *[128]u8) []const u8 {
    const bounded: usize = @intCast(@min(@max(offset, 0), @as(i64, @intCast(source.len))));
    var line: usize = 1;
    var column: usize = 1;
    for (source[0..bounded]) |byte| {
        if (byte == '\n') {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    return std.fmt.bufPrint(buffer, "syntax error at line {d}, col {d}", .{ line, column }) catch
        @panic("diagnostic buffer too small");
}

test "Rust PEG preserves language syntax-error diagnostics" {
    const api = floor.api();
    try bringPeg(api);
    err_mod.bringUp(api);
    try bringLang(api);

    const lang_obj = api.import("Lang") orelse return error.LangNotExported;
    for (malformed) |source| {
        const result = send1(api, lang_obj, "parse:", api.mkString(source.ptr, source.len));
        try std.testing.expect(result != api.nilObj);
        try std.testing.expectEqual(api.trueObj, send0(api, result, "isError"));
        try std.testing.expectEqual(api.intern("syntaxError"), send0(api, result, "kind"));

        const offset = api.smallIntValue(send0(api, lang_obj, "furthestFail"));
        var expected_buffer: [128]u8 = undefined;
        const expected = expectedMessage(source, offset, &expected_buffer);
        var message_len: usize = 0;
        const actual = api.stringBytes(send0(api, result, "message"), &message_len);
        try std.testing.expectEqualStrings(expected, actual[0..message_len]);
    }
}
