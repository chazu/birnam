//! Materialized source-project builds for `birnam build`.
//!
//! This is deliberately a host-side layer over project.Resolution. It compiles
//! the already-resolved source modules in dependency-first order, records the
//! identity of every resulting dylib, and publishes a complete build directory
//! only after all artifacts and manifests are present.

const builtin = @import("builtin");
const std = @import("std");
const artifact_contract = @import("artifact.zig");
const idapi = @import("idapi.zig");
const project = @import("project.zig");

const IdApi = idapi.IdApi;
const Oop = idapi.Oop;

pub const artifact_abi = artifact_contract.abi;
pub const profile = "debug";

const max_source_bytes = 16 << 20;
const FailureError = error{BuildFailed};

pub const Diagnostic = struct {
    kind: []const u8,
    message: []const u8,
};

pub const Artifact = struct {
    name: []const u8,
    relative_path: []const u8,
    source_path: []const u8,
    source_digest: []const u8,
    revision_digest: []const u8,
    requires: []const []const u8,
    checksum: []const u8,
    action: Action,
};

pub const NativeArtifact = struct {
    name: []const u8,
    relative_path: []const u8,
    checksum: []const u8,
    action: Action,
};

pub const Action = enum { compiled, reused };

const PreviousArtifact = struct {
    name: []const u8,
    relative_path: []const u8,
    source_path: []const u8,
    source_digest: []const u8,
    revision_digest: []const u8,
    requires: []const []const u8,
    checksum: []const u8,
};

const PreviousNativeArtifact = struct {
    name: []const u8,
    relative_path: []const u8,
    checksum: []const u8,
};

const PreviousBuild = struct {
    project_root: []const u8,
    target: []const u8,
    profile: []const u8,
    tool_version: []const u8,
    artifact_abi: []const u8,
    resolution_digest: []const u8,
    built_at: []const u8,
    artifacts: []const PreviousArtifact,
    native: []const PreviousNativeArtifact,
};

pub const Build = struct {
    project_root: []const u8,
    output_root: []const u8,
    target: []const u8,
    profile: []const u8,
    tool_version: []const u8,
    built_at: []const u8,
    resolution_digest: []const u8,
    artifacts: []const Artifact,
    native: []const NativeArtifact,
};

pub const Result = struct {
    arena: std.heap.ArenaAllocator,
    build: ?Build,
    diagnostic: ?Diagnostic,

    pub fn deinit(self: *Result) void {
        self.arena.deinit();
    }
};

const State = struct {
    arena: std.mem.Allocator,
    api: *IdApi,
    codegen: ?Oop = null,
    diagnostic_kind: []const u8 = "buildFailed",
    diagnostic_message: []const u8 = "project build failed",

    fn fail(self: *State, kind: []const u8, comptime fmt: []const u8, args: anytype) FailureError {
        self.diagnostic_kind = kind;
        self.diagnostic_message = std.fmt.allocPrint(self.arena, fmt, args) catch {
            self.diagnostic_kind = "buildFailed";
            self.diagnostic_message = "could not format build diagnostic";
            return FailureError.BuildFailed;
        };
        return FailureError.BuildFailed;
    }
};

fn put(state: *State, out: *std.ArrayList(u8), bytes: []const u8) FailureError!void {
    out.appendSlice(state.arena, bytes) catch
        return state.fail("buildFailed", "out of memory while writing build metadata", .{});
}

fn pathJoin(state: *State, left: []const u8, right: []const u8) FailureError![]const u8 {
    return std.fs.path.join(state.arena, &.{ left, right }) catch
        return state.fail("buildFailed", "out of memory while joining build paths", .{});
}

fn pathExists(path: []const u8) bool {
    std.fs.cwd().access(path, .{}) catch return false;
    return true;
}

fn makePath(state: *State, path: []const u8) FailureError!void {
    std.fs.cwd().makePath(path) catch |err|
        return state.fail("buildPublicationFailed", "could not create build directory {s}: {s}", .{ path, @errorName(err) });
}

fn duplicateStrings(state: *State, values: []const []const u8) FailureError![]const []const u8 {
    const result = state.arena.alloc([]const u8, values.len) catch
        return state.fail("buildFailed", "out of memory while recording module dependencies", .{});
    for (values, 0..) |value, i| {
        result[i] = state.arena.dupe(u8, value) catch
            return state.fail("buildFailed", "out of memory while recording module dependencies", .{});
    }
    return result;
}

fn isString(state: *State, value: Oop) bool {
    return value != null and state.api.vtableOf(value) == state.api.string_vt;
}

fn isVector(state: *State, value: Oop) bool {
    const vt = state.api.import("VectorVt") orelse return false;
    return value != null and state.api.vtableOf(value) == vt;
}

fn isMap(state: *State, value: Oop) bool {
    const vt = state.api.import("MapVt") orelse return false;
    return value != null and state.api.vtableOf(value) == vt;
}

fn isError(state: *State, value: Oop) bool {
    return value != null and state.api.isExported("Error") != 0 and
        state.api.send(value, state.api.intern("isError"), &.{}, 0) == state.api.trueObj;
}

fn global(state: *State, name: [:0]const u8) ?Oop {
    const globals = state.api.import("Globals") orelse return null;
    const value = state.api.send(globals, state.api.intern("at:"), &.{state.api.intern(name)}, 1);
    return if (value == state.api.nilObj) null else value;
}

