//! Cross-language smoke for the first Rust Store storage core.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");
const errmod = @import("error.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;

fn send(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8, args: []const Oop) Oop {
    return api.send(receiver, api.intern(selector), args.ptr, args.len);
}

fn count(api: *idapi.IdApi, store: Oop, e: Oop, attr: Oop) usize {
    return @intCast(api.smallIntValue(send(api, store, "count:attr:", &.{ e, attr })));
}

fn answer42(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = args;
    _ = n;
    return @ptrFromInt(42);
}

fn answer7(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = args;
    _ = n;
    return @ptrFromInt(7);
}

fn answer99(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = args;
    _ = n;
    return @ptrFromInt(99);
}

var triple_target_entity: Oop = null;
var triple_target_one: Oop = null;
var triple_target_many: Oop = null;
var triple_one_hits: usize = 0;
var triple_one_value: Oop = null;
var triple_many_hits: usize = 0;
var image_target_entity: Oop = null;
var image_capture_count: usize = 0;
var image_last_value: Oop = null;
var image_last_tx: i64 = -1;
var image_last_op: i64 = -1;

fn collectTriples(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    if (n == 3 and args[0] == triple_target_entity) {
        if (args[1] == triple_target_one) {
            triple_one_hits += 1;
            triple_one_value = args[2];
        } else if (args[1] == triple_target_many) {
            triple_many_hits += 1;
        }
    }
    return self;
}

fn resetTripleCapture() void {
    triple_one_hits = 0;
    triple_one_value = null;
    triple_many_hits = 0;
}

fn collectImageFact(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    if (n == 5 and args[0] == image_target_entity) {
        image_capture_count += 1;
        image_last_value = args[2];
        image_last_tx = image_api.?.smallIntValue(args[3]);
        image_last_op = image_api.?.smallIntValue(args[4]);
    }
    return self;
}
var image_api: ?*idapi.IdApi = null;

var mop_api: ?*idapi.IdApi = null;
var mop_authorized = true;
var mop_authorizer_calls: usize = 0;
var mop_observer_calls: usize = 0;

fn authorizeMop() callconv(.c) Oop {
    mop_authorizer_calls += 1;
    return if (mop_authorized) mop_api.?.trueObj else mop_api.?.falseObj;
}

fn observeMop(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    mop_observer_calls += 1;
    return self;
}

test "Rust Store core owns raw/flip, schema-aware assert, lookup, and basis" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_STORE_DYLIB") orelse
        return error.MissingRustStoreDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustStoreDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustStoreModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const store = api.import("Store") orelse return error.RustStoreNotExported;
    try std.testing.expect(api.import("StoreVt") != null);
    try std.testing.expect(api.import("CellVt") != null);
    try std.testing.expect(api.import("Globals") != null);

    const e = api.intern("rust_store_core_entity");
    const attr = api.intern("rust_store_core_attr");
    const first = api.intern("first");
    const second = api.intern("second");
    try std.testing.expect(send(api, store, "currentValue:attr:", &.{ e, attr }) == api.nilObj);
    _ = send(api, store, "rawAssert:attr:value:", &.{ e, attr, first });
    try std.testing.expect(send(api, store, "currentValue:attr:", &.{ e, attr }) == first);

    // The seeded schema has already flipped the store to policy-aware mode;
    // default :one replacement keeps one live value and advances the basis.
    _ = send(api, store, "assert:attr:value:", &.{ e, attr, second });
    try std.testing.expect(send(api, store, "currentValue:attr:", &.{ e, attr }) == second);
    try std.testing.expectEqual(@as(usize, 1), count(api, store, e, attr));
    try std.testing.expect(send(api, store, "hasValue:attr:", &.{ e, attr }) == api.trueObj);

    const tags = api.intern("rust_store_core_tags");
    const many = api.intern("many");
    const alpha = api.intern("alpha");
    const beta = api.intern("beta");
    _ = send(api, store, "assert:attr:value:", &.{ tags, api.intern("cardinality"), many });
    _ = send(api, store, "assert:attr:value:", &.{ e, tags, alpha });
    _ = send(api, store, "assert:attr:value:", &.{ e, tags, beta });
    try std.testing.expectEqual(@as(usize, 2), count(api, store, e, tags));

    const basis = send(api, store, "basis", &.{});
    try std.testing.expect(api.smallIntValue(basis) > 0);
    const fresh = send(api, store, "new", &.{});
    try std.testing.expect(fresh != null and fresh != store);
    try std.testing.expect(send(api, fresh, "currentValue:attr:", &.{ api.intern("cardinality"), api.intern("cardinality") }) == api.intern("one"));
}

