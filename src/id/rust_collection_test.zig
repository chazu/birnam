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
    try std.testing.expectEqual(vector_vt, api.vtableOf(vector));
    const probe_node = api.alloc(node_vt, 32) orelse return error.VectorNodeAllocationFailed;
    try std.testing.expectEqual(@as(usize, 32), api.objectPayloadSize(probe_node));

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
}
