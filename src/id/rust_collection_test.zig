//! Cross-language smoke for the first Rust Collection vector slice.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;
const Identity = *const fn () callconv(.c) [*:0]const u8;

fn send(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8, args: []const Oop) Oop {
    return api.send(receiver, api.intern(selector), args.ptr, args.len);
}

fn loadModule(api: *idapi.IdApi, path: []const u8) !void {
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.ModuleDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.ModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);
}

var sum: i64 = 0;

fn collectInteger(_self: Oop, args: [*]const Oop, _count: usize) callconv(.c) Oop {
    _ = _self;
    _ = _count;
    sum += floor.api().smallIntValue(args[0]);
    return floor.api().nilObj;
}

fn addInteger(_self: Oop, args: [*]const Oop, _count: usize) callconv(.c) Oop {
    _ = _self;
    _ = _count;
    const api = floor.api();
    return api.mkSmallInt(api.smallIntValue(args[0]) + api.smallIntValue(args[1]));
}

fn defaultInteger(_self: Oop, _args: [*]const Oop, _count: usize) callconv(.c) Oop {
    _ = _self;
    _ = _args;
    _ = _count;
    return floor.api().mkSmallInt(777);
}

var key_value_sum: i64 = 0;

fn addKeyAndValue(_self: Oop, args: [*]const Oop, _count: usize) callconv(.c) Oop {
    _ = _self;
    _ = _count;
    const api = floor.api();
    key_value_sum += api.smallIntValue(args[0]) + api.smallIntValue(args[1]);
    return api.nilObj;
}

