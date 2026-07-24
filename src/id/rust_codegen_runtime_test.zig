//! Exercise the explicit Rust Codegen emitter through its real IdApi boundary.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const artifact_inspector = @import("artifact_inspector.zig");
const collection = @import("collection.zig");
const zig_codegen = @import("codegen.zig");
const peg = @import("peg.zig");
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
    artifact_inspector.bringUp(api);
    collection.bringUp(api);
    try load(api, "BIRNAM_RUST_PEG_DYLIB");
    lang.bringUp(api);
    try load(api, "BIRNAM_RUST_TCC_DYLIB");
    try load(api, "BIRNAM_RUST_STORE_DYLIB");
    try load(api, "BIRNAM_RUST_CODEGEN_DYLIB");
    zig_codegen.bringUp(api);
    return api;
}

fn emittedBytes(api: *idapi.IdApi, source: []const u8) ![]const u8 {
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const result = api.send(codegen, api.intern("emit:"), &.{api.mkString(source.ptr, source.len)}, 1);
    if (result == api.nilObj) return error.EmitFailed;
    var length: usize = 0;
    const bytes = api.stringBytes(result, &length);
    return bytes[0..length];
}

test "Rust Codegen emits through Rust PEG and the live IdApi boundary" {
    const api = try setup();
    const simple = try emittedBytes(api, "7");
    try std.testing.expect(std.mem.indexOf(u8, simple, "void* __ev") != null);
    try std.testing.expect(std.mem.indexOf(u8, simple, "return MKV(7L);") != null);

    const block = try emittedBytes(api, "[:x | [x + 1]]");
    try std.testing.expect(std.mem.indexOf(u8, block, "void* __blk0") != null);
    try std.testing.expect(std.mem.indexOf(u8, block, "birnam_intern(\"value:\")") != null);
}

test "Rust Codegen preflights v4 image framing without loading it" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const valid_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/valid.image", .{directory});
    defer std.testing.allocator.free(valid_path);
    var bytes: [28 + 12 + 64 + 12 + 12]u8 = undefined;
    @memcpy(bytes[0..8], "BIRNAMIM");
    std.mem.writeInt(u32, bytes[8..12], 4, .little);
    std.mem.writeInt(u32, bytes[12..16], 0x0102_0304, .little);
    std.mem.writeInt(u32, bytes[16..20], 1, .little);
    std.mem.writeInt(u64, bytes[20..28], 0, .little);
    std.mem.writeInt(u32, bytes[28..32], 5, .little);
    std.mem.writeInt(u64, bytes[32..40], 64, .little);
    @memset(bytes[40..], 0);
    std.mem.writeInt(u32, bytes[104..108], 2, .little);
    std.mem.writeInt(u64, bytes[108..116], 12, .little);
    try tmp.dir.writeFile(.{ .sub_path = "valid.image", .data = &bytes });

    const valid = api.mkString(valid_path.ptr, valid_path.len);
    try std.testing.expectEqual(
        api.trueObj,
        api.send(codegen, api.intern("preflightImage:"), &.{valid}, 1),
    );

    std.mem.writeInt(u32, bytes[124..128], 1, .little);
    try tmp.dir.writeFile(.{ .sub_path = "invalid-facts.image", .data = &bytes });
    const invalid_facts_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/invalid-facts.image", .{directory});
    defer std.testing.allocator.free(invalid_facts_path);
    const invalid_facts = api.mkString(invalid_facts_path.ptr, invalid_facts_path.len);
    try std.testing.expectEqual(
        api.nilObj,
        api.send(codegen, api.intern("preflightImage:"), &.{invalid_facts}, 1),
    );
    const facts_diagnostic = api.send(codegen, api.intern("lastError"), &.{}, 0);
    var facts_diagnostic_len: usize = 0;
    const facts_diagnostic_bytes = api.stringBytes(facts_diagnostic, &facts_diagnostic_len);
    try std.testing.expect(
        std.mem.indexOf(u8, facts_diagnostic_bytes[0..facts_diagnostic_len], "facts") != null,
    );

    std.mem.writeInt(u32, bytes[124..128], 0, .little);
    bytes[0] = 'X';
    try tmp.dir.writeFile(.{ .sub_path = "invalid.image", .data = &bytes });
    const invalid_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/invalid.image", .{directory});
    defer std.testing.allocator.free(invalid_path);
    const invalid = api.mkString(invalid_path.ptr, invalid_path.len);
    try std.testing.expectEqual(
        api.nilObj,
        api.send(codegen, api.intern("preflightImage:"), &.{invalid}, 1),
    );
    const diagnostic = api.send(codegen, api.intern("lastError"), &.{}, 0);
    var diagnostic_len: usize = 0;
    const diagnostic_bytes = api.stringBytes(diagnostic, &diagnostic_len);
    try std.testing.expect(
        std.mem.indexOf(u8, diagnostic_bytes[0..diagnostic_len], "framing") != null,
    );
}

test "Rust Codegen maps a verified v4 code unit during image recovery" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const artifact_path = try std.fmt.allocPrint(
        std.testing.allocator,
        "{s}/librust_image_code_map.dylib",
        .{directory},
    );
    defer std.testing.allocator.free(artifact_path);
    const image_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/mapped.image", .{directory});
    defer std.testing.allocator.free(image_path);

    const source = api.mkString("7", 1);
    const artifact = api.mkString(artifact_path.ptr, artifact_path.len);
    try std.testing.expectEqual(
        artifact,
        api.send(codegen, api.intern("compile:output:"), &.{ source, artifact }, 2),
    );
    const artifact_bytes = try std.fs.cwd().readFileAlloc(
        std.testing.allocator,
        artifact_path,
        64 * 1024 * 1024,
    );
    defer std.testing.allocator.free(artifact_bytes);

    var image: std.ArrayList(u8) = .{};
    defer image.deinit(std.testing.allocator);
    var u32_bytes: [4]u8 = undefined;
    var u64_bytes: [8]u8 = undefined;
    try image.appendSlice(std.testing.allocator, "BIRNAMIM");
    std.mem.writeInt(u32, &u32_bytes, 4, .little);
    try image.appendSlice(std.testing.allocator, &u32_bytes);
    std.mem.writeInt(u32, &u32_bytes, 0x0102_0304, .little);
    try image.appendSlice(std.testing.allocator, &u32_bytes);
    std.mem.writeInt(u32, &u32_bytes, 1, .little);
    try image.appendSlice(std.testing.allocator, &u32_bytes);
    std.mem.writeInt(u64, &u64_bytes, 0, .little);
    try image.appendSlice(std.testing.allocator, &u64_bytes);
    std.mem.writeInt(u32, &u32_bytes, 5, .little);
    try image.appendSlice(std.testing.allocator, &u32_bytes);
    std.mem.writeInt(u64, &u64_bytes, 64, .little);
    try image.appendSlice(std.testing.allocator, &u64_bytes);
    try image.appendNTimes(std.testing.allocator, 0, 64);
    std.mem.writeInt(u32, &u32_bytes, 4, .little);
    try image.appendSlice(std.testing.allocator, &u32_bytes);
    std.mem.writeInt(u64, &u64_bytes, 72 + artifact_bytes.len, .little);
    try image.appendSlice(std.testing.allocator, &u64_bytes);
    std.mem.writeInt(u64, &u64_bytes, std.hash.Wyhash.hash(0, artifact_bytes), .little);
    try image.appendSlice(std.testing.allocator, &u64_bytes);
    try image.appendNTimes(std.testing.allocator, 0, 64);
    try image.appendSlice(std.testing.allocator, artifact_bytes);
    try tmp.dir.writeFile(.{ .sub_path = "mapped.image", .data = image.items });

    const image_object = api.mkString(image_path.ptr, image_path.len);
    try std.testing.expectEqual(
        api.nilObj,
        api.send(codegen, api.intern("mapImageCode:"), &.{image_object}, 1),
    );
    const pre_gate_diagnostic = api.send(codegen, api.intern("lastError"), &.{}, 0);
    var pre_gate_len: usize = 0;
    const pre_gate_bytes = api.stringBytes(pre_gate_diagnostic, &pre_gate_len);
    try std.testing.expect(
        std.mem.indexOf(u8, pre_gate_bytes[0..pre_gate_len], "image recovery") != null,
    );

    try std.testing.expectEqual(
        api.trueObj,
        api.send(codegen, api.intern("beginImageDispatchRecovery"), &.{}, 0),
    );
    const mapped = api.send(codegen, api.intern("mapImageCode:"), &.{image_object}, 1);
    try std.testing.expect(api.isSmallInt(mapped) != 0);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(mapped));
    try std.testing.expectEqual(
        api.trueObj,
        api.send(codegen, api.intern("endImageDispatchRecovery"), &.{}, 0),
    );
}

