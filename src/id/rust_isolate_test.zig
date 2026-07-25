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
const DecodeOop = *const fn (
    ?*anyopaque,
    ?[*]const u8,
    usize,
    ?*idapi.Oop,
) callconv(.c) c_int;
const ModuleInit = *const fn (?*anyopaque, ?*anyopaque) callconv(.c) ?*anyopaque;

const WorkerClient = struct {
    fd: std.posix.fd_t,
    result: *c_int,
};

fn echoArgument(self: idapi.Oop, args: [*]const idapi.Oop, count: usize) callconv(.c) idapi.Oop {
    if (count != 1) return null;
    if (floor.api().isSmallInt(args[0]) != 0 and floor.api().smallIntValue(args[0]) == 99)
        return self;
    return args[0];
}

fn runWorkerClient(client: WorkerClient) void {
    client.result.* = workerClientImpl(client.fd) catch -1;
}

fn workerClientImpl(fd: std.posix.fd_t) !c_int {
    var header: [20]u8 = undefined;
    try readExact(fd, &header);
    if (!std.mem.eql(u8, &header, &.{
        'B', 'I', 'S', 'O', 0, 1, 1, 0,
        0,   0,   0,   0,   0, 0, 0, 0,
        0,   0,   0,   0,
    })) return error.InvalidReady;

    var request: [29]u8 = undefined;
    @memcpy(request[0..4], "BISO");
    std.mem.writeInt(u16, request[4..6], 1, .big);
    request[6] = 3;
    request[7] = 0;
    std.mem.writeInt(u64, request[8..16], 1, .big);
    std.mem.writeInt(u32, request[16..20], 9, .big);
    request[20..].* = .{ 3, 0, 0, 0, 0, 0, 0, 0, 23 };
    try writeAll(fd, &request);
    try readExact(fd, &header);
    if (header[6] != 4 or std.mem.readInt(u64, header[8..16], .big) != 1 or
        std.mem.readInt(u32, header[16..20], .big) != 9) return error.InvalidReply;
    var payload: [9]u8 = undefined;
    try readExact(fd, &payload);
    if (!std.mem.eql(u8, &payload, &.{ 3, 0, 0, 0, 0, 0, 0, 0, 23 })) return error.InvalidPayload;

    std.mem.writeInt(u64, request[8..16], 2, .big);
    request[28] = 7;
    try writeAll(fd, &request);
    try readExact(fd, &header);
    if (header[6] != 4 or std.mem.readInt(u64, header[8..16], .big) != 2) return error.InvalidReply;
    try readExact(fd, &payload);
    if (payload[8] != 7) return error.InvalidPayload;

    std.mem.writeInt(u64, request[8..16], 3, .big);
    request[28] = 99;
    try writeAll(fd, &request);
    try readExact(fd, &header);
    if (header[6] != 5 or std.mem.readInt(u64, header[8..16], .big) != 3)
        return error.InvalidApplicationError;
    const error_len = std.mem.readInt(u32, header[16..20], .big);
    if (error_len > 256) return error.ErrorRecordTooLarge;
    var error_payload: [256]u8 = undefined;
    try readExact(fd, error_payload[0..error_len]);
    const error_bytes = error_payload[0..error_len];
    if (std.mem.indexOf(u8, error_bytes, "birnam.application-error.v1") == null or
        std.mem.indexOf(u8, error_bytes, "encodeFailed") == null or
        std.mem.indexOf(u8, error_bytes, "NonTransferable") == null)
        return error.InvalidApplicationError;

    var stop: [20]u8 = undefined;
    @memcpy(stop[0..4], "BISO");
    std.mem.writeInt(u16, stop[4..6], 1, .big);
    stop[6] = 6;
    stop[7] = 0;
    std.mem.writeInt(u64, stop[8..16], 4, .big);
    std.mem.writeInt(u32, stop[16..20], 0, .big);
    try writeAll(fd, &stop);
    try readExact(fd, &header);
    if (header[6] != 7 or std.mem.readInt(u64, header[8..16], .big) != 4 or
        std.mem.readInt(u32, header[16..20], .big) != 0) return error.InvalidStop;
    return 0;
}

fn runMalformedWorkerClient(client: WorkerClient) void {
    client.result.* = malformedWorkerClientImpl(client.fd) catch -1;
}

fn malformedWorkerClientImpl(fd: std.posix.fd_t) !c_int {
    var header: [20]u8 = undefined;
    try readExact(fd, &header);
    if (header[6] != 1) return error.InvalidReady;
    var request: [21]u8 = undefined;
    @memcpy(request[0..4], "BISO");
    std.mem.writeInt(u16, request[4..6], 1, .big);
    request[6] = 3;
    request[7] = 0;
    std.mem.writeInt(u64, request[8..16], 1, .big);
    std.mem.writeInt(u32, request[16..20], 1, .big);
    request[20] = 0xff;
    try writeAll(fd, &request);
    try readExact(fd, &header);
    if (header[6] != 8 or std.mem.readInt(u64, header[8..16], .big) != 1)
        return error.InvalidProtocolError;
    const error_len = std.mem.readInt(u32, header[16..20], .big);
    if (error_len > 256) return error.ErrorRecordTooLarge;
    var payload: [256]u8 = undefined;
    try readExact(fd, payload[0..error_len]);
    const bytes = payload[0..error_len];
    if (std.mem.indexOf(u8, bytes, "birnam.protocol-error.v1") == null or
        std.mem.indexOf(u8, bytes, "invalidPayload") == null or
        std.mem.indexOf(u8, bytes, "UnknownTag") == null)
        return error.InvalidProtocolError;
    return 0;
}

