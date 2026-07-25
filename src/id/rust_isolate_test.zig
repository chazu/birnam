//! Cross-language probe for the first Rust Isolate wire slice.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const Identity = *const fn () callconv(.c) [*:0]const u8;
const EncodeHeader = *const fn (u8, u64, u32, ?[*]u8) callconv(.c) c_int;
const DecodeHeader = *const fn (
    ?[*]const u8,
    usize,
    ?*u8,
    ?*u64,
    ?*u32,
) callconv(.c) c_int;
const WorkerDriver = *const fn (?*anyopaque, c_int, ?*anyopaque, ?*anyopaque) callconv(.c) c_int;
const WorkerBootError = *const fn (
    ?*anyopaque,
    c_int,
    [*:0]const u8,
    [*]const u8,
    usize,
) callconv(.c) void;
const Lifecycle = *const fn () callconv(.c) void;
const EncodeOop = *const fn (
    ?*anyopaque,
    ?*anyopaque,
    ?[*]u8,
    usize,
    ?*usize,
) callconv(.c) c_int;
const ModuleInit = *const fn (?*anyopaque, ?*anyopaque) callconv(.c) ?*anyopaque;

test "Rust Isolate exports the exact BISO header probe" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_ISOLATE_DYLIB") orelse
        return error.MissingRustIsolateDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustIsolateDlopenFailed;

    const raw_identity = api.dlsym(handle, "birnam_isolate_rust_identity") orelse
        return error.RustIsolateIdentityMissing;
    const identity: Identity = @ptrCast(@alignCast(raw_identity));
    try std.testing.expectEqualStrings("rust-isolate-r3-codec", std.mem.span(identity()));

    const raw_encode = api.dlsym(handle, "birnam_isolate_rust_encode_header") orelse
        return error.RustIsolateHeaderEncoderMissing;
    const encode: EncodeHeader = @ptrCast(@alignCast(raw_encode));
    var bytes: [20]u8 = undefined;
    try std.testing.expectEqual(
        @as(c_int, 0),
        encode(3, 0x0102030405060708, 0x00123344, &bytes),
    );
    try std.testing.expectEqualSlices(u8, &.{
        'B',  'I',  'S',  'O',  0, 1, 3, 0,
        1,    2,    3,    4,    5, 6, 7, 8,
        0x00, 0x12, 0x33, 0x44,
    }, &bytes);
    try std.testing.expectEqual(@as(c_int, 5), encode(3, 0, 0, &bytes));
    try std.testing.expectEqual(@as(c_int, -1), encode(3, 1, 0, null));

    const raw_decode = api.dlsym(handle, "birnam_isolate_rust_decode_header") orelse
        return error.RustIsolateHeaderDecoderMissing;
    const decode: DecodeHeader = @ptrCast(@alignCast(raw_decode));
    var decoded_kind: u8 = 0;
    var decoded_request_id: u64 = 0;
    var decoded_payload_len: u32 = 0;
    try std.testing.expectEqual(
        @as(c_int, 0),
        decode(&bytes, bytes.len, &decoded_kind, &decoded_request_id, &decoded_payload_len),
    );
    try std.testing.expectEqual(@as(u8, 3), decoded_kind);
    try std.testing.expectEqual(@as(u64, 0x0102030405060708), decoded_request_id);
    try std.testing.expectEqual(@as(u32, 0x00123344), decoded_payload_len);
    bytes[0] = 'X';
    try std.testing.expectEqual(
        @as(c_int, 1),
        decode(&bytes, bytes.len, &decoded_kind, &decoded_request_id, &decoded_payload_len),
    );
    try std.testing.expectEqual(
        @as(c_int, -1),
        decode(null, bytes.len, &decoded_kind, &decoded_request_id, &decoded_payload_len),
    );

    const raw_driver = api.dlsym(handle, "birnam_isolate_worker_driver") orelse
        return error.RustIsolateWorkerDriverMissing;
    const raw_boot_error = api.dlsym(handle, "birnam_isolate_worker_boot_error") orelse
        return error.RustIsolateWorkerBootErrorMissing;
    const raw_drain = api.dlsym(handle, "birnam_isolate_drain") orelse
        return error.RustIsolateDrainMissing;
    const raw_shutdown = api.dlsym(handle, "birnam_isolate_shutdown") orelse
        return error.RustIsolateShutdownMissing;
    const driver: WorkerDriver = @ptrCast(@alignCast(raw_driver));
    const boot_error: WorkerBootError = @ptrCast(@alignCast(raw_boot_error));
    const drain: Lifecycle = @ptrCast(@alignCast(raw_drain));
    const shutdown: Lifecycle = @ptrCast(@alignCast(raw_shutdown));
    const raw_encode_oop = api.dlsym(handle, "birnam_isolate_rust_encode_oop") orelse
        return error.RustIsolateOopEncoderMissing;
    const encode_oop: EncodeOop = @ptrCast(@alignCast(raw_encode_oop));
    _ = driver;
    _ = boot_error;
    drain();
    shutdown();

    var oop_bytes: [64]u8 = undefined;
    var oop_len: usize = 0;
    try std.testing.expectEqual(
        @as(c_int, 0),
        encode_oop(api, api.mkSmallInt(-42), &oop_bytes, oop_bytes.len, &oop_len),
    );
    try std.testing.expectEqual(@as(usize, 9), oop_len);
    try std.testing.expectEqualSlices(u8, &.{
        3, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xd6,
    }, oop_bytes[0..oop_len]);

    try std.testing.expectEqual(
        @as(c_int, 0),
        encode_oop(api, api.mkFloat(-0.0), &oop_bytes, oop_bytes.len, &oop_len),
    );
    try std.testing.expectEqual(@as(usize, 9), oop_len);
    try std.testing.expectEqualSlices(u8, &.{
        4, 0x80, 0, 0, 0, 0, 0, 0, 0,
    }, oop_bytes[0..oop_len]);

    const string_bytes = [_]u8{ 'a', 0, 'b' };
    const string = api.mkString(&string_bytes, string_bytes.len);
    try std.testing.expectEqual(
        @as(c_int, 0),
        encode_oop(api, string, &oop_bytes, oop_bytes.len, &oop_len),
    );
    try std.testing.expectEqual(@as(usize, 8), oop_len);
    try std.testing.expectEqualSlices(u8, &.{ 5, 0, 0, 0, 3, 'a', 0, 'b' }, oop_bytes[0..oop_len]);

    const symbol = api.intern("hello");
    try std.testing.expectEqual(
        @as(c_int, 0),
        encode_oop(api, symbol, &oop_bytes, oop_bytes.len, &oop_len),
    );
    try std.testing.expectEqual(@as(usize, 10), oop_len);
    try std.testing.expectEqualSlices(u8, &.{ 6, 0, 0, 0, 5, 'h', 'e', 'l', 'l', 'o' }, oop_bytes[0..oop_len]);

    try std.testing.expectEqual(
        @as(c_int, 19),
        encode_oop(api, api.object_vt, &oop_bytes, oop_bytes.len, &oop_len),
    );
    try std.testing.expectEqual(
        @as(c_int, -2),
        encode_oop(api, api.mkSmallInt(1), oop_bytes[0..1], 1, &oop_len),
    );
    try std.testing.expectEqual(@as(usize, 9), oop_len);

    const collection_path = std.posix.getenv("BIRNAM_RUST_COLLECTION_DYLIB") orelse
        return error.MissingRustCollectionDylib;
    const collection_path_z = try std.heap.c_allocator.dupeZ(u8, collection_path);
    defer std.heap.c_allocator.free(collection_path_z);
    const collection_handle = api.dlopen(collection_path_z.ptr, 1) orelse
        return error.RustCollectionDlopenFailed;
    const raw_collection_init = api.dlsym(collection_handle, "module_init") orelse
        return error.RustCollectionModuleInitMissing;
    const collection_init: ModuleInit = @ptrCast(@alignCast(raw_collection_init));
    _ = collection_init(api, null);

    const vector_global = api.import("Vector");
    var vector = api.send(vector_global, api.intern("new"), &.{}, 0);
    const vector_items = [_]idapi.Oop{ api.mkSmallInt(11), api.mkString("ok", 2) };
    for (vector_items) |item| vector = api.send(vector, api.intern("add:"), &[_]idapi.Oop{item}, 1);
    try std.testing.expectEqual(
        @as(c_int, 0),
        encode_oop(api, vector, &oop_bytes, oop_bytes.len, &oop_len),
    );
    try std.testing.expectEqualSlices(u8, &.{
        7,   0, 0, 0,  2,
        3,   0, 0, 0,  0,
        0,   0, 0, 11, 5,
        0,   0, 0, 2,  'o',
        'k',
    }, oop_bytes[0..oop_len]);

    const map_global = api.import("Map");
    var map = api.send(map_global, api.intern("new"), &.{}, 0);
    const key = api.intern("answer");
    map = api.send(map, api.intern("at:put:"), &[_]idapi.Oop{ key, api.mkSmallInt(42) }, 2);
    try std.testing.expectEqual(
        @as(c_int, 0),
        encode_oop(api, map, &oop_bytes, oop_bytes.len, &oop_len),
    );
    try std.testing.expectEqualSlices(u8, &.{
        8,   0,   0,   0,   1,
        6,   0,   0,   0,   6,
        'a', 'n', 's', 'w', 'e',
        'r', 3,   0,   0,   0,
        0,   0,   0,   0,   42,
    }, oop_bytes[0..oop_len]);
}