test "Rust Codegen replays scalar v4 facts through Rust Store recovery" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const store = api.import("Store") orelse return error.MissingStore;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const image_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/facts.image", .{directory});
    defer std.testing.allocator.free(image_path);

    var facts: std.ArrayList(u8) = .{};
    defer facts.deinit(std.testing.allocator);
    var u32_bytes: [4]u8 = undefined;
    var u64_bytes: [8]u8 = undefined;
    std.mem.writeInt(u64, &u64_bytes, 7, .little);
    try facts.appendSlice(std.testing.allocator, &u64_bytes);
    std.mem.writeInt(u32, &u32_bytes, 1, .little);
    try facts.appendSlice(std.testing.allocator, &u32_bytes);
    try facts.append(std.testing.allocator, 0);
    std.mem.writeInt(u64, &u64_bytes, 7, .little);
    try facts.appendSlice(std.testing.allocator, &u64_bytes);
    try facts.append(std.testing.allocator, 5);
    std.mem.writeInt(u32, &u32_bytes, 10, .little);
    try facts.appendSlice(std.testing.allocator, &u32_bytes);
    try facts.appendSlice(std.testing.allocator, "factEntity");
    try facts.append(std.testing.allocator, 5);
    std.mem.writeInt(u32, &u32_bytes, 8, .little);
    try facts.appendSlice(std.testing.allocator, &u32_bytes);
    try facts.appendSlice(std.testing.allocator, "factAttr");
    try facts.append(std.testing.allocator, 3);
    std.mem.writeInt(i64, &u64_bytes, 42, .little);
    try facts.appendSlice(std.testing.allocator, &u64_bytes);

    var image: std.ArrayList(u8) = .{};
    defer image.deinit(std.testing.allocator);
    try image.appendSlice(std.testing.allocator, "BIRNAMIM");
    std.mem.writeInt(u32, &u32_bytes, 4, .little);
    try image.appendSlice(std.testing.allocator, &u32_bytes);
    std.mem.writeInt(u32, &u32_bytes, 0x0102_0304, .little);
    try image.appendSlice(std.testing.allocator, &u32_bytes);
    std.mem.writeInt(u32, &u32_bytes, 1, .little);
    try image.appendSlice(std.testing.allocator, &u32_bytes);
    std.mem.writeInt(u64, &u64_bytes, 0, .little);
    try image.appendSlice(std.testing.allocator, &u64_bytes);
    std.mem.writeInt(u32, &u32_bytes, 5, .little);
    try image.appendSlice(std.testing.allocator, &u32_bytes);
    std.mem.writeInt(u64, &u64_bytes, 64, .little);
    try image.appendSlice(std.testing.allocator, &u64_bytes);
    try image.appendNTimes(std.testing.allocator, 0, 64);
    std.mem.writeInt(u32, &u32_bytes, 2, .little);
    try image.appendSlice(std.testing.allocator, &u32_bytes);
    std.mem.writeInt(u64, &u64_bytes, facts.items.len, .little);
    try image.appendSlice(std.testing.allocator, &u64_bytes);
    try image.appendSlice(std.testing.allocator, facts.items);
    try tmp.dir.writeFile(.{ .sub_path = "facts.image", .data = image.items });

    // A heap reference is deliberately not materialized at this rung. Build a
    // same-shape image with a unique entity/attribute and prove the replay
    // rejects it before Rust Store can observe a partial fact.
    const heap_image_path = try std.fmt.allocPrint(
        std.testing.allocator,
        "{s}/heap-facts.image",
        .{directory},
    );
    defer std.testing.allocator.free(heap_image_path);
    var heap_facts: std.ArrayList(u8) = .{};
    defer heap_facts.deinit(std.testing.allocator);
    try heap_facts.appendSlice(std.testing.allocator, facts.items[0..49]);
    @memcpy(heap_facts.items[26..36], "heapEntity");
    @memcpy(heap_facts.items[41..49], "heapAttr");
    try heap_facts.append(std.testing.allocator, 7);
    std.mem.writeInt(u32, &u32_bytes, 0, .little);
    try heap_facts.appendSlice(std.testing.allocator, &u32_bytes);
    var heap_image: std.ArrayList(u8) = .{};
    defer heap_image.deinit(std.testing.allocator);
    try heap_image.appendSlice(std.testing.allocator, image.items[0..116]);
    std.mem.writeInt(u64, heap_image.items[108..116], heap_facts.items.len, .little);
    try heap_image.appendSlice(std.testing.allocator, heap_facts.items);
    try tmp.dir.writeFile(.{ .sub_path = "heap-facts.image", .data = heap_image.items });

    const image_object = api.mkString(image_path.ptr, image_path.len);
    const heap_image_object = api.mkString(heap_image_path.ptr, heap_image_path.len);
    try std.testing.expectEqual(
        api.nilObj,
        api.send(codegen, api.intern("replayImageFacts:"), &.{image_object}, 1),
    );
    const pre_gate_diagnostic = api.send(codegen, api.intern("lastError"), &.{}, 0);
    var pre_gate_len: usize = 0;
    const pre_gate_bytes = api.stringBytes(pre_gate_diagnostic, &pre_gate_len);
    try std.testing.expect(
        std.mem.indexOf(u8, pre_gate_bytes[0..pre_gate_len], "image recovery") != null,
    );

    try std.testing.expectEqual(
        api.trueObj,
        api.send(codegen, api.intern("beginImageDispatchRecovery"), &.{}, 0),
    );
    const heap_rejected = api.send(codegen, api.intern("replayImageFacts:"), &.{heap_image_object}, 1);
    try std.testing.expectEqual(api.nilObj, heap_rejected);
    const heap_diagnostic = api.send(codegen, api.intern("lastError"), &.{}, 0);
    var heap_diagnostic_len: usize = 0;
    const heap_diagnostic_bytes = api.stringBytes(heap_diagnostic, &heap_diagnostic_len);
    try std.testing.expect(
        std.mem.indexOf(u8, heap_diagnostic_bytes[0..heap_diagnostic_len], "heap references") != null,
    );
    try std.testing.expectEqual(
        api.nilObj,
        api.send(
            store,
            api.intern("currentValue:attr:"),
            &.{ api.intern("heapEntity"), api.intern("heapAttr") },
            2,
        ),
    );
    const replayed = api.send(codegen, api.intern("replayImageFacts:"), &.{image_object}, 1);
    try std.testing.expect(api.isSmallInt(replayed) != 0);
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(replayed));
    const entity = api.intern("factEntity");
    const attribute = api.intern("factAttr");
    const value = api.send(store, api.intern("currentValue:attr:"), &.{ entity, attribute }, 2);
    try std.testing.expect(api.isSmallInt(value) != 0);
    try std.testing.expectEqual(@as(i64, 42), api.smallIntValue(value));
    try std.testing.expectEqual(
        api.trueObj,
        api.send(codegen, api.intern("endImageDispatchRecovery"), &.{}, 0),
    );
}

test "Rust Codegen evaluates a scalar program through Rust TCC" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const source = api.mkString("7", 1);
    const result = api.send(codegen, api.intern("eval:"), &.{source}, 1);
    try std.testing.expect(api.isSmallInt(result) != 0);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(result));

    // Scalar entry points are invoked directly from the relocated TCC symbol,
    // then their unit is marked disposable before it can become an installed
    // code owner. Repeat the path and collect to exercise that cleanup seam.
    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    for (0..32) |_| {
        const repeated = api.send(codegen, api.intern("eval:"), &.{source}, 1);
        try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(repeated));
    }
    _ = api.gcCollect();
}