fn writeAll(fd: std.posix.fd_t, bytes: []const u8) !void {
    var offset: usize = 0;
    while (offset < bytes.len) {
        const wrote = try std.posix.write(fd, bytes[offset..]);
        if (wrote == 0) return error.WriteFailed;
        offset += wrote;
    }
}

fn readExact(fd: std.posix.fd_t, bytes: []u8) !void {
    var offset: usize = 0;
    while (offset < bytes.len) {
        const got = try std.posix.read(fd, bytes[offset..]);
        if (got == 0) return error.EndOfStream;
        offset += got;
    }
}

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

    const raw_module_init = api.dlsym(handle, "module_init") orelse
        return error.RustIsolateModuleInitMissing;
    const module_init: ModuleInit = @ptrCast(@alignCast(raw_module_init));
    _ = module_init(api, null);
    inline for (.{ "IsolateVt", "Isolate", "IsolateHandleVt", "IsolateFailureVt" }) |name| {
        try std.testing.expectEqual(@as(c_int, 1), api.isExported(name));
    }
    const pending = api.send(
        api.import("Isolate"),
        api.intern("start:selector:image:"),
        &[_]idapi.Oop{ api.intern("Receiver"), api.intern("echo:"), api.mkString("image", 5) },
        3,
    );
    try std.testing.expectEqual(api.trueObj, api.send(pending, api.intern("isError"), &.{}, 0));
    try std.testing.expectEqual(api.intern("rustWorkerHostPending"), api.send(pending, api.intern("kind"), &.{}, 0));

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
    const raw_decode_oop = api.dlsym(handle, "birnam_isolate_rust_decode_oop") orelse
        return error.RustIsolateOopDecoderMissing;
    const decode_oop: DecodeOop = @ptrCast(@alignCast(raw_decode_oop));
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

    var decoded: idapi.Oop = null;
    const scalar_int_wire = [_]u8{ 3, 0, 0, 0, 0, 0, 0, 0, 23 };
    try std.testing.expectEqual(
        @as(c_int, 0),
        decode_oop(api, &scalar_int_wire, scalar_int_wire.len, &decoded),
    );
    try std.testing.expect(api.isSmallInt(decoded) != 0);
    try std.testing.expectEqual(@as(i64, 23), api.smallIntValue(decoded));

    const scalar_string_wire = [_]u8{ 5, 0, 0, 0, 3, 'a', 0, 'b' };
    decoded = null;
    try std.testing.expectEqual(
        @as(c_int, 0),
        decode_oop(api, &scalar_string_wire, scalar_string_wire.len, &decoded),
    );
    var decoded_string_len: usize = 0;
    const decoded_string_bytes = api.stringBytes(decoded, &decoded_string_len);
    try std.testing.expectEqualSlices(u8, "a\x00b", decoded_string_bytes[0..decoded_string_len]);

    const scalar_symbol_wire = [_]u8{ 6, 0, 0, 0, 5, 'h', 'e', 'l', 'l', 'o' };
    decoded = null;
    try std.testing.expectEqual(
        @as(c_int, 0),
        decode_oop(api, &scalar_symbol_wire, scalar_symbol_wire.len, &decoded),
    );
    try std.testing.expectEqualStrings("hello", std.mem.span(api.symbolName(decoded)));

    const scalar_float_wire = [_]u8{ 4, 0x3f, 0xf8, 0, 0, 0, 0, 0, 0 };
    decoded = null;
    try std.testing.expectEqual(
        @as(c_int, 0),
        decode_oop(api, &scalar_float_wire, scalar_float_wire.len, &decoded),
    );
    try std.testing.expect(api.isFloat(decoded) != 0);
    try std.testing.expectEqual(@as(f64, 1.5), api.floatValue(decoded));

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
    decoded = null;
    try std.testing.expectEqual(
        @as(c_int, 0),
        decode_oop(api, oop_bytes[0..oop_len].ptr, oop_len, &decoded),
    );
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(api.send(
        decoded,
        api.intern("size"),
        &.{},
        0,
    )));
    const vector_zero = [_]idapi.Oop{api.mkSmallInt(0)};
    try std.testing.expectEqual(@as(i64, 11), api.smallIntValue(api.send(
        decoded,
        api.intern("at:"),
        &vector_zero,
        1,
    )));
    const vector_one = [_]idapi.Oop{api.mkSmallInt(1)};
    var decoded_vector_string_len: usize = 0;
    const decoded_vector_string = api.stringBytes(
        api.send(decoded, api.intern("at:"), &vector_one, 1),
        &decoded_vector_string_len,
    );
    try std.testing.expectEqualSlices(u8, "ok", decoded_vector_string[0..decoded_vector_string_len]);

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
    decoded = null;
    try std.testing.expectEqual(
        @as(c_int, 0),
        decode_oop(api, oop_bytes[0..oop_len].ptr, oop_len, &decoded),
    );
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(
        decoded,
        api.intern("size"),
        &.{},
        0,
    )));
    const decoded_key = api.intern("answer");
    const decoded_key_args = [_]idapi.Oop{decoded_key};
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(api.send(
        decoded,
        api.intern("at:"),
        &decoded_key_args,
        1,
    )));

    oop_bytes[oop_len] = 0xff;
    decoded = null;
    try std.testing.expectEqual(
        @as(c_int, 8),
        decode_oop(api, oop_bytes[0 .. oop_len + 1].ptr, oop_len + 1, &decoded),
    );
    try std.testing.expectEqual(@as(idapi.Oop, null), decoded);

    const unknown_tag = [_]u8{0xff};
    try std.testing.expectEqual(
        @as(c_int, 9),
        decode_oop(api, &unknown_tag, unknown_tag.len, &decoded),
    );
    try std.testing.expectEqual(@as(idapi.Oop, null), decoded);
}

