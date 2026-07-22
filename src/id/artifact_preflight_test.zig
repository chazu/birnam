//! Selected-floor tests for the host's pre-activation artifact policy. The
//! fixtures are inert byte containers; no candidate library is ever dlopened.

const std = @import("std");
const artifact_contract = @import("artifact.zig");
const artifact_inspector = @import("artifact_inspector.zig");
const artifact_preflight = @import("artifact_preflight.zig");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

fn appendField(bytes: *std.ArrayList(u8), value: []const u8) !void {
    var encoded: [4]u8 = undefined;
    std.mem.writeInt(u32, &encoded, @intCast(value.len), .little);
    try bytes.appendSlice(std.testing.allocator, &encoded);
    try bytes.appendSlice(std.testing.allocator, value);
}

fn appendList(bytes: *std.ArrayList(u8), values: []const []const u8) !void {
    var encoded: [4]u8 = undefined;
    std.mem.writeInt(u32, &encoded, @intCast(values.len), .little);
    try bytes.appendSlice(std.testing.allocator, &encoded);
    for (values) |value| try appendField(bytes, value);
}

fn descriptorBytes(kind: []const u8, identity: []const u8, requires: []const []const u8) ![]u8 {
    var payload: std.ArrayList(u8) = .{};
    defer payload.deinit(std.testing.allocator);
    try appendField(&payload, kind);
    try appendField(&payload, identity);
    try appendField(&payload, artifact_contract.abi);
    try appendField(&payload, artifact_contract.target);
    try appendField(&payload, "source-digest");
    try appendField(&payload, "revision-digest");
    try appendList(&payload, requires);
    try appendList(&payload, &.{});
    try appendList(&payload, &.{"activate"});

    var file: std.ArrayList(u8) = .{};
    errdefer file.deinit(std.testing.allocator);
    try file.appendSlice(std.testing.allocator, "sentinel initializer: MUST NOT EXECUTE\n");
    try file.appendSlice(std.testing.allocator, artifact_contract.descriptor_magic);
    var encoded: [4]u8 = undefined;
    std.mem.writeInt(u32, &encoded, @intCast(payload.items.len), .little);
    try file.appendSlice(std.testing.allocator, &encoded);
    try file.appendSlice(std.testing.allocator, payload.items);
    return file.toOwnedSlice(std.testing.allocator);
}

fn writeDescriptor(dir: std.fs.Dir, filename: []const u8, kind: []const u8, identity: []const u8, requires: []const []const u8) !void {
    const bytes = try descriptorBytes(kind, identity, requires);
    defer std.testing.allocator.free(bytes);
    try dir.writeFile(.{ .sub_path = filename, .data = bytes });
}

fn inspect(api: *idapi.IdApi, directory: []const u8, filename: []const u8) !Oop {
    const path = try std.fs.path.join(std.testing.allocator, &.{ directory, filename });
    defer std.testing.allocator.free(path);
    return api.send(api.import("Artifact").?, api.intern("inspect:"), &.{api.mkString(path.ptr, path.len)}, 1);
}

fn setup() *idapi.IdApi {
    const api = floor.api();
    artifact_inspector.bringUp(api);
    return api;
}

test "path-only root preflight does not invent identity from filename" {
    const api = setup();
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try writeDescriptor(tmp.dir, "unrelated-filename.dylib", "module", "actualIdentity", &.{});
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const description = try inspect(api, directory, "unrelated-filename.dylib");
    var preflight = artifact_preflight.Preflight.init(std.testing.allocator, api, api.import("Artifact").?, directory);
    defer preflight.deinit();
    try preflight.walkRoot(description, null);
}

test "operation-supplied root identity is enforced" {
    const api = setup();
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try writeDescriptor(tmp.dir, "candidate.dylib", "module", "actualIdentity", &.{});
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const description = try inspect(api, directory, "candidate.dylib");
    var preflight = artifact_preflight.Preflight.init(std.testing.allocator, api, api.import("Artifact").?, directory);
    defer preflight.deinit();
    try std.testing.expectError(error.ArtifactIdentityMismatch, preflight.walkRoot(description, api.intern("expectedIdentity")));
}

test "preflight rejects wrong descriptor kind" {
    const api = setup();
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try writeDescriptor(tmp.dir, "candidate.dylib", "image", "candidate", &.{});
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const description = try inspect(api, directory, "candidate.dylib");
    var preflight = artifact_preflight.Preflight.init(std.testing.allocator, api, api.import("Artifact").?, directory);
    defer preflight.deinit();
    try std.testing.expectError(error.ArtifactKindMismatch, preflight.walkRoot(description, null));
}

test "preflight rejects dependency identity mismatch" {
    const api = setup();
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try writeDescriptor(tmp.dir, "root.dylib", "module", "root", &.{"dependency"});
    try writeDescriptor(tmp.dir, "libbirnam_dependency.dylib", "module", "impostor", &.{});
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const description = try inspect(api, directory, "root.dylib");
    var preflight = artifact_preflight.Preflight.init(std.testing.allocator, api, api.import("Artifact").?, directory);
    defer preflight.deinit();
    try std.testing.expectError(error.ArtifactIdentityMismatch, preflight.walkRoot(description, null));
}

test "preflight rejects incomplete dependency closure" {
    const api = setup();
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try writeDescriptor(tmp.dir, "root.dylib", "module", "root", &.{"missing"});
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const description = try inspect(api, directory, "root.dylib");
    var preflight = artifact_preflight.Preflight.init(std.testing.allocator, api, api.import("Artifact").?, directory);
    defer preflight.deinit();
    try std.testing.expectError(error.ArtifactDependencyInvalid, preflight.walkRoot(description, null));
}

test "compatible closure reaches activation boundary without higher rungs" {
    const api = setup();
    try std.testing.expect(api.import("Store") == null);
    try std.testing.expect(api.import("Codegen") == null);
    try std.testing.expect(api.import("Module") == null);
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try writeDescriptor(tmp.dir, "root.dylib", "module", "root", &.{"dependency"});
    try writeDescriptor(tmp.dir, "libbirnam_dependency.dylib", "module", "dependency", &.{});
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const description = try inspect(api, directory, "root.dylib");
    var preflight = artifact_preflight.Preflight.init(std.testing.allocator, api, api.import("Artifact").?, directory);
    defer preflight.deinit();
    try preflight.walkRoot(description, api.intern("root"));
    try std.testing.expect(api.import("Store") == null);
    try std.testing.expect(api.import("Codegen") == null);
    try std.testing.expect(api.import("Module") == null);
}
