//! Minimal, inert inspection of self-describing Birnam artifacts.
//!
//! This rung depends only on IdApi. It reads a pointer-free descriptor directly
//! from artifact bytes: no dlopen, constructor, runtime shim, Store, compiler,
//! or artifact entry point participates in inspection.

const std = @import("std");
const artifact_contract = @import("artifact.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const max_artifact_bytes = 1 << 30;

var g_api: *idapi.IdApi = undefined;
var artifact_vt: Oop = null;
var artifact_obj: Oop = null;
var descriptor_vt: Oop = null;
var last_failure: Oop = null;

const Descriptor = extern struct {
    kind: Oop,
    identity: Oop,
    artifact_abi: Oop,
    target: Oop,
    source_digest: Oop,
    revision_digest: Oop,
    path: Oop,
    requires: Oop,
    native_requires: Oop,
    capabilities: Oop,
    requires_count: Oop,
    native_requires_count: Oop,
    capabilities_count: Oop,
};

const Parsed = struct {
    kind: []const u8,
    identity: []const u8,
    artifact_abi: []const u8,
    target: []const u8,
    source_digest: []const u8,
    revision_digest: []const u8,
    requires: []const u8,
    native_requires: []const u8,
    capabilities: []const u8,
    requires_count: usize,
    native_requires_count: usize,
    capabilities_count: usize,
};

const Reader = struct {
    bytes: []const u8,
    position: usize = 0,

    fn readU32(self: *Reader) ?usize {
        if (self.position + 4 > self.bytes.len) return null;
        const value = std.mem.readInt(u32, self.bytes[self.position..][0..4], .little);
        self.position += 4;
        return value;
    }

    fn field(self: *Reader) ?[]const u8 {
        const len = self.readU32() orelse return null;
        if (self.position + len > self.bytes.len) return null;
        const value = self.bytes[self.position..][0..len];
        self.position += len;
        return value;
    }

    fn list(self: *Reader) ?struct { bytes: []const u8, count: usize } {
        const count = self.readU32() orelse return null;
        if (count > 4096) return null;
        const start = self.position;
        for (0..count) |_| _ = self.field() orelse return null;
        return .{ .bytes = self.bytes[start..self.position], .count = count };
    }
};

fn descriptor(value: Oop) *Descriptor {
    return @ptrCast(@alignCast(value.?));
}

fn fail(kind: [*:0]const u8) Oop {
    last_failure = g_api.intern(kind);
    return g_api.nilObj;
}

fn intern(bytes: []const u8) Oop {
    const z = std.heap.c_allocator.dupeZ(u8, bytes) catch @panic("artifact_inspector: oom");
    defer std.heap.c_allocator.free(z);
    return g_api.intern(z.ptr);
}

fn parseDescriptorAt(file: []const u8, start: usize) ?Parsed {
    const magic = artifact_contract.descriptor_magic;
    if (start + magic.len + 4 > file.len) return null;
    const payload_len = std.mem.readInt(u32, file[start + magic.len ..][0..4], .little);
    const payload_start = start + magic.len + 4;
    if (payload_start + payload_len > file.len) return null;
    var reader = Reader{ .bytes = file[payload_start..][0..payload_len] };
    const kind = reader.field() orelse return null;
    const identity = reader.field() orelse return null;
    const artifact_abi = reader.field() orelse return null;
    const target = reader.field() orelse return null;
    const source_digest = reader.field() orelse return null;
    const revision_digest = reader.field() orelse return null;
    const requires = reader.list() orelse return null;
    const native_requires = reader.list() orelse return null;
    const capabilities = reader.list() orelse return null;
    if (reader.position != reader.bytes.len) return null;
    return .{
        .kind = kind,
        .identity = identity,
        .artifact_abi = artifact_abi,
        .target = target,
        .source_digest = source_digest,
        .revision_digest = revision_digest,
        .requires = requires.bytes,
        .native_requires = native_requires.bytes,
        .capabilities = capabilities.bytes,
        .requires_count = requires.count,
        .native_requires_count = native_requires.count,
        .capabilities_count = capabilities.count,
    };
}

fn parseDescriptor(file: []const u8) ?Parsed {
    const magic = artifact_contract.descriptor_magic;
    var position: usize = 0;
    var found: ?Parsed = null;
    while (std.mem.indexOfPos(u8, file, position, magic)) |start| {
        if (parseDescriptorAt(file, start)) |parsed| {
            if (found != null) return null;
            found = parsed;
        }
        position = start + magic.len;
    }
    return found;
}

fn listField(bytes: []const u8, index: usize) ?[]const u8 {
    var reader = Reader{ .bytes = bytes };
    var current: usize = 0;
    while (reader.position < reader.bytes.len) : (current += 1) {
        const value = reader.field() orelse return null;
        if (current == index) return value;
    }
    return null;
}

fn nativeRequirementsPresent(parsed: Parsed, artifact_path: []const u8) bool {
    const directory = std.fs.path.dirname(artifact_path) orelse ".";
    for (0..parsed.native_requires_count) |i| {
        const name = listField(parsed.native_requires, i) orelse return false;
        if (name.len == 0 or std.fs.path.basename(name).len != name.len) return false;
        const path = std.fs.path.join(std.heap.c_allocator, &.{ directory, name }) catch return false;
        defer std.heap.c_allocator.free(path);
        std.fs.cwd().access(path, .{}) catch return false;
    }
    return true;
}

fn mInspect(_: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    if (n != 1 or g_api.vtableOf(args[0]) != g_api.string_vt) return fail("artifactPathInvalid");
    var path_len: usize = 0;
    const path = g_api.stringBytes(args[0], &path_len)[0..path_len];
    const bytes = std.fs.cwd().readFileAlloc(std.heap.c_allocator, path, max_artifact_bytes) catch return fail("artifactUnavailable");
    defer std.heap.c_allocator.free(bytes);
    const parsed = parseDescriptor(bytes) orelse return fail("artifactDescriptionMissing");
    if (!std.mem.eql(u8, parsed.artifact_abi, artifact_contract.abi)) return fail("artifactAbiMismatch");
    if (!std.mem.eql(u8, parsed.target, artifact_contract.target)) return fail("artifactTargetMismatch");
    if (parsed.identity.len == 0) return fail("artifactDescriptionMissing");
    if (!nativeRequirementsPresent(parsed, path)) return fail("nativeRequirementUnavailable");

    const result = g_api.alloc(descriptor_vt, @sizeOf(Descriptor));
    descriptor(result).* = .{
        .kind = intern(parsed.kind),
        .identity = intern(parsed.identity),
        .artifact_abi = g_api.mkString(parsed.artifact_abi.ptr, parsed.artifact_abi.len),
        .target = g_api.mkString(parsed.target.ptr, parsed.target.len),
        .source_digest = g_api.mkString(parsed.source_digest.ptr, parsed.source_digest.len),
        .revision_digest = g_api.mkString(parsed.revision_digest.ptr, parsed.revision_digest.len),
        .path = g_api.mkString(path.ptr, path.len),
        .requires = g_api.mkString(parsed.requires.ptr, parsed.requires.len),
        .native_requires = g_api.mkString(parsed.native_requires.ptr, parsed.native_requires.len),
        .capabilities = g_api.mkString(parsed.capabilities.ptr, parsed.capabilities.len),
        .requires_count = g_api.mkSmallInt(@intCast(parsed.requires_count)),
        .native_requires_count = g_api.mkSmallInt(@intCast(parsed.native_requires_count)),
        .capabilities_count = g_api.mkSmallInt(@intCast(parsed.capabilities_count)),
    };
    last_failure = g_api.nilObj;
    return result;
}

fn mLastFailure(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return last_failure;
}

fn mKind(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return descriptor(self).kind;
}
fn mIdentity(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return descriptor(self).identity;
}
fn mArtifactAbi(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return descriptor(self).artifact_abi;
}
fn mTarget(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return descriptor(self).target;
}
fn mSourceDigest(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return descriptor(self).source_digest;
}
fn mRevisionDigest(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return descriptor(self).revision_digest;
}
fn mPath(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return descriptor(self).path;
}

const ListKind = enum { roots, requires, native_requires, capabilities };

fn countOf(state: *Descriptor, kind: ListKind) usize {
    if (kind == .roots) return 1;
    const value = switch (kind) {
        .roots => unreachable,
        .requires => state.requires_count,
        .native_requires => state.native_requires_count,
        .capabilities => state.capabilities_count,
    };
    return @intCast(g_api.smallIntValue(value));
}

fn bytesOf(state: *Descriptor, kind: ListKind) []const u8 {
    const value = switch (kind) {
        .roots => return "",
        .requires => state.requires,
        .native_requires => state.native_requires,
        .capabilities => state.capabilities,
    };
    var len: usize = 0;
    return g_api.stringBytes(value, &len)[0..len];
}

fn valueAt(state: *Descriptor, kind: ListKind, index: usize) Oop {
    if (kind == .roots) return if (index == 0) state.identity else g_api.nilObj;
    if (index >= countOf(state, kind)) return g_api.nilObj;
    const value = listField(bytesOf(state, kind), index) orelse return g_api.nilObj;
    return if (kind == .native_requires) g_api.mkString(value.ptr, value.len) else intern(value);
}

fn vectorOf(self: Oop, kind: ListKind) Oop {
    const vector = g_api.import("Vector") orelse return g_api.nilObj;
    var result = g_api.send(vector, g_api.intern("new"), &.{}, 0);
    const state = descriptor(self);
    for (0..countOf(state, kind)) |i|
        result = g_api.send(result, g_api.intern("add:"), &.{valueAt(state, kind, i)}, 1);
    return result;
}

fn mRoots(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return vectorOf(self, .roots);
}
fn mRequires(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return vectorOf(self, .requires);
}
fn mNativeRequires(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return vectorOf(self, .native_requires);
}
fn mCapabilities(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return vectorOf(self, .capabilities);
}

fn mRootsCount(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return g_api.mkSmallInt(@intCast(countOf(descriptor(self), .roots)));
}
fn mRequiresCount(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return g_api.mkSmallInt(@intCast(countOf(descriptor(self), .requires)));
}
fn mNativeRequiresCount(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return g_api.mkSmallInt(@intCast(countOf(descriptor(self), .native_requires)));
}
fn mCapabilitiesCount(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return g_api.mkSmallInt(@intCast(countOf(descriptor(self), .capabilities)));
}

fn itemAt(self: Oop, args: [*]const Oop, n: usize, kind: ListKind) Oop {
    if (n != 1 or g_api.vtableOf(args[0]) != g_api.smallint_vt) return g_api.nilObj;
    const index = g_api.smallIntValue(args[0]);
    if (index < 0) return g_api.nilObj;
    return valueAt(descriptor(self), kind, @intCast(index));
}
fn mRootAt(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    return itemAt(self, args, n, .roots);
}
fn mRequireAt(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    return itemAt(self, args, n, .requires);
}
fn mNativeRequireAt(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    return itemAt(self, args, n, .native_requires);
}
fn mCapabilityAt(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    return itemAt(self, args, n, .capabilities);
}

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    last_failure = api.nilObj;
    const reloading = api.isExported("Artifact") != 0;
    if (reloading) {
        artifact_vt = api.import("ArtifactVt");
        artifact_obj = api.import("Artifact");
        descriptor_vt = api.import("ArtifactDescriptorVt");
    } else {
        artifact_vt = api.proto(api.object_vt);
        artifact_obj = api.alloc(artifact_vt, 0);
        descriptor_vt = api.proto(api.object_vt);
    }
    const layout = [_]u8{idapi.slot_oop} ** 13;
    api.setLayout(descriptor_vt, &layout, layout.len, idapi.slot_raw);
    api.method(artifact_vt, api.intern("inspect:"), &mInspect);
    api.method(artifact_vt, api.intern("lastFailure"), &mLastFailure);
    api.method(descriptor_vt, api.intern("kind"), &mKind);
    api.method(descriptor_vt, api.intern("identity"), &mIdentity);
    api.method(descriptor_vt, api.intern("artifactAbi"), &mArtifactAbi);
    api.method(descriptor_vt, api.intern("target"), &mTarget);
    api.method(descriptor_vt, api.intern("roots"), &mRoots);
    api.method(descriptor_vt, api.intern("rootsCount"), &mRootsCount);
    api.method(descriptor_vt, api.intern("rootAt:"), &mRootAt);
    api.method(descriptor_vt, api.intern("requires"), &mRequires);
    api.method(descriptor_vt, api.intern("requiresCount"), &mRequiresCount);
    api.method(descriptor_vt, api.intern("requireAt:"), &mRequireAt);
    api.method(descriptor_vt, api.intern("nativeRequires"), &mNativeRequires);
    api.method(descriptor_vt, api.intern("nativeRequiresCount"), &mNativeRequiresCount);
    api.method(descriptor_vt, api.intern("nativeRequireAt:"), &mNativeRequireAt);
    api.method(descriptor_vt, api.intern("capabilities"), &mCapabilities);
    api.method(descriptor_vt, api.intern("capabilitiesCount"), &mCapabilitiesCount);
    api.method(descriptor_vt, api.intern("capabilityAt:"), &mCapabilityAt);
    api.method(descriptor_vt, api.intern("sourceDigest"), &mSourceDigest);
    api.method(descriptor_vt, api.intern("revisionDigest"), &mRevisionDigest);
    api.method(descriptor_vt, api.intern("path"), &mPath);
    if (!reloading) {
        _ = api.exportObj("ArtifactVt", artifact_vt);
        _ = api.exportObj("Artifact", artifact_obj);
        _ = api.exportObj("ArtifactDescriptorVt", descriptor_vt);
    }
}

fn moduleInit(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}

comptime {
    if (!@import("builtin").is_test) @export(&moduleInit, .{ .name = "module_init" });
}
