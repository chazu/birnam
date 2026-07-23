//! Module-metadata and compiled-unit descriptor parity through Rust PEG.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const artifact_inspector = @import("artifact_inspector.zig");
const store = @import("store.zig");
const peg = @import("peg.zig");
const lang = @import("lang.zig");
const tcc = @import("tcc.zig");
const codegen = @import("codegen.zig");
const prelude = @import("prelude.zig");
const collection = @import("collection.zig");
const err_mod = @import("error.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;

fn bringPeg(api: *idapi.IdApi) !void {
    if (std.posix.getenv("BIRNAM_RUST_PEG_DYLIB")) |path| {
        const path_z = try std.heap.c_allocator.dupeZ(u8, path);
        defer std.heap.c_allocator.free(path_z);
        const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustPegDlopenFailed;
        const raw_init = api.dlsym(handle, "module_init") orelse return error.RustPegModuleInitMissing;
        const init: ModuleInit = @ptrCast(@alignCast(raw_init));
        _ = init(api, null);
    } else {
        peg.bringUp(api);
    }
}

fn setup() *idapi.IdApi {
    const api = floor.api();
    artifact_inspector.bringUp(api);
    store.bringUp(api);
    bringPeg(api) catch @panic("Rust PEG failed to load");
    lang.bringUp(api);
    tcc.bringUp(api);
    codegen.bringUp(api);
    prelude.bringUp(api);
    collection.bringUp(api);
    err_mod.bringUp(api);
    return api;
}

fn send0(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8) Oop {
    return api.send(receiver, api.intern(selector), &.{}, 0);
}

fn send1(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8, arg: Oop) Oop {
    return api.send(receiver, api.intern(selector), &.{arg}, 1);
}

fn send2(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8, a: Oop, b: Oop) Oop {
    return api.send(receiver, api.intern(selector), &.{ a, b }, 2);
}

fn vectorAt(api: *idapi.IdApi, vector: Oop, index: i64) Oop {
    return send1(api, vector, "at:", api.mkSmallInt(index));
}

fn stringBytes(api: *idapi.IdApi, string: Oop) []const u8 {
    var len: usize = 0;
    const bytes = api.stringBytes(string, &len);
    return bytes[0..len];
}

test "Rust PEG preserves module metadata and compiled-unit descriptors" {
    const api = setup();
    const Codegen = api.import("Codegen") orelse return error.CodegenNotExported;

    const metadata_source =
        "// prose first\n" ++
        "[Globals at: #MetadataProbe put: [Object clone]];\n" ++
        "[Module named: #metadataProbe requires: #[#file #time]];\n";
    const metadata = send1(api, Codegen, "moduleMetadataOf:", api.mkString(metadata_source.ptr, metadata_source.len));
    try std.testing.expect(metadata != api.nilObj);
    try std.testing.expectEqual(api.intern("metadataProbe"), vectorAt(api, metadata, 1));
    const metadata_requires = vectorAt(api, metadata, 2);
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(send0(api, metadata_requires, "size")));
    try std.testing.expectEqual(api.intern("file"), vectorAt(api, metadata_requires, 0));
    try std.testing.expectEqual(api.intern("time"), vectorAt(api, metadata_requires, 1));

    const legacy = "[Globals at: #LegacyProbe put: [Object clone]];\n";
    try std.testing.expectEqual(
        api.nilObj,
        send1(api, Codegen, "moduleMetadataOf:", api.mkString(legacy.ptr, legacy.len)),
    );

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const dir = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(dir);
    const path = try std.fs.path.join(std.testing.allocator, &.{ dir, "libbirnam_described.dylib" });
    defer std.testing.allocator.free(path);
    const source =
        "[Module named: #described requires: #[#file #time]]; " ++
        "[Globals at: #descriptorMagicFixture put: \"BIRNAM-ARTIFACT-DESCRIPTOR-V1\"]; " ++
        "[Reflect defineMethod: #descriptorProbe on: Integer as: [:self | 42]]";
    const compiled = send2(api, Codegen, "compile:output:", api.mkString(source.ptr, source.len), api.mkString(path.ptr, path.len));
    try std.testing.expect(compiled != api.nilObj);

    const integer_vt = api.vtableOf(api.mkSmallInt(0));
    try std.testing.expectEqual(
        @as(Oop, null),
        api.send(integer_vt, api.intern("lookup"), &.{api.intern("descriptorProbe")}, 1),
    );

    const Artifact = api.import("Artifact") orelse return error.ArtifactNotExported;
    const descriptor = send1(api, Artifact, "inspect:", api.mkString(path.ptr, path.len));
    try std.testing.expect(descriptor != api.nilObj);
    try std.testing.expectEqual(api.intern("module"), send0(api, descriptor, "kind"));
    try std.testing.expectEqual(api.intern("described"), send0(api, descriptor, "identity"));
    const requires = send0(api, descriptor, "requires");
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(send0(api, requires, "size")));
    try std.testing.expectEqual(api.intern("file"), vectorAt(api, requires, 0));
    try std.testing.expectEqual(api.intern("time"), vectorAt(api, requires, 1));
    const roots = send0(api, descriptor, "roots");
    try std.testing.expectEqual(api.intern("described"), vectorAt(api, roots, 0));
    try std.testing.expectEqualStrings("aarch64-macos", stringBytes(api, send0(api, descriptor, "target")));
    try std.testing.expectEqual(@as(Oop, null), api.send(integer_vt, api.intern("lookup"), &.{api.intern("descriptorProbe")}, 1));
}