test "Rust Store history and retraction preserve the opt-in timeline contract" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_STORE_DYLIB") orelse
        return error.MissingRustStoreDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustStoreDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustStoreModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const store = api.import("Store") orelse return error.RustStoreNotExported;
    const e = api.intern("rust_store_history_entity");
    const one_attr = api.intern("rust_store_history_one");
    const first = api.intern("first");
    const second = api.intern("second");
    const third = api.intern("third");

    _ = send(api, store, "assert:attr:value:", &.{ e, one_attr, first });
    _ = send(api, store, "assert:attr:value:", &.{ e, one_attr, second });
    try std.testing.expect(send(api, store, "currentValue:attr:", &.{ e, one_attr }) == second);
    try std.testing.expectEqual(@as(usize, 1), count(api, store, e, one_attr));
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(send(api, store, "history:attr:", &.{ e, one_attr })));

    const history_attr = api.intern("history");
    _ = send(api, store, "assert:attr:value:", &.{ one_attr, history_attr, api.trueObj });
    _ = send(api, store, "assert:attr:value:", &.{ e, one_attr, third });
    _ = send(api, store, "assert:attr:value:", &.{ e, one_attr, second });
    try std.testing.expect(send(api, store, "currentValue:attr:", &.{ e, one_attr }) == second);
    try std.testing.expectEqual(@as(usize, 1), count(api, store, e, one_attr));
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(send(api, store, "history:attr:", &.{ e, one_attr })));

    const many_attr = api.intern("rust_store_history_many");
    const many = api.intern("many");
    const alpha = api.intern("alpha");
    const beta = api.intern("beta");
    _ = send(api, store, "assert:attr:value:", &.{ many_attr, api.intern("cardinality"), many });
    _ = send(api, store, "assert:attr:value:", &.{ many_attr, history_attr, api.trueObj });
    _ = send(api, store, "assert:attr:value:", &.{ e, many_attr, alpha });
    _ = send(api, store, "assert:attr:value:", &.{ e, many_attr, beta });
    _ = send(api, store, "retract:attr:value:", &.{ e, many_attr, alpha });
    try std.testing.expectEqual(@as(usize, 1), count(api, store, e, many_attr));
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(send(api, store, "history:attr:", &.{ e, many_attr })));
    try std.testing.expect(send(api, store, "hasValue:attr:", &.{ e, many_attr }) == api.trueObj);
}

test "Rust Store attr reports preserve live first-appearance order" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_STORE_DYLIB") orelse
        return error.MissingRustStoreDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustStoreDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustStoreModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const store = api.import("Store") orelse return error.RustStoreNotExported;
    const entity = api.intern("rust_store_attr_report_entity");
    const first_attr = api.intern("rust_store_attr_report_first");
    const second_attr = api.intern("rust_store_attr_report_second");
    _ = send(api, store, "assert:attr:value:", &.{ entity, first_attr, api.intern("first") });
    _ = send(api, store, "assert:attr:value:", &.{ entity, second_attr, api.intern("second") });
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(send(api, store, "attrCount:", &.{entity})));
    try std.testing.expect(send(api, store, "attrAt:index:", &.{ entity, api.mkSmallInt(0) }) == first_attr);
    try std.testing.expect(send(api, store, "attrAt:index:", &.{ entity, api.mkSmallInt(1) }) == second_attr);

    _ = send(api, store, "retract:attr:value:", &.{ entity, first_attr, api.intern("first") });
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(send(api, store, "attrCount:", &.{entity})));
    try std.testing.expect(send(api, store, "attrAt:index:", &.{ entity, api.mkSmallInt(0) }) == second_attr);
    try std.testing.expect(send(api, store, "attrAt:index:", &.{ entity, api.mkSmallInt(1) }) == null);
}

