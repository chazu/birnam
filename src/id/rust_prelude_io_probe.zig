//! External-process proof for the Rust Prelude's stdin/stdout boundary.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;

fn loadModule(api: *idapi.IdApi, path: []const u8) !void {
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.ModuleDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.ModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);
}

pub fn main() !void {
    const api = floor.api();
    const prelude_path = std.posix.getenv("BIRNAM_RUST_PRELUDE_DYLIB") orelse
        return error.MissingRustPreludeDylib;
    try loadModule(api, prelude_path);
    const error_path = std.posix.getenv("BIRNAM_ERROR_DYLIB") orelse
        return error.MissingErrorDylib;
    try loadModule(api, error_path);

    const system = api.import("System") orelse return error.SystemMissing;
    const line = api.send(system, api.intern("readLine"), &.{}, 0);
    if (line == api.nilObj) return error.ExpectedInput;
    const echoed = api.send(system, api.intern("println:"), &[_]Oop{line}, 1);
    if (echoed != system) return error.OutputFailed;
}
