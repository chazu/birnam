//! Run the unchanged Zig language rung against the explicit Rust PEG dylib.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");
const lang = @import("lang.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;

fn loadRustPeg(api: *idapi.IdApi) !void {
    const path = std.posix.getenv("BIRNAM_RUST_PEG_DYLIB") orelse
        return error.MissingRustPegDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustPegDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustPegModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);
}

fn parse(api: *idapi.IdApi, source: []const u8) Oop {
    const language = api.import("Lang") orelse @panic("Rust PEG language smoke did not export Lang");
    return api.send(language, api.intern("parse:"), &.{api.mkString(source.ptr, source.len)}, 1);
}

test "unchanged Zig language parses through the explicit Rust PEG matcher" {
    const api = floor.api();
    try loadRustPeg(api);
    lang.bringUp(api);

    const program = parse(api, "let n := 30");
    try std.testing.expect(program != api.nilObj);
    try std.testing.expectEqual(api.intern("program"), api.send(program, api.intern("rule"), &.{}, 0));

    const samples = [_][]const u8{
        "let fizzy := [:n | [[n mod: 3] = 0]]",
        "[System println: [classify value: n]]",
        "[1 to: 20 do: [:n | [System println: n]]]",
        "let primes := [[2 to: n] select: [:k | [isPrime value: k]]]",
        "[[k > 1] & [[hasDivisor value: k] isNil]]",
        "[[\"a\" , \"b\"] printString]",
        "[x ifTrue: [| \"yes\"] ifFalse: [| \"no\"]]",
        "#[#[1 2] #[3 4]]",
        "{\"a\" -> 1, \"b\" -> 2}",
        "let a := 1; let b := 2;",
    };
    for (samples) |source| {
        try std.testing.expect(parse(api, source) != api.nilObj);
    }
    try std.testing.expect(parse(api, "[a ? b]") == api.nilObj);
}
