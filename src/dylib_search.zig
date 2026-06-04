//! Module-dylib search path for the dynamic AOT runtime.
//!
//! A `--aot=dynamic` binary loads its modules by `dlopen` at startup. The
//! image manifest records each module by the path used when it was saved, but
//! a portable binary must find the dylibs wherever the bundle was unpacked.
//! So we resolve by *basename* against a PATH-like search list:
//!
//!   1. each ':'-separated entry of `$BIRNAM_DYLIB_PATH`, in order
//!   2. the current working directory
//!   3. directories beneath the cwd (recursive; skips dotted dirs)
//!   4. the executable's own directory — so a staged `binary + dylibs` bundle
//!      is portable regardless of where it's run from
//!
//! The default (env unset) starts at the current dir and below, then falls
//! back to the exe dir for the staged-bundle case.

const std = @import("std");

/// Resolve a module dylib by basename. Returns an owned path (caller frees)
/// or null if no search entry contains it.
pub fn resolve(allocator: std.mem.Allocator, name: []const u8) ?[]u8 {
    const base = std.fs.path.basename(name);

    if (std.process.getEnvVarOwned(allocator, "BIRNAM_DYLIB_PATH")) |env| {
        defer allocator.free(env);
        var it = std.mem.tokenizeScalar(u8, env, ':');
        while (it.next()) |dir| {
            if (joinIfExists(allocator, dir, base)) |p| return p;
        }
    } else |_| {}

    if (joinIfExists(allocator, ".", base)) |p| return p;
    if (searchTree(allocator, ".", base)) |p| return p;

    if (std.fs.selfExeDirPathAlloc(allocator)) |exe_dir| {
        defer allocator.free(exe_dir);
        if (joinIfExists(allocator, exe_dir, base)) |p| return p;
    } else |_| {}

    return null;
}

fn joinIfExists(allocator: std.mem.Allocator, dir: []const u8, base: []const u8) ?[]u8 {
    const path = std.fs.path.join(allocator, &.{ dir, base }) catch return null;
    std.fs.cwd().access(path, .{}) catch {
        allocator.free(path);
        return null;
    };
    return path;
}

/// Recursively search `root` for a file named `base`. Skips dotted directory
/// components (`.git`, etc.) so the walk stays cheap and predictable.
fn searchTree(allocator: std.mem.Allocator, root: []const u8, base: []const u8) ?[]u8 {
    var dir = std.fs.cwd().openDir(root, .{ .iterate = true }) catch return null;
    defer dir.close();
    var walker = dir.walk(allocator) catch return null;
    defer walker.deinit();
    while (walker.next() catch return null) |entry| {
        if (entry.kind != .file) continue;
        if (std.mem.indexOf(u8, entry.path, "/.") != null) continue;
        if (std.mem.startsWith(u8, entry.path, ".")) continue;
        if (std.mem.eql(u8, std.fs.path.basename(entry.path), base)) {
            return std.fs.path.join(allocator, &.{ root, entry.path }) catch return null;
        }
    }
    return null;
}
