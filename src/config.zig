//! Project config for the interactive shell: `birnam.toml`.
//!
//! Discovered by walking the current directory upward to the filesystem root
//! (git-style) — the first `birnam.toml` found wins. It lets a project pin its
//! module set, extra dylib search dirs, and a snapshot to auto-load, so a bare
//! `birnam` in the project tree Just Works instead of needing a long argv.
//!
//! Recognised keys (a deliberately tiny TOML subset — string and string-array
//! values, `#` comments, `[section]` headers ignored):
//!
//!     modules     = ["peg", "lang"]   # ordered module load list (bare names ok)
//!     dylib_paths = ["build", "/opt/birnam/lib"]   # extra search dirs
//!     image       = "session.bim"     # snapshot to :load after modules
//!
//! Relative `dylib_paths` / `image` are resolved against the config file's own
//! directory, so a project is location-independent. All returned strings are
//! allocated on the caller's allocator and live for the process (never freed).

const std = @import("std");

pub const Config = struct {
    /// Directory containing the discovered `birnam.toml` (owned).
    dir: []const u8,
    /// `modules` key — null when absent (lets the shell distinguish "no key"
    /// from "explicitly empty", though both currently fall through to default).
    modules: ?[]const []const u8 = null,
    /// `dylib_paths`, each already joined against `dir` when relative.
    dylib_paths: []const []const u8 = &.{},
    /// `image` snapshot path, joined against `dir` when relative.
    image: ?[]const u8 = null,
};

/// Walk cwd upward for `birnam.toml`. Returns the parsed config, or null if
/// none is found (or on any read/parse error — config is best-effort).
pub fn discover(allocator: std.mem.Allocator) ?Config {
    var dir: []const u8 = std.process.getCwdAlloc(allocator) catch return null;
    while (true) {
        const path = std.fs.path.join(allocator, &.{ dir, "birnam.toml" }) catch return null;
        if (std.fs.cwd().access(path, .{})) {
            return parseFile(allocator, dir, path) catch null;
        } else |_| {}
        allocator.free(path);
        const parent = std.fs.path.dirname(dir) orelse return null;
        if (std.mem.eql(u8, parent, dir)) return null; // reached root
        dir = parent; // dirname is a slice of dir — safe to keep, dir outlives loop
    }
}

fn parseFile(allocator: std.mem.Allocator, dir: []const u8, path: []const u8) !Config {
    const src = try std.fs.cwd().readFileAlloc(allocator, path, 1 << 20);
    var cfg = Config{ .dir = dir };

    var lines = std.mem.splitScalar(u8, src, '\n');
    while (lines.next()) |raw| {
        const line = std.mem.trim(u8, raw, " \t\r");
        if (line.len == 0 or line[0] == '#' or line[0] == '[') continue;

        const eq = std.mem.indexOfScalar(u8, line, '=') orelse continue;
        const key = std.mem.trim(u8, line[0..eq], " \t");
        const val = std.mem.trim(u8, line[eq + 1 ..], " \t");

        if (std.mem.eql(u8, key, "modules")) {
            cfg.modules = try parseArray(allocator, val);
        } else if (std.mem.eql(u8, key, "dylib_paths")) {
            const dirs = try parseArray(allocator, val);
            cfg.dylib_paths = try joinAll(allocator, dir, dirs);
        } else if (std.mem.eql(u8, key, "image")) {
            if (parseString(val)) |s| cfg.image = try joinRel(allocator, dir, s);
        }
    }
    return cfg;
}

/// Extract a bare `"quoted string"` value (sans surrounding quotes). Returns
/// null if `val` isn't a single quoted string.
fn parseString(val: []const u8) ?[]const u8 {
    if (val.len >= 2 and val[0] == '"') {
        const end = std.mem.indexOfScalarPos(u8, val, 1, '"') orelse return null;
        return val[1..end];
    }
    return null;
}

/// Parse a single-line `["a", "b"]` string array. Tolerant: splits on commas,
/// strips quotes/whitespace, drops empties. Owned slice of owned strings.
fn parseArray(allocator: std.mem.Allocator, val: []const u8) ![]const []const u8 {
    var inner = std.mem.trim(u8, val, " \t");
    if (inner.len >= 1 and inner[0] == '[') inner = inner[1..];
    if (inner.len >= 1 and inner[inner.len - 1] == ']') inner = inner[0 .. inner.len - 1];

    var out = std.ArrayList([]const u8){};
    var it = std.mem.splitScalar(u8, inner, ',');
    while (it.next()) |tok| {
        const t = std.mem.trim(u8, tok, " \t\"");
        if (t.len == 0) continue;
        try out.append(allocator, try allocator.dupe(u8, t));
    }
    return out.toOwnedSlice(allocator);
}

fn joinAll(allocator: std.mem.Allocator, dir: []const u8, rels: []const []const u8) ![]const []const u8 {
    const out = try allocator.alloc([]const u8, rels.len);
    for (rels, 0..) |r, i| out[i] = try joinRel(allocator, dir, r);
    return out;
}

/// Join `rel` against `dir` unless it's already absolute. Owned.
fn joinRel(allocator: std.mem.Allocator, dir: []const u8, rel: []const u8) ![]const u8 {
    if (std.fs.path.isAbsolute(rel)) return allocator.dupe(u8, rel);
    return std.fs.path.join(allocator, &.{ dir, rel });
}
