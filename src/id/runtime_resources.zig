//! Runtime resource discovery for installed and bundled Birnam.
//!
//! The runtime has one fixed, relocatable layout rooted at the executable's
//! prefix. Tests and advanced embedding retain narrow per-resource overrides;
//! ordinary execution has no manifest, launcher, cwd probe, or checkout fallback.

const std = @import("std");

pub const Overrides = struct {
    lib_dir: ?[]const u8 = null,
    id_dylib: ?[]const u8 = null,
};

pub const Resources = struct {
    allocator: std.mem.Allocator,
    prefix: []u8,
    lib_dir: []u8,
    source_dir: []u8,
    rust_source_dir: []u8,
    guide_dir: []u8,
    id_dylib: []u8,

    pub fn deinit(self: *Resources) void {
        self.allocator.free(self.prefix);
        self.allocator.free(self.lib_dir);
        self.allocator.free(self.source_dir);
        self.allocator.free(self.rust_source_dir);
        self.allocator.free(self.guide_dir);
        self.allocator.free(self.id_dylib);
        self.* = undefined;
    }
};

fn resolvedOverrideOr(
    a: std.mem.Allocator,
    override: ?[]const u8,
    defaults: []const []const u8,
) ![]u8 {
    if (override) |path| return std.fs.path.resolve(a, &.{path});
    return std.fs.path.resolve(a, defaults);
}

/// Pure test seam for the one supported runtime layout:
///
///   <prefix>/bin/birnam
///   <prefix>/lib/*.dylib
///   <prefix>/share/birnam/{src/id,src/rust/birnam-id/src,docs/guide}
///
/// Explicit overrides replace only their named resource. In particular an id
/// dylib override is an exact file, while a lib-dir override also supplies the
/// default id dylib when no exact override was provided.
pub fn resolveFromExeDir(
    a: std.mem.Allocator,
    exe_dir: []const u8,
    overrides: Overrides,
) !Resources {
    const prefix = try std.fs.path.resolve(a, &.{ exe_dir, ".." });
    errdefer a.free(prefix);
    const lib_dir = try resolvedOverrideOr(a, overrides.lib_dir, &.{ prefix, "lib" });
    errdefer a.free(lib_dir);
    const source_dir = try std.fs.path.resolve(a, &.{ prefix, "share", "birnam", "src", "id" });
    errdefer a.free(source_dir);
    const rust_source_dir = try std.fs.path.resolve(a, &.{ prefix, "share", "birnam", "src", "rust", "birnam-id", "src" });
    errdefer a.free(rust_source_dir);
    const guide_dir = try std.fs.path.resolve(a, &.{ prefix, "share", "birnam", "docs", "guide" });
    errdefer a.free(guide_dir);
    const id_dylib = try resolvedOverrideOr(a, overrides.id_dylib, &.{ lib_dir, "libbirnam_id.dylib" });
    errdefer a.free(id_dylib);
    return .{
        .allocator = a,
        .prefix = prefix,
        .lib_dir = lib_dir,
        .source_dir = source_dir,
        .rust_source_dir = rust_source_dir,
        .guide_dir = guide_dir,
        .id_dylib = id_dylib,
    };
}

pub fn resolve(a: std.mem.Allocator) !Resources {
    const exe_dir = try std.fs.selfExeDirPathAlloc(a);
    defer a.free(exe_dir);
    return resolveFromExeDir(a, exe_dir, .{
        .lib_dir = std.posix.getenv("BIRNAM_LIB_DIR"),
        .id_dylib = std.posix.getenv("BIRNAM_ID_DYLIB"),
    });
}

test "runtime resources use one normalized executable-relative layout" {
    var resources = try resolveFromExeDir(std.testing.allocator, "/opt/birnam/bin", .{});
    defer resources.deinit();
    try std.testing.expectEqualStrings("/opt/birnam", resources.prefix);
    try std.testing.expectEqualStrings("/opt/birnam/lib", resources.lib_dir);
    try std.testing.expectEqualStrings("/opt/birnam/share/birnam/src/id", resources.source_dir);
    try std.testing.expectEqualStrings("/opt/birnam/share/birnam/src/rust/birnam-id/src", resources.rust_source_dir);
    try std.testing.expectEqualStrings("/opt/birnam/share/birnam/docs/guide", resources.guide_dir);
    try std.testing.expectEqualStrings("/opt/birnam/lib/libbirnam_id.dylib", resources.id_dylib);
}

test "runtime resource overrides remain narrow and exact" {
    var resources = try resolveFromExeDir(std.testing.allocator, "/opt/birnam/bin", .{
        .lib_dir = "/tmp/runtime/lib/../lib",
        .id_dylib = "/tmp/floor/custom-id.dylib",
    });
    defer resources.deinit();
    try std.testing.expectEqualStrings("/tmp/runtime/lib", resources.lib_dir);
    try std.testing.expectEqualStrings("/tmp/floor/custom-id.dylib", resources.id_dylib);
    try std.testing.expectEqualStrings("/opt/birnam/share/birnam/src/id", resources.source_dir);
    try std.testing.expectEqualStrings("/opt/birnam/share/birnam/src/rust/birnam-id/src", resources.rust_source_dir);
}

test "a lib override supplies the default id dylib" {
    var resources = try resolveFromExeDir(std.testing.allocator, "/opt/birnam/bin", .{
        .lib_dir = "/tmp/runtime-lib",
    });
    defer resources.deinit();
    try std.testing.expectEqualStrings("/tmp/runtime-lib/libbirnam_id.dylib", resources.id_dylib);
}