fn stringBytes(state: *State, value: Oop) []const u8 {
    var length: usize = 0;
    const bytes = state.api.stringBytes(value, &length);
    return bytes[0..length];
}

fn mapAt(state: *State, map: Oop, key: []const u8) FailureError!Oop {
    if (!isMap(state, map)) return state.fail("buildRecordMalformed", "expected a TOML table while reading {s}", .{key});
    return state.api.send(map, state.api.intern("at:"), &.{state.api.mkString(key.ptr, key.len)}, 1);
}

fn vectorSize(state: *State, vector: Oop, field: []const u8) FailureError!usize {
    if (!isVector(state, vector)) return state.fail("buildRecordMalformed", "{s} must be an array", .{field});
    const size = state.api.send(vector, state.api.intern("size"), &.{}, 0);
    if (state.api.isSmallInt(size) == 0 or state.api.smallIntValue(size) < 0)
        return state.fail("buildRecordMalformed", "could not read array {s}", .{field});
    return @intCast(state.api.smallIntValue(size));
}

fn recordString(state: *State, value: Oop, field: []const u8) FailureError![]const u8 {
    if (!isString(state, value)) return state.fail("buildRecordMalformed", "{s} must be a string", .{field});
    return state.arena.dupe(u8, stringBytes(state, value)) catch
        return state.fail("buildFailed", "out of memory while reading build record", .{});
}

fn recordStringList(state: *State, value: Oop, field: []const u8) FailureError![]const []const u8 {
    const count = try vectorSize(state, value, field);
    const values = state.arena.alloc([]const u8, count) catch
        return state.fail("buildFailed", "out of memory while reading build record", .{});
    for (0..count) |i| {
        const item = state.api.send(value, state.api.intern("at:"), &.{state.api.mkSmallInt(@intCast(i))}, 1);
        values[i] = try recordString(state, item, field);
    }
    return values;
}

fn parsePreviousBuildRecord(state: *State, final: []const u8) FailureError!PreviousBuild {
    const record_path = try pathJoin(state, final, "artifacts.toml");
    const bytes = std.fs.cwd().readFileAlloc(state.arena, record_path, max_source_bytes) catch
        return state.fail("buildRecordMalformed", "could not read {s}", .{record_path});
    const toml = global(state, "Toml") orelse
        return state.fail("buildRecordMalformed", "Toml is not available while reading {s}", .{record_path});
    const parsed = state.api.send(toml, state.api.intern("parse:"), &.{state.api.mkString(bytes.ptr, bytes.len)}, 1);
    if (isError(state, parsed)) return state.fail("buildRecordMalformed", "could not parse {s} as TOML", .{record_path});
    if (!isMap(state, parsed)) return state.fail("buildRecordMalformed", "{s} did not produce a TOML table", .{record_path});

    const schema = try recordString(state, try mapAt(state, parsed, "schema"), "schema");
    if (!std.mem.eql(u8, schema, "birnam-artifacts-v3"))
        return state.fail("buildRecordMalformed", "unsupported artifact record schema in {s}", .{record_path});
    const project_root = try recordString(state, try mapAt(state, parsed, "project_root"), "project_root");
    const target = try recordString(state, try mapAt(state, parsed, "target"), "target");
    const record_profile = try recordString(state, try mapAt(state, parsed, "profile"), "profile");
    const tool_version = try recordString(state, try mapAt(state, parsed, "tool_version"), "tool_version");
    const built_at = try recordString(state, try mapAt(state, parsed, "built_at"), "built_at");
    const artifact_abi_value = try recordString(state, try mapAt(state, parsed, "artifact_abi"), "artifact_abi");
    const resolution_digest = try recordString(state, try mapAt(state, parsed, "resolution_digest"), "resolution_digest");
    const artifact_values = try mapAt(state, parsed, "artifacts");
    const count = try vectorSize(state, artifact_values, "artifacts");
    const artifacts = state.arena.alloc(PreviousArtifact, count) catch
        return state.fail("buildFailed", "out of memory while reading build record", .{});
    for (0..count) |i| {
        const item = state.api.send(artifact_values, state.api.intern("at:"), &.{state.api.mkSmallInt(@intCast(i))}, 1);
        if (!isMap(state, item)) return state.fail("buildRecordMalformed", "artifacts must contain inline tables", .{});
        artifacts[i] = .{
            .name = try recordString(state, try mapAt(state, item, "name"), "artifact name"),
            .relative_path = try recordString(state, try mapAt(state, item, "path"), "artifact path"),
            .source_path = try recordString(state, try mapAt(state, item, "source"), "artifact source"),
            .source_digest = try recordString(state, try mapAt(state, item, "source_digest"), "artifact source digest"),
            .revision_digest = try recordString(state, try mapAt(state, item, "revision_digest"), "artifact revision digest"),
            .requires = try recordStringList(state, try mapAt(state, item, "requires"), "artifact requires"),
            .checksum = try recordString(state, try mapAt(state, item, "checksum"), "artifact checksum"),
        };
    }
    const native_values = try mapAt(state, parsed, "native");
    const native_count = try vectorSize(state, native_values, "native");
    const native = state.arena.alloc(PreviousNativeArtifact, native_count) catch
        return state.fail("buildFailed", "out of memory while reading native artifact records", .{});
    for (0..native_count) |i| {
        const item = state.api.send(native_values, state.api.intern("at:"), &.{state.api.mkSmallInt(@intCast(i))}, 1);
        if (!isMap(state, item)) return state.fail("buildRecordMalformed", "native must contain inline tables", .{});
        native[i] = .{
            .name = try recordString(state, try mapAt(state, item, "name"), "native artifact name"),
            .relative_path = try recordString(state, try mapAt(state, item, "path"), "native artifact path"),
            .checksum = try recordString(state, try mapAt(state, item, "checksum"), "native artifact checksum"),
        };
    }
    return .{
        .project_root = project_root,
        .target = target,
        .profile = record_profile,
        .tool_version = tool_version,
        .artifact_abi = artifact_abi_value,
        .resolution_digest = resolution_digest,
        .built_at = built_at,
        .artifacts = artifacts,
        .native = native,
    };
}

