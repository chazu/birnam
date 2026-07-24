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
    try tmp.dir.deleteFile("libnative.dylib");
    const loaded = api.send(codegen, api.intern("load:"), &.{output}, 1);
    try std.testing.expectEqual(api.nilObj, loaded);
    const diagnostic = api.send(codegen, api.intern("lastError"), &.{}, 0);
    var diagnostic_len: usize = 0;
    const diagnostic_bytes = api.stringBytes(diagnostic, &diagnostic_len);
    try std.testing.expect(std.mem.indexOf(u8, diagnostic_bytes[0..diagnostic_len], "native requirement") != null);
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

test "Rust Codegen retains registered module sources across collection" {
    const api = try setup();
    const codegen = api.import("RustCodegen") orelse return error.MissingRustCodegen;
    const name = api.intern("rustRegisteredModule");
    const source_text = "[Module named: #rustRegisteredModule requires: #[]]; 7";
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
    try std.testing.expectEqual(api.trueObj, api.send(codegen, api.intern("sealInitialGeneration"), &.{}, 0));
    try std.testing.expectEqual(
        api.nilObj,
        api.send(codegen, api.intern("recordModuleSource:named:"), &.{ source, name }, 2),
    );
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