test "Rust Codegen invokes an emitted block before its owner is collected" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const source = api.mkString("[:x | x]", 8);
    const block = api.send(codegen, api.intern("eval:"), &.{source}, 1);
    try std.testing.expect(block != api.nilObj);
    var anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&anchor));
    for (0..128) |_| _ = api.mkString("temporary", 9);
    _ = api.gcCollect();
    const argument = api.mkSmallInt(4);
    const result = api.send(block, api.intern("value:"), &.{argument}, 1);
    try std.testing.expect(api.isSmallInt(result) != 0);
    try std.testing.expectEqual(@as(i64, 4), api.smallIntValue(result));
}

test "Rust Codegen writes a compiled dylib through Rust TCC" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const path = try std.fmt.allocPrint(std.testing.allocator, "{s}/librust_codegen_test.dylib", .{directory});
    defer std.testing.allocator.free(path);
    const source = api.mkString("7", 1);
    const output = api.mkString(path.ptr, path.len);
    const result = api.send(codegen, api.intern("compile:output:"), &.{ source, output }, 2);
    try std.testing.expect(result != api.nilObj);
    const artifact = try std.fs.openFileAbsolute(path, .{});
    defer artifact.close();
    try std.testing.expect((try artifact.stat()).size > 0);
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.ArtifactDlopenFailed;
    const abi_symbol = api.dlsym(handle, "__birnam_artifact_abi") orelse
        return error.ArtifactAbiMissing;
    const abi: [*:0]const u8 = @ptrCast(abi_symbol);
    try std.testing.expectEqualStrings("birnam-artifact-generation-v1", std.mem.span(abi));
    const identity_symbol = api.dlsym(handle, "__birnam_artifact_identity") orelse
        return error.ArtifactIdentityMissing;
    const identity: [*:0]const u8 = @ptrCast(identity_symbol);
    try std.testing.expectEqualStrings("rust_codegen_test", std.mem.span(identity));
    const source_symbol = api.dlsym(handle, "__birnam_artifact_source") orelse
        return error.ArtifactSourceMissing;
    const source_len_symbol = api.dlsym(handle, "__birnam_artifact_source_len") orelse
        return error.ArtifactSourceLengthMissing;
    const source_len: *const usize = @ptrCast(@alignCast(source_len_symbol));
    const embedded_source: [*]const u8 = @ptrCast(source_symbol);
    try std.testing.expectEqualStrings("7", embedded_source[0..source_len.*]);
    _ = api.dlsym(handle, "__birnam_artifact_descriptor") orelse
        return error.ArtifactDescriptorMissing;
    const loaded = api.send(codegen, api.intern("load:"), &.{output}, 1);
    try std.testing.expect(api.isSmallInt(loaded) != 0);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(loaded));

    const bad_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/librust_codegen_bad.dylib", .{directory});
    defer std.testing.allocator.free(bad_path);
    const bad_source = api.mkString("[", 1);
    const bad_output = api.mkString(bad_path.ptr, bad_path.len);
    const failed = api.send(codegen, api.intern("compile:output:"), &.{ bad_source, bad_output }, 2);
    try std.testing.expect(failed == api.nilObj);
    try std.testing.expectError(error.FileNotFound, std.fs.openFileAbsolute(bad_path, .{}));
}

test "Rust Codegen publishes and clears a diagnostic" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const bad_source = api.mkString("[", 1);
    const failed = api.send(codegen, api.intern("eval:"), &.{bad_source}, 1);
    try std.testing.expect(failed == api.nilObj);
    const diagnostic = api.send(codegen, api.intern("lastError"), &.{}, 0);
    try std.testing.expect(diagnostic != api.nilObj);
    var length: usize = 0;
    const bytes = api.stringBytes(diagnostic, &length);
    try std.testing.expect(std.mem.indexOf(u8, bytes[0..length], "syntax error at byte") != null);

    const good_source = api.mkString("7", 1);
    const value = api.send(codegen, api.intern("eval:"), &.{good_source}, 1);
    try std.testing.expect(api.isSmallInt(value) != 0);
    try std.testing.expect(api.send(codegen, api.intern("lastError"), &.{}, 0) == api.nilObj);
}

test "Rust Codegen resolves a global through Rust Store" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const globals = api.import("Globals") orelse return error.MissingGlobals;
    const key = api.intern("rust_codegen_answer");
    const value = api.mkSmallInt(41);
    _ = api.send(globals, api.intern("at:put:"), &.{ key, value }, 2);
    const source = api.mkString("rust_codegen_answer", 19);
    const result = api.send(codegen, api.intern("eval:"), &.{source}, 1);
    try std.testing.expect(api.isSmallInt(result) != 0);
    try std.testing.expectEqual(@as(i64, 41), api.smallIntValue(result));

    const persisted_source = "let rust_codegen_persisted := 43";
    const persisted = api.mkString(persisted_source.ptr, persisted_source.len);
    const persisted_result = api.send(codegen, api.intern("evalPersisting:"), &.{persisted}, 1);
    try std.testing.expect(api.isSmallInt(persisted_result) != 0);
    const persisted_name = api.mkString("rust_codegen_persisted", 22);
    const persisted_read = api.send(codegen, api.intern("eval:"), &.{persisted_name}, 1);
    try std.testing.expect(api.isSmallInt(persisted_read) != 0);
    try std.testing.expectEqual(@as(i64, 43), api.smallIntValue(persisted_read));
}

test "Rust Codegen embeds files through source-aware eval and compile" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try tmp.dir.writeFile(.{ .sub_path = "asset.bin", .data = "a\x00b\xff" });
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const source_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/script.birnam", .{directory});
    defer std.testing.allocator.free(source_path);
    const output_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/librust_codegen_embed.dylib", .{directory});
    defer std.testing.allocator.free(output_path);
    const source_text = "[System embedFile: \"asset.bin\"]";
    const source = api.mkString(source_text.ptr, source_text.len);
    const source_file = api.mkString(source_path.ptr, source_path.len);

    const evaluated = api.send(codegen, api.intern("eval:from:"), &.{ source, source_file }, 2);
    var evaluated_len: usize = 0;
    const evaluated_bytes = api.stringBytes(evaluated, &evaluated_len);
    try std.testing.expectEqualSlices(u8, "a\x00b\xff", evaluated_bytes[0..evaluated_len]);

    const output = api.mkString(output_path.ptr, output_path.len);
    const compiled = api.send(codegen, api.intern("compile:output:from:"), &.{ source, output, source_file }, 3);
    try std.testing.expectEqual(output, compiled);
    try tmp.dir.deleteFile("asset.bin");
    const loaded = api.send(codegen, api.intern("load:"), &.{output}, 1);
    var loaded_len: usize = 0;
    const loaded_bytes = api.stringBytes(loaded, &loaded_len);
    try std.testing.expectEqualSlices(u8, "a\x00b\xff", loaded_bytes[0..loaded_len]);
}

