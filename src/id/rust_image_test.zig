//! Focused live proof for the Rust Image dynamic shell.
//!
//! This slice checks the frozen family/layout and selector surface before the
//! v4 save/load orchestration is moved across. The shell's mutating selectors
//! must remain fail-closed until those later authorities are installed.

const std = @import("std");
const builtin = @import("builtin");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");
const artifact_inspector = @import("artifact_inspector.zig");
const collection = @import("collection.zig");
const codegen = @import("codegen.zig");
const err_mod = @import("error.zig");
const zig_image = @import("image.zig");
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

fn loadModule(api: *idapi.IdApi, variable: []const u8) void {
    const path = std.posix.getenv(variable) orelse
        std.debug.panic("{s} must select a dynamic module", .{variable});
    const handle = dlopen(path.ptr, 0x1) orelse
        std.debug.panic("could not load dynamic module {s}", .{variable});
    const raw_init = dlsym(handle, "module_init") orelse
        std.debug.panic("dynamic module {s} has no module_init", .{variable});
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);
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

fn setupWithRustCodegen() *idapi.IdApi {
    const api = floor.api();
    const existing_codegen = api.import("Codegen");
    artifact_inspector.bringUp(api);
    collection.bringUp(api);
    loadModule(api, "BIRNAM_RUST_PEG_DYLIB");
    lang.bringUp(api);
    loadModule(api, "BIRNAM_RUST_TCC_DYLIB");
    if (existing_codegen == null or existing_codegen == api.nilObj) {
        loadModule(api, "BIRNAM_RUST_STORE_DYLIB");
    }
    loadModule(api, "BIRNAM_RUST_CODEGEN_DYLIB");
    if (existing_codegen == null or existing_codegen == api.nilObj) {
        codegen.bringUp(api);
    }
    prelude.bringUp(api);
    err_mod.bringUp(api);
    relational.bringUp(api);
    loadImageModule(api);
    return api;
}

fn imageArchTag() u32 {
    return switch (builtin.cpu.arch) {
        .aarch64 => 1,
        .x86_64 => 2,
        else => 0,
    };
}

fn imageAbiHash() u64 {
    const names = [_][]const u8{
        "birnam_send",
        "birnam_intern",
        "birnam_mkSmallInt",
        "birnam_smallIntValue",
        "birnam_mkFloat",
        "birnam_floatValue",
        "birnam_mkString",
        "birnam_nil",
        "birnam_true",
        "birnam_false",
        "birnam_objectVt",
        "birnam_proto",
        "birnam_method",
        "birnam_alloc",
        "birnam_cstr",
        "birnam_free_cstr",
        "birnam_layoutAllOops",
        "birnam_global",
        "birnam_candidate_validate_binding",
    };
    var hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (names) |name| {
        for (name) |byte| {
            hash ^= byte;
            hash = hash *% 0x100000001b3;
        }
    }
    var version: u32 = 2;
    for (0..4) |_| {
        hash ^= @as(u64, version & 0xff);
        hash = hash *% 0x100000001b3;
        version >>= 8;
    }
    return hash;
}

