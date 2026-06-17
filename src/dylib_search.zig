//! Module-dylib search path for both the dynamic AOT runtime and the
//! interactive `birnam` shell.
//!
//! A module is named on the command line (or in `birnam.toml`) either as a
//! full path (`zig-out/lib/liblang.dylib`) or — preferably — as a *bare name*
//! (`lang`). Resolution turns a bare name into a real file by:
//!
//!   1. if the name as given exists as a path, use it verbatim (literal mode —
//!      preserves the historical `birnam path/to/lib.dylib` invocation)
//!   2. otherwise probe each search directory for a matching basename, trying
//!      both the name as-is and the platform-decorated form
//!      (`lang` -> `liblang.dylib` on macOS / `liblang.so` on Linux)
//!
//! Search directories, in order:
//!
//!   1. `extra_dirs` passed by the caller (e.g. `dylib_paths` from birnam.toml,
//!      resolved relative to the config file)
//!   2. each ':'-separated entry of `$BIRNAM_DYLIB_PATH`, in order (dirs that
//!      directly hold dylibs)
//!   3. each ':'-separated *install root* of `$BIRNAM_PATH` (Go-style), probed
//!      at `<root>/lib` then `<root>`
//!   4. the current working directory
//!   5. directories beneath the cwd (recursive; skips dotted dirs)
//!   6. the executable's own directory, then its sibling `../lib` — i.e. the
//!      standard install layout `<prefix>/bin/birnam` + `<prefix>/lib/`, so a
//!      packaged install is zero-config and a flat `binary + dylibs` bundle is
//!      portable regardless of where it's run from

const std = @import("std");
const builtin = @import("builtin");

const lib_prefix = builtin.target.os.tag.libPrefix(builtin.target.abi);
const lib_suffix = builtin.target.os.tag.dynamicLibSuffix();

/// Resolve a module dylib by name. Returns an owned path (caller frees) or
/// null if nothing matched. Convenience wrapper with no caller-supplied dirs —
/// used by the AOT runtime, whose only configuration is the environment.
pub fn resolve(allocator: std.mem.Allocator, name: []const u8) ?[]u8 {
    return resolveWith(allocator, name, &.{});
}

/// As `resolve`, but searches `extra_dirs` (highest priority) before the
/// environment / cwd / exe-dir chain. Used by the shell to honour the
/// `dylib_paths` from a discovered `birnam.toml`.
pub fn resolveWith(allocator: std.mem.Allocator, name: []const u8, extra_dirs: []const []const u8) ?[]u8 {
    // (1) Literal path: honour an exact name that already points at a file.
    if (std.fs.cwd().access(name, .{})) {
        return allocator.dupe(u8, name) catch null;
    } else |_| {}
    return searchDirs(allocator, name, extra_dirs);
}