test "Rust Isolate worker dispatches copied values and stops" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_ISOLATE_DYLIB") orelse
        return error.MissingRustIsolateDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustIsolateDlopenFailed;
    const raw_driver = api.dlsym(handle, "birnam_isolate_worker_driver") orelse
        return error.RustIsolateWorkerDriverMissing;
    const driver: WorkerDriver = @ptrCast(@alignCast(raw_driver));
    const prelude_path = std.posix.getenv("BIRNAM_RUST_PRELUDE_DYLIB") orelse
        return error.MissingRustPreludeDylib;
    const prelude_path_z = try std.heap.c_allocator.dupeZ(u8, prelude_path);
    defer std.heap.c_allocator.free(prelude_path_z);
    const prelude_handle = api.dlopen(prelude_path_z.ptr, 1) orelse
        return error.RustPreludeDlopenFailed;
    const raw_prelude_init = api.dlsym(prelude_handle, "module_init") orelse
        return error.RustPreludeModuleInitMissing;
    const prelude_init: ModuleInit = @ptrCast(@alignCast(raw_prelude_init));
    _ = prelude_init(api, null);
    const error_path = std.posix.getenv("BIRNAM_ERROR_DYLIB") orelse
        return error.MissingRustErrorDylib;
    const error_path_z = try std.heap.c_allocator.dupeZ(u8, error_path);
    defer std.heap.c_allocator.free(error_path_z);
    const error_handle = api.dlopen(error_path_z.ptr, 1) orelse
        return error.RustErrorDlopenFailed;
    const raw_error_init = api.dlsym(error_handle, "module_init") orelse
        return error.RustErrorModuleInitMissing;
    const error_init: ModuleInit = @ptrCast(@alignCast(raw_error_init));
    _ = error_init(api, null);

    const receiver_vt = api.proto(api.object_vt);
    const selector = api.intern("echo:");
    api.method(receiver_vt, selector, &echoArgument);
    const receiver = api.alloc(receiver_vt, 0);

    var sockets: [2]std.posix.fd_t = undefined;
    if (std.c.socketpair(std.c.AF.UNIX, std.c.SOCK.STREAM, 0, &sockets) != 0)
        return error.SocketPairFailed;
    defer {
        std.posix.close(sockets[0]);
        std.posix.close(sockets[1]);
    }
    var client_result: c_int = -1;
    const client = try std.Thread.spawn(.{}, runWorkerClient, .{WorkerClient{
        .fd = sockets[0],
        .result = &client_result,
    }});
    const driver_result = driver(api, sockets[1], receiver, selector);
    client.join();
    try std.testing.expectEqual(@as(c_int, 0), driver_result);
    try std.testing.expectEqual(@as(c_int, 0), client_result);
}

test "Rust Isolate worker rejects malformed request payloads" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_ISOLATE_DYLIB") orelse
        return error.MissingRustIsolateDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustIsolateDlopenFailed;
    const raw_driver = api.dlsym(handle, "birnam_isolate_worker_driver") orelse
        return error.RustIsolateWorkerDriverMissing;
    const driver: WorkerDriver = @ptrCast(@alignCast(raw_driver));

    var sockets: [2]std.posix.fd_t = undefined;
    if (std.c.socketpair(std.c.AF.UNIX, std.c.SOCK.STREAM, 0, &sockets) != 0)
        return error.SocketPairFailed;
    defer {
        std.posix.close(sockets[0]);
        std.posix.close(sockets[1]);
    }
    var client_result: c_int = -1;
    const client = try std.Thread.spawn(.{}, runMalformedWorkerClient, .{WorkerClient{
        .fd = sockets[0],
        .result = &client_result,
    }});
    const driver_result = driver(api, sockets[1], null, null);
    client.join();
    try std.testing.expectEqual(@as(c_int, 1), driver_result);
    try std.testing.expectEqual(@as(c_int, 0), client_result);
}
