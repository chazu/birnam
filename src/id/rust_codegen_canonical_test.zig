//! Exercise the gated canonical Rust Codegen/Reflect publication.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const collection = @import("collection.zig");
const lang = @import("lang.zig");
const idapi = @import("idapi.zig");

const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;

fn load(api: *idapi.IdApi, variable: []const u8) !void {
    const path = std.posix.getenv(variable) orelse return error.MissingDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.DlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.ModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);
}

fn setup() !*idapi.IdApi {
    const api = floor.api();
    collection.bringUp(api);
    try load(api, "BIRNAM_RUST_PEG_DYLIB");
    lang.bringUp(api);
    try load(api, "BIRNAM_RUST_TCC_DYLIB");
    try load(api, "BIRNAM_RUST_STORE_DYLIB");
    try load(api, "BIRNAM_RUST_CODEGEN_DYLIB");
    return api;
}

const AuthorityManifest = struct {
    schema_version: u32,
    codegen_selectors: []const []const u8,
    reflect_selectors: []const []const u8,
    exported_symbols: []const []const u8,
};

fn directSelectorCount(api: *idapi.IdApi, vt: idapi.Oop) usize {
    var count: usize = 0;
    var index: usize = 0;
    while (api.vtableSelectorAt(vt, index)) |selector| : (index += 1) {
        _ = selector;
        count += 1;
    }
    return count;
}

fn hasDirectSelector(api: *idapi.IdApi, vt: idapi.Oop, expected: []const u8) bool {
    var index: usize = 0;
    while (api.vtableSelectorAt(vt, index)) |selector| : (index += 1) {
        if (std.mem.eql(u8, std.mem.span(api.symbolName(selector)), expected)) return true;
    }
    return false;
}

test "Rust Codegen publishes the frozen canonical families when explicitly gated" {
    const api = try setup();
    const parsed = try std.json.parseFromSlice(
        AuthorityManifest,
        std.testing.allocator,
        @embedFile("fixtures/codegen-authority-v1.json"),
        .{},
    );
    defer parsed.deinit();

    const codegen_vt = api.import("CodegenVt") orelse return error.MissingCodegenVt;
    const codegen = api.import("Codegen") orelse return error.MissingCodegen;
    const reflect_vt = api.import("ReflectVt") orelse return error.MissingReflectVt;
    const reflect = api.import("Reflect") orelse return error.MissingReflect;
    const collector_vt = api.import("ReflectFactCollectorVt") orelse return error.MissingCollectorVt;
    try std.testing.expectEqual(parsed.value.codegen_selectors.len, directSelectorCount(api, codegen_vt));
    try std.testing.expectEqual(parsed.value.reflect_selectors.len, directSelectorCount(api, reflect_vt));
    for (parsed.value.codegen_selectors) |selector| try std.testing.expect(hasDirectSelector(api, codegen_vt, selector));
    for (parsed.value.reflect_selectors) |selector| try std.testing.expect(hasDirectSelector(api, reflect_vt, selector));
    try std.testing.expectEqual(@as(usize, 1), directSelectorCount(api, collector_vt));
    try std.testing.expect(hasDirectSelector(api, collector_vt, "value:value:value:"));
    try std.testing.expectEqual(@as(u8, idapi.slot_raw), api.slotKind(collector_vt, 0));

    const value = api.send(codegen, api.intern("eval:"), &.{api.mkString("7", 1)}, 1);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(value));
    const name = api.send(reflect, api.intern("nameOf:"), &.{api.intern("canonicalProbe")}, 1);
    var length: usize = 0;
    const bytes = api.stringBytes(name, &length);
    try std.testing.expectEqualStrings("canonicalProbe", bytes[0..length]);
}
