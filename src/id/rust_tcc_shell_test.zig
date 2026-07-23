//! N4.1 cross-language test for the explicit Rust TCC dynamic-rung shell.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;
const Identity = *const fn () callconv(.c) [*:0]const u8;

test "Rust TCC shell freezes the current object export boundary" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_TCC_DYLIB") orelse
        return error.MissingRustTccDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustTccDlopenFailed;

    const raw_identity = api.dlsym(handle, "birnam_tcc_rust_identity") orelse
        return error.RustTccIdentityMissing;
    const identity: Identity = @ptrCast(@alignCast(raw_identity));
    try std.testing.expectEqualStrings("rust-tcc-n4.1-shell", std.mem.span(identity()));

    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustTccModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const tcc = api.import("Tcc") orelse return error.TccNotExported;
    const tcc_vt = api.import("TccVt") orelse return error.TccVtNotExported;
    const unit_vt = api.import("TccUnitVt") orelse return error.TccUnitVtNotExported;
    try std.testing.expectEqual(tcc_vt, api.vtableOf(tcc));
    try std.testing.expect(api.vtableOf(unit_vt) != null);
}
