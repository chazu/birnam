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
    /// `repl` key — names an alternate REPL front end (e.g. "linenoise"). When
    /// set, a bare interactive `birnam` hands the read-eval-print loop to the
    /// matching birnam-authored REPL instead of the built-in stdin loop.
    repl: ?[]const u8 = null,
    /// `[[wrapped]]` entries — first-party C libraries the `birnam wrap`
    /// subcommand builds into dylibs (via `cc`, never zig) for FFI wrapping.
    wrapped: []const WrappedLib = &.{},
};

/// One `[[wrapped]]` array-of-tables entry. Paths are joined against the config
/// dir when relative, so a wrapped lib is location-independent.
pub const WrappedLib = struct {
    /// `name` — derives the default output dylib (`lib<name>.<ext>`) and selects
    /// a single lib on `birnam wrap <name>`.
    name: []const u8,
    /// `sources` — C source files compiled together into the dylib (joined).
    sources: []const []const u8 = &.{},
    /// `wrapper` — the `.birnam` file holding the `Module load:`/`wrap:` decls
    /// (joined). Informational; the shell reports it after a successful build.
    wrapper: ?[]const u8 = null,
    /// `output` — dylib path to emit (joined). Default when absent:
    /// `lib<name>.<dylibext>` in the wrapper's dir, else the config dir.
    output: ?[]const u8 = null,
    /// `cflags` — extra flags passed to `cc` verbatim (not joined).
    cflags: []const []const u8 = &.{},
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

    // `[[wrapped]]` is an array-of-tables: each header opens a new entry that
    // subsequent key=val lines fill until the next header / EOF. A `pending`
    // raw entry is finalized (paths joined, defaults derived) into `wrapped`.
    var wrapped = std.ArrayList(WrappedLib){};
    var pending: ?WrappedRaw = null;
    // section state: are we inside a `[[wrapped]]` block (vs top-level)?
    var in_wrapped = false;

    var lines = std.mem.splitScalar(u8, src, '\n');
    while (lines.next()) |raw| {
        const line = std.mem.trim(u8, raw, " \t\r");
        if (line.len == 0 or line[0] == '#') continue;

        if (line[0] == '[') {
            // Section header. `[[wrapped]]` starts a new array entry; any other
            // `[section]` resets to top-level (its keys are ignored).
            if (std.mem.startsWith(u8, line, "[[wrapped]]")) {
                if (pending) |*p| try wrapped.append(allocator, try finalizeWrapped(allocator, dir, p));
                pending = WrappedRaw{};
                in_wrapped = true;
            } else {
                if (pending) |*p| try wrapped.append(allocator, try finalizeWrapped(allocator, dir, p));
                pending = null;
                in_wrapped = false;
            }
            continue;
        }

        const eq = std.mem.indexOfScalar(u8, line, '=') orelse continue;
        const key = std.mem.trim(u8, line[0..eq], " \t");
        const val = std.mem.trim(u8, line[eq + 1 ..], " \t");

        if (in_wrapped) {
            const p = &pending.?;
            if (std.mem.eql(u8, key, "name")) {
                if (parseString(val)) |s| p.name = try allocator.dupe(u8, s);
            } else if (std.mem.eql(u8, key, "sources")) {
                p.sources = try parseArray(allocator, val);
            } else if (std.mem.eql(u8, key, "wrapper")) {
                if (parseString(val)) |s| p.wrapper = try allocator.dupe(u8, s);
            } else if (std.mem.eql(u8, key, "output")) {
                if (parseString(val)) |s| p.output = try allocator.dupe(u8, s);
            } else if (std.mem.eql(u8, key, "cflags")) {
                p.cflags = try parseArray(allocator, val);
            }
            continue;
        }

        if (std.mem.eql(u8, key, "modules")) {
            cfg.modules = try parseArray(allocator, val);
        } else if (std.mem.eql(u8, key, "dylib_paths")) {
            const dirs = try parseArray(allocator, val);
            cfg.dylib_paths = try joinAll(allocator, dir, dirs);
        } else if (std.mem.eql(u8, key, "image")) {
            if (parseString(val)) |s| cfg.image = try joinRel(allocator, dir, s);
        } else if (std.mem.eql(u8, key, "repl")) {
            if (parseString(val)) |s| cfg.repl = try allocator.dupe(u8, s);
        }
    }
    if (pending) |*p| try wrapped.append(allocator, try finalizeWrapped(allocator, dir, p));
    cfg.wrapped = try wrapped.toOwnedSlice(allocator);
    return cfg;
}

/// Un-joined accumulator for a `[[wrapped]]` entry while parsing.
const WrappedRaw = struct {
    name: ?[]const u8 = null,
    sources: []const []const u8 = &.{},
    wrapper: ?[]const u8 = null,
    output: ?[]const u8 = null,
    cflags: []const []const u8 = &.{},
};

/// Join paths against `dir` and derive a default output dylib path.
fn finalizeWrapped(allocator: std.mem.Allocator, dir: []const u8, p: *const WrappedRaw) !WrappedLib {
    const name = p.name orelse "unnamed";
    const wrapper = if (p.wrapper) |w| try joinRel(allocator, dir, w) else null;
    const output = if (p.output) |o|
        try joinRel(allocator, dir, o)
    else blk: {
        // Default: lib<name>.<ext> beside the wrapper (else in the config dir).
        const base = if (wrapper) |w| std.fs.path.dirname(w) orelse dir else dir;
        const file = try std.fmt.allocPrint(allocator, "lib{s}{s}", .{ name, dylibExt() });
        break :blk try std.fs.path.join(allocator, &.{ base, file });
    };
    return .{
        .name = try allocator.dupe(u8, name),
        .sources = try joinAll(allocator, dir, p.sources),
        .wrapper = wrapper,
        .output = output,
        .cflags = p.cflags,
    };
}

/// Platform dynamic-library extension (with the leading dot).
pub fn dylibExt() []const u8 {
    return switch (@import("builtin").target.os.tag) {
        .macos => ".dylib",
        .windows => ".dll",
        else => ".so",
    };
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