fn readPreviousBuildRecord(state: *State, final: []const u8) ?PreviousBuild {
    return parsePreviousBuildRecord(state, final) catch null;
}

fn previousArtifact(build_record: PreviousBuild, name: []const u8) ?PreviousArtifact {
    for (build_record.artifacts) |artifact| {
        if (std.mem.eql(u8, artifact.name, name)) return artifact;
    }
    return null;
}

fn previousNative(build_record: PreviousBuild, name: []const u8) ?PreviousNativeArtifact {
    for (build_record.native) |artifact| {
        if (std.mem.eql(u8, artifact.name, name)) return artifact;
    }
    return null;
}

fn stringsEqual(left: []const []const u8, right: []const []const u8) bool {
    if (left.len != right.len) return false;
    for (left, right) |a, b| if (!std.mem.eql(u8, a, b)) return false;
    return true;
}

fn buildRecordCompatible(record: PreviousBuild, resolution: *const project.Resolution, target: []const u8, tool_version: []const u8) bool {
    return std.mem.eql(u8, record.project_root, resolution.project_root) and
        std.mem.eql(u8, record.target, target) and
        std.mem.eql(u8, record.profile, profile) and
        std.mem.eql(u8, record.tool_version, tool_version) and
        std.mem.eql(u8, record.artifact_abi, artifact_abi);
}

fn moduleIndex(resolution: *const project.Resolution, name: []const u8) ?usize {
    for (resolution.modules, 0..) |module, i| if (std.mem.eql(u8, module.name, name)) return i;
    return null;
}

fn fileChecksum(state: *State, path: []const u8) ?[]const u8 {
    const bytes = std.fs.cwd().readFileAlloc(state.arena, path, 1 << 30) catch return null;
    return sha256Hex(state, bytes);
}

fn reusableChecksum(state: *State, record: PreviousBuild, resolution: *const project.Resolution, module: project.Module, final: []const u8, target: []const u8, tool_version: []const u8, dirty: []const bool) FailureError!?[]const u8 {
    if (!buildRecordCompatible(record, resolution, target, tool_version)) return null;
    const old = previousArtifact(record, module.name) orelse return null;
    const relative_path = std.fmt.allocPrint(state.arena, "lib/libbirnam_{s}.dylib", .{module.name}) catch
        return state.fail("buildFailed", "out of memory while naming module #{s}", .{module.name});
    if (!std.mem.eql(u8, old.relative_path, relative_path) or
        !std.mem.eql(u8, old.source_path, module.path) or
        !std.mem.eql(u8, old.source_digest, module.source_digest) or
        !std.mem.eql(u8, old.revision_digest, module.revision_digest) or
        !stringsEqual(old.requires, module.requires)) return null;
    for (module.requires) |dependency| {
        const dependency_index = moduleIndex(resolution, dependency) orelse return null;
        if (dirty[dependency_index]) return null;
    }
    const old_path = try pathJoin(state, final, old.relative_path);
    const checksum = fileChecksum(state, old_path) orelse return null;
    if (!std.mem.eql(u8, checksum, old.checksum)) return null;
    return checksum;
}

fn artifactFromModule(state: *State, module: project.Module, relative_path: []const u8, checksum: []const u8, action: Action) FailureError!Artifact {
    return .{
        .name = state.arena.dupe(u8, module.name) catch @panic("project_build: oom"),
        .relative_path = relative_path,
        .source_path = state.arena.dupe(u8, module.path) catch @panic("project_build: oom"),
        .source_digest = state.arena.dupe(u8, module.source_digest) catch @panic("project_build: oom"),
        .revision_digest = state.arena.dupe(u8, module.revision_digest) catch @panic("project_build: oom"),
        .requires = try duplicateStrings(state, module.requires),
        .checksum = state.arena.dupe(u8, checksum) catch @panic("project_build: oom"),
        .action = action,
    };
}

fn sha256Hex(state: *State, bytes: []const u8) []const u8 {
    var digest: [32]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(bytes, &digest, .{});
    const hex = std.fmt.bytesToHex(digest, .lower);
    return state.arena.dupe(u8, &hex) catch @panic("project_build: oom");
}

fn targetIdentity(state: *State) []const u8 {
    return std.fmt.allocPrint(state.arena, "{s}-{s}", .{
        @tagName(builtin.cpu.arch),
        @tagName(builtin.os.tag),
    }) catch @panic("project_build: oom");
}

fn appendTomlString(state: *State, out: *std.ArrayList(u8), key: []const u8, value: []const u8) FailureError!void {
    try put(state, out, key);
    try put(state, out, " = \"");
    for (value) |byte| {
        if (byte == '\\' or byte == '"') try put(state, out, "\\");
        if (byte == '\n') {
            try put(state, out, "\\n");
        } else {
            out.append(state.arena, byte) catch
                return state.fail("buildFailed", "out of memory while writing build metadata", .{});
        }
    }
    try put(state, out, "\"\n");
}

