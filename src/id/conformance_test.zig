//! External IdApi conformance shared by every floor implementation.
//! This root must not import id.zig.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

var callback_api: ?*idapi.IdApi = null;
var dnu_selector: Oop = null;
var dnu_count: usize = 0;
var dnu_args: [16]Oop = undefined;

fn answer42(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return callback_api.?.mkSmallInt(42);
}

fn answer7(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return callback_api.?.mkSmallInt(7);
}

fn captureDnu(self: Oop, args: [*]const Oop, count: usize) callconv(.c) Oop {
    dnu_selector = args[0];
    dnu_count = count - 1;
    const forwarded = @min(dnu_args.len, dnu_count);
    if (forwarded > 0) @memcpy(dnu_args[0..forwarded], args[1 .. 1 + forwarded]);
    return self;
}

test "conformance harness selects one identified floor and initializes it once" {
    const first = floor.api();
    const second = floor.api();

    try std.testing.expect(first == second);
    try std.testing.expectEqual(@as(usize, 1), floor.initCallCount());
    try std.testing.expect(floor.retainedHandle() != null);
    try std.testing.expect(first.import("Object") != null);
    try std.testing.expect(first.import("Integer") != null);
    try std.testing.expect(first.import("String") != null);
}

test "dispatch is metacircular and a user method executes through IdApi" {
    const api = floor.api();
    callback_api = api;

    try std.testing.expect(api.vtableOf(api.vtable_vt) == api.vtable_vt);
    try std.testing.expect(api.vtableOf(api.object_vt) == api.vtable_vt);

    const point_vt = api.proto(api.object_vt);
    try std.testing.expect(api.vtableParent(point_vt) == api.object_vt);
    const answer = api.intern("conformanceDispatchAnswer");
    api.method(point_vt, answer, &answer42);
    const point = api.alloc(point_vt, 0);
    const result = api.send(point, answer, &.{}, 0);
    try std.testing.expect(api.isSmallInt(result) != 0);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(result));
}

test "direct method removal reveals live inheritance without a tombstone" {
    const api = floor.api();
    callback_api = api;

    const parent_vt = api.proto(api.object_vt);
    const child_vt = api.proto(parent_vt);
    const selector = api.intern("conformanceRemovableAnswer");
    api.method(parent_vt, selector, &answer42);
    api.method(child_vt, selector, &answer7);
    const object = api.alloc(child_vt, 0);

    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(api.send(object, selector, &.{}, 0)));
    const remove = api.intern("removeMethod:");
    const removed = api.send(child_vt, remove, &.{selector}, 1);
    const answer7_oop: Oop = @ptrCast(@constCast(&answer7));
    try std.testing.expect(removed == answer7_oop);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(api.send(object, selector, &.{}, 0)));

    // No copied parent binding: changing the parent remains immediately visible.
    api.method(parent_vt, selector, &answer7);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(api.send(object, selector, &.{}, 0)));
    try std.testing.expect(api.send(child_vt, remove, &.{selector}, 1) == null);
}

test "DNU is overridable and forwards exactly sixteen original arguments" {
    const api = floor.api();
    const proxy_vt = api.proto(api.object_vt);
    api.method(proxy_vt, api.intern("doesNotUnderstand:"), &captureDnu);
    const proxy = api.alloc(proxy_vt, 0);
    const missing = api.intern("conformanceMissing:with:many:arguments:");
    var original: [18]Oop = undefined;
    for (&original, 0..) |*arg, i| arg.* = api.mkSmallInt(@intCast(i));

    dnu_selector = null;
    dnu_count = 0;
    const result = api.send(proxy, missing, &original, original.len);
    try std.testing.expect(result == proxy);
    try std.testing.expect(dnu_selector == missing);
    try std.testing.expectEqual(@as(usize, 16), dnu_count);
    for (dnu_args, 0..) |arg, i| try std.testing.expect(arg == original[i]);
}

test "Integer preserves the full signed width" {
    const api = floor.api();
    const values = [_]i64{ std.math.minInt(i64), -1, 0, 1, 42, std.math.maxInt(i64) };
    for (values) |value| {
        const object = api.mkSmallInt(value);
        try std.testing.expect(api.isSmallInt(object) != 0);
        try std.testing.expect(api.vtableOf(object) == api.smallint_vt);
        try std.testing.expectEqual(value, api.smallIntValue(object));
        try std.testing.expect(api.isFloat(object) == 0);
        try std.testing.expect(api.isWord(object) == 0);
    }
}