fn appendImageRecord(image: *std.ArrayList(u8), allocator: std.mem.Allocator, kind: u32, payload: []const u8) !void {
    var u32_bytes: [4]u8 = undefined;
    var u64_bytes: [8]u8 = undefined;
    std.mem.writeInt(u32, &u32_bytes, kind, .little);
    try image.appendSlice(allocator, &u32_bytes);
    std.mem.writeInt(u64, &u64_bytes, payload.len, .little);
    try image.appendSlice(allocator, &u64_bytes);
    try image.appendSlice(allocator, payload);
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

fn initializeRustAuthorityGeneration(api: *idapi.IdApi) !void {
    const rust_codegen = api.import("RustCodegen") orelse return error.RustCodegenMissing;
    const sources = [_][]const u8{ MODULES_SRC, STDLIB_SRC, COMPILED_UNIT_SRC, AUTHORITY_FIXTURE_SRC };
    const names = [_][*:0]const u8{ "modules", "stdlib", "compiledUnit", "rustImageAuthority" };
    for (sources, names) |source, name| {
        const source_object = api.mkString(source.ptr, source.len);
        const staged = api.send(
            rust_codegen,
            api.intern("recordModuleSource:named:"),
            &.{ source_object, api.intern(name) },
            2,
        );
        try std.testing.expect(staged != api.nilObj);
        const eval_selector: Oop = if (std.mem.eql(u8, std.mem.span(name), "rustImageAuthority"))
            api.intern("evalPersisting:")
        else
            api.intern("eval:");
        const result = api.send(rust_codegen, eval_selector, &.{source_object}, 1);
        try std.testing.expect(result != api.nilObj);
    }
}

fn pathExists(path: []const u8) bool {
    std.fs.cwd().access(path, .{}) catch return false;
    return true;
}

fn stagedImageCodeCount() !usize {
    const temp_root = if (std.posix.getenv("TMPDIR")) |value| value[0..value.len] else "/tmp";
    var directory = std.fs.openDirAbsolute(temp_root, .{ .iterate = true }) catch return 0;
    defer directory.close();
    var entries = directory.iterate();
    var count: usize = 0;
    while (try entries.next()) |entry| {
        if (std.mem.startsWith(u8, entry.name, "birnam-image-code-")) {
            count += 1;
        }
    }
    return count;
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

    var malformed_tmp = std.testing.tmpDir(.{});
    defer malformed_tmp.cleanup();
    try malformed_tmp.dir.writeFile(.{ .sub_path = "malformed.image", .data = "not an image" });
    const malformed_dir = try malformed_tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(malformed_dir);
    const malformed_path = try std.fs.path.join(std.testing.allocator, &.{ malformed_dir, "malformed.image" });
    defer std.testing.allocator.free(malformed_path);
    try std.testing.expect(send(api, system, "loadImage:", &.{str(api, malformed_path)}) == api.nilObj);
    try std.testing.expect(send(api, system, "loadImage:rebuild:", &.{ str(api, malformed_path), api.trueObj }) == api.nilObj);
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
        "Rust Image code mapping/rebuild orchestration is not installed",
        detail[0..detail_len],
    );
    try std.testing.expect(!pathExists(path));
}