fn appendTomlArray(state: *State, out: *std.ArrayList(u8), key: []const u8, values: []const []const u8) FailureError!void {
    try put(state, out, key);
    try put(state, out, " = [");
    for (values, 0..) |value, i| {
        if (i != 0) try put(state, out, ", ");
        try put(state, out, "\"");
        for (value) |byte| {
            if (byte == '\\' or byte == '"') try put(state, out, "\\");
            if (byte == '\n') {
                try put(state, out, "\\n");
            } else {
                out.append(state.arena, byte) catch
                    return state.fail("buildFailed", "out of memory while writing build metadata", .{});
            }
        }
        try put(state, out, "\"");
    }
    try put(state, out, "]\n");
}

fn writeFile(state: *State, path: []const u8, bytes: []const u8) FailureError!void {
    const file = std.fs.cwd().createFile(path, .{ .truncate = true }) catch
        return state.fail("buildPublicationFailed", "could not create build record {s}", .{path});
    defer file.close();
    file.writeAll(bytes) catch
        return state.fail("buildPublicationFailed", "could not write build record {s}", .{path});
    file.sync() catch
        return state.fail("buildPublicationFailed", "could not flush build record {s}", .{path});
}

fn appendInlinePrefix(state: *State, out: *std.ArrayList(u8), first: *bool, key: []const u8) FailureError!void {
    if (!first.*) try put(state, out, ", ");
    first.* = false;
    try put(state, out, key);
    try put(state, out, " = ");
}

fn appendInlineString(state: *State, out: *std.ArrayList(u8), first: *bool, key: []const u8, value: []const u8) FailureError!void {
    try appendInlinePrefix(state, out, first, key);
    try put(state, out, "\"");
    for (value) |byte| {
        if (byte == '\\' or byte == '"') try put(state, out, "\\");
        if (byte == '\n') {
            try put(state, out, "\\n");
        } else {
            out.append(state.arena, byte) catch
                return state.fail("buildFailed", "out of memory while writing build metadata", .{});
        }
    }
    try put(state, out, "\"");
}

fn appendInlineArray(state: *State, out: *std.ArrayList(u8), first: *bool, key: []const u8, values: []const []const u8) FailureError!void {
    try appendInlinePrefix(state, out, first, key);
    try put(state, out, "[");
    for (values, 0..) |value, i| {
        if (i != 0) try put(state, out, ", ");
        try put(state, out, "\"");
        for (value) |byte| {
            if (byte == '\\' or byte == '"') try put(state, out, "\\");
            if (byte == '\n') {
                try put(state, out, "\\n");
            } else {
                out.append(state.arena, byte) catch
                    return state.fail("buildFailed", "out of memory while writing build metadata", .{});
            }
        }
        try put(state, out, "\"");
    }
    try put(state, out, "]");
}

fn writeInlineModule(state: *State, out: *std.ArrayList(u8), module: project.Module) FailureError!void {
    var first = true;
    try put(state, out, "{ ");
    try appendInlineString(state, out, &first, "name", module.name);
    try appendInlineString(state, out, &first, "source", module.path);
    try appendInlineString(state, out, &first, "source_digest", module.source_digest);
    try appendInlineString(state, out, &first, "revision_digest", module.revision_digest);
    try appendInlineArray(state, out, &first, "requires", module.requires);
    try put(state, out, " },\n");
}

fn writeInlineArtifact(state: *State, out: *std.ArrayList(u8), artifact: Artifact) FailureError!void {
    var first = true;
    try put(state, out, "{ ");
    try appendInlineString(state, out, &first, "name", artifact.name);
    try appendInlineString(state, out, &first, "path", artifact.relative_path);
    try appendInlineString(state, out, &first, "source", artifact.source_path);
    try appendInlineString(state, out, &first, "source_digest", artifact.source_digest);
    try appendInlineString(state, out, &first, "revision_digest", artifact.revision_digest);
    try appendInlineArray(state, out, &first, "requires", artifact.requires);
    try appendInlineString(state, out, &first, "checksum", artifact.checksum);
    try put(state, out, " },\n");
}

fn writeInlineNative(state: *State, out: *std.ArrayList(u8), artifact: NativeArtifact) FailureError!void {
    var first = true;
    try put(state, out, "{ ");
    try appendInlineString(state, out, &first, "name", artifact.name);
    try appendInlineString(state, out, &first, "path", artifact.relative_path);
    try appendInlineString(state, out, &first, "checksum", artifact.checksum);
    try put(state, out, " },\n");
}

fn writeResolutionRecord(state: *State, resolution: *const project.Resolution, stage: []const u8, target: []const u8, tool_version: []const u8, built_at: []const u8) FailureError!void {
    var out: std.ArrayList(u8) = .{};
    const record_path = try pathJoin(state, stage, "resolution.toml");
    defer out.deinit(state.arena);

    try appendTomlString(state, &out, "schema", "birnam-resolution-v2");
    try appendTomlString(state, &out, "project_root", resolution.project_root);
    try appendTomlString(state, &out, "manifest", resolution.manifest_path);
    try appendTomlString(state, &out, "target", target);
    try appendTomlString(state, &out, "profile", profile);
    try appendTomlString(state, &out, "tool_version", tool_version);
    try appendTomlString(state, &out, "built_at", built_at);
    try appendTomlString(state, &out, "resolution_digest", resolution.digest);
    try put(state, &out, "modules = [\n");
    for (resolution.modules) |module| try writeInlineModule(state, &out, module);
    try put(state, &out, "]\n");
    try writeFile(state, record_path, out.items);
}

