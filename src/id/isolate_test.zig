const std = @import("std");
const id = @import("id.zig");
const prelude = @import("prelude.zig");
const collection = @import("collection.zig");
const errors = @import("error.zig");
const isolate = @import("isolate.zig");

const Oop = @import("idapi.zig").Oop;

fn apiReady() *@import("idapi.zig").IdApi {
    const api = id.id_init();
    prelude.bringUp(api);
    collection.bringUp(api);
    errors.bringUp(api);
    isolate.bringUp(api);
    return api;
}

fn s0(api: anytype, o: Oop, sel: [*:0]const u8) Oop {
    return api.send(o, api.intern(sel), &.{}, 0);
}
fn s1(api: anytype, o: Oop, sel: [*:0]const u8, a: Oop) Oop {
    return api.send(o, api.intern(sel), &[_]Oop{a}, 1);
}
fn s2(api: anytype, o: Oop, sel: [*:0]const u8, a: Oop, b: Oop) Oop {
    return api.send(o, api.intern(sel), &[_]Oop{ a, b }, 2);
}

const digest_a = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const digest_b = "1123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const digest_c = "2123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const content_path = "/tmp/.birnam-candidates/" ++ digest_c ++ ".dylib";

fn activateRequest(api: anytype) Oop {
    var request = s0(api, api.import("Vector"), "new");
    request = s1(api, request, "add:", api.intern("activate"));
    request = s1(api, request, "add:", api.intern("rolloutFixture"));
    request = s1(api, request, "add:", api.mkString(digest_b, digest_b.len));
    request = s1(api, request, "add:", api.mkString("module source", 13));
    request = s1(api, request, "add:", api.mkString(digest_c, digest_c.len));
    request = s1(api, request, "add:", api.mkString(content_path, content_path.len));
    request = s1(api, request, "add:", api.mkString(digest_a, digest_a.len));
    return request;
}

fn phaseOk() callconv(.c) c_int {
    return 1;
}

const DriverContext = struct {
    api: *@import("idapi.zig").IdApi,
    fd: std.posix.fd_t,
    receiver: Oop,
    selector: Oop,
    status: c_int = -1,
};

fn runDriver(ctx: *DriverContext) void {
    ctx.status = isolate.workerDriver(ctx.api, ctx.fd, ctx.receiver, ctx.selector, &phaseOk, &phaseOk);
}

test "wire header is exactly network-order wire-v1" {
    const bytes = try isolate.encodeHeader(.{ .kind = .request, .request_id = 0x0102030405060708, .payload_len = 0x00123344 });
    try std.testing.expectEqualSlices(u8, &.{
        'B',  'I',  'S',  'O',  0, 1, 3, 0,
        1,    2,    3,    4,    5, 6, 7, 8,
        0x00, 0x12, 0x33, 0x44,
    }, &bytes);
    const h = try isolate.decodeHeader(&bytes);
    try std.testing.expectEqual(isolate.FrameKind.request, h.kind);
    try std.testing.expectEqual(@as(u64, 0x0102030405060708), h.request_id);
}

test "wire header rejects zero ordinary ids and nonzero boot ids" {
    try std.testing.expectError(error.InvalidRequestId, isolate.encodeHeader(.{ .kind = .request, .request_id = 0, .payload_len = 0 }));
    try std.testing.expectError(error.InvalidRequestId, isolate.encodeHeader(.{ .kind = .ready, .request_id = 1, .payload_len = 0 }));
    _ = try isolate.encodeHeader(.{ .kind = .protocol_error, .request_id = 0, .payload_len = 1 });
}

test "wire-v1 reserves fixed blocking evolution frame kinds" {
    const request = try isolate.encodeHeader(.{ .kind = .evolution_request, .request_id = 9, .payload_len = 1 });
    const reply = try isolate.encodeHeader(.{ .kind = .evolution_reply, .request_id = 9, .payload_len = 1 });
    try std.testing.expectEqual(@as(u8, 9), request[6]);
    try std.testing.expectEqual(@as(u8, 10), reply[6]);
}

test "evolution rollback record is positional and rejects noncanonical digest" {
    const api = apiReady();
    const digest = api.mkString("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", 64);
    var request = s0(api, api.import("Vector"), "new");
    request = s1(api, request, "add:", api.intern("rollback"));
    request = s1(api, request, "add:", digest);
    const encoded = try isolate.encodeEvolutionRequest(api, request);
    defer std.heap.c_allocator.free(encoded);
    const decoded = try isolate.decodeEvolutionRequest(api, encoded);
    try std.testing.expect(decoded.operation == api.intern("rollback"));
    try std.testing.expect(decoded.accepted_base != api.nilObj);

    var invalid = s0(api, api.import("Vector"), "new");
    invalid = s1(api, invalid, "add:", api.intern("rollback"));
    invalid = s1(api, invalid, "add:", api.mkString("ABCDEF", 6));
    try std.testing.expectError(error.InvalidRecord, isolate.encodeEvolutionRequest(api, invalid));
}