test "Rust Codegen descriptors preserve module and native requirements" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const artifact = api.import("Artifact") orelse return error.MissingArtifact;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try tmp.dir.writeFile(.{ .sub_path = "libnative.dylib", .data = "fixture" });
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const output_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/librust_codegen_metadata.dylib", .{directory});
    defer std.testing.allocator.free(output_path);
    const source_text =
        "[Module named: #metadataProbe requires: #[#dep]]; [Alien dlopen: \"libnative.dylib\"]; 7";
    const source = api.mkString(source_text.ptr, source_text.len);
    const output = api.mkString(output_path.ptr, output_path.len);
    const compiled = api.send(codegen, api.intern("compile:output:"), &.{ source, output }, 2);
    try std.testing.expectEqual(output, compiled);
    const descriptor = api.send(artifact, api.intern("inspect:"), &.{output}, 1);
    try std.testing.expect(descriptor != api.nilObj);
    try std.testing.expectEqual(api.intern("metadataProbe"), api.send(descriptor, api.intern("identity"), &.{}, 0));
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(descriptor, api.intern("requiresCount"), &.{}, 0)));
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(descriptor, api.intern("nativeRequiresCount"), &.{}, 0)));
    var descriptor_source_len: usize = 0;
    var descriptor_revision_len: usize = 0;
    const descriptor_source = api.stringBytes(api.send(descriptor, api.intern("sourceDigest"), &.{}, 0), &descriptor_source_len);
    const descriptor_revision = api.stringBytes(api.send(descriptor, api.intern("revisionDigest"), &.{}, 0), &descriptor_revision_len);
    try std.testing.expect(!std.mem.eql(u8, descriptor_source[0..descriptor_source_len], descriptor_revision[0..descriptor_revision_len]));
    try tmp.dir.deleteFile("libnative.dylib");
    const loaded = api.send(codegen, api.intern("load:"), &.{output}, 1);
    try std.testing.expectEqual(api.nilObj, loaded);
    const diagnostic = api.send(codegen, api.intern("lastError"), &.{}, 0);
    var diagnostic_len: usize = 0;
    const diagnostic_bytes = api.stringBytes(diagnostic, &diagnostic_len);
    try std.testing.expect(std.mem.indexOf(u8, diagnostic_bytes[0..diagnostic_len], "native requirement") != null);
}

test "Rust Codegen rejects artifact dependencies before module policy or init" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const globals = api.import("Globals") orelse return error.MissingGlobals;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const output_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/librust_codegen_dependency_guard.dylib", .{directory});
    defer std.testing.allocator.free(output_path);
    const source_text = "[Module named: #dependencyGuard requires: #[#dependencyDefinitelyMissing]]; let rust_dependency_guard := 99";
    const source = api.mkString(source_text.ptr, source_text.len);
    const output = api.mkString(output_path.ptr, output_path.len);
    try std.testing.expectEqual(output, api.send(codegen, api.intern("compile:output:"), &.{ source, output }, 2));

    const guard = api.intern("rust_dependency_guard");
    try std.testing.expectEqual(api.nilObj, api.send(globals, api.intern("at:"), &.{guard}, 1));
    const loaded = api.send(codegen, api.intern("load:"), &.{output}, 1);
    try std.testing.expectEqual(api.nilObj, loaded);
    const after = api.send(globals, api.intern("at:"), &.{guard}, 1);
    try std.testing.expectEqual(api.nilObj, after);
    const diagnostic = api.send(codegen, api.intern("lastError"), &.{}, 0);
    var diagnostic_len: usize = 0;
    const diagnostic_bytes = api.stringBytes(diagnostic, &diagnostic_len);
    try std.testing.expect(std.mem.indexOf(u8, diagnostic_bytes[0..diagnostic_len], "module") != null);
}

test "Rust Codegen compile root anchors embedded files independently of source path" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try tmp.dir.writeFile(.{ .sub_path = "asset.bin", .data = "rooted" });
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const source_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/nested/script.birnam", .{directory});
    defer std.testing.allocator.free(source_path);
    const output_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/librust_codegen_root.dylib", .{directory});
    defer std.testing.allocator.free(output_path);
    const source_text = "[System embedFile: \"asset.bin\"]";
    const source = api.mkString(source_text.ptr, source_text.len);
    const source_file = api.mkString(source_path.ptr, source_path.len);
    const from_result = api.send(codegen, api.intern("eval:from:"), &.{ source, source_file }, 2);
    try std.testing.expectEqual(api.nilObj, from_result);
    const output = api.mkString(output_path.ptr, output_path.len);
    const root = api.mkString(directory.ptr, directory.len);
    const compiled = api.send(codegen, api.intern("compile:output:root:"), &.{ source, output, root }, 3);
    try std.testing.expectEqual(output, compiled);
    try tmp.dir.deleteFile("asset.bin");
    const loaded = api.send(codegen, api.intern("load:"), &.{output}, 1);
    var loaded_len: usize = 0;
    const loaded_bytes = api.stringBytes(loaded, &loaded_len);
    try std.testing.expectEqualSlices(u8, "rooted", loaded_bytes[0..loaded_len]);
}

test "Rust Codegen reports distinct embedded file literals" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const source_text =
        "[System embedFile: \"a.txt\"]; [[:x | [System embedFile: \"b.bin\"]] value: 1]; [System embedFile: \"a.txt\"]";
    const source = api.mkString(source_text.ptr, source_text.len);
    const files = api.send(codegen, api.intern("embeddedFilesOf:"), &.{source}, 1);
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(api.send(files, api.intern("size"), &.{}, 0)));
    const first = api.send(files, api.intern("at:"), &.{api.mkSmallInt(0)}, 1);
    const second = api.send(files, api.intern("at:"), &.{api.mkSmallInt(1)}, 1);
    var first_len: usize = 0;
    var second_len: usize = 0;
    try std.testing.expectEqualStrings("a.txt", api.stringBytes(first, &first_len)[0..first_len]);
    try std.testing.expectEqualStrings("b.bin", api.stringBytes(second, &second_len)[0..second_len]);
}

test "Rust Codegen exposes module metadata without evaluating source" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const source_text = "[Module named: #rustMetadata requires: #[#depA #depB]]; [Alien dlopen: \"libdep.dylib\"]";
    const source = api.mkString(source_text.ptr, source_text.len);
    const metadata = api.send(codegen, api.intern("moduleMetadataOf:"), &.{source}, 1);
    try std.testing.expect(metadata != api.nilObj);
    try std.testing.expectEqual(api.intern("moduleMetadata"), api.send(metadata, api.intern("at:"), &.{api.mkSmallInt(0)}, 1));
    try std.testing.expectEqual(api.intern("rustMetadata"), api.send(metadata, api.intern("at:"), &.{api.mkSmallInt(1)}, 1));
    const requires = api.send(metadata, api.intern("at:"), &.{api.mkSmallInt(2)}, 1);
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(api.send(requires, api.intern("size"), &.{}, 0)));
    const revision = api.send(codegen, api.intern("sourceRevisionOf:"), &.{source}, 1);
    var metadata_revision_len: usize = 0;
    var source_revision_len: usize = 0;
    const metadata_revision = api.stringBytes(
        api.send(metadata, api.intern("at:"), &.{api.mkSmallInt(4)}, 1),
        &metadata_revision_len,
    );
    const source_revision = api.stringBytes(
        api.send(revision, api.intern("at:"), &.{api.mkSmallInt(6)}, 1),
        &source_revision_len,
    );
    try std.testing.expectEqualSlices(
        u8,
        metadata_revision[0..metadata_revision_len],
        source_revision[0..source_revision_len],
    );
    const legacy = api.mkString("7", 1);
    try std.testing.expectEqual(api.nilObj, api.send(codegen, api.intern("moduleMetadataOf:"), &.{legacy}, 1));
}

test "Rust Codegen exposes immutable source revisions" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const source_text = "[Module named: #rustRevision requires: #[#dep]]; 7";
    const source = api.mkString(source_text.ptr, source_text.len);
    const revision = api.send(codegen, api.intern("sourceRevisionOf:"), &.{source}, 1);
    try std.testing.expect(revision != api.nilObj);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(api.send(revision, api.intern("size"), &.{}, 0)));
    try std.testing.expectEqual(api.intern("sourceRevision"), api.send(revision, api.intern("at:"), &.{api.mkSmallInt(0)}, 1));
    try std.testing.expectEqual(api.intern("rustRevision"), api.send(revision, api.intern("at:"), &.{api.mkSmallInt(1)}, 1));
    try std.testing.expectEqual(source, api.send(revision, api.intern("at:"), &.{api.mkSmallInt(3)}, 1));
    const legacy = api.mkString("7", 1);
    try std.testing.expectEqual(api.nilObj, api.send(codegen, api.intern("sourceRevisionOf:"), &.{legacy}, 1));
}