fn writeArtifactRecord(state: *State, build_result: *const Build) FailureError!void {
    var out: std.ArrayList(u8) = .{};
    const record_path = try pathJoin(state, build_result.output_root, "artifacts.toml");
    defer out.deinit(state.arena);

    try appendTomlString(state, &out, "schema", "birnam-artifacts-v3");
    try appendTomlString(state, &out, "project_root", build_result.project_root);
    try appendTomlString(state, &out, "target", build_result.target);
    try appendTomlString(state, &out, "profile", build_result.profile);
    try appendTomlString(state, &out, "tool_version", build_result.tool_version);
    try appendTomlString(state, &out, "built_at", build_result.built_at);
    try appendTomlString(state, &out, "artifact_abi", artifact_abi);
    try appendTomlString(state, &out, "resolution_digest", build_result.resolution_digest);
    try put(state, &out, "artifacts = [\n");
    for (build_result.artifacts) |artifact| try writeInlineArtifact(state, &out, artifact);
    try put(state, &out, "]\n");
    try put(state, &out, "native = [\n");
    for (build_result.native) |artifact| try writeInlineNative(state, &out, artifact);
    try put(state, &out, "]\n");
    try writeFile(state, record_path, out.items);
}

fn compilerError(state: *State, module: []const u8) FailureError {
    const codegen = state.codegen orelse return state.fail("buildToolUnavailable", "Codegen is not available", .{});
    const last_error = state.api.send(codegen, state.api.intern("lastError"), &.{}, 0);
    if (last_error != state.api.nilObj and state.api.vtableOf(last_error) == state.api.string_vt) {
        var length: usize = 0;
        const bytes = state.api.stringBytes(last_error, &length);
        const detail = state.arena.dupe(u8, bytes[0..length]) catch
            return state.fail("compileFailed", "could not compile module #{s}", .{module});
        return state.fail("compileFailed", "could not compile module #{s}: {s}", .{ module, detail });
    }
    return state.fail("compileFailed", "could not compile module #{s}", .{module});
}

fn compileArtifact(state: *State, module: project.Module, output_path: []const u8) FailureError!Artifact {
    const source = std.fs.cwd().readFileAlloc(state.arena, module.path, max_source_bytes) catch
        return state.fail("sourceReadFailed", "could not read source for #{s}: {s}", .{ module.name, module.path });
    const codegen = state.codegen orelse return state.fail("buildToolUnavailable", "Codegen is not available", .{});
    const result = state.api.send(codegen, state.api.intern("compile:output:"), &[_]Oop{
        state.api.mkString(source.ptr, source.len),
        state.api.mkString(output_path.ptr, output_path.len),
    }, 2);
    if (result == state.api.nilObj) return compilerError(state, module.name);

    const artifact_bytes = std.fs.cwd().readFileAlloc(state.arena, output_path, 1 << 30) catch
        return state.fail("artifactMissing", "compiler did not produce an artifact for #{s}", .{module.name});
    const relative_path = std.fmt.allocPrint(state.arena, "lib/libbirnam_{s}.dylib", .{module.name}) catch
        return state.fail("buildFailed", "out of memory while naming artifact #{s}", .{module.name});
    return .{
        .name = state.arena.dupe(u8, module.name) catch @panic("project_build: oom"),
        .relative_path = relative_path,
        .source_path = state.arena.dupe(u8, module.path) catch @panic("project_build: oom"),
        .source_digest = state.arena.dupe(u8, module.source_digest) catch @panic("project_build: oom"),
        .revision_digest = state.arena.dupe(u8, module.revision_digest) catch @panic("project_build: oom"),
        .requires = try duplicateStrings(state, module.requires),
        .checksum = sha256Hex(state, artifact_bytes),
        .action = .compiled,
    };
}

fn expandCommandArg(state: *State, input: []const u8, project_root: []const u8, wrapped: project.Wrapped, output: []const u8, stage: []const u8, stage_lib: []const u8) FailureError![]const u8 {
    var out: std.ArrayList(u8) = .{};
    var i: usize = 0;
    while (i < input.len) {
        const replacement: ?[]const u8 = if (std.mem.startsWith(u8, input[i..], "{output}")) output else if (std.mem.startsWith(u8, input[i..], "{project}")) project_root else if (std.mem.startsWith(u8, input[i..], "{build}")) stage else if (std.mem.startsWith(u8, input[i..], "{lib}")) stage_lib else null;
        if (replacement) |value| {
            const token_len = if (std.mem.startsWith(u8, input[i..], "{output}")) "{output}".len else if (std.mem.startsWith(u8, input[i..], "{project}")) "{project}".len else if (std.mem.startsWith(u8, input[i..], "{build}")) "{build}".len else "{lib}".len;
            out.appendSlice(state.arena, value) catch
                return state.fail("nativeBuildFailed", "out of memory while expanding the build command for wrapped {s}", .{wrapped.name});
            i += token_len;
        } else {
            out.append(state.arena, input[i]) catch
                return state.fail("nativeBuildFailed", "out of memory while expanding the build command for wrapped {s}", .{wrapped.name});
            i += 1;
        }
    }
    return out.toOwnedSlice(state.arena) catch
        return state.fail("nativeBuildFailed", "out of memory while expanding the build command for wrapped {s}", .{wrapped.name});
}