test "Rust Image delegates an admitted code unit to Rust Codegen" {
    const api = setupWithRustCodegen();
    const rust_codegen = api.import("RustCodegen") orelse return error.RustCodegenMissing;
    const runtime_store = api.import("Store") orelse return error.StoreMissing;
    const sentinel_entity = api.intern("rust_image_sentinel_entity");
    const sentinel_attribute = api.intern("rust_image_sentinel_attribute");
    const sentinel_value = api.mkSmallInt(33);
    _ = send(api, runtime_store, "assert:attr:value:", &.{ sentinel_entity, sentinel_attribute, sentinel_value });
    defer _ = send(api, runtime_store, "retract:attr:value:", &.{ sentinel_entity, sentinel_attribute, sentinel_value });
    const heap_layout = [_]u8{idapi.slot_raw};
    const heap_vt = api.proto(api.object_vt);
    api.setLayout(heap_vt, &heap_layout, heap_layout.len, idapi.slot_raw);
    _ = api.exportObj("RustImageHeapVt", heap_vt);

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const allocator = std.testing.allocator;
    const directory = try tmp.dir.realpathAlloc(allocator, ".");
    defer allocator.free(directory);
    const artifact_path = try std.fs.path.join(allocator, &.{ directory, "rust-image-mapped.dylib" });
    defer allocator.free(artifact_path);
    const image_path = try std.fs.path.join(allocator, &.{ directory, "rust-image-mapped.image" });
    defer allocator.free(image_path);

    const source = "[Globals at: #rustImageMapped put: 91]";
    const artifact = api.mkString(artifact_path.ptr, artifact_path.len);
    const compiled = api.send(rust_codegen, api.intern("compile:output:"), &.{ str(api, source), artifact }, 2);
    try std.testing.expect(compiled != api.nilObj);
    var compiled_len: usize = 0;
    const compiled_bytes = api.stringBytes(compiled, &compiled_len);
    try std.testing.expectEqualStrings(artifact_path, compiled_bytes[0..compiled_len]);
    const artifact_bytes = try std.fs.cwd().readFileAlloc(allocator, artifact_path, 64 * 1024 * 1024);
    defer allocator.free(artifact_bytes);

    var code_payload: std.ArrayList(u8) = .{};
    defer code_payload.deinit(allocator);
    var u64_bytes: [8]u8 = undefined;
    std.mem.writeInt(u64, &u64_bytes, std.hash.Wyhash.hash(0, artifact_bytes), .little);
    try code_payload.appendSlice(allocator, &u64_bytes);
    try code_payload.appendNTimes(allocator, 0, 64);
    try code_payload.appendSlice(allocator, artifact_bytes);

    var image: std.ArrayList(u8) = .{};
    defer image.deinit(allocator);
    try image.appendSlice(allocator, "BIRNAMIM");
    var u32_bytes: [4]u8 = undefined;
    std.mem.writeInt(u32, &u32_bytes, 4, .little);
    try image.appendSlice(allocator, &u32_bytes);
    std.mem.writeInt(u32, &u32_bytes, 0x0102_0304, .little);
    try image.appendSlice(allocator, &u32_bytes);
    std.mem.writeInt(u32, &u32_bytes, imageArchTag(), .little);
    try image.appendSlice(allocator, &u32_bytes);
    std.mem.writeInt(u64, &u64_bytes, imageAbiHash(), .little);
    try image.appendSlice(allocator, &u64_bytes);
    var authority: [64]u8 = .{0} ** 64;
    if (api.import("Codegen")) |zig_codegen| {
        const digest = send(api, zig_codegen, "currentGenerationDigest", &.{});
        if (api.vtableOf(digest) == api.string_vt) {
            var digest_len: usize = 0;
            const digest_bytes = api.stringBytes(digest, &digest_len);
            if (digest_len == authority.len) {
                std.mem.copyForwards(u8, &authority, digest_bytes[0..digest_len]);
            }
        }
    }
    std.mem.copyForwards(u8, code_payload.items[8 .. 8 + authority.len], &authority);

    var facts_payload: std.ArrayList(u8) = .{};
    defer facts_payload.deinit(allocator);
    std.mem.writeInt(u64, &u64_bytes, 2, .little);
    try facts_payload.appendSlice(allocator, &u64_bytes);
    std.mem.writeInt(u32, &u32_bytes, 2, .little);
    try facts_payload.appendSlice(allocator, &u32_bytes);
    try facts_payload.append(allocator, 0);
    std.mem.writeInt(u64, &u64_bytes, 1, .little);
    try facts_payload.appendSlice(allocator, &u64_bytes);
    for ([_][]const u8{ "rust_image_heap_entity", "rust_image_heap_value" }) |name| {
        try facts_payload.append(allocator, 5);
        std.mem.writeInt(u32, &u32_bytes, @intCast(name.len), .little);
        try facts_payload.appendSlice(allocator, u32_bytes[0..4]);
        try facts_payload.appendSlice(allocator, name);
    }
    try facts_payload.append(allocator, 7);
    std.mem.writeInt(u32, &u32_bytes, 0, .little);
    try facts_payload.appendSlice(allocator, u32_bytes[0..4]);
    try facts_payload.append(allocator, 0);
    std.mem.writeInt(u64, &u64_bytes, 2, .little);
    try facts_payload.appendSlice(allocator, &u64_bytes);
    for ([_][]const u8{ "rust_image_recovered_entity", "rust_image_recovered_attribute" }) |name| {
        try facts_payload.append(allocator, 5);
        std.mem.writeInt(u32, &u32_bytes, @intCast(name.len), .little);
        try facts_payload.appendSlice(allocator, u32_bytes[0..4]);
        try facts_payload.appendSlice(allocator, name);
    }
    try facts_payload.append(allocator, 3);
    std.mem.writeInt(i64, &u64_bytes, 42, .little);
    try facts_payload.appendSlice(allocator, &u64_bytes);

    var heap_payload: std.ArrayList(u8) = .{};
    defer heap_payload.deinit(allocator);
    std.mem.writeInt(u32, &u32_bytes, 1, .little);
    try heap_payload.appendSlice(allocator, &u32_bytes);
    const heap_name = "RustImageHeapVt";
    std.mem.writeInt(u32, &u32_bytes, heap_name.len, .little);
    try heap_payload.appendSlice(allocator, &u32_bytes);
    try heap_payload.appendSlice(allocator, heap_name);
    std.mem.writeInt(u32, &u32_bytes, 1, .little);
    try heap_payload.appendSlice(allocator, &u32_bytes);
    std.mem.writeInt(u64, &u64_bytes, 0x1122_3344_5566_7788, .little);
    try heap_payload.appendSlice(allocator, &u64_bytes);

    // A checksum-valid but non-dylib code unit must fail during read-only
    // admission, before image facts or heap state can be replayed. The Rust
    // Codegen preflight also closes and removes its temporary staging file.
    const staged_before = try stagedImageCodeCount();
    var invalid_code_payload: std.ArrayList(u8) = .{};
    defer invalid_code_payload.deinit(allocator);
    std.mem.writeInt(u64, &u64_bytes, std.hash.Wyhash.hash(0, "not-a-dylib"), .little);
    try invalid_code_payload.appendSlice(allocator, &u64_bytes);
    try invalid_code_payload.appendSlice(allocator, &authority);
    try invalid_code_payload.appendSlice(allocator, "not-a-dylib");
    var invalid_image: std.ArrayList(u8) = .{};
    defer invalid_image.deinit(allocator);
    try invalid_image.appendSlice(allocator, "BIRNAMIM");
    std.mem.writeInt(u32, &u32_bytes, 4, .little);
    try invalid_image.appendSlice(allocator, &u32_bytes);
    std.mem.writeInt(u32, &u32_bytes, 0x0102_0304, .little);
    try invalid_image.appendSlice(allocator, &u32_bytes);
    std.mem.writeInt(u32, &u32_bytes, imageArchTag(), .little);
    try invalid_image.appendSlice(allocator, &u32_bytes);
    std.mem.writeInt(u64, &u64_bytes, imageAbiHash(), .little);
    try invalid_image.appendSlice(allocator, &u64_bytes);
    try appendImageRecord(&invalid_image, allocator, 5, &authority);
    try appendImageRecord(&invalid_image, allocator, 4, invalid_code_payload.items);
    const invalid_image_path = try std.fs.path.join(allocator, &.{ directory, "rust-image-invalid-code.image" });
    defer allocator.free(invalid_image_path);
    try tmp.dir.writeFile(.{ .sub_path = "rust-image-invalid-code.image", .data = invalid_image.items });
    const system = api.import("System") orelse return error.SystemMissing;
    const rejected = send(api, system, "loadImage:", &.{str(api, invalid_image_path)});
    try std.testing.expect(rejected == api.nilObj);
    const retained_before = send(api, runtime_store, "currentValue:attr:", &.{ sentinel_entity, sentinel_attribute });
    try std.testing.expect(api.isSmallInt(retained_before) != 0);
    try std.testing.expectEqual(@as(i64, 33), api.smallIntValue(retained_before));
    try std.testing.expectEqual(staged_before, try stagedImageCodeCount());

    try appendImageRecord(&image, allocator, 5, &authority);
    try appendImageRecord(&image, allocator, 3, heap_payload.items);
    try appendImageRecord(&image, allocator, 2, facts_payload.items);
    try appendImageRecord(&image, allocator, 4, code_payload.items);
    try tmp.dir.writeFile(.{ .sub_path = "rust-image-mapped.image", .data = image.items });

    const loaded = send(api, system, "loadImage:", &.{str(api, image_path)});
    if (loaded != api.trueObj) {
        const diagnostic = send(api, rust_codegen, "lastError", &.{});
        if (api.vtableOf(diagnostic) == api.string_vt) {
            var diagnostic_len: usize = 0;
            const diagnostic_bytes = api.stringBytes(diagnostic, &diagnostic_len);
            std.debug.print("Rust Image mapped diagnostic={s}\n", .{diagnostic_bytes[0..diagnostic_len]});
        }
    }
    try std.testing.expect(loaded == api.trueObj);
    const retained = send(api, runtime_store, "currentValue:attr:", &.{ sentinel_entity, sentinel_attribute });
    try std.testing.expect(retained == api.nilObj);
    const recovered = send(
        api,
        runtime_store,
        "currentValue:attr:",
        &.{ api.intern("rust_image_recovered_entity"), api.intern("rust_image_recovered_attribute") },
    );
    try std.testing.expect(api.isSmallInt(recovered) != 0);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(recovered));
    const heap_value = send(
        api,
        runtime_store,
        "currentValue:attr:",
        &.{ api.intern("rust_image_heap_entity"), api.intern("rust_image_heap_value") },
    );
    try std.testing.expect(heap_value != api.nilObj);
    try std.testing.expectEqual(api.import("RustImageHeapVt"), api.vtableOf(heap_value));
    const heap_slots: [*]usize = @ptrCast(@alignCast(heap_value.?));
    try std.testing.expectEqual(@as(usize, 0x1122_3344_5566_7788), heap_slots[0]);
    const globals = api.import("Globals") orelse return error.GlobalsMissing;
    const mapped = send(api, globals, "at:", &.{api.intern("rustImageMapped")});
    try std.testing.expect(api.isSmallInt(mapped) != 0);
    try std.testing.expectEqual(@as(i64, 91), api.smallIntValue(mapped));
    try std.testing.expectEqual(staged_before, try stagedImageCodeCount());

    const rebuilt = send(api, system, "loadImage:rebuild:", &.{ str(api, image_path), api.trueObj });
    try std.testing.expect(rebuilt == api.falseObj);
    const rebuilt_fact = send(
        api,
        runtime_store,
        "currentValue:attr:",
        &.{ api.intern("rust_image_recovered_entity"), api.intern("rust_image_recovered_attribute") },
    );
    try std.testing.expect(api.isSmallInt(rebuilt_fact) != 0);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(rebuilt_fact));
}