test "Rust Store image bridge preserves raw facts, transactions, and ops" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_STORE_DYLIB") orelse
        return error.MissingRustStoreDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustStoreDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustStoreModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const store = api.import("Store") orelse return error.RustStoreNotExported;
    const entity = api.intern("rust_store_image_entity");
    const attribute = api.intern("rust_store_image_attribute");
    const value = api.intern("rust_store_image_value");
    image_api = api;
    image_target_entity = entity;
    image_capture_count = 0;
    image_last_value = null;
    image_last_tx = -1;
    image_last_op = -1;
    const collector_vt = api.proto(api.object_vt);
    api.method(collector_vt, api.intern("e:a:v:tx:op:"), &collectImageFact);
    const collector = api.alloc(collector_vt, 0);

    _ = send(api, store, "assert:attr:value:", &.{ entity, attribute, value });
    _ = send(api, store, "imageFactsInto:", &.{collector});
    try std.testing.expectEqual(@as(usize, 1), image_capture_count);
    try std.testing.expect(image_last_value == value);
    try std.testing.expectEqual(@as(i64, 0), image_last_op);
    const saved_tx = image_last_tx;

    _ = send(api, store, "imageLoadBegin", &.{});
    try std.testing.expect(send(api, store, "currentValue:attr:", &.{ entity, attribute }) == api.nilObj);
    _ = send(api, store, "imageAddFact:attr:value:tx:op:", &.{
        entity,
        attribute,
        value,
        api.mkSmallInt(saved_tx),
        api.mkSmallInt(0),
    });
    _ = send(api, store, "imageLoadEnd:raw:", &.{ api.mkSmallInt(saved_tx + 1), api.falseObj });
    try std.testing.expect(send(api, store, "currentValue:attr:", &.{ entity, attribute }) == value);
    try std.testing.expectEqual(saved_tx + 1, api.smallIntValue(send(api, store, "basis", &.{})));

    _ = send(api, store, "imageAddFact:attr:value:tx:op:", &.{
        entity,
        attribute,
        value,
        api.mkSmallInt(saved_tx + 1),
        api.mkSmallInt(1),
    });
    try std.testing.expect(send(api, store, "currentValue:attr:", &.{ entity, attribute }) == api.nilObj);
}

test "Rust Store Cells are traced handles over one store entity/attribute pair" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_STORE_DYLIB") orelse
        return error.MissingRustStoreDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustStoreDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustStoreModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const store = api.import("Store") orelse return error.RustStoreNotExported;
    const entity = api.intern("rust_store_cell_entity");
    const attribute = api.intern("rust_store_cell_attribute");
    const first = api.intern("cell_first");
    const second = api.intern("cell_second");
    const cell = send(api, store, "cellFor:attr:", &.{ entity, attribute });
    try std.testing.expect(cell != null);
    try std.testing.expect(send(api, cell, "value", &.{}) == api.nilObj);
    _ = send(api, cell, "set:", &.{first});
    try std.testing.expect(send(api, cell, "value", &.{}) == first);
    _ = send(api, cell, "set:", &.{second});
    try std.testing.expect(send(api, cell, "value", &.{}) == second);
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(send(api, cell, "history", &.{})));

    const second_cell = send(api, store, "cellFor:attr:", &.{ entity, attribute });
    try std.testing.expect(send(api, second_cell, "value", &.{}) == second);
}