fn runWrappedCommand(state: *State, resolution: *const project.Resolution, wrapped: project.Wrapped, command: []const []const u8, output: []const u8, stage: []const u8, stage_lib: []const u8) FailureError!void {
    const argv = state.arena.alloc([]const u8, command.len) catch
        return state.fail("nativeBuildFailed", "out of memory while preparing the build command for wrapped {s}", .{wrapped.name});
    for (command, 0..) |argument, i| argv[i] = try expandCommandArg(state, argument, resolution.project_root, wrapped, output, stage, stage_lib);
    const result = std.process.Child.run(.{
        .allocator = state.arena,
        .argv = argv,
        .cwd = resolution.project_root,
        .max_output_bytes = 16 << 20,
    }) catch |err|
        return state.fail("nativeBuildFailed", "could not run the build command for wrapped {s}: {s}", .{ wrapped.name, @errorName(err) });
    switch (result.term) {
        .Exited => |code| if (code != 0)
            return state.fail("nativeBuildFailed", "build command for wrapped {s} exited {d}: {s}{s}", .{ wrapped.name, code, result.stdout, result.stderr }),
        else => return state.fail("nativeBuildFailed", "build command for wrapped {s} did not exit normally: {s}", .{ wrapped.name, @tagName(result.term) }),
    }
}

fn expectedNativePath(state: *State, wrapped: project.Wrapped) FailureError![]const u8 {
    return std.fmt.allocPrint(state.arena, "lib/lib{s}.dylib", .{wrapped.name}) catch
        return state.fail("buildFailed", "out of memory while naming wrapped library {s}", .{wrapped.name});
}

fn buildWrapped(state: *State, resolution: *const project.Resolution, wrapped: project.Wrapped, stage: []const u8, stage_lib: []const u8) FailureError!NativeArtifact {
    const native_root = try pathJoin(state, stage, "native");
    const wrapped_root = try pathJoin(state, native_root, wrapped.name);
    try makePath(state, wrapped_root);
    const filename = std.fmt.allocPrint(state.arena, "lib{s}.dylib", .{wrapped.name}) catch
        return state.fail("buildFailed", "out of memory while naming wrapped library {s}", .{wrapped.name});
    const output = try pathJoin(state, wrapped_root, filename);
    const stage_output = try pathJoin(state, stage_lib, filename);
    std.fs.cwd().deleteFile(output) catch {};
    for (wrapped.build) |command| try runWrappedCommand(state, resolution, wrapped, command, output, stage, stage_lib);
    if (!pathExists(output))
        return state.fail("nativeArtifactMissing", "build commands for wrapped {s} did not produce {s}", .{ wrapped.name, output });
    std.fs.cwd().copyFile(output, std.fs.cwd(), stage_output, .{}) catch |err|
        return state.fail("nativeArtifactMissing", "could not stage wrapped library {s}: {s}", .{ wrapped.name, @errorName(err) });
    const bytes = std.fs.cwd().readFileAlloc(state.arena, stage_output, 1 << 30) catch
        return state.fail("nativeArtifactMissing", "could not read staged wrapped library {s}", .{wrapped.name});
    return .{
        .name = state.arena.dupe(u8, wrapped.name) catch @panic("project_build: oom"),
        .relative_path = try expectedNativePath(state, wrapped),
        .checksum = sha256Hex(state, bytes),
        .action = .compiled,
    };
}

fn publish(state: *State, stage: []const u8, final: []const u8, target_root: []const u8, token: []const u8) FailureError!void {
    const backup = try pathJoin(state, target_root, std.fmt.allocPrint(state.arena, ".previous-{s}", .{token}) catch
        return state.fail("buildPublicationFailed", "out of memory while preparing publication", .{}));
    std.fs.cwd().deleteTree(backup) catch {};

    var had_previous = false;
    if (pathExists(final)) {
        std.fs.cwd().rename(final, backup) catch
            return state.fail("buildPublicationFailed", "could not preserve the previous build at {s}", .{final});
        had_previous = true;
    }
    std.fs.cwd().rename(stage, final) catch |err| {
        if (had_previous) std.fs.cwd().rename(backup, final) catch {};
        return state.fail("buildPublicationFailed", "could not publish build at {s}: {s}", .{ final, @errorName(err) });
    };
    if (had_previous) std.fs.cwd().deleteTree(backup) catch {};
}

fn expectedArtifactPath(state: *State, module: project.Module) FailureError![]const u8 {
    return std.fmt.allocPrint(state.arena, "lib/libbirnam_{s}.dylib", .{module.name}) catch
        return state.fail("buildFailed", "out of memory while naming module #{s}", .{module.name});
}

fn artifactMetadataMatches(state: *State, old: PreviousArtifact, module: project.Module) FailureError!bool {
    const relative_path = try expectedArtifactPath(state, module);
    return std.mem.eql(u8, old.name, module.name) and
        std.mem.eql(u8, old.relative_path, relative_path) and
        std.mem.eql(u8, old.source_path, module.path) and
        std.mem.eql(u8, old.source_digest, module.source_digest) and
        std.mem.eql(u8, old.revision_digest, module.revision_digest) and
        stringsEqual(old.requires, module.requires);
}