test "Rust Codegen bootstraps and retains its durable generation authority" {
    const api = setupWithRustCodegen();
    try initializeRustAuthorityGeneration(api);

    const rust_codegen = api.import("RustCodegen") orelse return error.RustCodegenMissing;
    const runtime_store = api.import("Store") orelse return error.StoreMissing;
    const globals = api.import("Globals") orelse return error.GlobalsMissing;
    const generations = send(api, globals, "at:", &.{api.intern("DispatchGeneration")});
    try std.testing.expect(generations != api.nilObj);

    const initialized = send(api, rust_codegen, "initializeCurrentGeneration", &.{});
    try std.testing.expect(initialized != api.nilObj);
    try std.testing.expectEqual(
        initialized,
        send(api, runtime_store, "currentValue:attr:", &.{ api.intern("dispatch"), api.intern("activeGeneration") }),
    );
    try std.testing.expectEqual(initialized, send(api, generations, "current", &.{}));

    const digest = send(api, rust_codegen, "currentGenerationDigest", &.{});
    try std.testing.expectEqual(api.string_vt, api.vtableOf(digest));
    var digest_len: usize = 0;
    const digest_bytes = api.stringBytes(digest, &digest_len);
    try std.testing.expectEqual(@as(usize, 64), digest_len);
    const recorded = send(api, runtime_store, "currentValue:attr:", &.{ initialized, api.intern("baseDigest") });
    var recorded_len: usize = 0;
    const recorded_bytes = api.stringBytes(recorded, &recorded_len);
    try std.testing.expectEqual(@as(usize, 64), recorded_len);
    try std.testing.expectEqualSlices(u8, digest_bytes[0..digest_len], recorded_bytes[0..recorded_len]);

    // The explicit Rust-writer selection reaches Rust Codegen for the capture
    // phase, but publication remains fail-closed until the complete Store/heap
    // writer authority is installed.
    const system = api.import("System") orelse return error.SystemMissing;
    const report = send(api, system, "imageSaveReport", &.{});
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const image_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/rust-authority.image", .{directory});
    defer std.testing.allocator.free(image_path);
    var prior_image = try std.fs.createFileAbsolute(image_path, .{});
    defer prior_image.close();
    try prior_image.writeAll("prior-image");
    try std.testing.expectEqual(api.nilObj, send(api, system, "saveImage:", &.{str(api, image_path)}));
    try std.testing.expectEqual(api.intern("rejected"), send(api, report, "status", &.{}));
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(send(api, report, "codeUnits", &.{})));
    const preserved = try std.fs.cwd().readFileAlloc(std.testing.allocator, image_path, 1 << 20);
    defer std.testing.allocator.free(preserved);
    try std.testing.expectEqualStrings("prior-image", preserved);

    // Re-entry is idempotent: an existing canonical durable generation is
    // returned, never replaced by a second Rust-side projection object.
    try std.testing.expectEqual(initialized, send(api, rust_codegen, "initializeCurrentGeneration", &.{}));
    try std.testing.expectEqual(initialized, send(api, generations, "current", &.{}));
}

