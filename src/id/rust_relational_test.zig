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
const CollectWindow = *const fn (Oop, u8, u64, u64, ?[*]ClassifiedTriple, usize, *usize) callconv(.c) c_int;
const ProjectLive = *const fn (Oop, u8, Oop, Oop, Oop, ?[*]Classified, usize, *usize) callconv(.c) c_int;
const ProjectPattern = extern struct { entity: Oop, attribute: Oop, value: Oop };
const ProjectJoinLive = *const fn (Oop, u8, ProjectPattern, ProjectPattern, ?[*]Classified, usize, *usize) callconv(.c) c_int;
const ProjectPredicateLive = *const fn (Oop, u8, ProjectPattern, u8, u8, Oop, ?[*]Classified, usize, *usize) callconv(.c) c_int;
const ProjectNegatedLive = *const fn (Oop, u8, ProjectPattern, ProjectPattern, ?[*]Classified, usize, *usize) callconv(.c) c_int;

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

fn hasTriple(
    records: []const ClassifiedTriple,
    entity: Oop,
    attribute: Oop,
    integer: i64,
) bool {
    for (records) |record| {
        if (record.entity.tag == 0 and record.entity.identity == @intFromPtr(entity) and
            record.attribute.tag == 0 and record.attribute.identity == @intFromPtr(attribute) and
            record.value.tag == 1 and record.value.integer == integer)
        {
            return true;
        }
    }
    return false;
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
    try std.testing.expectEqualStrings("rust-relational-r4.5-project", std.mem.span(identity()));

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

    try std.testing.expect(hasTriple(records[0..written], entity, attribute, 73));
    try std.testing.expectEqual(@as(c_int, -5), collect(store, null, 0, &written));
}

test "Rust Relational callback preserves Store transaction windows" {
    const api = floor.api();
    try loadEnvModule(api, "BIRNAM_RELATIONAL_STORE_DYLIB");
    const relation_path = std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB") orelse
        return error.MissingRustRelationalDylib;
    try loadModule(api, relation_path);
    const relation_z = try std.heap.c_allocator.dupeZ(u8, relation_path);
    defer std.heap.c_allocator.free(relation_z);
    const relation_handle = api.dlopen(relation_z.ptr, 1) orelse return error.RustRelationalDlopenFailed;
    const collect_raw = api.dlsym(relation_handle, "birnam_relational_rust_collect_window") orelse
        return error.RustRelationalCollectWindowMissing;
    const collect: CollectWindow = @ptrCast(@alignCast(collect_raw));

    const store = api.import("Store") orelse return error.RustRelationalStoreMissing;
    const entity = api.intern("rust_relational_window_entity");
    const attribute = api.intern("rust_relational_window_attribute");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ attribute, api.intern("history"), api.trueObj }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ entity, attribute, api.mkSmallInt(11) }, 3);
    const after_first = api.send(store, api.intern("basis"), &.{}, 0);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ entity, attribute, api.mkSmallInt(22) }, 3);
    const after_second = api.send(store, api.intern("basis"), &.{}, 0);

    var records: [32]ClassifiedTriple = undefined;
    var written: usize = 0;
    try std.testing.expectEqual(
        @as(c_int, 0),
        collect(store, 1, 0, @intCast(api.smallIntValue(after_first)), &records, records.len, &written),
    );
    try std.testing.expect(hasTriple(records[0..written], entity, attribute, 11));
    try std.testing.expect(!hasTriple(records[0..written], entity, attribute, 22));

    try std.testing.expectEqual(
        @as(c_int, 0),
        collect(store, 2, @intCast(api.smallIntValue(after_first)), 0, &records, records.len, &written),
    );
    try std.testing.expect(hasTriple(records[0..written], entity, attribute, 22));
    try std.testing.expect(!hasTriple(records[0..written], entity, attribute, 11));

    try std.testing.expectEqual(
        @as(c_int, 0),
        collect(
            store,
            3,
            @intCast(api.smallIntValue(after_first)),
            @intCast(api.smallIntValue(after_second)),
            &records,
            records.len,
            &written,
        ),
    );
    try std.testing.expect(hasTriple(records[0..written], entity, attribute, 22));
    try std.testing.expectEqual(@as(c_int, -7), collect(store, 9, 0, 0, &records, records.len, &written));
}

