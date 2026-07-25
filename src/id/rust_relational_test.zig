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
const ProjectDisjunctionLive = *const fn (Oop, u8, ProjectPattern, ProjectPattern, ?[*]Classified, usize, *usize) callconv(.c) c_int;
const ProjectRecursiveLive = *const fn (Oop, u8, ProjectPattern, Oop, ?[*]Classified, usize, *usize) callconv(.c) c_int;
const AggregateLive = *const fn (Oop, u8, ProjectPattern, u8, ?*Classified) callconv(.c) c_int;
const ProjectWindowLive = *const fn (Oop, u8, u64, u64, u8, ProjectPattern, ?[*]Classified, usize, *usize) callconv(.c) c_int;
const PullSpecNode = extern struct {
    attribute: Oop,
    kind: u8,
    reserved: [7]u8,
    child_start: u32,
    child_len: u32,
};
const PulledRecord = extern struct {
    depth: u8,
    reserved: [7]u8,
    path: [4]Classified,
    value: Classified,
};
const PullLive = *const fn (Oop, Oop, ?[*]const PullSpecNode, usize, usize, ?[*]PulledRecord, usize, *usize) callconv(.c) c_int;
const GroupedRecord = extern struct {
    group: Classified,
    aggregate: Classified,
    aggregate_status: u8,
    reserved: [7]u8,
};
const GroupedPattern = extern struct {
    entity: Oop,
    attribute: Oop,
    value: Oop,
    entity_variable: u32,
    attribute_variable: u32,
    value_variable: u32,
    reserved: u32,
};
const GroupAggregateLive = *const fn (Oop, GroupedPattern, GroupedPattern, u32, u32, u8, ?[*]GroupedRecord, usize, *usize) callconv(.c) c_int;
const RuleTermNode = extern struct {
    object: Oop,
    variable: u32,
    kind: u8,
    reserved: [3]u8,
};
const RuleClauseNode = extern struct {
    kind: u8,
    reserved: [3]u8,
    name: Oop,
    terms: ?[*]const RuleTermNode,
    term_count: u32,
    reserved_tail: u32,
};
const RuleDefinitionNode = extern struct {
    name: Oop,
    params: ?[*]const u32,
    param_count: u32,
    reserved_params: u32,
    body: ?[*]const RuleClauseNode,
    body_count: u32,
    reserved_body: u32,
};
const RuleBodyNode = extern struct {
    kind: u8,
    reserved: [3]u8,
    name: Oop,
    terms: ?[*]const RuleTermNode,
    term_count: u32,
    reserved_terms: u32,
    children: ?[*]const RuleBodyNode,
    child_count: u32,
    reserved_children: u32,
};
const RuleTreeDefinitionNode = extern struct {
    name: Oop,
    params: ?[*]const u32,
    param_count: u32,
    reserved_params: u32,
    body: ?[*]const RuleBodyNode,
    body_count: u32,
    reserved_body: u32,
};
const ProjectRuleLive = *const fn (Oop, ?[*]const RuleDefinitionNode, usize, Oop, ?[*]const RuleTermNode, usize, u32, ?[*]Classified, usize, *usize) callconv(.c) c_int;
const ProjectRuleTreeLive = *const fn (Oop, ?[*]const RuleTreeDefinitionNode, usize, Oop, ?[*]const RuleTermNode, usize, u32, ?[*]Classified, usize, *usize) callconv(.c) c_int;
const ProjectRuleStoreLive = *const fn (Oop, Oop, ?[*]const RuleTermNode, usize, u32, ?[*]Classified, usize, *usize) callconv(.c) c_int;

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

fn ruleVar(variable: u32) RuleTermNode {
    return .{ .object = null, .variable = variable, .kind = 1, .reserved = [_]u8{0} ** 3 };
}

fn ruleConst(object: Oop) RuleTermNode {
    return .{ .object = object, .variable = 0, .kind = 0, .reserved = [_]u8{0} ** 3 };
}