test "evolution activate record has a stable schema and round-trips every field" {
    const api = apiReady();
    const request = activateRequest(api);
    const encoded = try isolate.encodeEvolutionRequest(api, request);
    defer std.heap.c_allocator.free(encoded);

    const schema = "birnam.evolution-activate.v1";
    try std.testing.expectEqual(@as(u8, 9), encoded[0]);
    try std.testing.expectEqual(@as(u16, schema.len), std.mem.readInt(u16, encoded[1..3], .big));
    try std.testing.expectEqualSlices(u8, schema, encoded[3 .. 3 + schema.len]);
    try std.testing.expectEqual(@as(u16, 6), std.mem.readInt(u16, encoded[3 + schema.len ..][0..2], .big));

    const decoded = try isolate.decodeEvolutionRequest(api, encoded);
    try std.testing.expect(decoded.operation == api.intern("activate"));
    try std.testing.expect(decoded.module == api.intern("rolloutFixture"));
    var fields: [7]Oop = undefined;
    for (&fields, 0..) |*field, i| field.* = s1(api, decoded.request, "at:", api.mkSmallInt(@intCast(i)));
    try std.testing.expect(fields[0] == api.intern("activate"));
    try std.testing.expect(fields[1] == api.intern("rolloutFixture"));
    try std.testing.expect(fields[3] != api.nilObj);
    try std.testing.expect(decoded.accepted_base == fields[6]);
}

test "evolution activate record rejects invalid operations fields and content addresses" {
    const api = apiReady();
    var fields: [7]Oop = undefined;
    const valid = activateRequest(api);
    for (&fields, 0..) |*field, i| field.* = s1(api, valid, "at:", api.mkSmallInt(@intCast(i)));

    fields[0] = api.intern("deploy");
    try std.testing.expectError(error.InvalidRecord, isolate.encodeEvolutionRequest(api, makeVector(api, &fields)));
    fields[0] = api.intern("activate");
    fields[3] = api.mkSmallInt(1);
    try std.testing.expectError(error.InvalidRecord, isolate.encodeEvolutionRequest(api, makeVector(api, &fields)));
    fields[3] = api.nilObj;
    fields[5] = api.mkString("/tmp/not-the-checksum.dylib", 27);
    try std.testing.expectError(error.InvalidRecord, isolate.encodeEvolutionRequest(api, makeVector(api, &fields)));
    fields[5] = api.mkString(content_path, content_path.len);
    fields[6] = api.mkString("ABCDEF", 6);
    try std.testing.expectError(error.InvalidRecord, isolate.encodeEvolutionRequest(api, makeVector(api, &fields)));
}

fn makeVector(api: anytype, fields: []const Oop) Oop {
    var vector = s0(api, api.import("Vector"), "new");
    for (fields) |field| vector = s1(api, vector, "add:", field);
    return vector;
}

test "evolution receipt record round-trips copied fields and rejects malformed records" {
    const api = apiReady();
    const receipt = isolate.makeReceipt(.{
        api.intern("activate"),
        api.intern("rolloutFixture"),
        api.mkString(digest_a, digest_a.len),
        api.mkString(digest_a, digest_a.len),
        api.mkString(digest_b, digest_b.len),
        api.intern("activated"),
        api.nilObj,
        api.mkString("generation activated", 20),
    });
    const encoded = try isolate.encodeEvolutionReceipt(api, receipt);
    defer std.heap.c_allocator.free(encoded);
    const schema = "birnam.evolution-receipt.v1";
    try std.testing.expectEqualSlices(u8, schema, encoded[3 .. 3 + schema.len]);
    try std.testing.expectEqual(@as(u16, 8), std.mem.readInt(u16, encoded[3 + schema.len ..][0..2], .big));

    const decoded = try isolate.decodeEvolutionReceipt(api, encoded);
    try std.testing.expect(s0(api, decoded, "operation") == api.intern("activate"));
    try std.testing.expect(s0(api, decoded, "module") == api.intern("rolloutFixture"));
    try std.testing.expect(s0(api, decoded, "outcome") == api.intern("activated"));
    try std.testing.expect(s0(api, decoded, "errorKind") == api.nilObj);
    try std.testing.expect(s0(api, decoded, "isError") == api.falseObj);

    var bad_count = try std.testing.allocator.dupe(u8, encoded);
    defer std.testing.allocator.free(bad_count);
    bad_count[4 + schema.len] = 7;
    try std.testing.expectError(error.UnknownRecordSchema, isolate.decodeEvolutionReceipt(api, bad_count));
    const trailing = try std.mem.concat(std.testing.allocator, u8, &.{ encoded, &.{0} });
    defer std.testing.allocator.free(trailing);
    try std.testing.expectError(error.TrailingBytes, isolate.decodeEvolutionReceipt(api, trailing));

    const invalid = isolate.makeReceipt(.{
        api.intern("activate"),               api.intern("rolloutFixture"),         api.intern("notDigest"),
        api.mkString(digest_a, digest_a.len), api.mkString(digest_b, digest_b.len), api.intern("activated"),
        api.nilObj,                           api.mkString("bad", 3),
    });
    try std.testing.expectError(error.InvalidRecord, isolate.encodeEvolutionReceipt(api, invalid));
}