test "Rust Relational kernel projects a Store pattern across the ABI" {
    const api = floor.api();
    try loadEnvModule(api, "BIRNAM_RELATIONAL_STORE_DYLIB");
    const relation_path = std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB") orelse
        return error.MissingRustRelationalDylib;
    try loadModule(api, relation_path);
    const relation_z = try std.heap.c_allocator.dupeZ(u8, relation_path);
    defer std.heap.c_allocator.free(relation_z);
    const relation_handle = api.dlopen(relation_z.ptr, 1) orelse return error.RustRelationalDlopenFailed;
    const project_raw = api.dlsym(relation_handle, "birnam_relational_rust_project_live") orelse
        return error.RustRelationalProjectMissing;
    const project: ProjectLive = @ptrCast(@alignCast(project_raw));

    const store = api.import("Store") orelse return error.RustRelationalStoreMissing;
    const entity = api.intern("rust_relational_project_entity");
    const attribute = api.intern("rust_relational_project_attribute");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ entity, attribute, api.mkSmallInt(123) }, 3);

    var values: [8]Classified = undefined;
    var written: usize = 0;
    try std.testing.expectEqual(
        @as(c_int, 0),
        project(store, 0, null, attribute, api.mkSmallInt(123), &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 1), written);
    try std.testing.expectEqual(@as(u8, 0), values[0].tag);
    try std.testing.expectEqual(@intFromPtr(entity), @as(usize, @intCast(values[0].identity)));

    try std.testing.expectEqual(
        @as(c_int, -5),
        project(store, 0, null, attribute, api.mkSmallInt(123), null, 0, &written),
    );
    try std.testing.expectEqual(
        @as(c_int, 0),
        project(store, 0, null, attribute, api.mkSmallInt(999), &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 0), written);
    try std.testing.expectEqual(
        @as(c_int, -7),
        project(store, 3, null, attribute, api.mkSmallInt(123), &values, values.len, &written),
    );
}

test "Rust Relational kernel joins two Store patterns" {
    const api = floor.api();
    try loadEnvModule(api, "BIRNAM_RELATIONAL_STORE_DYLIB");
    const relation_path = std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB") orelse
        return error.MissingRustRelationalDylib;
    try loadModule(api, relation_path);
    const relation_z = try std.heap.c_allocator.dupeZ(u8, relation_path);
    defer std.heap.c_allocator.free(relation_z);
    const relation_handle = api.dlopen(relation_z.ptr, 1) orelse return error.RustRelationalDlopenFailed;
    const join_raw = api.dlsym(relation_handle, "birnam_relational_rust_project_join_live") orelse
        return error.RustRelationalProjectJoinMissing;
    const join: ProjectJoinLive = @ptrCast(@alignCast(join_raw));

    const store = api.import("Store") orelse return error.RustRelationalStoreMissing;
    const entity = api.intern("rust_relational_join_entity");
    const other = api.intern("rust_relational_join_other");
    const city = api.intern("rust_relational_join_city");
    const pet = api.intern("rust_relational_join_pet");
    const boston = api.intern("rust_relational_join_boston");
    const dog = api.intern("rust_relational_join_dog");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ entity, city, boston }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ entity, pet, dog }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ other, city, boston }, 3);

    var values: [8]Classified = undefined;
    var written: usize = 0;
    try std.testing.expectEqual(
        @as(c_int, 0),
        join(
            store,
            0,
            .{ .entity = null, .attribute = city, .value = boston },
            .{ .entity = null, .attribute = pet, .value = dog },
            &values,
            values.len,
            &written,
        ),
    );
    try std.testing.expectEqual(@as(usize, 1), written);
    try std.testing.expectEqual(@as(u8, 0), values[0].tag);
    try std.testing.expectEqual(@intFromPtr(entity), @as(usize, @intCast(values[0].identity)));
}