fn makeVector(api: *idapi.IdApi, elements: []const Oop) Oop {
    const factory = api.import("Vector") orelse return null;
    const empty = [_]Oop{};
    const vector = api.send(factory, api.intern("new"), &empty, 0);
    var result = vector;
    for (elements) |element| result = api.send(result, api.intern("add:"), &.{element}, 1);
    return result;
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

test "Rust Relational kernel unions two disjunction branches" {
    const api = floor.api();
    try loadEnvModule(api, "BIRNAM_RELATIONAL_STORE_DYLIB");
    const relation_path = std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB") orelse
        return error.MissingRustRelationalDylib;
    try loadModule(api, relation_path);
    const relation_z = try std.heap.c_allocator.dupeZ(u8, relation_path);
    defer std.heap.c_allocator.free(relation_z);
    const relation_handle = api.dlopen(relation_z.ptr, 1) orelse return error.RustRelationalDlopenFailed;
    const disjunction_raw = api.dlsym(relation_handle, "birnam_relational_rust_project_disjunction_live") orelse
        return error.RustRelationalDisjunctionMissing;
    const disjunction: ProjectDisjunctionLive = @ptrCast(@alignCast(disjunction_raw));

    const store = api.import("Store") orelse return error.RustRelationalStoreMissing;
    const alice = api.intern("rust_relational_disjunction_alice");
    const bob = api.intern("rust_relational_disjunction_bob");
    const carol = api.intern("rust_relational_disjunction_carol");
    const city = api.intern("rust_relational_disjunction_city");
    const pet = api.intern("rust_relational_disjunction_pet");
    const boston = api.intern("rust_relational_disjunction_boston");
    const denver = api.intern("rust_relational_disjunction_denver");
    const cat = api.intern("rust_relational_disjunction_cat");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ alice, city, boston }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ bob, city, boston }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ carol, city, denver }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ bob, pet, cat }, 3);

    const boston_or_denver = ProjectPattern{ .entity = null, .attribute = city, .value = boston };
    const denver_branch = ProjectPattern{ .entity = null, .attribute = city, .value = denver };
    var values: [8]Classified = undefined;
    var written: usize = 0;
    try std.testing.expectEqual(
        @as(c_int, 0),
        disjunction(store, 0, boston_or_denver, denver_branch, &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 3), written);
    try std.testing.expectEqual(@intFromPtr(alice), @as(usize, @intCast(values[0].identity)));
    try std.testing.expectEqual(@intFromPtr(bob), @as(usize, @intCast(values[1].identity)));
    try std.testing.expectEqual(@intFromPtr(carol), @as(usize, @intCast(values[2].identity)));

    const cat_or_denver = ProjectPattern{ .entity = null, .attribute = pet, .value = cat };
    try std.testing.expectEqual(
        @as(c_int, 0),
        disjunction(store, 0, cat_or_denver, denver_branch, &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 2), written);
    try std.testing.expectEqual(@intFromPtr(bob), @as(usize, @intCast(values[0].identity)));
    try std.testing.expectEqual(@intFromPtr(carol), @as(usize, @intCast(values[1].identity)));

    try std.testing.expectEqual(
        @as(c_int, -5),
        disjunction(store, 0, boston_or_denver, denver_branch, null, 0, &written),
    );
    try std.testing.expectEqual(
        @as(c_int, -7),
        disjunction(store, 3, boston_or_denver, denver_branch, &values, values.len, &written),
    );
}

test "Rust Relational kernel reaches a finite recursive edge fixpoint" {
    const api = floor.api();
    try loadEnvModule(api, "BIRNAM_RELATIONAL_STORE_DYLIB");
    const relation_path = std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB") orelse
        return error.MissingRustRelationalDylib;
    try loadModule(api, relation_path);
    const relation_z = try std.heap.c_allocator.dupeZ(u8, relation_path);
    defer std.heap.c_allocator.free(relation_z);
    const relation_handle = api.dlopen(relation_z.ptr, 1) orelse return error.RustRelationalDlopenFailed;
    const recursive_raw = api.dlsym(relation_handle, "birnam_relational_rust_project_recursive_live") orelse
        return error.RustRelationalRecursiveMissing;
    const recursive: ProjectRecursiveLive = @ptrCast(@alignCast(recursive_raw));

    const store = api.import("Store") orelse return error.RustRelationalStoreMissing;
    const tom = api.intern("rust_relational_recursive_tom");
    const bob = api.intern("rust_relational_recursive_bob");
    const ann = api.intern("rust_relational_recursive_ann");
    const kim = api.intern("rust_relational_recursive_kim");
    const parent = api.intern("rust_relational_recursive_parent");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ tom, parent, bob }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ bob, parent, ann }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ ann, parent, kim }, 3);

    const edge = ProjectPattern{ .entity = null, .attribute = parent, .value = null };
    var values: [8]Classified = undefined;
    var written: usize = 0;
    try std.testing.expectEqual(
        @as(c_int, 0),
        recursive(store, 2, edge, tom, &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 3), written);
    try std.testing.expectEqual(@intFromPtr(bob), @as(usize, @intCast(values[0].identity)));
    try std.testing.expectEqual(@intFromPtr(ann), @as(usize, @intCast(values[1].identity)));
    try std.testing.expectEqual(@intFromPtr(kim), @as(usize, @intCast(values[2].identity)));

    try std.testing.expectEqual(
        @as(c_int, 0),
        recursive(store, 0, edge, tom, &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 1), written);
    try std.testing.expectEqual(@intFromPtr(tom), @as(usize, @intCast(values[0].identity)));

    try std.testing.expectEqual(
        @as(c_int, -5),
        recursive(store, 2, edge, tom, null, 0, &written),
    );
    try std.testing.expectEqual(
        @as(c_int, -7),
        recursive(store, 1, edge, tom, &values, values.len, &written),
    );
    try std.testing.expectEqual(
        @as(c_int, -7),
        recursive(
            store,
            2,
            .{ .entity = tom, .attribute = parent, .value = null },
            tom,
            &values,
            values.len,
            &written,
        ),
    );
}

