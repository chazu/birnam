//! Cross-language smoke for the first Rust Store storage core.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

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
