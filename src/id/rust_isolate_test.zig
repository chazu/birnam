//! Cross-language probe for the first Rust Isolate wire slice.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");

const Identity = *const fn () callconv(.c) [*:0]const u8;
const EncodeHeader = *const fn (u8, u64, u32, ?[*]u8) callconv(.c) c_int;
const DecodeHeader = *const fn (
    ?[*]const u8,
    usize,
    ?*u8,
    ?*u64,
    ?*u32,
) callconv(.c) c_int;

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
}
