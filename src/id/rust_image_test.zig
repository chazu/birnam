//! Focused live proof for the Rust Image dynamic shell.
//!
//! This slice checks the frozen family/layout and selector surface before the
//! v4 save/load orchestration is moved across. The shell's mutating selectors
//! must remain fail-closed until those later authorities are installed.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");
const artifact_inspector = @import("artifact_inspector.zig");
const collection = @import("collection.zig");
const codegen = @import("codegen.zig");
const err_mod = @import("error.zig");
const lang = @import("lang.zig");
const peg = @import("peg.zig");
const prelude = @import("prelude.zig");
const relational = @import("relational.zig");
const store = @import("store.zig");
const tcc = @import("tcc.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;
const Identity = *const fn () callconv(.c) [*:0]const u8;

const RTLD_NOW: c_int = 0x2;
const RTLD_LOCAL: c_int = 0x4;

const MODULES_SRC = @embedFile("modules.birnam");
const STDLIB_SRC = @embedFile("stdlib.birnam");
const COMPILED_UNIT_SRC = @embedFile("compiled-unit.birnam");
const AUTHORITY_FIXTURE_SRC = "[Module named: #rustImageAuthority requires: #[]]; let RustImageAuthorityValue := 17";

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

fn setupWithZigCodegen() *idapi.IdApi {
    const api = floor.api();
    artifact_inspector.bringUp(api);
    store.bringUp(api);
    peg.bringUp(api);
    lang.bringUp(api);
    tcc.bringUp(api);
    codegen.bringUp(api);
    prelude.bringUp(api);
    collection.bringUp(api);
    err_mod.bringUp(api);
    relational.bringUp(api);
    loadImageModule(api);
    return api;
}

fn initializeAuthorityGeneration(api: *idapi.IdApi) !void {
    const codegen_object = api.import("Codegen") orelse return error.CodegenMissing;
    const sources = [_][]const u8{ MODULES_SRC, STDLIB_SRC, COMPILED_UNIT_SRC, AUTHORITY_FIXTURE_SRC };
    const names = [_][*:0]const u8{ "modules", "stdlib", "compiledUnit", "rustImageAuthority" };
    for (sources, names) |source, name| {
        const source_object = api.mkString(source.ptr, source.len);
        const staged = api.send(
            codegen_object,
            api.intern("recordModuleSource:named:"),
            &.{ source_object, api.intern(name) },
            2,
        );
        try std.testing.expect(staged != api.nilObj);
        const eval_selector: Oop = if (std.mem.eql(u8, std.mem.span(name), "rustImageAuthority"))
            api.intern("evalPersisting:")
        else
            api.intern("eval:");
        const result = api.send(codegen_object, eval_selector, &.{source_object}, 1);
        try std.testing.expect(result != api.nilObj);
    }
    const globals = api.import("Globals") orelse return error.GlobalsMissing;
    const generations = api.send(globals, api.intern("at:"), &.{api.intern("DispatchGeneration")}, 1);
    try std.testing.expect(generations != api.nilObj);
    const initialized = api.send(generations, api.intern("initializeCurrent"), &.{}, 0);
    try std.testing.expect(initialized != api.nilObj);
}

fn pathExists(path: []const u8) bool {
    std.fs.cwd().access(path, .{}) catch return false;
    return true;
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
        try std.testing.expectEqualStrings("Rust Image code mapping/rebuild orchestration is not installed", shell_detail[0..detail_len]);
    } else {
        try std.testing.expectEqualStrings("Rust Image fact preflight rejected unsupported values", shell_detail[0..detail_len]);
    }
    _ = send(api, api.import("Store").?, "retract:attr:value:", &.{ rejected_entity, rejected_attribute, rejected_parent });
    try std.testing.expect(send(api, system, "saveImage:", &.{str(api, "shell.image")}) == api.nilObj);
    try std.testing.expectEqualStrings("rejected", std.mem.span(api.symbolName(send(api, report, "status", &.{}))));
    try std.testing.expectEqual(@as(i64, 0), api.smallIntValue(send(api, report, "rejectedSlots", &.{})));
    detail_len = 0;
    const writer_detail = api.stringBytes(send(api, report, "detail", &.{}), &detail_len);
    try std.testing.expectEqualStrings("Rust Image generation authority is not available", writer_detail[0..detail_len]);
    try std.testing.expect(send(api, system, "loadImage:", &.{str(api, "shell.image")}) == api.nilObj);
    try std.testing.expect(send(api, system, "loadImage:rebuild:", &.{ str(api, "shell.image"), api.trueObj }) == api.nilObj);
    try std.testing.expect(send(api, system, "bakeExe:", &.{str(api, "shell-exe")}) == api.nilObj);

    const collector_vt = api.import("ImageCollectorVt") orelse return error.ImageCollectorVtMissing;
    const collector = api.alloc(collector_vt, 0);
    try std.testing.expect(send(api, collector, "e:a:v:tx:op:", &.{ api.nilObj, api.nilObj, api.nilObj, api.mkSmallInt(0), api.mkSmallInt(0) }) == api.nilObj);
}

test "Rust Image live probe accepts Zig Codegen authority without publishing" {
    const api = setupWithZigCodegen();
    try initializeAuthorityGeneration(api);

    const codegen_object = api.import("Codegen") orelse return error.CodegenMissing;
    const digest = api.send(codegen_object, api.intern("currentGenerationDigest"), &.{}, 0);
    try std.testing.expectEqual(api.string_vt, api.vtableOf(digest));
    var digest_len: usize = 0;
    const digest_bytes = api.stringBytes(digest, &digest_len);
    try std.testing.expectEqual(@as(usize, 64), digest_len);
    for (digest_bytes[0..digest_len]) |byte| {
        try std.testing.expect(std.ascii.isHex(byte));
    }

    const system = api.import("System") orelse return error.SystemMissing;
    const report = api.send(system, api.intern("imageSaveReport"), &.{}, 0);
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const allocator = std.testing.allocator;
    const dir = try tmp.dir.realpathAlloc(allocator, ".");
    defer allocator.free(dir);
    const path = try std.fs.path.join(allocator, &.{ dir, "authority.image" });
    defer allocator.free(path);

    try std.testing.expect(!pathExists(path));
    try std.testing.expect(api.send(system, api.intern("saveImage:"), &.{api.mkString(path.ptr, path.len)}, 1) == api.nilObj);
    try std.testing.expectEqual(api.intern("rejected"), api.send(report, api.intern("status"), &.{}, 0));
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(report, api.intern("codeUnits"), &.{}, 0)));
    var detail_len: usize = 0;
    const detail = api.stringBytes(api.send(report, api.intern("detail"), &.{}, 0), &detail_len);
    try std.testing.expectEqualStrings(
        "Rust Image fact preflight rejected unsupported values",
        detail[0..detail_len],
    );
    try std.testing.expect(!pathExists(path));
}