test "Rust Relational kernel folds distinct aggregate values" {
    const api = floor.api();
    try loadEnvModule(api, "BIRNAM_RELATIONAL_STORE_DYLIB");
    const relation_path = std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB") orelse
        return error.MissingRustRelationalDylib;
    try loadModule(api, relation_path);
    const relation_z = try std.heap.c_allocator.dupeZ(u8, relation_path);
    defer std.heap.c_allocator.free(relation_z);
    const relation_handle = api.dlopen(relation_z.ptr, 1) orelse return error.RustRelationalDlopenFailed;
    const aggregate_raw = api.dlsym(relation_handle, "birnam_relational_rust_aggregate_live") orelse
        return error.RustRelationalAggregateMissing;
    const aggregate: AggregateLive = @ptrCast(@alignCast(aggregate_raw));

    const store = api.import("Store") orelse return error.RustRelationalStoreMissing;
    const alice = api.intern("rust_relational_aggregate_alice");
    const bob = api.intern("rust_relational_aggregate_bob");
    const carol = api.intern("rust_relational_aggregate_carol");
    const dave = api.intern("rust_relational_aggregate_dave");
    const age = api.intern("rust_relational_aggregate_age");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ alice, age, api.mkSmallInt(18) }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ bob, age, api.mkSmallInt(30) }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ carol, age, api.mkSmallInt(40) }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ dave, age, api.mkSmallInt(30) }, 3);

    const pattern = ProjectPattern{ .entity = null, .attribute = age, .value = null };
    var output: Classified = undefined;
    try std.testing.expectEqual(@as(c_int, 0), aggregate(store, 0, pattern, 0, &output));
    try std.testing.expectEqual(@as(u8, 1), output.tag);
    try std.testing.expectEqual(@as(i64, 4), output.integer);
    try std.testing.expectEqual(@as(c_int, 0), aggregate(store, 2, pattern, 0, &output));
    try std.testing.expectEqual(@as(i64, 3), output.integer);
    try std.testing.expectEqual(@as(c_int, 0), aggregate(store, 2, pattern, 1, &output));
    try std.testing.expectEqual(@as(i64, 88), output.integer);
    try std.testing.expectEqual(@as(c_int, 0), aggregate(store, 2, pattern, 2, &output));
    try std.testing.expectEqual(@as(i64, 18), output.integer);
    try std.testing.expectEqual(@as(c_int, 0), aggregate(store, 2, pattern, 3, &output));
    try std.testing.expectEqual(@as(i64, 40), output.integer);
    try std.testing.expectEqual(@as(c_int, 0), aggregate(store, 2, pattern, 4, &output));
    try std.testing.expectEqual(@as(i64, 29), output.integer);

    const missing = ProjectPattern{ .entity = null, .attribute = api.intern("rust_relational_aggregate_missing"), .value = null };
    try std.testing.expectEqual(@as(c_int, 0), aggregate(store, 2, missing, 1, &output));
    try std.testing.expectEqual(@as(i64, 0), output.integer);
    try std.testing.expectEqual(@as(c_int, -8), aggregate(store, 2, missing, 2, &output));
    try std.testing.expectEqual(@as(c_int, -7), aggregate(store, 2, pattern, 9, &output));

    const city = api.intern("rust_relational_aggregate_city");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ alice, city, api.intern("Boston") }, 3);
    const nonnumeric = ProjectPattern{ .entity = null, .attribute = city, .value = null };
    try std.testing.expectEqual(@as(c_int, -4), aggregate(store, 2, nonnumeric, 2, &output));
}

test "Rust Relational kernel projects Store transaction windows" {
    const api = floor.api();
    try loadEnvModule(api, "BIRNAM_RELATIONAL_STORE_DYLIB");
    const relation_path = std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB") orelse
        return error.MissingRustRelationalDylib;
    try loadModule(api, relation_path);
    const relation_z = try std.heap.c_allocator.dupeZ(u8, relation_path);
    defer std.heap.c_allocator.free(relation_z);
    const relation_handle = api.dlopen(relation_z.ptr, 1) orelse return error.RustRelationalDlopenFailed;
    const window_raw = api.dlsym(relation_handle, "birnam_relational_rust_project_window_live") orelse
        return error.RustRelationalProjectWindowMissing;
    const window: ProjectWindowLive = @ptrCast(@alignCast(window_raw));

    const store = api.import("Store") orelse return error.RustRelationalStoreMissing;
    const entity = api.intern("rust_relational_window_project_entity");
    const attribute = api.intern("rust_relational_window_project_attribute");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ attribute, api.intern("history"), api.trueObj }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ entity, attribute, api.mkSmallInt(11) }, 3);
    const after_first = api.send(store, api.intern("basis"), &.{}, 0);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ entity, attribute, api.mkSmallInt(22) }, 3);
    const after_second = api.send(store, api.intern("basis"), &.{}, 0);

    const pattern = ProjectPattern{ .entity = null, .attribute = attribute, .value = null };
    var values: [8]Classified = undefined;
    var written: usize = 0;
    try std.testing.expectEqual(
        @as(c_int, 0),
        window(store, 1, 0, @intCast(api.smallIntValue(after_first)), 2, pattern, &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 1), written);
    try std.testing.expectEqual(@as(i64, 11), values[0].integer);

    try std.testing.expectEqual(
        @as(c_int, 0),
        window(store, 2, @intCast(api.smallIntValue(after_first)), 0, 2, pattern, &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 1), written);
    try std.testing.expectEqual(@as(i64, 22), values[0].integer);

    try std.testing.expectEqual(
        @as(c_int, 0),
        window(
            store,
            3,
            @intCast(api.smallIntValue(after_first)),
            @intCast(api.smallIntValue(after_second)),
            2,
            pattern,
            &values,
            values.len,
            &written,
        ),
    );
    try std.testing.expectEqual(@as(usize, 1), written);
    try std.testing.expectEqual(@as(i64, 22), values[0].integer);

    try std.testing.expectEqual(
        @as(c_int, -7),
        window(store, 9, 0, 0, 2, pattern, &values, values.len, &written),
    );
    try std.testing.expectEqual(
        @as(c_int, -7),
        window(store, 1, 0, std.math.maxInt(u64), 2, pattern, &values, values.len, &written),
    );
}

