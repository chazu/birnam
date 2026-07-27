//! Selected-floor tests for the host's pre-activation artifact policy. The
//! fixtures are inert byte containers; no candidate library is ever dlopened.

const std = @import("std");
const artifact_contract = @import("artifact.zig");
const artifact_preflight = @import("artifact_preflight.zig");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");
const rung_adapter = @import("rung_adapter_test.zig");

const Oop = idapi.Oop;
const shared_fixture = @embedFile("artifact-preflight-v1.txt");

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
    return rung_adapter.loadSelected(api, "artifact-inspector");
}

fn hexNibble(value: u8) !u8 {
    return switch (value) {
        '0'...'9' => value - '0',
        'a'...'f' => value - 'a' + 10,
        'A'...'F' => value - 'A' + 10,
        else => error.InvalidSharedFixture,
    };
}

fn decodeSharedHex(allocator: std.mem.Allocator, encoded: []const u8) ![]u8 {
    if (encoded.len % 2 != 0) return error.InvalidSharedFixture;
    const bytes = try allocator.alloc(u8, encoded.len / 2);
    errdefer allocator.free(bytes);
    for (0..bytes.len) |index| {
        bytes[index] = (try hexNibble(encoded[index * 2])) * 16 +
            try hexNibble(encoded[index * 2 + 1]);
    }
    return bytes;
}

fn writeSharedCase(
    allocator: std.mem.Allocator,
    dir: std.fs.Dir,
    case_name: []const u8,
) !void {
    var lines = std.mem.tokenizeScalar(u8, shared_fixture, '\n');
    var found = false;
    while (lines.next()) |raw_line| {
        const line = std.mem.trim(u8, raw_line, " \t\r");
        if (line.len == 0 or line[0] == '#') continue;
        const separator = std.mem.indexOfScalar(u8, line, '|') orelse return error.InvalidSharedFixture;
        const key = line[0..separator];
        const encoded = line[separator + 1 ..];
        const slash = std.mem.indexOfScalar(u8, key, '/') orelse return error.InvalidSharedFixture;
        if (!std.mem.eql(u8, key[0..slash], case_name)) continue;
        found = true;
        const filename = key[slash + 1 ..];
        const bytes = try decodeSharedHex(allocator, encoded);
        defer allocator.free(bytes);
        if (std.mem.eql(u8, case_name, "sentinel") and std.mem.eql(u8, filename, "root.dylib")) {
            const sentinel_path = std.posix.getenv("BIRNAM_PREFLIGHT_SENTINEL_DYLIB") orelse
                @panic("shared sentinel case requires a real sentinel dylib");
            const dylib = try std.fs.cwd().readFileAlloc(allocator, sentinel_path, 1 << 30);
            defer allocator.free(dylib);
            const combined = try allocator.alloc(u8, dylib.len + bytes.len);
            defer allocator.free(combined);
            @memcpy(combined[0..dylib.len], dylib);
            @memcpy(combined[dylib.len..], bytes);
            try dir.writeFile(.{ .sub_path = filename, .data = combined });
        } else {
            try dir.writeFile(.{ .sub_path = filename, .data = bytes });
        }
    }
    if (!found) return error.MissingSharedFixture;
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

test "real aborting artifact initializer is unreachable during preflight" {
    const api = setup();
    const sentinel_path = std.posix.getenv("BIRNAM_PREFLIGHT_SENTINEL_DYLIB") orelse
        @panic("BIRNAM_PREFLIGHT_SENTINEL_DYLIB must name the real sentinel fixture");
    const dylib = try std.fs.cwd().readFileAlloc(std.testing.allocator, sentinel_path, 1 << 30);
    defer std.testing.allocator.free(dylib);
    const descriptor = try descriptorBytes("module", "sentinel", &.{});
    defer std.testing.allocator.free(descriptor);

    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var candidate = try tmp.dir.createFile("candidate.dylib", .{});
    try candidate.writeAll(dylib);
    try candidate.writeAll(descriptor);
    candidate.close();

    const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
    defer std.testing.allocator.free(directory);
    const description = try inspect(api, directory, "candidate.dylib");
    var preflight = artifact_preflight.Preflight.init(std.testing.allocator, api, api.import("Artifact").?, directory);
    defer preflight.deinit();
    try preflight.walkRoot(description, api.intern("sentinel"));

    // Reaching here proves neither exported aborting initializer ran. No higher
    // rung was resolved as an incidental consequence of inspecting the dylib.
    try std.testing.expect(api.import("Store") == null);
    try std.testing.expect(api.import("Codegen") == null);
    try std.testing.expect(api.import("Module") == null);
}

test "shared fixture matrix matches Rust preflight cases" {
    const api = setup();
    const cases = [_][]const u8{
        "path-only-root",
        "supplied-identity-mismatch",
        "wrong-kind-before-walk",
        "nested-wrong-kind",
        "sibling-identity-mismatch",
        "missing-sibling",
        "ordered-valid-closure",
        "sentinel",
    };
    for (cases) |case_name| {
        var tmp = std.testing.tmpDir(.{});
        defer tmp.cleanup();
        try writeSharedCase(std.testing.allocator, tmp.dir, case_name);
        try tmp.dir.writeFile(.{ .sub_path = "existing-target", .data = "prior-publication" });
        const directory = try tmp.dir.realpathAlloc(std.testing.allocator, ".");
        defer std.testing.allocator.free(directory);
        const description = try inspect(api, directory, "root.dylib");
        var preflight = artifact_preflight.Preflight.init(
            std.testing.allocator,
            api,
            api.import("Artifact").?,
            directory,
        );
        defer preflight.deinit();
        if (std.mem.eql(u8, case_name, "path-only-root")) {
            try preflight.walkRoot(description, null);
        } else if (std.mem.eql(u8, case_name, "supplied-identity-mismatch")) {
            try std.testing.expectError(
                error.ArtifactIdentityMismatch,
                preflight.walkRoot(description, api.intern("expectedIdentity")),
            );
        } else if (std.mem.eql(u8, case_name, "wrong-kind-before-walk")) {
            try std.testing.expectError(
                error.ArtifactKindMismatch,
                preflight.walkRoot(description, api.intern("candidate")),
            );
        } else if (std.mem.eql(u8, case_name, "nested-wrong-kind")) {
            try std.testing.expectError(
                error.ArtifactKindMismatch,
                preflight.walkRoot(description, api.intern("root")),
            );
        } else if (std.mem.eql(u8, case_name, "sibling-identity-mismatch")) {
            try std.testing.expectError(error.ArtifactIdentityMismatch, preflight.walkRoot(description, null));
        } else if (std.mem.eql(u8, case_name, "missing-sibling")) {
            try std.testing.expectError(error.ArtifactDependencyInvalid, preflight.walkRoot(description, null));
            const target = try tmp.dir.readFileAlloc(std.testing.allocator, "existing-target", 1024);
            defer std.testing.allocator.free(target);
            try std.testing.expectEqualStrings("prior-publication", target);
            try std.testing.expect(api.import("Store") == null);
            try std.testing.expect(api.import("Module") == null);
        } else if (std.mem.eql(u8, case_name, "ordered-valid-closure")) {
            try preflight.walkRoot(description, api.intern("root"));
        } else if (std.mem.eql(u8, case_name, "sentinel")) {
            try preflight.walkRoot(description, api.intern("sentinel"));
        } else unreachable;
    }
}