fn validatePublished(state: *State, resolution: *const project.Resolution, tool_version: []const u8) FailureError!Build {
    const target = targetIdentity(state);
    const build_root = try pathJoin(state, resolution.project_root, ".birnam/build");
    const target_root = try pathJoin(state, build_root, target);
    const final = try pathJoin(state, target_root, profile);
    const record_path = try pathJoin(state, final, "artifacts.toml");
    if (!pathExists(final) or !pathExists(record_path))
        return state.fail("projectBuildNotFound", "no successful build exists at {s}; run birnam build or birnam run --build", .{final});

    const record = parsePreviousBuildRecord(state, final) catch {
        return state.fail("projectArtifactRecordInvalid", "could not read a valid artifact record at {s}", .{record_path});
    };
    if (!buildRecordCompatible(record, resolution, target, tool_version) or
        !std.mem.eql(u8, record.resolution_digest, resolution.digest))
        return state.fail("projectArtifactStale", "published artifacts at {s} do not match the current project resolution", .{final});
    if (record.artifacts.len != resolution.modules.len)
        return state.fail("projectArtifactStale", "published artifact set at {s} is incomplete", .{final});
    if (record.native.len != resolution.wrapped.len)
        return state.fail("projectArtifactStale", "published native artifact set at {s} is incomplete", .{final});

    const artifacts = state.arena.alloc(Artifact, resolution.modules.len) catch
        return state.fail("buildFailed", "out of memory while validating artifacts", .{});
    for (resolution.modules, 0..) |module, i| {
        const old = previousArtifact(record, module.name) orelse
            return state.fail("projectArtifactStale", "published artifacts do not contain module #{s}", .{module.name});
        if (!(try artifactMetadataMatches(state, old, module)))
            return state.fail("projectArtifactStale", "published artifact for #{s} does not match its source metadata", .{module.name});
        const path = try pathJoin(state, final, old.relative_path);
        if (!pathExists(path)) return state.fail("projectArtifactMissing", "published artifact for #{s} is missing at {s}", .{ module.name, path });
        const checksum = fileChecksum(state, path) orelse
            return state.fail("projectArtifactMissing", "could not read published artifact for #{s}", .{module.name});
        if (!std.mem.eql(u8, checksum, old.checksum))
            return state.fail("projectArtifactStale", "published artifact for #{s} failed checksum validation", .{module.name});
        artifacts[i] = try artifactFromModule(state, module, try expectedArtifactPath(state, module), checksum, .reused);
    }
    const native = state.arena.alloc(NativeArtifact, resolution.wrapped.len) catch
        return state.fail("buildFailed", "out of memory while validating native artifacts", .{});
    for (resolution.wrapped, 0..) |wrapped, i| {
        const old = previousNative(record, wrapped.name) orelse
            return state.fail("projectArtifactStale", "published artifacts do not contain wrapped library {s}", .{wrapped.name});
        const expected_path = try expectedNativePath(state, wrapped);
        if (!std.mem.eql(u8, old.relative_path, expected_path))
            return state.fail("projectArtifactStale", "published wrapped library {s} has an unexpected path", .{wrapped.name});
        const path = try pathJoin(state, final, old.relative_path);
        if (!pathExists(path)) return state.fail("projectArtifactMissing", "published wrapped library {s} is missing at {s}", .{ wrapped.name, path });
        const checksum = fileChecksum(state, path) orelse
            return state.fail("projectArtifactMissing", "could not read published wrapped library {s}", .{wrapped.name});
        if (!std.mem.eql(u8, checksum, old.checksum))
            return state.fail("projectArtifactStale", "published wrapped library {s} failed checksum validation", .{wrapped.name});
        native[i] = .{
            .name = state.arena.dupe(u8, wrapped.name) catch @panic("project_build: oom"),
            .relative_path = expected_path,
            .checksum = checksum,
            .action = .reused,
        };
    }
    return .{
        .project_root = resolution.project_root,
        .output_root = final,
        .target = target,
        .profile = profile,
        .tool_version = state.arena.dupe(u8, tool_version) catch @panic("project_build: oom"),
        .built_at = record.built_at,
        .resolution_digest = resolution.digest,
        .artifacts = artifacts,
        .native = native,
    };
}