test "Rust Relational pull flattens Store cardinality and nested references" {
    const api = floor.api();
    try loadEnvModule(api, "BIRNAM_RELATIONAL_STORE_DYLIB");
    const relation_path = std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB") orelse
        return error.MissingRustRelationalDylib;
    try loadModule(api, relation_path);
    const relation_z = try std.heap.c_allocator.dupeZ(u8, relation_path);
    defer std.heap.c_allocator.free(relation_z);
    const relation_handle = api.dlopen(relation_z.ptr, 1) orelse return error.RustRelationalDlopenFailed;
    const pull_raw = api.dlsym(relation_handle, "birnam_relational_rust_pull_live") orelse
        return error.RustRelationalPullMissing;
    const pull: PullLive = @ptrCast(@alignCast(pull_raw));

    const store = api.import("Store") orelse return error.RustRelationalStoreMissing;
    const alice = api.intern("rust_relational_pull_alice");
    const bob = api.intern("rust_relational_pull_bob");
    const carol = api.intern("rust_relational_pull_carol");
    const friend = api.intern("rust_relational_pull_friend");
    const name = api.intern("rust_relational_pull_name");
    const bob_name = api.intern("rust_relational_pull_bob_name");
    const carol_name = api.intern("rust_relational_pull_carol_name");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ friend, api.intern("cardinality"), api.intern("many") }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ alice, friend, bob }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ alice, friend, carol }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ bob, name, bob_name }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ carol, name, carol_name }, 3);

    const nodes = [_]PullSpecNode{
        .{ .attribute = friend, .kind = 1, .reserved = [_]u8{0} ** 7, .child_start = 1, .child_len = 1 },
        .{ .attribute = name, .kind = 0, .reserved = [_]u8{0} ** 7, .child_start = 0, .child_len = 0 },
    };
    var records: [8]PulledRecord = undefined;
    var written: usize = 0;
    try std.testing.expectEqual(
        @as(c_int, 0),
        pull(store, alice, &nodes, 1, nodes.len, &records, records.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 2), written);
    for (records[0..written]) |record| {
        try std.testing.expectEqual(@as(u8, 2), record.depth);
        try std.testing.expectEqual(@intFromPtr(friend), @as(usize, @intCast(record.path[0].identity)));
        try std.testing.expectEqual(@intFromPtr(name), @as(usize, @intCast(record.path[1].identity)));
        try std.testing.expect(record.value.identity == @intFromPtr(bob_name) or record.value.identity == @intFromPtr(carol_name));
    }
    try std.testing.expectEqual(
        @as(c_int, -5),
        pull(store, alice, &nodes, 1, nodes.len, null, 0, &written),
    );

    var malformed = nodes;
    malformed[0].kind = 2;
    try std.testing.expectEqual(
        @as(c_int, -7),
        pull(store, alice, &malformed, 1, malformed.len, &records, records.len, &written),
    );
}

test "Rust Relational grouped aggregate preserves group order and distinct values" {
    const api = floor.api();
    try loadEnvModule(api, "BIRNAM_RELATIONAL_STORE_DYLIB");
    const relation_path = std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB") orelse
        return error.MissingRustRelationalDylib;
    try loadModule(api, relation_path);
    const relation_z = try std.heap.c_allocator.dupeZ(u8, relation_path);
    defer std.heap.c_allocator.free(relation_z);
    const relation_handle = api.dlopen(relation_z.ptr, 1) orelse return error.RustRelationalDlopenFailed;
    const grouped_raw = api.dlsym(relation_handle, "birnam_relational_rust_group_aggregate_live") orelse
        return error.RustRelationalGroupedAggregateMissing;
    const grouped: GroupAggregateLive = @ptrCast(@alignCast(grouped_raw));

    const store = api.import("Store") orelse return error.RustRelationalStoreMissing;
    const city = api.intern("rust_relational_group_city");
    const age = api.intern("rust_relational_group_age");
    const boston = api.intern("rust_relational_group_boston");
    const denver = api.intern("rust_relational_group_denver");
    const one = api.intern("rust_relational_group_one");
    const two = api.intern("rust_relational_group_two");
    const three = api.intern("rust_relational_group_three");
    const four = api.intern("rust_relational_group_four");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ one, city, boston }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ one, age, api.mkSmallInt(20) }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ two, city, boston }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ two, age, api.mkSmallInt(40) }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ three, city, denver }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ three, age, api.mkSmallInt(30) }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ four, city, denver }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ four, age, api.mkSmallInt(30) }, 3);

    const city_pattern = GroupedPattern{
        .entity = null,
        .attribute = city,
        .value = null,
        .entity_variable = 0,
        .attribute_variable = 3,
        .value_variable = 1,
        .reserved = 0,
    };
    const age_pattern = GroupedPattern{
        .entity = null,
        .attribute = age,
        .value = null,
        .entity_variable = 0,
        .attribute_variable = 4,
        .value_variable = 2,
        .reserved = 0,
    };
    var records: [8]GroupedRecord = undefined;
    var written: usize = 0;
    try std.testing.expectEqual(
        @as(c_int, 0),
        grouped(store, city_pattern, age_pattern, 1, 2, 1, &records, records.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 2), written);
    try std.testing.expectEqual(@intFromPtr(boston), @as(usize, @intCast(records[0].group.identity)));
    try std.testing.expectEqual(@as(i64, 60), records[0].aggregate.integer);
    try std.testing.expectEqual(@intFromPtr(denver), @as(usize, @intCast(records[1].group.identity)));
    try std.testing.expectEqual(@as(i64, 30), records[1].aggregate.integer);

    try std.testing.expectEqual(
        @as(c_int, 0),
        grouped(store, city_pattern, age_pattern, 1, 0, 0, &records, records.len, &written),
    );
    try std.testing.expectEqual(@as(i64, 2), records[0].aggregate.integer);
    try std.testing.expectEqual(@as(i64, 2), records[1].aggregate.integer);
    try std.testing.expectEqual(
        @as(c_int, -5),
        grouped(store, city_pattern, age_pattern, 1, 2, 1, null, 0, &written),
    );
    try std.testing.expectEqual(
        @as(c_int, -7),
        grouped(store, city_pattern, age_pattern, 1, 2, 9, &records, records.len, &written),
    );
}