test "Rust Store MOP preserves authorization, ordering, mirror, and raw invalidation" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_STORE_DYLIB") orelse
        return error.MissingRustStoreDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustStoreDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustStoreModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const store = api.import("Store") orelse return error.RustStoreNotExported;
    mop_api = api;
    const vt = api.proto(api.object_vt);
    const owner = api.alloc(vt, 0);
    api.setOwner(vt, owner);
    const selector = api.intern("rust_store_mop_selector");
    const method42: Oop = @ptrCast(@constCast(&answer42));
    const method7: Oop = @ptrCast(@constCast(&answer7));
    const observer_vt = api.proto(api.object_vt);
    api.method(observer_vt, api.intern("managedMethodChanged:on:"), &observeMop);
    const observer = api.alloc(observer_vt, 0);
    _ = send(api, store, "setMethodMutationObserver:", &.{observer});
    const authorizer_address: i64 = @intCast(@intFromPtr(&authorizeMop));
    _ = send(api, store, "setMethodMutationAuthorizer:", &.{api.mkSmallInt(authorizer_address)});

    mop_authorized = false;
    mop_authorizer_calls = 0;
    mop_observer_calls = 0;
    try std.testing.expect(send(api, store, "addMethod:on:with:", &.{ selector, vt, method42 }) == api.falseObj);
    try std.testing.expectEqual(@as(usize, 1), mop_authorizer_calls);
    try std.testing.expectEqual(@as(usize, 0), mop_observer_calls);
    try std.testing.expect(send(api, store, "rawCurrentValue:attr:", &.{ vt, selector }) == null);

    mop_authorized = true;
    _ = send(api, store, "addMethod:on:with:", &.{ selector, vt, method42 });
    try std.testing.expectEqual(@as(usize, 1), mop_observer_calls);
    try std.testing.expect(send(api, store, "rawCurrentValue:attr:", &.{ vt, selector }) == method42);
    const instance = api.alloc(vt, 0);
    try std.testing.expect(@intFromPtr(send(api, instance, "rust_store_mop_selector", &.{})) == 42);

    _ = send(api, store, "addMethod:on:with:", &.{ selector, vt, method7 });
    try std.testing.expectEqual(@as(usize, 2), mop_observer_calls);
    try std.testing.expect(send(api, store, "rawCurrentValue:attr:", &.{ vt, selector }) == method7);
    try std.testing.expect(@intFromPtr(send(api, instance, "rust_store_mop_selector", &.{})) == 7);

    mop_authorized = false;
    try std.testing.expect(send(api, store, "removeMethod:on:", &.{ selector, vt }) == api.falseObj);
    try std.testing.expectEqual(@as(usize, 2), mop_observer_calls);
    try std.testing.expect(send(api, store, "rawCurrentValue:attr:", &.{ vt, selector }) == method7);

    mop_authorized = true;
    _ = send(api, store, "removeMethod:on:", &.{ selector, vt });
    try std.testing.expectEqual(@as(usize, 3), mop_observer_calls);
    try std.testing.expect(send(api, store, "rawCurrentValue:attr:", &.{ vt, selector }) == null);

    // A raw overwrite is visible to the floor but cannot leave a stale mirror.
    api.method(vt, selector, &answer42);
    _ = send(api, store, "backfillMirror", &.{});
    try std.testing.expect(send(api, store, "rawCurrentValue:attr:", &.{ vt, selector }) != null);
    api.method(vt, selector, &answer7);
    try std.testing.expect(send(api, store, "rawCurrentValue:attr:", &.{ vt, selector }) == null);
}

test "Rust Store Globals preserve absent versus bound-nil values" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_STORE_DYLIB") orelse
        return error.MissingRustStoreDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustStoreDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustStoreModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const globals = api.import("Globals") orelse return error.RustGlobalsNotExported;
    const missing = api.intern("rust_global_missing");
    const bound = api.intern("rust_global_bound");
    const value = api.intern("global_value");
    try std.testing.expect(send(api, globals, "at:", &.{missing}) == api.nilObj);
    try std.testing.expect(send(api, globals, "includes:", &.{missing}) == api.falseObj);

    const block_vt = api.proto(api.object_vt);
    api.method(block_vt, api.intern("value"), &answer99);
    const fallback = api.alloc(block_vt, 0);
    try std.testing.expect(@intFromPtr(send(api, globals, "at:ifAbsent:", &.{ missing, fallback })) == 99);

    try std.testing.expect(send(api, globals, "at:put:", &.{ bound, value }) == value);
    try std.testing.expect(send(api, globals, "at:", &.{bound}) == value);
    try std.testing.expect(send(api, globals, "includes:", &.{bound}) == api.trueObj);
    try std.testing.expect(send(api, globals, "at:ifAbsent:", &.{ bound, fallback }) == value);

    _ = send(api, globals, "at:put:", &.{ bound, api.nilObj });
    try std.testing.expect(send(api, globals, "at:", &.{bound}) == api.nilObj);
    try std.testing.expect(send(api, globals, "includes:", &.{bound}) == api.trueObj);
    try std.testing.expect(send(api, globals, "at:ifAbsent:", &.{ bound, fallback }) == api.nilObj);
}

