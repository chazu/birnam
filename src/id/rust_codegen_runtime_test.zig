//! Exercise the explicit Rust Codegen emitter through its real IdApi boundary.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
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
    try load(api, "BIRNAM_RUST_PEG_DYLIB");
    lang.bringUp(api);
    try load(api, "BIRNAM_RUST_TCC_DYLIB");
    try load(api, "BIRNAM_RUST_STORE_DYLIB");
    try load(api, "BIRNAM_RUST_CODEGEN_DYLIB");
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