test "Rust Relational rule probe reaches recursive and mutual fixpoints" {
    const api = floor.api();
    try loadEnvModule(api, "BIRNAM_RELATIONAL_STORE_DYLIB");
    try loadEnvModule(api, "BIRNAM_COLLECTION_DYLIB");
    try loadEnvModule(api, "BIRNAM_ZIG_RELATIONAL_DYLIB");
    const relation_path = std.posix.getenv("BIRNAM_RUST_RELATIONAL_DYLIB") orelse
        return error.MissingRustRelationalDylib;
    try loadModule(api, relation_path);
    const relation_z = try std.heap.c_allocator.dupeZ(u8, relation_path);
    defer std.heap.c_allocator.free(relation_z);
    const relation_handle = api.dlopen(relation_z.ptr, 1) orelse return error.RustRelationalDlopenFailed;
    const rule_raw = api.dlsym(relation_handle, "birnam_relational_rust_project_rule_live") orelse
        return error.RustRelationalRuleMissing;
    const rule: ProjectRuleLive = @ptrCast(@alignCast(rule_raw));

    const store = api.import("Store") orelse return error.RustRelationalStoreMissing;
    const empty_args = [_]Oop{};
    const first_entity = api.send(store, api.intern("entity"), &empty_args, 0);
    const second_entity = api.send(store, api.intern("entity"), &empty_args, 0);
    try std.testing.expect(first_entity != null);
    try std.testing.expect(second_entity != null);
    try std.testing.expect(first_entity != second_entity);
    const ancestor = api.intern("rust_relational_rule_ancestor");
    const tom = api.intern("rust_relational_rule_tom");
    const bob = api.intern("rust_relational_rule_bob");
    const ann = api.intern("rust_relational_rule_ann");
    const kim = api.intern("rust_relational_rule_kim");
    const parent = api.intern("rust_relational_rule_parent");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ tom, parent, bob }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ bob, parent, ann }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ ann, parent, kim }, 3);

    const ancestor_params = [_]u32{ 0, 1 };
    const base_terms = [_]RuleTermNode{ ruleVar(0), ruleConst(parent), ruleVar(1) };
    const base_body = [_]RuleClauseNode{
        .{ .kind = 0, .reserved = [_]u8{0} ** 3, .name = null, .terms = &base_terms, .term_count = 3, .reserved_tail = 0 },
    };
    const rec_triple_terms = [_]RuleTermNode{ ruleVar(0), ruleConst(parent), ruleVar(2) };
    const rec_call_terms = [_]RuleTermNode{ ruleVar(2), ruleVar(1) };
    const rec_body = [_]RuleClauseNode{
        .{ .kind = 0, .reserved = [_]u8{0} ** 3, .name = null, .terms = &rec_triple_terms, .term_count = 3, .reserved_tail = 0 },
        .{ .kind = 1, .reserved = [_]u8{0} ** 3, .name = ancestor, .terms = &rec_call_terms, .term_count = 2, .reserved_tail = 0 },
    };
    const definitions = [_]RuleDefinitionNode{
        .{ .name = ancestor, .params = &ancestor_params, .param_count = 2, .reserved_params = 0, .body = &base_body, .body_count = 1, .reserved_body = 0 },
        .{ .name = ancestor, .params = &ancestor_params, .param_count = 2, .reserved_params = 0, .body = &rec_body, .body_count = 2, .reserved_body = 0 },
    };
    const query_terms = [_]RuleTermNode{ ruleConst(tom), ruleVar(1) };
    var values: [16]Classified = undefined;
    var written: usize = 0;
    try std.testing.expectEqual(
        @as(c_int, 0),
        rule(store, &definitions, definitions.len, ancestor, &query_terms, query_terms.len, 1, &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 3), written);
    try std.testing.expectEqual(@intFromPtr(bob), @as(usize, @intCast(values[0].identity)));
    try std.testing.expectEqual(@intFromPtr(ann), @as(usize, @intCast(values[1].identity)));
    try std.testing.expectEqual(@intFromPtr(kim), @as(usize, @intCast(values[2].identity)));

    const threshold_rule = api.intern("rust_relational_rule_threshold");
    const high = api.intern("rust_relational_rule_high");
    const low = api.intern("rust_relational_rule_low");
    const score = api.intern("rust_relational_rule_score");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ high, score, api.mkSmallInt(20) }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ low, score, api.mkSmallInt(5) }, 3);
    const threshold_params = [_]u32{0};
    const threshold_triple = [_]RuleTermNode{ ruleVar(0), ruleConst(score), ruleVar(1) };
    const threshold_predicate = [_]RuleTermNode{ ruleVar(1), ruleConst(api.mkSmallInt(10)) };
    const threshold_body = [_]RuleClauseNode{
        .{ .kind = 0, .reserved = [_]u8{0} ** 3, .name = null, .terms = &threshold_triple, .term_count = 3, .reserved_tail = 0 },
        .{ .kind = 2, .reserved = [_]u8{0} ** 3, .name = api.intern(">"), .terms = &threshold_predicate, .term_count = 2, .reserved_tail = 0 },
    };
    const threshold_definition = [_]RuleDefinitionNode{
        .{ .name = threshold_rule, .params = &threshold_params, .param_count = 1, .reserved_params = 0, .body = &threshold_body, .body_count = threshold_body.len, .reserved_body = 0 },
    };
    const threshold_query = [_]RuleTermNode{ruleVar(0)};
    try std.testing.expectEqual(
        @as(c_int, 0),
        rule(store, &threshold_definition, threshold_definition.len, threshold_rule, &threshold_query, threshold_query.len, 0, &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 1), written);
    try std.testing.expectEqual(@intFromPtr(high), @as(usize, @intCast(values[0].identity)));

    const canonical_rule = api.intern("rust_relational_rule_store_ancestor");
    const logic_var = api.import("LogicVar") orelse return error.RustRelationalLogicVarMissing;
    const named = api.intern("named:");
    const x = api.send(logic_var, named, &.{api.intern("rust_relational_rule_store_x")}, 1);
    const y = api.send(logic_var, named, &.{api.intern("rust_relational_rule_store_y")}, 1);
    const z = api.send(logic_var, named, &.{api.intern("rust_relational_rule_store_z")}, 1);
    const canonical_query_clause = makeVector(api, &.{ x, parent, y });
    const canonical_query_body = makeVector(api, &.{canonical_query_clause});
    const canonical_query = api.send(
        store,
        api.intern("findAll:where:"),
        &.{ x, canonical_query_body },
        2,
    );
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(api.send(canonical_query, api.intern("size"), &.{}, 0)));
    try std.testing.expect(api.send(canonical_query, api.intern("at:"), &.{api.mkSmallInt(0)}, 1) == tom);
    try std.testing.expect(api.send(canonical_query, api.intern("at:"), &.{api.mkSmallInt(1)}, 1) == bob);
    try std.testing.expect(api.send(canonical_query, api.intern("at:"), &.{api.mkSmallInt(2)}, 1) == ann);
    const planner_high = api.intern("rust_relational_rule_planner_high");
    const planner_low = api.intern("rust_relational_rule_planner_low");
    const score_attribute = api.intern("rust_relational_rule_planner_score");
    const score_variable = api.send(logic_var, named, &.{api.intern("rust_relational_rule_planner_score_var")}, 1);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ planner_high, score_attribute, api.mkSmallInt(20) }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ planner_low, score_attribute, api.mkSmallInt(5) }, 3);
    const planner_predicate = makeVector(api, &.{ score_variable, api.intern("gt"), api.mkSmallInt(10) });
    const planner_triple = makeVector(api, &.{ x, score_attribute, score_variable });
    const planner_body = makeVector(api, &.{ planner_predicate, planner_triple });
    const planner_query = api.send(store, api.intern("findAll:where:"), &.{ x, planner_body }, 2);
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(planner_query, api.intern("size"), &.{}, 0)));
    try std.testing.expect(api.send(planner_query, api.intern("at:"), &.{api.mkSmallInt(0)}, 1) == planner_high);
    const rule_params = makeVector(api, &.{ x, y });
    const rule_base_clause = makeVector(api, &.{ x, parent, y });
    const rule_base_body = makeVector(api, &.{rule_base_clause});
    _ = api.send(
        store,
        api.intern("defineRule:params:body:"),
        &.{ canonical_rule, rule_params, rule_base_body },
        3,
    );
    const rule_recursive_clause = makeVector(api, &.{ x, parent, z });
    const rule_recursive_call = makeVector(api, &.{ api.intern("rule"), canonical_rule, z, y });
    const rule_recursive_body = makeVector(api, &.{ rule_recursive_clause, rule_recursive_call });
    _ = api.send(
        store,
        api.intern("defineRule:params:body:"),
        &.{ canonical_rule, rule_params, rule_recursive_body },
        3,
    );
    const store_rule_raw = api.dlsym(relation_handle, "birnam_relational_rust_project_rule_store_live") orelse
        return error.RustRelationalStoreRuleMissing;
    const store_rule: ProjectRuleStoreLive = @ptrCast(@alignCast(store_rule_raw));
    _ = api.send(store, api.intern("rebuildRules"), &empty_args, 0);
    try std.testing.expectEqual(
        @as(c_int, 0),
        store_rule(store, canonical_rule, &query_terms, query_terms.len, 1, &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 3), written);
    try std.testing.expectEqual(@intFromPtr(bob), @as(usize, @intCast(values[0].identity)));
    try std.testing.expectEqual(@intFromPtr(ann), @as(usize, @intCast(values[1].identity)));
    try std.testing.expectEqual(@intFromPtr(kim), @as(usize, @intCast(values[2].identity)));

    const tree_raw = api.dlsym(relation_handle, "birnam_relational_rust_project_rule_tree_live") orelse
        return error.RustRelationalRuleTreeMissing;
    const tree: ProjectRuleTreeLive = @ptrCast(@alignCast(tree_raw));
    const clean = api.intern("rust_relational_rule_clean");
    const blocked = api.intern("rust_relational_rule_blocked");
    const class = api.intern("rust_relational_rule_class");
    const alpha = api.intern("rust_relational_rule_alpha");
    const beta = api.intern("rust_relational_rule_beta");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ high, blocked, api.intern("yes") }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ high, class, alpha }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ clean, score, api.mkSmallInt(25) }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ clean, class, alpha }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ low, class, beta }, 3);
    const tree_triple_terms = [_]RuleTermNode{ ruleVar(0), ruleConst(score), ruleVar(1) };
    const tree_predicate_terms = [_]RuleTermNode{ ruleVar(1), ruleConst(api.mkSmallInt(10)) };
    const tree_negated_terms = [_]RuleTermNode{ ruleVar(0), ruleConst(blocked), ruleConst(api.intern("yes")) };
    const tree_negated_children = [_]RuleBodyNode{
        .{ .kind = 0, .reserved = [_]u8{0} ** 3, .name = null, .terms = &tree_negated_terms, .term_count = 3, .reserved_terms = 0, .children = null, .child_count = 0, .reserved_children = 0 },
    };
    const tree_alpha_terms = [_]RuleTermNode{ ruleVar(0), ruleConst(class), ruleConst(alpha) };
    const tree_beta_terms = [_]RuleTermNode{ ruleVar(0), ruleConst(class), ruleConst(beta) };
    const tree_alpha_children = [_]RuleBodyNode{
        .{ .kind = 0, .reserved = [_]u8{0} ** 3, .name = null, .terms = &tree_alpha_terms, .term_count = 3, .reserved_terms = 0, .children = null, .child_count = 0, .reserved_children = 0 },
    };
    const tree_beta_children = [_]RuleBodyNode{
        .{ .kind = 0, .reserved = [_]u8{0} ** 3, .name = null, .terms = &tree_beta_terms, .term_count = 3, .reserved_terms = 0, .children = null, .child_count = 0, .reserved_children = 0 },
    };
    const tree_branches = [_]RuleBodyNode{
        .{ .kind = 5, .reserved = [_]u8{0} ** 3, .name = null, .terms = null, .term_count = 0, .reserved_terms = 0, .children = &tree_alpha_children, .child_count = 1, .reserved_children = 0 },
        .{ .kind = 5, .reserved = [_]u8{0} ** 3, .name = null, .terms = null, .term_count = 0, .reserved_terms = 0, .children = &tree_beta_children, .child_count = 1, .reserved_children = 0 },
    };
    const tree_body = [_]RuleBodyNode{
        .{ .kind = 0, .reserved = [_]u8{0} ** 3, .name = null, .terms = &tree_triple_terms, .term_count = 3, .reserved_terms = 0, .children = null, .child_count = 0, .reserved_children = 0 },
        .{ .kind = 2, .reserved = [_]u8{0} ** 3, .name = api.intern(">"), .terms = &tree_predicate_terms, .term_count = 2, .reserved_terms = 0, .children = null, .child_count = 0, .reserved_children = 0 },
        .{ .kind = 3, .reserved = [_]u8{0} ** 3, .name = null, .terms = null, .term_count = 0, .reserved_terms = 0, .children = &tree_negated_children, .child_count = 1, .reserved_children = 0 },
        .{ .kind = 4, .reserved = [_]u8{0} ** 3, .name = null, .terms = null, .term_count = 0, .reserved_terms = 0, .children = &tree_branches, .child_count = 2, .reserved_children = 0 },
    };
    const tree_definition = [_]RuleTreeDefinitionNode{
        .{ .name = threshold_rule, .params = &threshold_params, .param_count = 1, .reserved_params = 0, .body = &tree_body, .body_count = tree_body.len, .reserved_body = 0 },
    };
    try std.testing.expectEqual(
        @as(c_int, 0),
        tree(store, &tree_definition, tree_definition.len, threshold_rule, &threshold_query, threshold_query.len, 0, &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 1), written);
    try std.testing.expectEqual(@intFromPtr(clean), @as(usize, @intCast(values[0].identity)));

    const nested_store_rule = api.intern("rust_relational_rule_store_nested");
    const nx = api.send(logic_var, named, &.{api.intern("rust_relational_rule_store_nested_x")}, 1);
    const nv = api.send(logic_var, named, &.{api.intern("rust_relational_rule_store_nested_v")}, 1);
    const nested_params = makeVector(api, &.{nx});
    const nested_score = makeVector(api, &.{ nx, score, nv });
    const nested_predicate = makeVector(api, &.{ nv, api.intern(">"), api.mkSmallInt(10) });
    const nested_not_inner = makeVector(api, &.{ nx, blocked, api.intern("yes") });
    const nested_not = makeVector(api, &.{ api.intern("not"), nested_not_inner });
    const nested_or_alpha = makeVector(api, &.{ nx, class, alpha });
    const nested_or_beta = makeVector(api, &.{ nx, class, beta });
    const nested_or = makeVector(api, &.{ api.intern("or"), nested_or_alpha, nested_or_beta });
    const nested_body = makeVector(api, &.{ nested_score, nested_predicate, nested_not, nested_or });
    _ = api.send(
        store,
        api.intern("defineRule:params:body:"),
        &.{ nested_store_rule, nested_params, nested_body },
        3,
    );
    _ = api.send(store, api.intern("rebuildRules"), &empty_args, 0);
    try std.testing.expectEqual(
        @as(c_int, 0),
        store_rule(store, nested_store_rule, &threshold_query, threshold_query.len, 0, &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 1), written);
    try std.testing.expectEqual(@intFromPtr(clean), @as(usize, @intCast(values[0].identity)));

    const even = api.intern("rust_relational_rule_even");
    const odd = api.intern("rust_relational_rule_odd");
    const is_zero = api.intern("rust_relational_rule_isZero");
    const yes = api.intern("rust_relational_rule_yes");
    const succ = api.intern("rust_relational_rule_succ");
    const n0 = api.intern("rust_relational_rule_n0");
    const n1 = api.intern("rust_relational_rule_n1");
    const n2 = api.intern("rust_relational_rule_n2");
    const n3 = api.intern("rust_relational_rule_n3");
    const n4 = api.intern("rust_relational_rule_n4");
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ n0, is_zero, yes }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ n0, succ, n1 }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ n1, succ, n2 }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ n2, succ, n3 }, 3);
    _ = api.send(store, api.intern("assert:attr:value:"), &.{ n3, succ, n4 }, 3);

    const one_param = [_]u32{0};
    const even_base_terms = [_]RuleTermNode{ ruleVar(0), ruleConst(is_zero), ruleConst(yes) };
    const even_base_body = [_]RuleClauseNode{
        .{ .kind = 0, .reserved = [_]u8{0} ** 3, .name = null, .terms = &even_base_terms, .term_count = 3, .reserved_tail = 0 },
    };
    const even_rec_triple = [_]RuleTermNode{ ruleVar(1), ruleConst(succ), ruleVar(0) };
    const even_rec_call = [_]RuleTermNode{ ruleVar(1) };
    const even_rec_body = [_]RuleClauseNode{
        .{ .kind = 0, .reserved = [_]u8{0} ** 3, .name = null, .terms = &even_rec_triple, .term_count = 3, .reserved_tail = 0 },
        .{ .kind = 1, .reserved = [_]u8{0} ** 3, .name = odd, .terms = &even_rec_call, .term_count = 1, .reserved_tail = 0 },
    };
    const odd_triple = [_]RuleTermNode{ ruleVar(1), ruleConst(succ), ruleVar(0) };
    const odd_call = [_]RuleTermNode{ ruleVar(1) };
    const odd_body = [_]RuleClauseNode{
        .{ .kind = 0, .reserved = [_]u8{0} ** 3, .name = null, .terms = &odd_triple, .term_count = 3, .reserved_tail = 0 },
        .{ .kind = 1, .reserved = [_]u8{0} ** 3, .name = even, .terms = &odd_call, .term_count = 1, .reserved_tail = 0 },
    };
    const mutual_definitions = [_]RuleDefinitionNode{
        .{ .name = even, .params = &one_param, .param_count = 1, .reserved_params = 0, .body = &even_base_body, .body_count = 1, .reserved_body = 0 },
        .{ .name = even, .params = &one_param, .param_count = 1, .reserved_params = 0, .body = &even_rec_body, .body_count = 2, .reserved_body = 0 },
        .{ .name = odd, .params = &one_param, .param_count = 1, .reserved_params = 0, .body = &odd_body, .body_count = 2, .reserved_body = 0 },
    };
    const even_query = [_]RuleTermNode{ruleVar(0)};
    try std.testing.expectEqual(
        @as(c_int, 0),
        rule(store, &mutual_definitions, mutual_definitions.len, even, &even_query, even_query.len, 0, &values, values.len, &written),
    );
    try std.testing.expectEqual(@as(usize, 3), written);
    try std.testing.expectEqual(@intFromPtr(n0), @as(usize, @intCast(values[0].identity)));
    try std.testing.expectEqual(@intFromPtr(n2), @as(usize, @intCast(values[1].identity)));
    try std.testing.expectEqual(@intFromPtr(n4), @as(usize, @intCast(values[2].identity)));
    try std.testing.expectEqual(
        @as(c_int, -5),
        rule(store, &mutual_definitions, mutual_definitions.len, even, &even_query, even_query.len, 0, null, 0, &written),
    );
}