fn run(state: *State, resolution: *const project.Resolution, tool_version: []const u8) FailureError!Build {
    state.codegen = state.api.import("Codegen") orelse
        return state.fail("buildToolUnavailable", "Codegen is not available", .{});

    const target = targetIdentity(state);
    const built_at = std.fmt.allocPrint(state.arena, "{d}", .{std.time.timestamp()}) catch @panic("project_build: oom");
    const build_root = try pathJoin(state, resolution.project_root, ".birnam/build");
    const target_root = try pathJoin(state, build_root, target);
    try makePath(state, target_root);

    var token_value: u64 = undefined;
    std.crypto.random.bytes(std.mem.asBytes(&token_value));
    const token = std.fmt.allocPrint(state.arena, "{x}", .{token_value}) catch @panic("project_build: oom");
    const stage = try pathJoin(state, target_root, std.fmt.allocPrint(state.arena, ".staging-{s}", .{token}) catch
        return state.fail("buildPublicationFailed", "out of memory while preparing staging", .{}));
    const final = try pathJoin(state, target_root, profile);
    const previous = readPreviousBuildRecord(state, final);
    const stage_lib = try pathJoin(state, stage, "lib");
    std.fs.cwd().deleteTree(stage) catch {};
    try makePath(state, stage_lib);
    var published = false;
    defer if (!published) std.fs.cwd().deleteTree(stage) catch {};

    const artifacts = state.arena.alloc(Artifact, resolution.modules.len) catch
        return state.fail("buildFailed", "out of memory while preparing artifacts", .{});
    const native = state.arena.alloc(NativeArtifact, resolution.wrapped.len) catch
        return state.fail("buildFailed", "out of memory while preparing native artifacts", .{});
    for (resolution.wrapped, 0..) |wrapped, i| native[i] = try buildWrapped(state, resolution, wrapped, stage, stage_lib);
    const dirty = state.arena.alloc(bool, resolution.modules.len) catch
        return state.fail("buildFailed", "out of memory while preparing invalidation state", .{});
    @memset(dirty, false);
    for (resolution.modules, 0..) |module, i| {
        const filename = std.fmt.allocPrint(state.arena, "libbirnam_{s}.dylib", .{module.name}) catch
            return state.fail("buildFailed", "out of memory while naming module #{s}", .{module.name});
        const output_path = try pathJoin(state, stage_lib, filename);
        const reused_checksum = if (previous) |record|
            try reusableChecksum(state, record, resolution, module, final, target, tool_version, dirty)
        else
            null;
        if (reused_checksum) |checksum| {
            const old_path = try pathJoin(state, final, std.fmt.allocPrint(state.arena, "lib/libbirnam_{s}.dylib", .{module.name}) catch
                return state.fail("buildFailed", "out of memory while naming module #{s}", .{module.name}));
            std.fs.cwd().copyFile(old_path, std.fs.cwd(), output_path, .{}) catch |err|
                return state.fail("artifactReuseFailed", "could not reuse artifact for #{s}: {s}", .{ module.name, @errorName(err) });
            artifacts[i] = try artifactFromModule(state, module, std.fmt.allocPrint(state.arena, "lib/libbirnam_{s}.dylib", .{module.name}) catch
                return state.fail("buildFailed", "out of memory while naming module #{s}", .{module.name}), checksum, .reused);
        } else {
            dirty[i] = true;
            artifacts[i] = try compileArtifact(state, module, output_path);
        }
    }

    const materialized = Build{
        .project_root = resolution.project_root,
        .output_root = final,
        .target = target,
        .profile = profile,
        .tool_version = state.arena.dupe(u8, tool_version) catch @panic("project_build: oom"),
        .built_at = built_at,
        .resolution_digest = resolution.digest,
        .artifacts = artifacts,
        .native = native,
    };
    try writeResolutionRecord(state, resolution, stage, target, tool_version, built_at);
    // The artifact record is written into the staging directory before the
    // directory is made visible. Its path is temporarily patched to the stage
    // for this write; all paths inside the record remain project-relative.
    const staged_build = Build{ .project_root = materialized.project_root, .output_root = stage, .target = materialized.target, .profile = materialized.profile, .tool_version = materialized.tool_version, .built_at = materialized.built_at, .resolution_digest = materialized.resolution_digest, .artifacts = materialized.artifacts, .native = materialized.native };
    try writeArtifactRecord(state, &staged_build);
    try publish(state, stage, final, target_root, token);
    published = true;
    return materialized;
}

pub fn build(api: *IdApi, allocator: std.mem.Allocator, resolution: *const project.Resolution, tool_version: []const u8) Result {
    var arena = std.heap.ArenaAllocator.init(allocator);
    var state = State{ .arena = arena.allocator(), .api = api };
    const result = run(&state, resolution, tool_version) catch {
        return .{ .arena = arena, .build = null, .diagnostic = .{ .kind = state.diagnostic_kind, .message = state.diagnostic_message } };
    };
    return .{ .arena = arena, .build = result, .diagnostic = null };
}

pub fn validate(api: *IdApi, allocator: std.mem.Allocator, resolution: *const project.Resolution, tool_version: []const u8) Result {
    var arena = std.heap.ArenaAllocator.init(allocator);
    var state = State{ .arena = arena.allocator(), .api = api };
    const result = validatePublished(&state, resolution, tool_version) catch {
        return .{ .arena = arena, .build = null, .diagnostic = .{ .kind = state.diagnostic_kind, .message = state.diagnostic_message } };
    };
    return .{ .arena = arena, .build = result, .diagnostic = null };
}

pub fn writeReport(result: *const Result, writer: anytype) !void {
    if (result.diagnostic) |diagnostic| {
        try writer.print("{s}: {s}\n", .{ diagnostic.kind, diagnostic.message });
        return;
    }
    const build_result = result.build.?;
    try writer.print("project: {s}\n", .{build_result.project_root});
    try writer.print("build: {s}\n", .{build_result.output_root});
    try writer.print("target: {s}\n", .{build_result.target});
    try writer.print("profile: {s}\n", .{build_result.profile});
    try writer.print("tool-version: {s}\n", .{build_result.tool_version});
    try writer.print("built-at: {s}\n", .{build_result.built_at});
    try writer.print("resolution-digest: {s}\n", .{build_result.resolution_digest});
    try writer.writeAll("artifacts:\n");
    if (build_result.artifacts.len == 0) try writer.writeAll("  (none)\n");
    for (build_result.artifacts) |artifact| {
        try writer.print("  #{s} [{s}] <- {s} checksum={s}\n", .{ artifact.name, @tagName(artifact.action), artifact.relative_path, artifact.checksum });
    }
    if (build_result.native.len != 0) {
        try writer.writeAll("native:\n");
        for (build_result.native) |artifact| {
            try writer.print("  {s} [{s}] <- {s} checksum={s}\n", .{ artifact.name, @tagName(artifact.action), artifact.relative_path, artifact.checksum });
        }
    }
}
