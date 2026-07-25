//! Focused live proof for the Rust Image dynamic shell.
//!
//! This slice checks the frozen family/layout and selector surface before the
//! v4 save/load orchestration is moved across. The shell's mutating selectors
//! must remain fail-closed until those later authorities are installed.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");
const prelude = @import("prelude.zig");
const store = @import("store.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;
const Identity = *const fn () callconv(.c) [*:0]const u8;

const RTLD_NOW: c_int = 0x2;
const RTLD_LOCAL: c_int = 0x4;

extern fn dlopen(path: [*:0]const u8, mode: c_int) ?*anyopaque;
extern fn dlsym(handle: ?*anyopaque, name: [*:0]const u8) ?*anyopaque;

fn str(api: *idapi.IdApi, bytes: []const u8) Oop {
    return api.mkString(bytes.ptr, bytes.len);
}

fn send(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8, args: []const Oop) Oop {
    return api.send(receiver, api.intern(selector), args.ptr, args.len);
}

fn loadImageModule(api: *idapi.IdApi) void {
    const path = std.posix.getenv("BIRNAM_RUST_IMAGE_DYLIB") orelse
        std.debug.panic("BIRNAM_RUST_IMAGE_DYLIB must select the Rust Image shell", .{});
    const handle = dlopen(path.ptr, RTLD_NOW | RTLD_LOCAL) orelse
        std.debug.panic("could not load Rust Image shell", .{});
    const raw_identity = dlsym(handle, "birnam_image_rust_identity") orelse
        std.debug.panic("Rust Image shell has no identity export", .{});
    const identity: Identity = @ptrCast(@alignCast(raw_identity));
    tryIdentity(identity);
    const raw_init = dlsym(handle, "module_init") orelse
        std.debug.panic("Rust Image shell has no module_init", .{});
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);
}

fn tryIdentity(identity: Identity) void {
    if (!std.mem.eql(u8, std.mem.span(identity()), "rust-image-r6-shell")) {
        std.debug.panic("Rust Image shell identity mismatch", .{});
    }
}

