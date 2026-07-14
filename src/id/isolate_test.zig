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
