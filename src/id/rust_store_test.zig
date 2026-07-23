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
