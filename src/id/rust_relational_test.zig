//! Explicit Rust Relational value-classification probe.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (?*anyopaque, ?*anyopaque) callconv(.c) ?*anyopaque;
const Identity = *const fn () callconv(.c) [*:0]const u8;
const Classified = extern struct {
    tag: u8,
    reserved: [7]u8,
    integer: i64,
    float: f64,
    identity: u64,
    byte_len: usize,
};
const Classify = *const fn (Oop, ?*Classified, ?[*]u8, usize) callconv(.c) c_int;

test "Rust Relational classifier copies scalar values and preserves identities" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB") orelse
        return error.MissingRustRelationalDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustRelationalDlopenFailed;
    const init_raw = api.dlsym(handle, "module_init") orelse return error.RustRelationalModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(init_raw));
    _ = init(api, null);
    _ = init(api, null);

    const identity_raw = api.dlsym(handle, "birnam_relational_rust_identity") orelse
        return error.RustRelationalIdentityMissing;
    const identity: Identity = @ptrCast(@alignCast(identity_raw));
    try std.testing.expectEqualStrings("rust-relational-r4.3-kernel", std.mem.span(identity()));

    const classify_raw = api.dlsym(handle, "birnam_relational_rust_classify") orelse
        return error.RustRelationalClassifyMissing;
    const classify: Classify = @ptrCast(@alignCast(classify_raw));
    var output: Classified = undefined;
    var bytes: [32]u8 = undefined;

    try std.testing.expectEqual(@as(c_int, 0), classify(api.mkSmallInt(42), &output, &bytes, bytes.len));
    try std.testing.expectEqual(@as(u8, 1), output.tag);
    try std.testing.expectEqual(@as(i64, 42), output.integer);

    try std.testing.expectEqual(@as(c_int, 0), classify(api.mkFloat(3.5), &output, &bytes, bytes.len));
    try std.testing.expectEqual(@as(u8, 2), output.tag);
    try std.testing.expectEqual(@as(f64, 3.5), output.float);

    const text = api.mkString("hello".ptr, 5);
    try std.testing.expectEqual(@as(c_int, 0), classify(text, &output, &bytes, bytes.len));
    try std.testing.expectEqual(@as(u8, 3), output.tag);
    try std.testing.expectEqual(@as(usize, 5), output.byte_len);
    try std.testing.expectEqualStrings("hello", bytes[0..output.byte_len]);
    try std.testing.expectEqual(@as(c_int, -4), classify(text, &output, &bytes, 2));

    const symbol = api.intern("relational_identity_probe");
    try std.testing.expectEqual(@as(c_int, 0), classify(symbol, &output, &bytes, bytes.len));
    try std.testing.expectEqual(@as(u8, 0), output.tag);
    try std.testing.expectEqual(@intFromPtr(symbol), @as(usize, @intCast(output.identity)));
    try std.testing.expectEqual(@as(c_int, -1), classify(symbol, null, &bytes, bytes.len));
}
