//! R4-deferred collection tests that require an active tracing and sweeping GC.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const prelude = @import("prelude.zig");
const collection = @import("collection.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

var gapi: *idapi.IdApi = undefined;

fn setup() *idapi.IdApi {
    const api = floor.api();
    gapi = api;
    prelude.bringUp(api);
    collection.bringUp(api);
    return api;
}

fn s0(receiver: Oop, selector: [*:0]const u8) Oop {
    return gapi.send(receiver, gapi.intern(selector), &.{}, 0);
}

fn s1(receiver: Oop, selector: [*:0]const u8, argument: Oop) Oop {
    return gapi.send(receiver, gapi.intern(selector), &.{argument}, 1);
}

fn s2(receiver: Oop, selector: [*:0]const u8, first: Oop, second: Oop) Oop {
    return gapi.send(receiver, gapi.intern(selector), &.{ first, second }, 2);
}

fn integerValue(object: Oop) i64 {
    return gapi.smallIntValue(object);
}

fn stringEquals(object: Oop, expected: []const u8) bool {
    var length: usize = 0;
    const pointer = gapi.stringBytes(object, &length);
    return std.mem.eql(u8, pointer[0..length], expected);
}

/// Allocate `count` throwaway objects and answer a Vector of Ranges whose raw
/// i64 endpoints alias their addresses.
noinline fn makeAliasedGarbage(api: *idapi.IdApi, count: usize) Oop {
    var vector = s0(api.import("Vector"), "new");
    for (0..count) |_| {
        const garbage = api.alloc(api.object_vt, 64);
        const address: i64 = @bitCast(@intFromPtr(garbage.?));
        const range = s1(api.mkSmallInt(address), "to:", api.mkSmallInt(address));
        vector = s1(vector, "add:", range);
    }
    return vector;
}

noinline fn launderStack() void {
    var buffer: [512]usize = undefined;
    for (&buffer, 0..) |*word, index| word.* = index;
    std.mem.doNotOptimizeAway(&buffer);
}

test "F3 layouts: precise trace frees garbage that a raw word aliases" {
    const api = setup();
    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    _ = &anchor;
    defer api.gcSetPrecise(0);

    const vector = makeAliasedGarbage(api, 60);
    launderStack();
    _ = api.gcCollect();
    const live_conservative = api.gcLiveCount();

    api.gcSetPrecise(1);
    const reclaimed = api.gcCollect();
    try std.testing.expect(reclaimed >= 40);
    try std.testing.expectEqual(live_conservative - reclaimed, api.gcLiveCount());
    try std.testing.expectEqual(@as(i64, 60), integerValue(s0(vector, "size")));
}

noinline fn churn(api: *idapi.IdApi, count: usize) void {
    var last: Oop = null;
    for (0..count) |_| last = api.mkString("throwaway garbage string", 24);
    std.mem.doNotOptimizeAway(&last);
}

test "F3 layouts: a mixed live structure survives precise collection under churn" {
    const api = setup();
    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    _ = &anchor;
    defer api.gcSetPrecise(0);
    api.gcSetPrecise(1);

    var vector = s0(api.import("Vector"), "new");
    vector = s1(vector, "add:", api.mkString("alpha", 5));
    vector = s1(vector, "add:", api.mkString("beta", 4));
    var map = s0(api.import("Map"), "new");
    map = s2(map, "at:put:", api.intern("vec"), vector);
    map = s2(
        map,
        "at:put:",
        api.mkString("range", 5),
        s1(api.mkSmallInt(1), "to:", api.mkSmallInt(9)),
    );

    churn(api, 20_000);
    _ = api.gcCollect();

    const restored = s1(map, "at:", api.intern("vec"));
    try std.testing.expectEqual(@as(i64, 2), integerValue(s0(restored, "size")));
    try std.testing.expect(stringEquals(s1(restored, "at:", api.mkSmallInt(0)), "alpha"));
    try std.testing.expect(stringEquals(s1(restored, "at:", api.mkSmallInt(1)), "beta"));
    const range = s1(map, "at:", api.mkString("range", 5));
    try std.testing.expectEqual(@as(i64, 9), integerValue(s0(range, "size")));
}