test "Rust Image reads a Zig-authored v4 data image" {
    const api = setupWithRustCodegen();
    const runtime_store = api.import("Store") orelse return error.StoreMissing;
    const system = api.import("System") orelse return error.SystemMissing;
    const entity = api.intern("rust_image_cross_floor_entity");
    const attribute = api.intern("rust_image_cross_floor_attribute");
    const value = api.mkSmallInt(707);
    _ = send(api, runtime_store, "assert:attr:value:", &.{ entity, attribute, value });
    defer _ = send(api, runtime_store, "retract:attr:value:", &.{ entity, attribute, value });

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const allocator = std.testing.allocator;
    const directory = try tmp.dir.realpathAlloc(allocator, ".");
    defer allocator.free(directory);
    const image_path = try std.fs.path.join(allocator, &.{ directory, "zig-writer.image" });
    defer allocator.free(image_path);

    // Install the Zig orchestration only for publication. Rust Store and Rust
    // Codegen remain the live authorities, so this isolates the Image writer
    // half of the cross-floor matrix.
    zig_image.bringUp(api);
    try std.testing.expect(send(api, system, "saveImage:", &.{str(api, image_path)}) == system);
    try std.testing.expectEqual(api.intern("saved"), send(api, send(api, system, "imageSaveReport", &.{}), "status", &.{}));

    _ = send(api, runtime_store, "retract:attr:value:", &.{ entity, attribute, value });
    loadImageModule(api);

    // A data-only v4 image is accepted by Rust Image and answers false because
    // there is no mapped code unit. The fact must still be restored wholesale.
    try std.testing.expect(send(api, system, "loadImage:", &.{str(api, image_path)}) == api.falseObj);
    const recovered = send(api, runtime_store, "currentValue:attr:", &.{ entity, attribute });
    try std.testing.expect(api.isSmallInt(recovered) != 0);
    try std.testing.expectEqual(@as(i64, 707), api.smallIntValue(recovered));
}