test "Rust Codegen partitions method and non-method revision digests" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const source_a_text =
        "[Module named: #partitionedRevision requires: #[]]; [Reflect defineMethod: #answer on: Thing as: [:self | 7]]; 7";
    const source_b_text =
        "[Module named: #partitionedRevision requires: #[]]; [Reflect defineMethod: #answer on: Thing as: [:self | 8]]; 7";
    const source_c_text =
        "[Module named: #partitionedRevision requires: #[]]; [Reflect defineMethod: #answer on: Thing as: [:self | 7]]; 8";
    const source_a = api.mkString(source_a_text.ptr, source_a_text.len);
    const source_b = api.mkString(source_b_text.ptr, source_b_text.len);
    const source_c = api.mkString(source_c_text.ptr, source_c_text.len);
    const revision_a = api.send(codegen, api.intern("sourceRevisionOf:"), &.{source_a}, 1);
    const revision_b = api.send(codegen, api.intern("sourceRevisionOf:"), &.{source_b}, 1);
    const revision_c = api.send(codegen, api.intern("sourceRevisionOf:"), &.{source_c}, 1);
    try std.testing.expect(revision_a != api.nilObj);
    try std.testing.expect(revision_b != api.nilObj);
    try std.testing.expect(revision_c != api.nilObj);

    var a_non_len: usize = 0;
    var b_non_len: usize = 0;
    var c_non_len: usize = 0;
    var a_method_len: usize = 0;
    var b_method_len: usize = 0;
    var c_method_len: usize = 0;
    const a_non = api.stringBytes(api.send(revision_a, api.intern("at:"), &.{api.mkSmallInt(4)}, 1), &a_non_len);
    const b_non = api.stringBytes(api.send(revision_b, api.intern("at:"), &.{api.mkSmallInt(4)}, 1), &b_non_len);
    const c_non = api.stringBytes(api.send(revision_c, api.intern("at:"), &.{api.mkSmallInt(4)}, 1), &c_non_len);
    const a_method = api.stringBytes(api.send(revision_a, api.intern("at:"), &.{api.mkSmallInt(5)}, 1), &a_method_len);
    const b_method = api.stringBytes(api.send(revision_b, api.intern("at:"), &.{api.mkSmallInt(5)}, 1), &b_method_len);
    const c_method = api.stringBytes(api.send(revision_c, api.intern("at:"), &.{api.mkSmallInt(5)}, 1), &c_method_len);
    try std.testing.expectEqualSlices(u8, a_non[0..a_non_len], b_non[0..b_non_len]);
    try std.testing.expect(!std.mem.eql(u8, a_non[0..a_non_len], c_non[0..c_non_len]));
    try std.testing.expect(!std.mem.eql(u8, a_method[0..a_method_len], b_method[0..b_method_len]));
    try std.testing.expectEqualSlices(u8, a_method[0..a_method_len], c_method[0..c_method_len]);
}