test "Rust Relational kernel filters a bound value with a numeric predicate" {
    const api = floor.api();
    try loadEnvModule(api, "BIRNAM_RELATIONAL_STORE_DYLIB");
    const relation_path = std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB") orelse
        return error.MissingRustRelationalDylib;
    try loadModule(api, relation_path);
    const relation_z = try std.heap.c_allocator.dupeZ(u8, relation_path);
    defer std.heap.c_allocator.free(relation_z);
    const relation_handle = api.dlopen(relation_z.ptr, 1) orelse return error.RustRelationalDlopenFailed;
    const predicate_raw = api.dlsym(relation_handle, "birnam_relational_rust_project_predicate_live") orelse
        return error.RustRelationalPredicateMissing;
    const predicate: ProjectPredicateLive = @ptrCast(@alignCast(predicate_raw));

    const store = api.import("Store") orelse return error.RustRelationalStoreMissing;
    const young = api.intern("rust_relational_predicate_young");
    const mid = api.intern("rust_relational_predicate_mid");
    const fractional = api.intern("rust_relational_predicate_fractional");
    const age = api.intern("rust_relational_predicate_age");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ young, age, api.mkSmallInt(18) }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ mid, age, api.mkSmallInt(30) }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ fractional, age, api.mkFloat(30.5) }, 3);

    const pattern = ProjectPattern{ .entity = null, .attribute = age, .value = null };
    var values: [8]Classified = undefined;
    var written: usize = 0;
    try std.testing.expectEqual(
        @as(c_int, 0),
        predicate(store, 0, pattern, 2, 2, api.mkSmallInt(18), &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 2), written);
    try std.testing.expectEqual(@intFromPtr(mid), @as(usize, @intCast(values[0].identity)));
    try std.testing.expectEqual(@intFromPtr(fractional), @as(usize, @intCast(values[1].identity)));

    try std.testing.expectEqual(
        @as(c_int, 0),
        predicate(store, 0, pattern, 2, 4, api.mkSmallInt(30), &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 1), written);
    try std.testing.expectEqual(@intFromPtr(mid), @as(usize, @intCast(values[0].identity)));

    try std.testing.expectEqual(
        @as(c_int, -7),
        predicate(store, 0, pattern, 2, 9, api.mkSmallInt(18), &values, values.len, &written),
    );
    try std.testing.expectEqual(
        @as(c_int, -7),
        predicate(store, 0, pattern, 1, 2, api.mkSmallInt(18), &values, values.len, &written),
    );
}

test "Rust Relational kernel applies negation without escaping inner bindings" {
    const api = floor.api();
    try loadEnvModule(api, "BIRNAM_RELATIONAL_STORE_DYLIB");
    const relation_path = std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB") orelse
        return error.MissingRustRelationalDylib;
    try loadModule(api, relation_path);
    const relation_z = try std.heap.c_allocator.dupeZ(u8, relation_path);
    defer std.heap.c_allocator.free(relation_z);
    const relation_handle = api.dlopen(relation_z.ptr, 1) orelse return error.RustRelationalDlopenFailed;
    const negated_raw = api.dlsym(relation_handle, "birnam_relational_rust_project_negated_live") orelse
        return error.RustRelationalNegatedMissing;
    const negated: ProjectNegatedLive = @ptrCast(@alignCast(negated_raw));

    const store = api.import("Store") orelse return error.RustRelationalStoreMissing;
    const alice = api.intern("rust_relational_negated_alice");
    const bob = api.intern("rust_relational_negated_bob");
    const city = api.intern("rust_relational_negated_city");
    const pet = api.intern("rust_relational_negated_pet");
    const boston = api.intern("rust_relational_negated_boston");
    const cat = api.intern("rust_relational_negated_cat");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ alice, city, boston }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ alice, pet, api.intern("rust_relational_negated_dog") }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ bob, city, boston }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ bob, pet, cat }, 3);

    const positive = ProjectPattern{ .entity = null, .attribute = city, .value = boston };
    const negative = ProjectPattern{ .entity = null, .attribute = pet, .value = cat };
    var values: [8]Classified = undefined;
    var written: usize = 0;
    try std.testing.expectEqual(
        @as(c_int, 0),
        negated(store, 0, positive, negative, &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 1), written);
    try std.testing.expectEqual(@intFromPtr(alice), @as(usize, @intCast(values[0].identity)));

    const no_fish = ProjectPattern{ .entity = null, .attribute = pet, .value = api.intern("rust_relational_negated_fish") };
    try std.testing.expectEqual(
        @as(c_int, 0),
        negated(store, 0, positive, no_fish, &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 2), written);
    try std.testing.expectEqual(@intFromPtr(alice), @as(usize, @intCast(values[0].identity)));
    try std.testing.expectEqual(@intFromPtr(bob), @as(usize, @intCast(values[1].identity)));

    try std.testing.expectEqual(
        @as(c_int, -7),
        negated(store, 3, positive, negative, &values, values.len, &written),
    );
}