test "Float preserves boxed f64 bit patterns" {
    const api = floor.api();
    const patterns = [_]u64{
        0x0000000000000000, // +0
        0x8000000000000000, // -0
        0x3ff4000000000000, // 1.25
        0x7ff0000000000000, // +infinity
        0x7ff8000000001234, // quiet NaN with payload
    };
    for (patterns) |bits| {
        const value: f64 = @bitCast(bits);
        const object = api.mkFloat(value);
        try std.testing.expect(api.isFloat(object) != 0);
        try std.testing.expect(api.vtableOf(object) == api.float_vt);
        try std.testing.expectEqual(bits, @as(u64, @bitCast(api.floatValue(object))));
        try std.testing.expect(api.isSmallInt(object) == 0);
        try std.testing.expect(api.isWord(object) == 0);
    }
}

test "Word preserves every opaque machine bit" {
    const api = floor.api();
    const values = [_]u64{ 0, 1, 0x7fffffffffffffff, 0x8000000000000000, 0xfedcba9876543210, std.math.maxInt(u64) };
    for (values) |value| {
        const object = api.mkWord(value);
        try std.testing.expect(api.isWord(object) != 0);
        try std.testing.expect(api.vtableOf(object) == api.word_vt);
        try std.testing.expectEqual(value, api.wordValue(object));
        try std.testing.expect(api.isSmallInt(object) == 0);
        try std.testing.expect(api.isFloat(object) == 0);
    }
}

test "String owns an uninterned byte-for-byte copy" {
    const api = floor.api();
    var source = [_]u8{ 0xff, 0x00, 'b', 'i', 'r', 'n', 'a', 'm' };
    const first = api.mkString(&source, source.len);
    const second = api.mkString(&source, source.len);
    source[0] = 0;

    var first_len: usize = 0;
    const first_bytes = api.stringBytes(first, &first_len);
    try std.testing.expectEqual(@as(usize, 8), first_len);
    try std.testing.expectEqualSlices(u8, &.{ 0xff, 0x00, 'b', 'i', 'r', 'n', 'a', 'm' }, first_bytes[0..first_len]);
    try std.testing.expect(first != second);
    try std.testing.expect(api.vtableOf(first) == api.string_vt);
    try std.testing.expect(api.isSmallInt(first) == 0);

    const empty = api.mkString("", 0);
    var empty_len: usize = 1;
    _ = api.stringBytes(empty, &empty_len);
    try std.testing.expectEqual(@as(usize, 0), empty_len);
}

test "nil and Boolean are distinct canonical heap singletons" {
    const api = floor.api();
    try std.testing.expect(api.nilObj != null);
    try std.testing.expect(api.trueObj != null);
    try std.testing.expect(api.falseObj != null);
    try std.testing.expect(api.nilObj != api.trueObj);
    try std.testing.expect(api.nilObj != api.falseObj);
    try std.testing.expect(api.trueObj != api.falseObj);
    try std.testing.expect(api.vtableOf(api.nilObj) != null);
    try std.testing.expect(api.vtableOf(api.trueObj) == api.vtableOf(api.falseObj));
    try std.testing.expect(api.import("Boolean") == api.trueObj);
}

test "public pointer predicates reject non-object Oop carriers" {
    const api = floor.api();
    const foreign_words: [2]usize align(16) = .{ 0xfeed, 0xbeef };
    const candidates = [_]Oop{
        null,
        @ptrCast(@constCast(&answer42)),
        @ptrCast(@constCast(&foreign_words)),
        @ptrFromInt(0x1000),
        @ptrFromInt(0x1001),
    };
    for (candidates) |candidate| {
        try std.testing.expect(api.vtableOf(candidate) == null);
        try std.testing.expect(api.isSmallInt(candidate) == 0);
        try std.testing.expect(api.isFloat(candidate) == 0);
        try std.testing.expect(api.isWord(candidate) == 0);
        try std.testing.expect(api.vtableOwner(candidate) == null);
    }
}
