//! Host policy for validating a pointer-free compiled-artifact description and
//! its sibling dependency closure before any candidate artifact is loaded.
//!
//! This module depends only on IdApi values produced by the inert artifact
//! inspector. It must remain usable before Store, Codegen, Module, Collection,
//! or an artifact initializer exists.

const std = @import("std");
const idapi = @import("idapi.zig");

const IdApi = idapi.IdApi;
const Oop = idapi.Oop;

pub const Preflight = struct {
    allocator: std.mem.Allocator,
    api: *IdApi,
    artifact: Oop,
    directory: []const u8,
    visiting: std.StringHashMap(void),
    inspected: std.StringHashMap(void),

    pub fn init(allocator: std.mem.Allocator, api: *IdApi, artifact: Oop, directory: []const u8) Preflight {
        return .{
            .allocator = allocator,
            .api = api,
            .artifact = artifact,
            .directory = directory,
            .visiting = std.StringHashMap(void).init(allocator),
            .inspected = std.StringHashMap(void).init(allocator),
        };
    }

    pub fn deinit(self: *Preflight) void {
        self.visiting.deinit();
        self.inspected.deinit();
    }

    fn diagnostic(kind: []const u8, path: []const u8) void {
        const stderr = std.fs.File.stderr();
        stderr.writeAll("birnam: artifact preflight failed: ") catch {};
        stderr.writeAll(kind) catch {};
        stderr.writeAll(": ") catch {};
        stderr.writeAll(path) catch {};
        stderr.writeAll("\n") catch {};
    }

    /// Validate a root and its complete sibling closure. `expected_identity` is
    /// deliberately optional: path-only launch has no independent identity to
    /// compare, while project/build operations do and must supply it.
    pub fn walkRoot(self: *Preflight, description: Oop, expected_identity: ?Oop) !void {
        try self.validateKindAndIdentity(description, self.directory);
        if (expected_identity) |expected| {
            const actual = self.api.send(description, self.api.intern("identity"), &.{}, 0);
            if (actual != expected) {
                diagnostic("artifactIdentityMismatch", self.directory);
                return error.ArtifactIdentityMismatch;
            }
        }
        try self.walk(description);
    }

    fn validateKindAndIdentity(self: *Preflight, description: Oop, path: []const u8) !void {
        if (self.api.send(description, self.api.intern("kind"), &.{}, 0) != self.api.intern("module")) {
            diagnostic("artifactKindMismatch", path);
            return error.ArtifactKindMismatch;
        }
        const identity = self.api.send(description, self.api.intern("identity"), &.{}, 0);
        if (self.api.vtableOf(identity) != self.api.symbol_vt) {
            diagnostic("artifactDescriptionMissing", path);
            return error.ArtifactDescriptionMissing;
        }
    }

    fn walk(self: *Preflight, description: Oop) !void {
        try self.validateKindAndIdentity(description, self.directory);
        const identity = self.api.send(description, self.api.intern("identity"), &.{}, 0);
        const name = std.mem.span(self.api.symbolName(identity));
        if (self.inspected.contains(name)) return;
        if (self.visiting.contains(name)) {
            diagnostic("circularDependency", name);
            return error.ArtifactDependencyCycle;
        }
        try self.visiting.put(name, {});
        defer _ = self.visiting.remove(name);

        const count_value = self.api.send(description, self.api.intern("requiresCount"), &.{}, 0);
        if (self.api.vtableOf(count_value) != self.api.smallint_vt) {
            diagnostic("artifactDescriptionMissing", name);
            return error.ArtifactDescriptionMissing;
        }
        const count = self.api.smallIntValue(count_value);
        if (count < 0 or count > 4096) {
            diagnostic("artifactDescriptionMissing", name);
            return error.ArtifactDescriptionMissing;
        }
        var index: i64 = 0;
        while (index < count) : (index += 1) {
            const requirement = self.api.send(description, self.api.intern("requireAt:"), &.{self.api.mkSmallInt(index)}, 1);
            if (self.api.vtableOf(requirement) != self.api.symbol_vt) {
                diagnostic("artifactDescriptionMissing", name);
                return error.ArtifactDescriptionMissing;
            }
            const required_name = std.mem.span(self.api.symbolName(requirement));
            const filename = try std.fmt.allocPrint(self.allocator, "libbirnam_{s}.dylib", .{required_name});
            defer self.allocator.free(filename);
            const path = try std.fs.path.join(self.allocator, &.{ self.directory, filename });
            defer self.allocator.free(path);
            const dependency = self.api.send(self.artifact, self.api.intern("inspect:"), &.{self.api.mkString(path.ptr, path.len)}, 1);
            if (dependency == self.api.nilObj) {
                const failure = self.api.send(self.artifact, self.api.intern("lastFailure"), &.{}, 0);
                const failure_name = if (failure != self.api.nilObj and self.api.vtableOf(failure) == self.api.symbol_vt)
                    std.mem.span(self.api.symbolName(failure))
                else
                    "artifactDescriptionMissing";
                diagnostic(failure_name, path);
                return error.ArtifactDependencyInvalid;
            }
            if (self.api.send(dependency, self.api.intern("identity"), &.{}, 0) != requirement) {
                diagnostic("artifactIdentityMismatch", path);
                return error.ArtifactIdentityMismatch;
            }
            try self.walk(dependency);
        }
        try self.inspected.put(name, {});
    }
};