test "worker loop finishes an executing request before reading a queued evolution frame" {
    const api = apiReady();
    var sockets: [2]std.posix.fd_t = undefined;
    try std.testing.expectEqual(@as(c_int, 0), std.c.socketpair(std.c.AF.UNIX, std.c.SOCK.STREAM, 0, &sockets));
    defer std.posix.close(sockets[0]);
    defer std.posix.close(sockets[1]);

    var ctx: DriverContext = .{
        .api = api,
        .fd = sockets[1],
        .receiver = api.import("Isolate"),
        .selector = api.intern("_isolateSleep:"),
    };
    const thread = try std.Thread.spawn(.{}, runDriver, .{&ctx});
    defer thread.join();

    const ready = try isolate.readFrame(sockets[0], 1_000);
    defer ready.deinit();
    try std.testing.expectEqual(isolate.FrameKind.ready, ready.header.kind);
    const request = try isolate.encodeValue(api, api.mkSmallInt(30));
    defer std.heap.c_allocator.free(request);
    try isolate.writeFrame(sockets[0], .request, 1, request, 1_000);
    try isolate.writeFrame(sockets[0], .evolution_request, 2, &.{0}, 1_000);

    const first = try isolate.readFrame(sockets[0], 1_000);
    defer first.deinit();
    try std.testing.expectEqual(isolate.FrameKind.reply_ok, first.header.kind);
    try std.testing.expectEqual(@as(u64, 1), first.header.request_id);
    const second = try isolate.readFrame(sockets[0], 1_000);
    defer second.deinit();
    try std.testing.expectEqual(isolate.FrameKind.protocol_error, second.header.kind);
    try std.testing.expectEqual(@as(u64, 2), second.header.request_id);
}

test "value codec round-trips scalars vector and map" {
    const api = apiReady();
    var vector = s0(api, api.import("Vector"), "new");
    vector = s1(api, vector, "add:", api.nilObj);
    vector = s1(api, vector, "add:", api.mkSmallInt(-42));
    vector = s1(api, vector, "add:", api.mkFloat(-0.0));
    vector = s1(api, vector, "add:", api.mkString("a\x00b", 3));
    vector = s1(api, vector, "add:", api.intern("hello"));

    var map = s0(api, api.import("Map"), "new");
    map = s2(api, map, "at:put:", api.intern("items"), vector);
    map = s2(api, map, "at:put:", api.mkString("ok", 2), api.trueObj);

    const encoded = try isolate.encodeValue(api, map);
    defer std.heap.c_allocator.free(encoded);
    const decoded = try isolate.decodeValue(api, encoded);
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(s0(api, decoded, "size")));
    const items = s1(api, decoded, "at:", api.intern("items"));
    try std.testing.expectEqual(@as(i64, 5), api.smallIntValue(s0(api, items, "size")));
    try std.testing.expectEqual(@as(i64, -42), api.smallIntValue(s1(api, items, "at:", api.mkSmallInt(1))));
    const nul = s1(api, items, "at:", api.mkSmallInt(3));
    var len: usize = 0;
    try std.testing.expectEqualSlices(u8, "a\x00b", api.stringBytes(nul, &len)[0..len]);
}

test "decoder rejects duplicate map keys and trailing bytes" {
    const api = apiReady();
    const duplicate = &[_]u8{
        8, 0, 0, 0, 2,
        3, 0, 0, 0, 0,
        0, 0, 0, 1, 0,
        3, 0, 0, 0, 0,
        0, 0, 0, 1, 1,
    };
    try std.testing.expectError(error.DuplicateMapKey, isolate.decodeValue(api, duplicate));
    try std.testing.expectError(error.TrailingBytes, isolate.decodeValue(api, &.{ 0, 0 }));
}

test "IsolateFailure exposes structured outcome fields" {
    const api = apiReady();
    const failure = isolate.makeFailure(api.intern("timeout"), api.intern("deadline"), api.mkString("expired", 7), api.mkSmallInt(9), api.mkSmallInt(4));
    try std.testing.expect(s0(api, failure, "isError") == api.trueObj);
    try std.testing.expect(s0(api, failure, "class") == api.intern("timeout"));
    try std.testing.expect(s0(api, failure, "kind") == api.intern("deadline"));
    try std.testing.expectEqual(@as(i64, 9), api.smallIntValue(s0(api, failure, "exitStatus")));
    try std.testing.expectEqual(@as(i64, 4), api.smallIntValue(s0(api, failure, "requestId")));
}