test "Rust Codegen retains registered module sources across collection" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const store = api.import("Store") orelse return error.MissingStore;
    const name = api.intern("rustRegisteredModule");
    const source_text = "[Module named: #rustRegisteredModule requires: #[]]; [Reflect defineMethod: #rustRegisteredAnswer on: Integer as: [:self | 321]]; 7";
    const source = api.mkString(source_text.ptr, source_text.len);
    try std.testing.expectEqual(
        source,
        api.send(codegen, api.intern("recordModuleSource:named:"), &.{ source, name }, 2),
    );
    try std.testing.expectEqual(
        source,
        api.send(codegen, api.intern("recordModuleSource:named:"), &.{ source, name }, 2),
    );
    _ = api.gcCollect();
    try std.testing.expectEqual(source, api.send(codegen, api.intern("moduleSourceFor:"), &.{name}, 1));
    const projection = api.send(codegen, api.intern("initialGenerationProjection"), &.{}, 0);
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(api.send(projection, api.intern("size"), &.{}, 0)));
    const source_map = api.send(projection, api.intern("at:"), &.{api.mkSmallInt(1)}, 1);
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(source_map, api.intern("size"), &.{}, 0)));
    const entry = api.send(source_map, api.intern("at:"), &.{api.mkSmallInt(0)}, 1);
    try std.testing.expectEqual(name, api.send(entry, api.intern("at:"), &.{api.mkSmallInt(0)}, 1));
    try std.testing.expectEqual(api.nilObj, api.send(entry, api.intern("at:"), &.{api.mkSmallInt(2)}, 1));
    var digest_len: usize = 0;
    _ = api.stringBytes(api.send(projection, api.intern("at:"), &.{api.mkSmallInt(2)}, 1), &digest_len);
    try std.testing.expectEqual(@as(usize, 64), digest_len);
    const object = api.import("Object") orelse return error.MissingObject;
    const generation = api.send(object, api.intern("clone"), &.{}, 0);
    const source_map_value = api.send(projection, api.intern("at:"), &.{api.mkSmallInt(1)}, 1);
    const base_digest = api.send(projection, api.intern("at:"), &.{api.mkSmallInt(2)}, 1);
    const source_map_cell = api.send(
        store,
        api.intern("cellFor:attr:"),
        &.{ generation, api.intern("sourceMap") },
        2,
    );
    _ = api.send(source_map_cell, api.intern("set:"), &.{source_map_value}, 1);
    const base_digest_cell = api.send(
        store,
        api.intern("cellFor:attr:"),
        &.{ generation, api.intern("baseDigest") },
        2,
    );
    _ = api.send(base_digest_cell, api.intern("set:"), &.{base_digest}, 1);
    const active_generation_cell = api.send(
        store,
        api.intern("cellFor:attr:"),
        &.{ api.intern("dispatch"), api.intern("activeGeneration") },
        2,
    );
    _ = api.send(active_generation_cell, api.intern("set:"), &.{generation}, 1);
    try std.testing.expectEqual(base_digest, api.send(codegen, api.intern("currentGenerationDigest"), &.{}, 0));
    const malformed_digest = api.mkString("0000000000000000000000000000000000000000000000000000000000000000", 64);
    _ = api.send(base_digest_cell, api.intern("set:"), &.{malformed_digest}, 1);
    try std.testing.expectEqual(api.nilObj, api.send(codegen, api.intern("currentGenerationDigest"), &.{}, 0));
    _ = api.send(base_digest_cell, api.intern("set:"), &.{base_digest}, 1);
    _ = api.send(active_generation_cell, api.intern("set:"), &.{api.nilObj}, 1);
    const revision = api.send(entry, api.intern("at:"), &.{api.mkSmallInt(1)}, 1);
    const successor_unit = api.mkSmallInt(99);
    const successor = api.send(
        codegen,
        api.intern("successorGenerationProjection:replacing:with:unit:"),
        &.{ source_map, name, revision, successor_unit },
        4,
    );
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(api.send(successor, api.intern("size"), &.{}, 0)));
    const successor_map = api.send(successor, api.intern("at:"), &.{api.mkSmallInt(1)}, 1);
    const successor_entry = api.send(successor_map, api.intern("at:"), &.{api.mkSmallInt(0)}, 1);
    try std.testing.expectEqual(successor_unit, api.send(successor_entry, api.intern("at:"), &.{api.mkSmallInt(2)}, 1));
    const ordered_sources = api.send(codegen, api.intern("generationSources"), &.{}, 0);
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(ordered_sources, api.intern("size"), &.{}, 0)));
    try std.testing.expectEqual(source, api.send(ordered_sources, api.intern("at:"), &.{api.mkSmallInt(0)}, 1));
    const method_sources = api.send(codegen, api.intern("generationMethodSources"), &.{}, 0);
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(api.send(method_sources, api.intern("size"), &.{}, 0)));
    const globals = api.import("Globals") orelse return error.MissingGlobals;
    try std.testing.expectEqual(
        api.import("Reflect") orelse return error.MissingReflect,
        api.send(globals, api.intern("at:put:"), &.{ api.intern("Reflect"), api.import("Reflect") orelse return error.MissingReflect }, 2),
    );
    try std.testing.expectEqual(
        api.import("Integer") orelse return error.MissingInteger,
        api.send(globals, api.intern("at:put:"), &.{ api.intern("Integer"), api.import("Integer") orelse return error.MissingInteger }, 2),
    );
    try std.testing.expectEqual(
        api.nilObj,
        api.send(codegen, api.intern("replayMethods"), &.{}, 0),
    );
    const replay_alien_source = api.mkString("7", 1);
    const replay_alien_cell = api.send(
        store,
        api.intern("cellFor:attr:"),
        &.{ api.intern("__aliens__"), api.intern("__alien_7") },
        2,
    );
    _ = api.send(replay_alien_cell, api.intern("set:"), &.{replay_alien_source}, 1);
    const derived_global_name = api.intern("rustDerivedGlobal");
    const derived_global_source = api.mkString("43", 2);
    const derived_global_cell = api.send(
        store,
        api.intern("cellFor:attr:"),
        &.{ api.intern("__globalsrc__"), derived_global_name },
        2,
    );
    _ = api.send(derived_global_cell, api.intern("set:"), &.{derived_global_source}, 1);
    try std.testing.expectEqual(
        api.trueObj,
        api.send(codegen, api.intern("beginImageDispatchRecovery"), &.{}, 0),
    );
    try std.testing.expectEqual(
        @as(i64, 1),
        api.smallIntValue(api.send(codegen, api.intern("replayMethods"), &.{}, 0)),
    );
    try std.testing.expectEqual(
        @as(i64, 321),
        api.smallIntValue(api.send(api.mkSmallInt(7), api.intern("rustRegisteredAnswer"), &.{}, 0)),
    );
    try std.testing.expectEqual(
        api.trueObj,
        api.send(codegen, api.intern("endImageDispatchRecovery"), &.{}, 0),
    );
    try std.testing.expectEqual(
        api.trueObj,
        api.send(codegen, api.intern("beginImageDispatchRecovery"), &.{}, 0),
    );
    try std.testing.expectEqual(
        @as(i64, 1),
        api.smallIntValue(api.send(codegen, api.intern("replayAliens"), &.{}, 0)),
    );
    try std.testing.expectEqual(
        @as(i64, 1),
        api.smallIntValue(api.send(codegen, api.intern("deriveImageGlobals"), &.{}, 0)),
    );
    const derived_global = api.send(globals, api.intern("at:"), &.{derived_global_name}, 1);
    try std.testing.expect(api.isSmallInt(derived_global) != 0);
    try std.testing.expectEqual(@as(i64, 43), api.smallIntValue(derived_global));
    try std.testing.expectEqual(
        @as(i64, 0),
        api.smallIntValue(api.send(codegen, api.intern("deriveImageMethodTargets"), &.{}, 0)),
    );
    try std.testing.expectEqual(
        api.trueObj,
        api.send(codegen, api.intern("endImageDispatchRecovery"), &.{}, 0),
    );
    try std.testing.expectEqual(api.nilObj, api.send(codegen, api.intern("currentGenerationDigest"), &.{}, 0));
    var image_tmp = std.testing.tmpDir(.{});
    defer image_tmp.cleanup();
    const image_directory = try image_tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(image_directory);
    const image_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/librust_image_generation.dylib", .{image_directory});
    defer std.testing.allocator.free(image_path);
    const image_output = api.mkString(image_path.ptr, image_path.len);
    _ = api.send(active_generation_cell, api.intern("set:"), &.{generation}, 1);
    try std.testing.expectEqual(image_output, api.send(codegen, api.intern("compileImageCode:"), &.{image_output}, 1));
    const stale_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/librust_image_stale.dylib", .{image_directory});
    defer std.testing.allocator.free(stale_path);
    const stale_output = api.mkString(stale_path.ptr, stale_path.len);
    const wrong_digest = api.mkString("0000000000000000000000000000000000000000000000000000000000000000", 64);
    _ = api.send(base_digest_cell, api.intern("set:"), &.{wrong_digest}, 1);
    try std.testing.expectEqual(api.nilObj, api.send(codegen, api.intern("compileImageCode:"), &.{stale_output}, 1));
    _ = api.send(base_digest_cell, api.intern("set:"), &.{base_digest}, 1);
    _ = api.send(active_generation_cell, api.intern("set:"), &.{api.nilObj}, 1);
    const image_artifact = try std.fs.openFileAbsolute(image_path, .{});
    defer image_artifact.close();
    try std.testing.expect((try image_artifact.stat()).size > 0);
    const changed_text = "[Module named: #rustRegisteredModule requires: #[]]; 8";
    const changed_source = api.mkString(changed_text.ptr, changed_text.len);
    const changed_revision = api.send(codegen, api.intern("sourceRevisionOf:"), &.{changed_source}, 1);
    try std.testing.expectEqual(
        api.nilObj,
        api.send(
            codegen,
            api.intern("successorGenerationProjection:replacing:with:unit:"),
            &.{ source_map, name, changed_revision, successor_unit },
            4,
        ),
    );

    // A revision with one missing digest slot is not a valid lifecycle value,
    // even when its non-method digest and module identity match the current
    // revision. The projection must reject it before recomputing a successor.
    const vector = api.import("Vector") orelse return error.MissingVector;
    var malformed_revision = api.send(vector, api.intern("new"), &.{}, 0);
    for (0..7) |index| {
        const value = if (index == 5)
            api.nilObj
        else
            api.send(revision, api.intern("at:"), &.{api.mkSmallInt(@intCast(index))}, 1);
        malformed_revision = api.send(malformed_revision, api.intern("add:"), &.{value}, 1);
    }
    try std.testing.expectEqual(
        api.nilObj,
        api.send(
            codegen,
            api.intern("successorGenerationProjection:replacing:with:unit:"),
            &.{ source_map, name, malformed_revision, successor_unit },
            4,
        ),
    );

    // Duplicate names among untouched entries are invalid too; checking only
    // the replacement target would let a malformed generation digest through.
    const other_name = api.intern("rustOtherModule");
    const other_text = "[Module named: #rustOtherModule requires: #[]]; 9";
    const other_source = api.mkString(other_text.ptr, other_text.len);
    const other_revision = api.send(codegen, api.intern("sourceRevisionOf:"), &.{other_source}, 1);
    var other_entry = api.send(vector, api.intern("new"), &.{}, 0);
    other_entry = api.send(other_entry, api.intern("add:"), &.{other_name}, 1);
    other_entry = api.send(other_entry, api.intern("add:"), &.{other_revision}, 1);
    other_entry = api.send(other_entry, api.intern("add:"), &.{api.nilObj}, 1);
    var duplicate_map = api.send(vector, api.intern("new"), &.{}, 0);
    duplicate_map = api.send(duplicate_map, api.intern("add:"), &.{entry}, 1);
    duplicate_map = api.send(duplicate_map, api.intern("add:"), &.{other_entry}, 1);
    duplicate_map = api.send(duplicate_map, api.intern("add:"), &.{other_entry}, 1);
    try std.testing.expectEqual(
        api.nilObj,
        api.send(
            codegen,
            api.intern("successorGenerationProjection:replacing:with:unit:"),
            &.{ duplicate_map, name, revision, successor_unit },
            4,
        ),
    );

    const mismatch_name = api.intern("rustOtherModule");
    try std.testing.expectEqual(
        api.nilObj,
        api.send(codegen, api.intern("recordModuleSource:named:"), &.{ source, mismatch_name }, 2),
    );

    // Registration uses the same strict top-level scan as the revision
    // projection. A recursive metadata walk must not let a later duplicate
    // declaration enter the source registry.
    const duplicate_text = "[Module named: #rustDuplicateOne requires: #[]]; [Module named: #rustDuplicateTwo requires: #[]]";
    const duplicate_source = api.mkString(duplicate_text.ptr, duplicate_text.len);
    try std.testing.expectEqual(
        api.nilObj,
        api.send(
            codegen,
            api.intern("recordModuleSource:named:"),
            &.{ duplicate_source, api.intern("rustDuplicateTwo") },
            2,
        ),
    );
    try std.testing.expectEqual(api.trueObj, api.send(codegen, api.intern("sealInitialGeneration"), &.{}, 0));
    try std.testing.expectEqual(
        api.nilObj,
        api.send(codegen, api.intern("recordModuleSource:named:"), &.{ source, name }, 2),
    );
}

