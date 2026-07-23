//! Exercise the unchanged Zig codegen rung after swapping only PEG to Rust.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const artifact_inspector = @import("artifact_inspector.zig");
const store = @import("store.zig");
const lang = @import("lang.zig");
const tcc = @import("tcc.zig");
const codegen = @import("codegen.zig");
const prelude = @import("prelude.zig");
const collection = @import("collection.zig");
const errmod = @import("error.zig");
const idapi = @import("idapi.zig");

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

fn setup() *idapi.IdApi {
    const api = floor.api();
    artifact_inspector.bringUp(api);
    store.bringUp(api);
    loadRustPeg(api) catch @panic("Rust PEG failed to load");
    lang.bringUp(api);
    tcc.bringUp(api);
    codegen.bringUp(api);
    prelude.bringUp(api);
    collection.bringUp(api);
    errmod.bringUp(api);
    return api;
}

fn eval(api: *idapi.IdApi, source: []const u8) Oop {
    const codegen_object = api.import("Codegen") orelse @panic("Codegen was not exported");
    return api.send(codegen_object, api.intern("eval:"), &.{api.mkString(source.ptr, source.len)}, 1);
}

test "unchanged Zig codegen evaluates through the explicit Rust PEG matcher" {
    const api = setup();
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(eval(api, "[1 + 2]")));
    try std.testing.expectEqual(@as(i64, 12), api.smallIntValue(eval(api, "let x := 10; [x + 2]")));
    try std.testing.expectEqual(@as(i64, 6), api.smallIntValue(eval(api, "[[1 to: 3] inject: 0 into: [:sum :n | [sum + n]]]")));
}
