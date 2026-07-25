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
const ClassifiedTriple = extern struct {
    entity: Classified,
    attribute: Classified,
    value: Classified,
};
const CollectLive = *const fn (Oop, ?[*]ClassifiedTriple, usize, *usize) callconv(.c) c_int;

fn loadModule(api: *idapi.IdApi, path: []const u8) !void {
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustRelationalDlopenFailed;
    const init_raw = api.dlsym(handle, "module_init") orelse return error.RustRelationalModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(init_raw));
    _ = init(api, null);
}

fn loadEnvModule(api: *idapi.IdApi, variable: []const u8) !void {
    const path = std.posix.getenv(variable) orelse return error.MissingRustRelationalStoreDylib;
    try loadModule(api, path);
}

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
    try std.testing.expectEqualStrings("rust-relational-r4.4-bridge", std.mem.span(identity()));

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

test "Rust Relational callback copies Store live triples" {
    const api = floor.api();
    try loadEnvModule(api, "BIRNAM_RELATIONAL_STORE_DYLIB");
    const relation_path = std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB") orelse
        return error.MissingRustRelationalDylib;
    try loadModule(api, relation_path);

    const store = api.import("Store") orelse return error.RustRelationalStoreMissing;
    const entity = api.intern("rust_relational_bridge_entity");
    const attribute = api.intern("rust_relational_bridge_attribute");
    const value = api.mkSmallInt(73);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ entity, attribute, value }, 3);

    const collect_raw = api.dlsym(
        api.dlopen(std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB").?.ptr, 1) orelse
            return error.RustRelationalDlopenFailed,
        "birnam_relational_rust_collect_live",
    ) orelse return error.RustRelationalCollectLiveMissing;
    const collect: CollectLive = @ptrCast(@alignCast(collect_raw));
    var records: [32]ClassifiedTriple = undefined;
    var written: usize = 0;
    try std.testing.expectEqual(@as(c_int, 0), collect(store, &records, records.len, &written));
    try std.testing.expect(written >= 1);

    var found = false;
    for (records[0..written]) |record| {
        if (record.entity.tag == 0 and record.entity.identity == @intFromPtr(entity) and
            record.attribute.tag == 0 and record.attribute.identity == @intFromPtr(attribute) and
            record.value.tag == 1 and record.value.integer == 73)
        {
            found = true;
            break;
        }
    }
    try std.testing.expect(found);
    try std.testing.expectEqual(@as(c_int, -5), collect(store, null, 0, &written));
}