test "Rust Codegen inspects and maps a Zig candidate" {
    const api = try setup();
    const rust_codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const old_codegen = api.import("Codegen") orelse return error.MissingCodegen;
    const store = api.import("Store") orelse return error.MissingStore;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const path = try std.fmt.allocPrint(std.testing.allocator, "{s}/rust_candidate.dylib", .{directory});
    defer std.testing.allocator.free(path);
    const source_text = "[Module named: #rustCandidate requires: #[]]; [Reflect defineMethod: #rustCandidateAnswer on: Integer as: [:self | 123]]";
    const source = api.mkString(source_text.ptr, source_text.len);
    const output = api.mkString(path.ptr, path.len);
    const compiled = api.send(
        old_codegen,
        api.intern("compileCandidate:named:output:"),
        &.{ source, api.intern("rustCandidate"), output },
        3,
    );
    try std.testing.expect(compiled != api.nilObj);

    const unit = api.send(api.import("Object") orelse return error.MissingObject, api.intern("clone"), &.{}, 0);
    const inspected = api.send(
        rust_codegen,
        api.intern("inspectCandidate:into:"),
        &.{ output, unit },
        2,
    );
    try std.testing.expectEqual(unit, inspected);
    try std.testing.expectEqual(
        api.intern("candidateArtifact"),
        api.send(store, api.intern("currentValue:attr:"), &.{ unit, api.intern("kind") }, 2),
    );
    try std.testing.expectEqual(
        api.falseObj,
        api.send(store, api.intern("currentValue:attr:"), &.{ unit, api.intern("mapped") }, 2),
    );
    const mapped = api.send(rust_codegen, api.intern("mapCandidate:"), &.{unit}, 1);
    try std.testing.expectEqual(unit, mapped);
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("candidateMapped:"), &.{unit}, 1));
    const content_path = api.send(rust_codegen, api.intern("candidateContentPath:"), &.{unit}, 1);
    try std.testing.expect(api.vtableOf(content_path) == api.string_vt);
    try std.testing.expectEqual(
        api.trueObj,
        api.send(store, api.intern("currentValue:attr:"), &.{ unit, api.intern("mapped") }, 2),
    );
    const token = api.send(rust_codegen, api.intern("prepareActivation:"), &.{unit}, 1);
    try std.testing.expect(token != api.nilObj);
    const committed = api.send(rust_codegen, api.intern("commitPrepared:"), &.{token}, 1);
    try std.testing.expectEqual(unit, committed);
    try std.testing.expectEqual(
        api.intern("active"),
        api.send(store, api.intern("currentValue:attr:"), &.{ unit, api.intern("state") }, 2),
    );
    try std.testing.expectEqual(@as(i64, 123), api.smallIntValue(api.send(api.mkSmallInt(3), api.intern("rustCandidateAnswer"), &.{}, 0)));
    try std.testing.expectEqual(unit, api.send(rust_codegen, api.intern("reconcileCandidate:"), &.{unit}, 1));
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("reconcileAllCandidates"), &.{}, 0));

    // A successor may be prepared against the active predecessor. Activation
    // records the supersession ledger, and rolling the successor back restores
    // both the predecessor's Store state and its dispatch carrier.
    const successor_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/rust_candidate_successor.dylib", .{directory});
    defer std.testing.allocator.free(successor_path);
    const successor_text = "[Module named: #rustCandidateSuccessor requires: #[]]; [Reflect defineMethod: #rustCandidateAnswer on: Integer as: [:self | 456]]";
    const successor_source = api.mkString(successor_text.ptr, successor_text.len);
    const successor_output = api.mkString(successor_path.ptr, successor_path.len);
    try std.testing.expect(api.send(
        old_codegen,
        api.intern("compileCandidate:named:output:"),
        &.{ successor_source, api.intern("rustCandidateSuccessor"), successor_output },
        3,
    ) != api.nilObj);
    const successor_unit = api.send(api.import("Object") orelse return error.MissingObject, api.intern("clone"), &.{}, 0);
    try std.testing.expectEqual(successor_unit, api.send(
        rust_codegen,
        api.intern("inspectCandidate:into:"),
        &.{ successor_output, successor_unit },
        2,
    ));
    try std.testing.expectEqual(successor_unit, api.send(rust_codegen, api.intern("mapCandidate:"), &.{successor_unit}, 1));
    const vector = api.import("Vector") orelse return error.MissingVector;
    const object = api.import("Object") orelse return error.MissingObject;
    var successor_plan = api.send(vector, api.intern("new"), &.{}, 0);
    const successor_event = api.send(object, api.intern("clone"), &.{}, 0);
    const successor_basis = api.send(object, api.intern("clone"), &.{}, 0);
    const expected_generation = api.send(object, api.intern("clone"), &.{}, 0);
    const successor_generation = api.send(object, api.intern("clone"), &.{}, 0);
    for ([_]idapi.Oop{
        api.intern("activate"),
        successor_unit,
        api.intern("rustCandidateSuccessor"),
        unit,
        successor_event,
        successor_basis,
        expected_generation,
        successor_generation,
    }) |value| {
        successor_plan = api.send(successor_plan, api.intern("add:"), &.{value}, 1);
    }
    const successor_token = api.send(rust_codegen, api.intern("prepareActivation:plan:"), &.{ successor_unit, successor_plan }, 2);
    try std.testing.expect(successor_token != api.nilObj);
    try std.testing.expectEqual(successor_unit, api.send(rust_codegen, api.intern("commitPrepared:"), &.{successor_token}, 1));
    try std.testing.expectEqual(
        api.intern("active"),
        api.send(store, api.intern("currentValue:attr:"), &.{ successor_unit, api.intern("state") }, 2),
    );
    try std.testing.expectEqual(
        api.intern("superseded"),
        api.send(store, api.intern("currentValue:attr:"), &.{ unit, api.intern("state") }, 2),
    );
    try std.testing.expectEqual(@as(i64, 456), api.smallIntValue(api.send(api.mkSmallInt(3), api.intern("rustCandidateAnswer"), &.{}, 0)));

    var rollback_plan = api.send(vector, api.intern("new"), &.{}, 0);
    const rollback_event = api.send(object, api.intern("clone"), &.{}, 0);
    const rollback_basis = api.send(object, api.intern("clone"), &.{}, 0);
    const stale_expected_generation = api.send(object, api.intern("clone"), &.{}, 0);
    var stale_rollback_plan = api.send(vector, api.intern("new"), &.{}, 0);
    for ([_]idapi.Oop{
        api.intern("rollback"),
        successor_unit,
        api.intern("rustCandidateSuccessor"),
        unit,
        rollback_event,
        rollback_basis,
        stale_expected_generation,
        expected_generation,
    }) |value| {
        stale_rollback_plan = api.send(stale_rollback_plan, api.intern("add:"), &.{value}, 1);
    }
    try std.testing.expectEqual(
        api.nilObj,
        api.send(rust_codegen, api.intern("prepareRollback:plan:"), &.{ successor_unit, stale_rollback_plan }, 2),
    );
    for ([_]idapi.Oop{
        api.intern("rollback"),
        successor_unit,
        api.intern("rustCandidateSuccessor"),
        unit,
        rollback_event,
        rollback_basis,
        successor_generation,
        expected_generation,
    }) |value| {
        rollback_plan = api.send(rollback_plan, api.intern("add:"), &.{value}, 1);
    }
    const successor_rollback_token = api.send(rust_codegen, api.intern("prepareRollback:plan:"), &.{ successor_unit, rollback_plan }, 2);
    try std.testing.expect(successor_rollback_token != api.nilObj);
    try std.testing.expectEqual(successor_unit, api.send(rust_codegen, api.intern("commitPreparedRollback:"), &.{successor_rollback_token}, 1));
    try std.testing.expectEqual(
        api.intern("active"),
        api.send(store, api.intern("currentValue:attr:"), &.{ unit, api.intern("state") }, 2),
    );
    try std.testing.expectEqual(@as(i64, 123), api.smallIntValue(api.send(api.mkSmallInt(3), api.intern("rustCandidateAnswer"), &.{}, 0)));

    const rollback_token = api.send(rust_codegen, api.intern("prepareRollback:"), &.{unit}, 1);
    try std.testing.expect(rollback_token != api.nilObj);
    const rolled_back = api.send(rust_codegen, api.intern("commitPreparedRollback:"), &.{rollback_token}, 1);
    try std.testing.expectEqual(unit, rolled_back);
    try std.testing.expectEqual(
        api.intern("rolledBack"),
        api.send(store, api.intern("currentValue:attr:"), &.{ unit, api.intern("state") }, 2),
    );
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("enterRunning"), &.{}, 0));
    try std.testing.expectEqual(api.nilObj, api.send(rust_codegen, api.intern("authorizeManagedMethodMutation"), &.{}, 0));
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("beginQuiescence"), &.{}, 0));
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("authorizeManagedMethodMutation"), &.{}, 0));
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("endQuiescence"), &.{}, 0));
    try std.testing.expectEqual(api.nilObj, api.send(rust_codegen, api.intern("authorizeManagedMethodMutation"), &.{}, 0));
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("beginImageDispatchRecovery"), &.{}, 0));
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("authorizeManagedMethodMutation"), &.{}, 0));
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("endImageDispatchRecovery"), &.{}, 0));
    try std.testing.expectEqual(api.nilObj, api.send(rust_codegen, api.intern("authorizeManagedMethodMutation"), &.{}, 0));

    // Commit repeats the durable topology check. Prepare a canonical first
    // generation, mutate the successor predecessor after prepare, and prove
    // that no activation occurs for the structurally stale token.
    const lifecycle_revision = api.send(rust_codegen, api.intern("sourceRevisionOf:"), &.{source}, 1);
    var lifecycle_map = api.send(vector, api.intern("new"), &.{}, 0);
    var lifecycle_entry = api.send(vector, api.intern("new"), &.{}, 0);
    for ([_]idapi.Oop{ api.intern("rustCandidate"), lifecycle_revision, api.nilObj }) |value| {
        lifecycle_entry = api.send(lifecycle_entry, api.intern("add:"), &.{value}, 1);
    }
    lifecycle_map = api.send(lifecycle_map, api.intern("add:"), &.{lifecycle_entry}, 1);
    const expected_projection = api.send(
        rust_codegen,
        api.intern("successorGenerationProjection:replacing:with:unit:"),
        &.{ lifecycle_map, api.intern("rustCandidate"), lifecycle_revision, api.nilObj },
        4,
    );
    const successor_projection = api.send(
        rust_codegen,
        api.intern("successorGenerationProjection:replacing:with:unit:"),
        &.{ lifecycle_map, api.intern("rustCandidate"), lifecycle_revision, unit },
        4,
    );
    try std.testing.expect(expected_projection != api.nilObj);
    try std.testing.expect(successor_projection != api.nilObj);
    const durable_expected_generation = api.send(object, api.intern("clone"), &.{}, 0);
    const durable_successor_generation = api.send(object, api.intern("clone"), &.{}, 0);
    const expected_map = api.send(expected_projection, api.intern("at:"), &.{api.mkSmallInt(1)}, 1);
    const expected_digest = api.send(expected_projection, api.intern("at:"), &.{api.mkSmallInt(2)}, 1);
    const successor_map = api.send(successor_projection, api.intern("at:"), &.{api.mkSmallInt(1)}, 1);
    const successor_digest = api.send(successor_projection, api.intern("at:"), &.{api.mkSmallInt(2)}, 1);
    var fact_cell = api.send(store, api.intern("cellFor:attr:"), &.{ durable_expected_generation, api.intern("sourceMap") }, 2);
    _ = api.send(fact_cell, api.intern("set:"), &.{expected_map}, 1);
    fact_cell = api.send(store, api.intern("cellFor:attr:"), &.{ durable_expected_generation, api.intern("baseDigest") }, 2);
    _ = api.send(fact_cell, api.intern("set:"), &.{expected_digest}, 1);
    fact_cell = api.send(store, api.intern("cellFor:attr:"), &.{ durable_successor_generation, api.intern("sourceMap") }, 2);
    _ = api.send(fact_cell, api.intern("set:"), &.{successor_map}, 1);
    fact_cell = api.send(store, api.intern("cellFor:attr:"), &.{ durable_successor_generation, api.intern("baseDigest") }, 2);
    _ = api.send(fact_cell, api.intern("set:"), &.{successor_digest}, 1);
    fact_cell = api.send(store, api.intern("cellFor:attr:"), &.{ durable_successor_generation, api.intern("predecessor") }, 2);
    _ = api.send(fact_cell, api.intern("set:"), &.{durable_expected_generation}, 1);
    fact_cell = api.send(store, api.intern("cellFor:attr:"), &.{ durable_successor_generation, api.intern("deltaUnit") }, 2);
    _ = api.send(fact_cell, api.intern("set:"), &.{unit}, 1);
    const durable_cell = api.send(
        store,
        api.intern("cellFor:attr:"),
        &.{ api.intern("dispatch"), api.intern("activeGeneration") },
        2,
    );
    _ = api.send(durable_cell, api.intern("set:"), &.{durable_expected_generation}, 1);
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("beginQuiescence"), &.{}, 0));
    var stale_commit_plan = api.send(vector, api.intern("new"), &.{}, 0);
    for ([_]idapi.Oop{
        api.intern("activate"),
        unit,
        api.intern("rustCandidate"),
        api.nilObj,
        api.send(object, api.intern("clone"), &.{}, 0),
        api.send(object, api.intern("clone"), &.{}, 0),
        durable_expected_generation,
        durable_successor_generation,
    }) |value| {
        stale_commit_plan = api.send(stale_commit_plan, api.intern("add:"), &.{value}, 1);
    }
    const stale_token = api.send(rust_codegen, api.intern("prepareActivation:plan:"), &.{ unit, stale_commit_plan }, 2);
    try std.testing.expect(stale_token != api.nilObj);
    const bad_predecessor = api.send(object, api.intern("clone"), &.{}, 0);
    fact_cell = api.send(store, api.intern("cellFor:attr:"), &.{ durable_successor_generation, api.intern("predecessor") }, 2);
    _ = api.send(fact_cell, api.intern("set:"), &.{bad_predecessor}, 1);
    try std.testing.expectEqual(api.nilObj, api.send(rust_codegen, api.intern("commitPrepared:"), &.{stale_token}, 1));
    _ = api.send(durable_cell, api.intern("set:"), &.{api.nilObj}, 1);
    try std.testing.expectEqual(api.trueObj, api.send(rust_codegen, api.intern("endQuiescence"), &.{}, 0));
}