test "Rust Image shell preserves report layout and selector surface" {
    const api = floor.api();
    prelude.bringUp(api);
    store.bringUp(api);
    loadImageModule(api);

    const report_vt = api.import("ImageSaveReportVt") orelse return error.ImageReportVtMissing;
    const report = api.import("ImageSaveReport") orelse return error.ImageReportMissing;
    try std.testing.expectEqual(report_vt, api.vtableOf(report));
    try std.testing.expectEqual(@as(usize, 0), api.objectPayloadSize(report));
    try std.testing.expectEqualStrings("ImageSaveReportVt", std.mem.span(api.exportedNameOf(report_vt).?));
    try std.testing.expectEqualStrings("ImageSaveReport", std.mem.span(api.exportedNameOf(report).?));

    const status = send(api, report, "status", &.{});
    try std.testing.expectEqualStrings("never", std.mem.span(api.symbolName(status)));
    for ([_][*:0]const u8{
        "savedFacts",
        "recoveredFacts",
        "rejectedFacts",
        "recoveredSlots",
        "rejectedSlots",
        "codeUnits",
    }) |selector| {
        const value = api.send(report, api.intern(selector), &.{}, 0);
        try std.testing.expectEqual(@as(i64, 0), api.smallIntValue(value));
    }
    const detail = send(api, report, "detail", &.{});
    var detail_len: usize = 0;
    _ = api.stringBytes(detail, &detail_len);
    try std.testing.expectEqual(@as(usize, 0), detail_len);

    const system = api.import("System") orelse return error.SystemMissing;
    const fixture_vt = api.proto(api.object_vt);
    api.setLayout(fixture_vt, null, 0, idapi.slot_raw);
    _ = api.exportObj("RustImageFixtureVt", fixture_vt);
    const fixture = api.alloc(fixture_vt, 0);
    const fixture_entity = api.intern("rust_image_fixture_entity");
    const fixture_attribute = api.intern("rust_image_fixture_value");
    _ = send(api, api.import("Store").?, "assert:attr:value:", &.{ fixture_entity, fixture_attribute, fixture });
    defer _ = send(api, api.import("Store").?, "retract:attr:value:", &.{ fixture_entity, fixture_attribute, fixture });
    const nested_vt = api.proto(api.object_vt);
    const nested_kinds = [_]u8{idapi.slot_oop};
    api.setLayout(nested_vt, &nested_kinds, nested_kinds.len, idapi.slot_raw);
    _ = api.exportObj("RustImageNestedFixtureVt", nested_vt);
    const nested = api.alloc(nested_vt, @sizeOf(usize));
    const nested_words: [*]usize = @ptrCast(@alignCast(nested));
    nested_words[0] = @intFromPtr(fixture.?);
    const nested_entity = api.intern("rust_image_nested_fixture_entity");
    const nested_attribute = api.intern("rust_image_nested_fixture_value");
    _ = send(api, api.import("Store").?, "assert:attr:value:", &.{ nested_entity, nested_attribute, nested });
    defer _ = send(api, api.import("Store").?, "retract:attr:value:", &.{ nested_entity, nested_attribute, nested });

    const cycle_a = api.alloc(nested_vt, @sizeOf(usize));
    const cycle_b = api.alloc(nested_vt, @sizeOf(usize));
    const cycle_a_words: [*]usize = @ptrCast(@alignCast(cycle_a));
    const cycle_b_words: [*]usize = @ptrCast(@alignCast(cycle_b));
    cycle_a_words[0] = @intFromPtr(cycle_b.?);
    cycle_b_words[0] = @intFromPtr(cycle_a.?);
    const cycle_entity = api.intern("rust_image_cycle_entity");
    const cycle_attribute = api.intern("rust_image_cycle_value");
    _ = send(api, api.import("Store").?, "assert:attr:value:", &.{ cycle_entity, cycle_attribute, cycle_a });
    defer _ = send(api, api.import("Store").?, "retract:attr:value:", &.{ cycle_entity, cycle_attribute, cycle_a });

    const unsupported_vt = api.proto(api.object_vt);
    api.setLayout(unsupported_vt, null, 0, idapi.slot_raw);
    const unsupported = api.alloc(unsupported_vt, 0);
    const rejected_parent = api.alloc(nested_vt, @sizeOf(usize));
    const rejected_parent_words: [*]usize = @ptrCast(@alignCast(rejected_parent));
    rejected_parent_words[0] = @intFromPtr(unsupported.?);
    const rejected_entity = api.intern("rust_image_rejected_entity");
    const rejected_attribute = api.intern("rust_image_rejected_value");
    _ = send(api, api.import("Store").?, "assert:attr:value:", &.{ rejected_entity, rejected_attribute, rejected_parent });
    defer _ = send(api, api.import("Store").?, "retract:attr:value:", &.{ rejected_entity, rejected_attribute, rejected_parent });

    const identity = send(api, system, "imageCompositionIdentity", &.{});
    var identity_len: usize = 0;
    const identity_bytes = api.stringBytes(identity, &identity_len);
    try std.testing.expectEqualStrings("rust-image-r6-shell", identity_bytes[0..identity_len]);
    try std.testing.expect(send(api, system, "imageSaveReport", &.{}) == report);
    try std.testing.expect(send(api, system, "saveImage:", &.{api.nilObj}) == api.nilObj);
    try std.testing.expectEqualStrings("rejected", std.mem.span(api.symbolName(send(api, report, "status", &.{}))));
    detail_len = 0;
    const invalid_detail = api.stringBytes(send(api, report, "detail", &.{}), &detail_len);
    try std.testing.expectEqualStrings("saveImage: expects one String path", invalid_detail[0..detail_len]);
    try std.testing.expect(send(api, system, "saveImage:", &.{str(api, "shell.image")}) == api.nilObj);
    try std.testing.expectEqualStrings("rejected", std.mem.span(api.symbolName(send(api, report, "status", &.{}))));
    detail_len = 0;
    const shell_detail = api.stringBytes(send(api, report, "detail", &.{}), &detail_len);
    const rejected_facts = api.smallIntValue(send(api, report, "rejectedFacts", &.{}));
    const saved_facts = api.smallIntValue(send(api, report, "savedFacts", &.{}));
    const rejected_slots = api.smallIntValue(send(api, report, "rejectedSlots", &.{}));
    try std.testing.expect(saved_facts + rejected_facts > 0);
    try std.testing.expect(rejected_slots > 0);
    if (rejected_slots > 0) {
        try std.testing.expectEqualStrings("Rust Image heap preflight rejected unsupported layout", shell_detail[0..detail_len]);
    } else if (rejected_facts == 0) {
        try std.testing.expectEqualStrings("Rust Image writer/publisher orchestration is not installed", shell_detail[0..detail_len]);
    } else {
        try std.testing.expectEqualStrings("Rust Image fact preflight rejected unsupported values", shell_detail[0..detail_len]);
    }
    try std.testing.expect(send(api, system, "loadImage:", &.{str(api, "shell.image")}) == api.nilObj);
    try std.testing.expect(send(api, system, "loadImage:rebuild:", &.{ str(api, "shell.image"), api.trueObj }) == api.nilObj);
    try std.testing.expect(send(api, system, "bakeExe:", &.{str(api, "shell-exe")}) == api.nilObj);

    const collector_vt = api.import("ImageCollectorVt") orelse return error.ImageCollectorVtMissing;
    const collector = api.alloc(collector_vt, 0);
    try std.testing.expect(send(api, collector, "e:a:v:tx:op:", &.{ api.nilObj, api.nilObj, api.nilObj, api.mkSmallInt(0), api.mkSmallInt(0) }) == api.nilObj);
}
