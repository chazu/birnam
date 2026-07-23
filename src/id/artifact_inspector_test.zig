const std = @import("std");
const artifact_contract = @import("artifact.zig");
const idapi = @import("idapi.zig");
const rung_adapter = @import("rung_adapter_test.zig");

const Oop = idapi.Oop;

fn descriptorField(bytes: *std.ArrayList(u8), value: []const u8) !void {
    var encoded: [4]u8 = undefined;
    std.mem.writeInt(u32, &encoded, @intCast(value.len), .little);
    try bytes.appendSlice(std.testing.allocator, &encoded);
    try bytes.appendSlice(std.testing.allocator, value);
}

fn descriptorList(bytes: *std.ArrayList(u8), values: []const []const u8) !void {
    var encoded: [4]u8 = undefined;
    std.mem.writeInt(u32, &encoded, @intCast(values.len), .little);
    try bytes.appendSlice(std.testing.allocator, &encoded);
    for (values) |value| try descriptorField(bytes, value);
}

fn descriptorBytes(
    kind: []const u8,
    identity: []const u8,
    artifact_abi: []const u8,
    target: []const u8,
    native_requires: []const []const u8,
) ![]u8 {
    var payload: std.ArrayList(u8) = .{};
    defer payload.deinit(std.testing.allocator);
    try descriptorField(&payload, kind);
    try descriptorField(&payload, identity);
    try descriptorField(&payload, artifact_abi);
    try descriptorField(&payload, target);
    try descriptorField(&payload, "source-digest");
    try descriptorField(&payload, "revision-digest");
    try descriptorList(&payload, &.{});
    try descriptorList(&payload, native_requires);
    try descriptorList(&payload, &.{"activate"});

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

fn testApi() *idapi.IdApi {
    const api = @import("floor_adapter_test.zig").api();
    return rung_adapter.loadSelected(api, "artifact-inspector");
}

fn inspectFixture(api: *idapi.IdApi, bytes: []const u8) !Oop {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try tmp.dir.writeFile(.{ .sub_path = "candidate.dylib", .data = bytes });
    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const path = try std.fs.path.join(std.testing.allocator, &.{ directory, "candidate.dylib" });
    defer std.testing.allocator.free(path);
    return api.send(api.import("Artifact").?, api.intern("inspect:"), &.{api.mkString(path.ptr, path.len)}, 1);
}

test "pointer-free artifact inspection needs only the selected ID floor" {
    const api = testApi();
    try std.testing.expect(api.import("Store") == null);
    try std.testing.expect(api.import("Codegen") == null);
    const bytes = try descriptorBytes("module", "candidate", artifact_contract.abi, artifact_contract.target, &.{});
    defer std.testing.allocator.free(bytes);

    const inspected = try inspectFixture(api, bytes);
    try std.testing.expect(inspected != api.nilObj);
    try std.testing.expect(api.send(inspected, api.intern("kind"), &.{}, 0) == api.intern("module"));
    try std.testing.expect(api.send(inspected, api.intern("identity"), &.{}, 0) == api.intern("candidate"));
}

test "pointer-free artifact inspection rejects ABI mismatch" {
    const api = testApi();
    const bytes = try descriptorBytes("module", "candidate", "wrong-abi", artifact_contract.target, &.{});
    defer std.testing.allocator.free(bytes);
    try std.testing.expect(try inspectFixture(api, bytes) == api.nilObj);
    try std.testing.expect(api.send(api.import("Artifact").?, api.intern("lastFailure"), &.{}, 0) == api.intern("artifactAbiMismatch"));
}

test "pointer-free artifact inspection rejects target mismatch" {
    const api = testApi();
    const bytes = try descriptorBytes("module", "candidate", artifact_contract.abi, "wrong-target", &.{});
    defer std.testing.allocator.free(bytes);
    try std.testing.expect(try inspectFixture(api, bytes) == api.nilObj);
    try std.testing.expect(api.send(api.import("Artifact").?, api.intern("lastFailure"), &.{}, 0) == api.intern("artifactTargetMismatch"));
}

test "pointer-free artifact inspection rejects unavailable native requirement" {
    const api = testApi();
    const bytes = try descriptorBytes("module", "candidate", artifact_contract.abi, artifact_contract.target, &.{"libdefinitely_missing.dylib"});
    defer std.testing.allocator.free(bytes);
    try std.testing.expect(try inspectFixture(api, bytes) == api.nilObj);
    try std.testing.expect(api.send(api.import("Artifact").?, api.intern("lastFailure"), &.{}, 0) == api.intern("nativeRequirementUnavailable"));
}