fn searchDirs(allocator: std.mem.Allocator, name: []const u8, extra_dirs: []const []const u8) ?[]u8 {
    const base = std.fs.path.basename(name);

    // Candidate basenames, in priority order:
    //   0. the name as-is (covers full filenames / already-decorated names)
    //   1. the platform-decorated form:        lang -> liblang.dylib
    //   2. the birnam module convention:       lang -> libbirnam_lang.dylib
    // Built dylibs follow (2); (1) is the generic fallback for non-birnam libs.
    var cands_buf: [3][]const u8 = undefined;
    var n: usize = 1;
    cands_buf[0] = base;

    const decorated = std.fmt.allocPrint(allocator, "{s}{s}{s}", .{ lib_prefix, base, lib_suffix }) catch return null;
    defer allocator.free(decorated);
    if (!std.mem.eql(u8, decorated, base)) {
        cands_buf[n] = decorated;
        n += 1;
    }

    const conv = if (std.mem.startsWith(u8, base, "birnam_"))
        @as(?[]u8, null) // already prefixed — decorated form above covers it
    else
        std.fmt.allocPrint(allocator, "{s}birnam_{s}{s}", .{ lib_prefix, base, lib_suffix }) catch null;
    defer if (conv) |c| allocator.free(c);
    if (conv) |c| {
        cands_buf[n] = c;
        n += 1;
    }

    const cands = cands_buf[0..n];

    for (extra_dirs) |dir| {
        if (joinFirst(allocator, dir, cands)) |p| return p;
    }

    if (std.process.getEnvVarOwned(allocator, "BIRNAM_DYLIB_PATH")) |env| {
        defer allocator.free(env);
        var it = std.mem.tokenizeScalar(u8, env, ':');
        while (it.next()) |dir| {
            if (joinFirst(allocator, dir, cands)) |p| return p;
        }
    } else |_| {}

    // `$BIRNAM_PATH` — Go-style ':'-separated list of *install roots*. Each
    // root is probed at `<root>/lib` (the conventional layout, e.g. an install
    // prefix or a `zig-out`) then at `<root>` itself.
    if (std.process.getEnvVarOwned(allocator, "BIRNAM_PATH")) |env| {
        defer allocator.free(env);
        var it = std.mem.tokenizeScalar(u8, env, ':');
        while (it.next()) |root| {
            if (std.fs.path.join(allocator, &.{ root, "lib" })) |lib_dir| {
                defer allocator.free(lib_dir);
                if (joinFirst(allocator, lib_dir, cands)) |p| return p;
            } else |_| {}
            if (joinFirst(allocator, root, cands)) |p| return p;
        }
    } else |_| {}

    if (joinFirst(allocator, ".", cands)) |p| return p;
    if (searchTree(allocator, ".", cands)) |p| return p;

    // The binary's own dir, then its sibling `lib/` — i.e. the standard install
    // layout `<prefix>/bin/birnam` + `<prefix>/lib/`. This makes a packaged
    // install zero-config: a `birnam` in `<prefix>/bin` finds its modules in
    // `<prefix>/lib` with no `$BIRNAM_PATH` set. (The first form also covers a
    // flat `binary + dylibs` staging bundle.)
    if (std.fs.selfExeDirPathAlloc(allocator)) |exe_dir| {
        defer allocator.free(exe_dir);
        if (joinFirst(allocator, exe_dir, cands)) |p| return p;
        if (std.fs.path.join(allocator, &.{ exe_dir, "..", "lib" })) |sib| {
            defer allocator.free(sib);
            if (joinFirst(allocator, sib, cands)) |p| return p;
        } else |_| {}
    } else |_| {}

    return null;
}

/// Try each candidate basename in `dir`; return the first that exists.
fn joinFirst(allocator: std.mem.Allocator, dir: []const u8, cands: []const []const u8) ?[]u8 {
    for (cands) |base| {
        const path = std.fs.path.join(allocator, &.{ dir, base }) catch continue;
        std.fs.cwd().access(path, .{}) catch {
            allocator.free(path);
            continue;
        };
        return path;
    }
    return null;
}

/// Recursively search `root` for a file whose basename matches any candidate.
/// Skips dotted directory components (`.git`, etc.) so the walk stays cheap.
fn searchTree(allocator: std.mem.Allocator, root: []const u8, cands: []const []const u8) ?[]u8 {
    var dir = std.fs.cwd().openDir(root, .{ .iterate = true }) catch return null;
    defer dir.close();
    var walker = dir.walk(allocator) catch return null;
    defer walker.deinit();
    while (walker.next() catch return null) |entry| {
        if (entry.kind != .file) continue;
        if (std.mem.indexOf(u8, entry.path, "/.") != null) continue;
        if (std.mem.startsWith(u8, entry.path, ".")) continue;
        const eb = std.fs.path.basename(entry.path);
        for (cands) |base| {
            if (std.mem.eql(u8, eb, base)) {
                return std.fs.path.join(allocator, &.{ root, entry.path }) catch return null;
            }
        }
    }
    return null;
}