test "Rust Collection vector protocol preserves persistent indexed values" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_COLLECTION_DYLIB") orelse
        return error.MissingRustCollectionDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustCollectionDlopenFailed;

    const prelude_path = std.posix.getenv("BIRNAM_RUST_PRELUDE_DYLIB") orelse
        return error.MissingRustPreludeDylib;
    try loadModule(api, prelude_path);

    const raw_identity = api.dlsym(handle, "birnam_collection_rust_identity") orelse
        return error.RustCollectionIdentityMissing;
    const identity: Identity = @ptrCast(@alignCast(raw_identity));
    try std.testing.expectEqualStrings("rust-collection-r1-vector", std.mem.span(identity()));

    const raw_init = api.dlsym(handle, "module_init") orelse
        return error.RustCollectionModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const vector = api.import("Vector") orelse return error.VectorMissing;
    const vector_vt = api.import("VectorVt") orelse return error.VectorVtMissing;
    const node_vt = api.import("VectorNode") orelse return error.VectorNodeMissing;
    const range = api.import("Range") orelse return error.RangeMissing;
    const range_vt = api.import("RangeVt") orelse return error.RangeVtMissing;
    const map = api.import("Map") orelse return error.MapMissing;
    const map_vt = api.import("MapVt") orelse return error.MapVtMissing;
    const map_node_vt = api.import("MapNode") orelse return error.MapNodeVtMissing;
    try std.testing.expectEqual(vector_vt, api.vtableOf(vector));
    try std.testing.expectEqual(range_vt, api.vtableOf(range));
    try std.testing.expectEqual(map_vt, api.vtableOf(map));
    const probe_node = api.alloc(node_vt, 32) orelse return error.VectorNodeAllocationFailed;
    try std.testing.expectEqual(@as(usize, 32), api.objectPayloadSize(probe_node));
    const probe_range = api.alloc(range_vt, 16) orelse return error.RangeAllocationFailed;
    try std.testing.expectEqual(@as(usize, 16), api.objectPayloadSize(probe_range));
    const probe_map = api.alloc(map_vt, 16) orelse return error.MapAllocationFailed;
    try std.testing.expectEqual(@as(usize, 16), api.objectPayloadSize(probe_map));
    const probe_map_node = api.alloc(map_node_vt, 32) orelse return error.MapNodeAllocationFailed;
    try std.testing.expectEqual(@as(usize, 32), api.objectPayloadSize(probe_map_node));

    const first = send(api, vector, "add:", &.{api.mkSmallInt(10)});
    const second = send(api, first, "add:", &.{api.mkSmallInt(20)});
    const vector_size = send(api, vector, "size", &.{});
    const first_size = send(api, first, "size", &.{});
    const second_size = send(api, second, "size", &.{});
    try std.testing.expectEqual(@as(i64, 0), api.smallIntValue(vector_size));
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(first_size));
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(second_size));
    try std.testing.expectEqual(@as(i64, 10), api.smallIntValue(send(api, second, "at:", &.{api.mkSmallInt(0)})));
    try std.testing.expectEqual(@as(i64, 20), api.smallIntValue(send(api, second, "at:", &.{api.mkSmallInt(1)})));
    try std.testing.expectEqual(api.nilObj, send(api, second, "at:", &.{api.mkSmallInt(2)}));

    var vector_with_tail = vector;
    var value: i64 = 0;
    while (value < 40) : (value += 1) {
        vector_with_tail = send(api, vector_with_tail, "add:", &.{api.mkSmallInt(value)}) orelse
            return error.VectorAppendFailed;
    }
    try std.testing.expectEqual(@as(i64, 40), api.smallIntValue(send(api, vector_with_tail, "size", &.{})));
    try std.testing.expectEqual(@as(i64, 0), api.smallIntValue(send(api, vector_with_tail, "at:", &.{api.mkSmallInt(0)})));
    try std.testing.expectEqual(@as(i64, 31), api.smallIntValue(send(api, vector_with_tail, "at:", &.{api.mkSmallInt(31)})));
    try std.testing.expectEqual(@as(i64, 32), api.smallIntValue(send(api, vector_with_tail, "at:", &.{api.mkSmallInt(32)})));
    try std.testing.expectEqual(@as(i64, 39), api.smallIntValue(send(api, vector_with_tail, "at:", &.{api.mkSmallInt(39)})));

    const rendered = send(api, second, "printString", &.{});
    var rendered_len: usize = 0;
    const rendered_bytes = api.stringBytes(rendered, &rendered_len);
    try std.testing.expectEqualStrings("#[10 20]", rendered_bytes[0..rendered_len]);

    sum = 0;
    const block_vt = api.proto(api.object_vt);
    api.method(block_vt, api.intern("value:"), &collectInteger);
    const block = api.alloc(block_vt, 0);
    try std.testing.expectEqual(second, send(api, second, "do:", &.{block}));
    try std.testing.expectEqual(@as(i64, 30), sum);

    const fold_vt = api.proto(api.object_vt);
    api.method(fold_vt, api.intern("value:value:"), &addInteger);
    const fold_block = api.alloc(fold_vt, 0);
    const folded = send(api, second, "inject:into:", &.{ api.mkSmallInt(0), fold_block });
    try std.testing.expectEqual(@as(i64, 30), api.smallIntValue(folded));

    const copied = send(api, vector_with_tail, "copyFrom:to:", &.{ api.mkSmallInt(10), api.mkSmallInt(13) });
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(send(api, copied, "size", &.{})));
    try std.testing.expectEqual(@as(i64, 10), api.smallIntValue(send(api, copied, "at:", &.{api.mkSmallInt(0)})));
    try std.testing.expectEqual(api.nilObj, send(api, vector_with_tail, "copyFrom:to:", &.{ api.mkSmallInt(13), api.mkSmallInt(10) }));

    const reversed = send(api, second, "reversed", &.{});
    try std.testing.expectEqual(@as(i64, 20), api.smallIntValue(send(api, reversed, "at:", &.{api.mkSmallInt(0)})));
    try std.testing.expectEqual(@as(i64, 10), api.smallIntValue(send(api, reversed, "at:", &.{api.mkSmallInt(1)})));
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(send(api, second, "indexOf:", &.{api.mkSmallInt(20)})));
    try std.testing.expectEqual(api.nilObj, send(api, second, "indexOf:", &.{api.mkSmallInt(99)}));

    const joined = send(api, first, ",", &.{second});
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(send(api, joined, "size", &.{})));
    try std.testing.expectEqual(@as(i64, 20), api.smallIntValue(send(api, joined, "at:", &.{api.mkSmallInt(2)})));

    const integer_range = send(api, api.mkSmallInt(3), "to:", &.{api.mkSmallInt(5)});
    try std.testing.expectEqual(range_vt, api.vtableOf(integer_range));
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(send(api, integer_range, "size", &.{})));
    try std.testing.expectEqual(api.trueObj, send(api, integer_range, "includes:", &.{api.mkSmallInt(4)}));
    try std.testing.expectEqual(api.falseObj, send(api, integer_range, "includes:", &.{api.mkSmallInt(6)}));
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(send(api, integer_range, "at:", &.{api.mkSmallInt(0)})));
    try std.testing.expectEqual(@as(i64, 5), api.smallIntValue(send(api, integer_range, "at:", &.{api.mkSmallInt(2)})));
    try std.testing.expectEqual(api.nilObj, send(api, integer_range, "at:", &.{api.mkSmallInt(3)}));

    sum = 0;
    try std.testing.expectEqual(integer_range, send(api, integer_range, "do:", &.{block}));
    try std.testing.expectEqual(@as(i64, 12), sum);
    const range_folded = send(api, integer_range, "inject:into:", &.{ api.mkSmallInt(0), fold_block });
    try std.testing.expectEqual(@as(i64, 12), api.smallIntValue(range_folded));

    const range_rendered = send(api, integer_range, "printString", &.{});
    var range_rendered_len: usize = 0;
    const range_rendered_bytes = api.stringBytes(range_rendered, &range_rendered_len);
    try std.testing.expectEqualStrings("#[3 4 5]", range_rendered_bytes[0..range_rendered_len]);

    const range_reversed = send(api, integer_range, "reversed", &.{});
    try std.testing.expectEqual(@as(i64, 5), api.smallIntValue(send(api, range_reversed, "at:", &.{api.mkSmallInt(0)})));
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(send(api, range_reversed, "at:", &.{api.mkSmallInt(2)})));
    const range_joined = send(api, integer_range, ",", &.{second});
    try std.testing.expectEqual(@as(i64, 5), api.smallIntValue(send(api, range_joined, "size", &.{})));
    try std.testing.expectEqual(@as(i64, 20), api.smallIntValue(send(api, range_joined, "at:", &.{api.mkSmallInt(4)})));

    const empty_map = send(api, map, "new", &.{});
    try std.testing.expectEqual(@as(i64, 0), api.smallIntValue(send(api, empty_map, "size", &.{})));
    const map_one = send(api, empty_map, "at:put:", &.{ api.mkSmallInt(1), api.mkSmallInt(10) });
    const map_two = send(api, map_one, "at:put:", &.{ api.mkSmallInt(2), api.mkSmallInt(20) });
    try std.testing.expectEqual(@as(i64, 0), api.smallIntValue(send(api, empty_map, "size", &.{})));
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(send(api, map_one, "size", &.{})));
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(send(api, map_two, "size", &.{})));
    try std.testing.expectEqual(@as(i64, 20), api.smallIntValue(send(api, map_two, "at:", &.{api.mkSmallInt(2)})));
    try std.testing.expectEqual(api.trueObj, send(api, map_two, "includesKey:", &.{api.mkSmallInt(1)}));
    try std.testing.expectEqual(api.falseObj, send(api, map_two, "includesKey:", &.{api.mkSmallInt(3)}));

    const fallback_vt = api.proto(api.object_vt);
    api.method(fallback_vt, api.intern("value"), &defaultInteger);
    const fallback = api.alloc(fallback_vt, 0);
    try std.testing.expectEqual(@as(i64, 777), api.smallIntValue(send(api, map_two, "at:ifAbsent:", &.{ api.mkSmallInt(99), fallback })));

    const replaced = send(api, map_two, "at:put:", &.{ api.mkSmallInt(2), api.mkSmallInt(200) });
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(send(api, replaced, "size", &.{})));
    try std.testing.expectEqual(@as(i64, 20), api.smallIntValue(send(api, map_two, "at:", &.{api.mkSmallInt(2)})));
    try std.testing.expectEqual(@as(i64, 200), api.smallIntValue(send(api, replaced, "at:", &.{api.mkSmallInt(2)})));

    var many = empty_map;
    var map_value: i64 = 0;
    while (map_value < 50) : (map_value += 1) {
        many = send(api, many, "at:put:", &.{ api.mkSmallInt(map_value), api.mkSmallInt(map_value * 10) }) orelse
            return error.MapAssociationFailed;
    }
    try std.testing.expectEqual(@as(i64, 50), api.smallIntValue(send(api, many, "size", &.{})));
    try std.testing.expectEqual(@as(i64, 490), api.smallIntValue(send(api, many, "at:", &.{api.mkSmallInt(49)})));
    const map_keys = send(api, many, "keys", &.{});
    const map_values = send(api, many, "values", &.{});
    try std.testing.expect(send(api, map_keys, "indexOf:", &.{api.mkSmallInt(0)}) != api.nilObj);
    try std.testing.expect(send(api, map_values, "indexOf:", &.{api.mkSmallInt(490)}) != api.nilObj);

    sum = 0;
    const map_block_vt = api.proto(api.object_vt);
    api.method(map_block_vt, api.intern("value:"), &collectInteger);
    const map_block = api.alloc(map_block_vt, 0);
    try std.testing.expectEqual(many, send(api, many, "do:", &.{map_block}));
    try std.testing.expectEqual(@as(i64, 12250), sum);
    const map_fold = send(api, many, "inject:into:", &.{ api.mkSmallInt(0), fold_block });
    try std.testing.expectEqual(@as(i64, 12250), api.smallIntValue(map_fold));

    key_value_sum = 0;
    const kv_block_vt = api.proto(api.object_vt);
    api.method(kv_block_vt, api.intern("value:value:"), &addKeyAndValue);
    const kv_block = api.alloc(kv_block_vt, 0);
    try std.testing.expectEqual(many, send(api, many, "keysAndValuesDo:", &.{kv_block}));
    try std.testing.expectEqual(@as(i64, 1225 + 12250), key_value_sum);

    const map_rendered = send(api, map_two, "printString", &.{});
    var map_rendered_len: usize = 0;
    const map_rendered_bytes = api.stringBytes(map_rendered, &map_rendered_len);
    try std.testing.expect(map_rendered_bytes[0] == '{' and map_rendered_bytes[map_rendered_len - 1] == '}');
    try std.testing.expect(std.mem.indexOf(u8, map_rendered_bytes[0..map_rendered_len], "1 -> 10") != null);
    try std.testing.expect(std.mem.indexOf(u8, map_rendered_bytes[0..map_rendered_len], "2 -> 20") != null);

    const empty_range = send(api, api.mkSmallInt(5), "to:", &.{api.mkSmallInt(3)});
    try std.testing.expectEqual(@as(i64, 0), api.smallIntValue(send(api, empty_range, "size", &.{})));
    try std.testing.expectEqual(api.falseObj, send(api, empty_range, "includes:", &.{api.mkSmallInt(4)}));
    const empty_reversed = send(api, empty_range, "reversed", &.{});
    try std.testing.expectEqual(@as(i64, 0), api.smallIntValue(send(api, empty_reversed, "size", &.{})));
}