test "Rust Store preserves honest as-of reads and live triple windows" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_STORE_DYLIB") orelse
        return error.MissingRustStoreDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustStoreDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustStoreModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);
    errmod.bringUp(api);

    const store = api.import("Store") orelse return error.RustStoreNotExported;
    const entity = api.intern("rust_store_window_entity");
    const one_attr = api.intern("rust_store_window_one");
    const many_attr = api.intern("rust_store_window_many");
    const first = api.intern("window_first");
    const second = api.intern("window_second");
    const alpha = api.intern("window_alpha");
    const beta = api.intern("window_beta");

    triple_target_entity = entity;
    triple_target_one = one_attr;
    triple_target_many = many_attr;
    const block_vt = api.proto(api.object_vt);
    api.method(block_vt, api.intern("value:value:value:"), &collectTriples);
    const block = api.alloc(block_vt, 0);

    _ = send(api, store, "assert:attr:value:", &.{ entity, one_attr, first });
    const after_first = send(api, store, "basis", &.{});
    _ = send(api, store, "assert:attr:value:", &.{ entity, one_attr, second });
    const after_second = send(api, store, "basis", &.{});
    try std.testing.expect(send(api, store, "valueOf:attr:asOf:", &.{ entity, one_attr, after_second }) == second);
    const lost = send(api, store, "valueOf:attr:asOf:", &.{ entity, one_attr, after_first });
    try std.testing.expect(send(api, lost, "isError", &.{}) == api.trueObj);
    try std.testing.expect(send(api, lost, "kind", &.{}) == api.intern("historyNotRetained"));

    _ = send(api, store, "assert:attr:value:", &.{ many_attr, api.intern("cardinality"), api.intern("many") });
    _ = send(api, store, "assert:attr:value:", &.{ many_attr, api.intern("history"), api.trueObj });
    _ = send(api, store, "assert:attr:value:", &.{ entity, many_attr, alpha });
    const after_alpha = send(api, store, "basis", &.{});
    _ = send(api, store, "assert:attr:value:", &.{ entity, many_attr, beta });
    const after_beta = send(api, store, "basis", &.{});
    _ = send(api, store, "retract:attr:value:", &.{ entity, many_attr, alpha });

    resetTripleCapture();
    _ = send(api, store, "liveTriplesDo:", &.{block});
    try std.testing.expectEqual(@as(usize, 1), triple_one_hits);
    try std.testing.expect(triple_one_value == second);
    try std.testing.expectEqual(@as(usize, 1), triple_many_hits);

    resetTripleCapture();
    _ = send(api, store, "liveTriplesAsOf:do:", &.{ after_alpha, block });
    try std.testing.expectEqual(@as(usize, 1), triple_one_hits);
    try std.testing.expect(triple_one_value == second);
    try std.testing.expectEqual(@as(usize, 1), triple_many_hits);

    resetTripleCapture();
    _ = send(api, store, "liveTriplesSince:do:", &.{ after_alpha, block });
    try std.testing.expectEqual(@as(usize, 0), triple_one_hits);
    try std.testing.expectEqual(@as(usize, 1), triple_many_hits);

    resetTripleCapture();
    _ = send(api, store, "liveTriplesBetween:and:do:", &.{ after_alpha, after_beta, block });
    try std.testing.expectEqual(@as(usize, 0), triple_one_hits);
    try std.testing.expectEqual(@as(usize, 1), triple_many_hits);
}