test "Rust and Zig Codegen artifacts cross-load through the frozen ABI" {
    const api = try setup();
    const rust_codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const old_codegen = api.import("Codegen") orelse return error.MissingCodegen;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const zig_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/libbirnam_cross_zig.dylib", .{directory});
    defer std.testing.allocator.free(zig_path);
    const rust_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/libbirnam_cross_rust.dylib", .{directory});
    defer std.testing.allocator.free(rust_path);
    const source = api.mkString("7", 1);
    const zig_output = api.mkString(zig_path.ptr, zig_path.len);
    const rust_output = api.mkString(rust_path.ptr, rust_path.len);
    try std.testing.expectEqual(zig_output, api.send(old_codegen, api.intern("compile:output:"), &.{ source, zig_output }, 2));
    const zig_loaded_by_zig = api.send(old_codegen, api.intern("load:"), &.{zig_output}, 1);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(zig_loaded_by_zig));
    const rust_loaded_zig = api.send(rust_codegen, api.intern("load:"), &.{zig_output}, 1);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(rust_loaded_zig));
    try std.testing.expectEqual(rust_output, api.send(rust_codegen, api.intern("compile:output:"), &.{ source, rust_output }, 2));
    const zig_loaded_rust = api.send(old_codegen, api.intern("load:"), &.{rust_output}, 1);
    try std.testing.expectEqual(@as(i64, 7), api.smallIntValue(zig_loaded_rust));
}