test "Rust Image rejects duplicate and truncated authority before Store mutation" {
    const api = setupWithRustCodegen();
    const runtime_store = api.import("Store") orelse return error.StoreMissing;
    const system = api.import("System") orelse return error.SystemMissing;
    const sentinel_entity = api.intern("rust_image_corruption_sentinel_entity");
    const sentinel_attribute = api.intern("rust_image_corruption_sentinel_attribute");
    const sentinel_value = api.mkSmallInt(919);
    _ = send(api, runtime_store, "assert:attr:value:", &.{ sentinel_entity, sentinel_attribute, sentinel_value });
    defer _ = send(api, runtime_store, "retract:attr:value:", &.{ sentinel_entity, sentinel_attribute, sentinel_value });

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const allocator = std.testing.allocator;
    const directory = try tmp.dir.realpathAlloc(allocator, ".");
    defer allocator.free(directory);
    const duplicate_path = try std.fs.path.join(allocator, &.{ directory, "duplicate-authority.image" });
    defer allocator.free(duplicate_path);
    const duplicate_code_path = try std.fs.path.join(allocator, &.{ directory, "duplicate-code.image" });
    defer allocator.free(duplicate_code_path);
    const truncated_path = try std.fs.path.join(allocator, &.{ directory, "truncated-authority.image" });
    defer allocator.free(truncated_path);

    var duplicate: std.ArrayList(u8) = .{};
    defer duplicate.deinit(allocator);
    try duplicate.appendSlice(allocator, "BIRNAMIM");
    var u32_bytes: [4]u8 = undefined;
    var u64_bytes: [8]u8 = undefined;
    std.mem.writeInt(u32, &u32_bytes, 4, .little);
    try duplicate.appendSlice(allocator, &u32_bytes);
    std.mem.writeInt(u32, &u32_bytes, 0x0102_0304, .little);
    try duplicate.appendSlice(allocator, &u32_bytes);
    std.mem.writeInt(u32, &u32_bytes, imageArchTag(), .little);
    try duplicate.appendSlice(allocator, &u32_bytes);
    std.mem.writeInt(u64, &u64_bytes, imageAbiHash(), .little);
    try duplicate.appendSlice(allocator, &u64_bytes);
    const authority = [_]u8{0} ** 64;
    try appendImageRecord(&duplicate, allocator, 5, &authority);
    try appendImageRecord(&duplicate, allocator, 5, &authority);
    try tmp.dir.writeFile(.{ .sub_path = "duplicate-authority.image", .data = duplicate.items });

    var duplicate_code: std.ArrayList(u8) = .{};
    defer duplicate_code.deinit(allocator);
    try duplicate_code.appendSlice(allocator, duplicate.items[0 .. 8 + 4 + 4 + 4 + 8]);
    try appendImageRecord(&duplicate_code, allocator, 5, &authority);
    var code_payload: std.ArrayList(u8) = .{};
    defer code_payload.deinit(allocator);
    const empty_unit = [_]u8{};
    std.mem.writeInt(u64, &u64_bytes, std.hash.Wyhash.hash(0, &empty_unit), .little);
    try code_payload.appendSlice(allocator, &u64_bytes);
    try code_payload.appendSlice(allocator, &authority);
    try appendImageRecord(&duplicate_code, allocator, 4, code_payload.items);
    try appendImageRecord(&duplicate_code, allocator, 4, code_payload.items);
    try tmp.dir.writeFile(.{ .sub_path = "duplicate-code.image", .data = duplicate_code.items });

    var truncated_bytes: std.ArrayList(u8) = .{};
    defer truncated_bytes.deinit(allocator);
    try truncated_bytes.appendSlice(allocator, duplicate.items[0 .. 8 + 4 + 4 + 4 + 8]);
    std.mem.writeInt(u32, &u32_bytes, 5, .little);
    try truncated_bytes.appendSlice(allocator, &u32_bytes);
    std.mem.writeInt(u64, &u64_bytes, 64, .little);
    try truncated_bytes.appendSlice(allocator, &u64_bytes);
    try truncated_bytes.appendSlice(allocator, "short");
    try tmp.dir.writeFile(.{ .sub_path = "truncated-authority.image", .data = truncated_bytes.items });

    try std.testing.expect(send(api, system, "loadImage:", &.{str(api, duplicate_path)}) == api.nilObj);
    try std.testing.expect(send(api, system, "loadImage:", &.{str(api, duplicate_code_path)}) == api.nilObj);
    try std.testing.expect(send(api, system, "loadImage:", &.{str(api, truncated_path)}) == api.nilObj);
    const retained = send(api, runtime_store, "currentValue:attr:", &.{ sentinel_entity, sentinel_attribute });
    try std.testing.expect(api.isSmallInt(retained) != 0);
    try std.testing.expectEqual(@as(i64, 919), api.smallIntValue(retained));
}
