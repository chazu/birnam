//! Read-only source project resolution for `birnam check`.
//!
//! This is deliberately a host-side adapter. TOML is still parsed by Birnam's
//! self-hosted Toml module, while module declarations are inspected by the
//! existing full-CST Codegen scanner. The adapter owns filesystem traversal,
//! source/CST observation, and hashing; ProjectPolicy owns provider selection,
//! dependency closure, deterministic ordering, and diagnostics. It never
//! evaluates a project source or calls Module load:.

const std = @import("std");
const idapi = @import("idapi.zig");

const IdApi = idapi.IdApi;
const Oop = idapi.Oop;

const max_source_bytes = 16 << 20;
const FailureError = error{ProjectResolutionFailed};

pub const Diagnostic = struct {
    kind: []const u8,
    message: []const u8,
};

pub const Module = struct {
    name: []const u8,
    path: []const u8,
    sources: []const []const u8,
    source: []const u8,
    project_root: []const u8,
    requires: []const []const u8,
    source_digest: []const u8,
    revision_digest: []const u8,
};

pub const RunConfig = struct {
    repl: []const u8,
    socket_bind: []const u8,
    socket_port: i64,
};

/// A project-owned native wrapper. Each build entry is an argv vector; the
/// host substitutes `{output}` with a private staging path before execution.
/// The first slice intentionally keeps wrapper declarations on the root
/// project, matching the project-local build output and run directory.
pub const Wrapped = struct {
    name: []const u8,
    build: []const []const []const u8,
};

pub const Resolution = struct {
    project_root: []const u8,
    manifest_path: []const u8,
    project_name: []const u8,
    main_path: ?[]const u8,
    load_roots: []const []const u8,
    run: RunConfig,
    modules: []const Module,
    wrapped: []const Wrapped,
    digest: []const u8,
};

/// The arena lives in the result so both successful records and failure
/// diagnostics have one explicit lifetime. Callers must always deinit it.
pub const Result = struct {
    arena: std.heap.ArenaAllocator,
    resolution: ?Resolution,
    diagnostic: ?Diagnostic,

    pub fn deinit(self: *Result) void {
        self.arena.deinit();
    }
};

const Dependency = struct {
    name: []const u8,
    path: []const u8,
};

/// An explicit ordered source composition. `entry` owns the canonical
/// `Module named:requires:` declaration; `files` are source fragments that
/// become part of the same compiled unit in the order written.
pub const ModuleSpec = struct {
    entry: []const u8,
    files: []const []const u8,
};

const Manifest = struct {
    name: []const u8,
    version: ?[]const u8,
    main: ?[]const u8,
    source_roots: []const []const u8,
    loads: []const []const u8,
    modules: []const ModuleSpec,
    dependencies: []const Dependency,
    run: RunConfig,
    wrapped: []const Wrapped,
    digest: []const u8,
};

const Source = struct {
    name: []const u8,
    path: []const u8,
    sources: []const []const u8,
    source: []const u8,
    project_root: []const u8,
    requires: []const []const u8,
    source_digest: []const u8,
    revision_digest: []const u8,
};

const Project = struct {
    root: []const u8,
    manifest_path: []const u8,
    manifest: Manifest,
    sources: []const Source,
};

const State = struct {
    arena: std.mem.Allocator,
    api: *IdApi,
    projects: std.ArrayList(*Project) = .{},
    diagnostic_kind: []const u8 = "projectResolutionFailed",
    diagnostic_message: []const u8 = "project resolution failed",

    fn fail(self: *State, kind: []const u8, comptime fmt: []const u8, args: anytype) FailureError {
        self.diagnostic_kind = kind;
        self.diagnostic_message = std.fmt.allocPrint(self.arena, fmt, args) catch {
            self.diagnostic_kind = "projectResolutionFailed";
            self.diagnostic_message = "could not format project diagnostic";
            return FailureError.ProjectResolutionFailed;
        };
        return FailureError.ProjectResolutionFailed;
    }
};

fn lessString(_: void, a: []const u8, b: []const u8) bool {
    return std.mem.order(u8, a, b) == .lt;
}

fn sortStrings(items: []const []const u8) void {
    std.sort.heap([]const u8, @constCast(items), {}, lessString);
}

fn isString(api: *IdApi, value: Oop) bool {
    return value != null and api.vtableOf(value) == api.string_vt;
}

fn isSymbol(api: *IdApi, value: Oop) bool {
    return value != null and api.vtableOf(value) == api.symbol_vt;
}

fn isVector(api: *IdApi, value: Oop) bool {
    const vt = api.import("VectorVt") orelse return false;
    return value != null and api.vtableOf(value) == vt;
}

fn isMap(api: *IdApi, value: Oop) bool {
    const vt = api.import("MapVt") orelse return false;
    return value != null and api.vtableOf(value) == vt;
}

fn isError(api: *IdApi, value: Oop) bool {
    return value != null and api.isExported("Error") != 0 and
        api.send(value, api.intern("isError"), &.{}, 0) == api.trueObj;
}

fn global(state: *State, name: [:0]const u8) ?Oop {
    const globals = state.api.import("Globals") orelse return null;
    const value = state.api.send(globals, state.api.intern("at:"), &.{state.api.intern(name)}, 1);
    return if (value == state.api.nilObj) null else value;
}

fn stringBytes(api: *IdApi, value: Oop) []const u8 {
    var len: usize = 0;
    const ptr = api.stringBytes(value, &len);
    return ptr[0..len];
}

fn duplicateString(state: *State, value: Oop, field: []const u8) FailureError![]const u8 {
    if (!isString(state.api, value)) return state.fail("manifestValueInvalid", "{s} must be a string", .{field});
    return state.arena.dupe(u8, stringBytes(state.api, value)) catch
        return state.fail("projectResolutionFailed", "out of memory while reading {s}", .{field});
}

fn mapAt(state: *State, map: Oop, key: []const u8) FailureError!Oop {
    if (!isMap(state.api, map)) return state.fail("manifestMalformed", "expected a TOML table while reading {s}", .{key});
    const key_oop = state.api.mkString(key.ptr, key.len);
    return state.api.send(map, state.api.intern("at:"), &.{key_oop}, 1);
}

fn vectorSize(state: *State, vector: Oop, field: []const u8) FailureError!usize {
    if (!isVector(state.api, vector)) return state.fail("manifestValueInvalid", "{s} must be an array", .{field});
    const size = state.api.send(vector, state.api.intern("size"), &.{}, 0);
    if (state.api.isSmallInt(size) == 0 or state.api.smallIntValue(size) < 0)
        return state.fail("manifestValueInvalid", "could not read array {s}", .{field});
    return @intCast(state.api.smallIntValue(size));
}

fn newVector(state: *State) FailureError!Oop {
    const vector = state.api.import("Vector") orelse
        return state.fail("projectPolicyUnavailable", "Vector is not available while preparing project observations", .{});
    const result = state.api.send(vector, state.api.intern("new"), &.{}, 0);
    if (result == null or result == state.api.nilObj)
        return state.fail("projectPolicyUnavailable", "could not allocate a project observation vector", .{});
    return result;
}

fn vectorAdd(state: *State, vector: Oop, value: Oop) FailureError!Oop {
    const result = state.api.send(vector, state.api.intern("add:"), &.{value}, 1);
    if (result == null or result == state.api.nilObj)
        return state.fail("projectPolicyUnavailable", "could not append to a project observation vector", .{});
    return result;
}

fn newMap(state: *State) FailureError!Oop {
    const map = state.api.import("Map") orelse
        return state.fail("projectPolicyUnavailable", "Map is not available while preparing project observations", .{});
    const result = state.api.send(map, state.api.intern("new"), &.{}, 0);
    if (result == null or result == state.api.nilObj)
        return state.fail("projectPolicyUnavailable", "could not allocate a project observation map", .{});
    return result;
}

fn mapPut(state: *State, map: Oop, key: []const u8, value: Oop) FailureError!Oop {
    const key_value = state.api.mkString(key.ptr, key.len);
    const result = state.api.send(map, state.api.intern("at:put:"), &.{ key_value, value }, 2);
    if (result == null or result == state.api.nilObj)
        return state.fail("projectPolicyUnavailable", "could not write project observation field {s}", .{key});
    return result;
}

fn mapPutString(state: *State, map: Oop, key: []const u8, value: []const u8) FailureError!Oop {
    return mapPut(state, map, key, state.api.mkString(value.ptr, value.len));
}

fn mapPutStringVector(state: *State, map: Oop, key: []const u8, values: []const []const u8) FailureError!Oop {
    var vector = try newVector(state);
    for (values) |value| vector = try vectorAdd(state, vector, state.api.mkString(value.ptr, value.len));
    return mapPut(state, map, key, vector);
}

fn duplicateStringSlice(state: *State, values: []const []const u8) FailureError![]const []const u8 {
    const out = state.arena.alloc([]const u8, values.len) catch
        return state.fail("projectResolutionFailed", "out of memory while copying project values", .{});
    for (values, 0..) |value, i| {
        out[i] = state.arena.dupe(u8, value) catch
            return state.fail("projectResolutionFailed", "out of memory while copying project values", .{});
    }
    return out;
}

fn parseStringList(state: *State, value: Oop, field: []const u8, allow_single: bool) FailureError![]const []const u8 {
    if (value == state.api.nilObj) return state.arena.alloc([]const u8, 0) catch
        return state.fail("projectResolutionFailed", "out of memory while reading {s}", .{field});

    if (allow_single and isString(state.api, value)) {
        const one = try duplicateString(state, value, field);
        const out = state.arena.alloc([]const u8, 1) catch
            return state.fail("projectResolutionFailed", "out of memory while reading {s}", .{field});
        out[0] = one;
        return out;
    }

    const n = try vectorSize(state, value, field);
    const out = state.arena.alloc([]const u8, n) catch
        return state.fail("projectResolutionFailed", "out of memory while reading {s}", .{field});
    for (0..n) |i| {
        const item = state.api.send(value, state.api.intern("at:"), &.{state.api.mkSmallInt(@intCast(i))}, 1);
        out[i] = try duplicateString(state, item, field);
    }
    return out;
}

fn parseWrapped(state: *State, parsed: Oop) FailureError![]const Wrapped {
    const wrapped_value = try mapAt(state, parsed, "wrapped");
    if (wrapped_value == state.api.nilObj) return state.arena.alloc(Wrapped, 0) catch
        return state.fail("projectResolutionFailed", "out of memory while reading wrapped declarations", .{});

    const count = try vectorSize(state, wrapped_value, "wrapped");
    const wrapped = state.arena.alloc(Wrapped, count) catch
        return state.fail("projectResolutionFailed", "out of memory while reading wrapped declarations", .{});
    for (0..count) |i| {
        const item = state.api.send(wrapped_value, state.api.intern("at:"), &.{state.api.mkSmallInt(@intCast(i))}, 1);
        if (!isMap(state.api, item)) return state.fail("manifestValueInvalid", "wrapped entries must be TOML tables", .{});
        const name = try duplicateString(state, try mapAt(state, item, "name"), "wrapped.name");
        if (name.len == 0 or std.mem.indexOfScalar(u8, name, '/') != null or std.fs.path.isAbsolute(name))
            return state.fail("manifestValueInvalid", "wrapped.name must be a non-empty logical name without path separators", .{});

        const build_value = try mapAt(state, item, "build");
        if (build_value == state.api.nilObj)
            return state.fail("manifestValueInvalid", "wrapped.{s} must declare build commands", .{name});
        const command_count = try vectorSize(state, build_value, "wrapped.build");
        if (command_count == 0) return state.fail("manifestValueInvalid", "wrapped.{s}.build must not be empty", .{name});
        const commands = state.arena.alloc([]const []const u8, command_count) catch
            return state.fail("projectResolutionFailed", "out of memory while reading wrapped build commands", .{});
        var has_output = false;
        for (0..command_count) |command_index| {
            const command = state.api.send(build_value, state.api.intern("at:"), &.{state.api.mkSmallInt(@intCast(command_index))}, 1);
            commands[command_index] = try parseStringList(state, command, "wrapped.build command", false);
            if (commands[command_index].len == 0)
                return state.fail("manifestValueInvalid", "wrapped.{s}.build commands must not be empty", .{name});
            for (commands[command_index]) |argument| {
                if (std.mem.indexOf(u8, argument, "{output}") != null) has_output = true;
            }
        }
        if (!has_output)
            return state.fail("manifestValueInvalid", "wrapped.{s}.build must use {{output}} for the dylib output path", .{name});
        wrapped[i] = .{ .name = name, .build = commands };
    }
    for (wrapped, 0..) |entry, i| for (wrapped[0..i]) |old| if (std.mem.eql(u8, old.name, entry.name))
        return state.fail("duplicateWrappedDeclaration", "wrapped library {s} is declared more than once", .{entry.name});
    return wrapped;
}

fn parseModuleSpecs(state: *State, parsed: Oop) FailureError![]const ModuleSpec {
    const modules_value = try mapAt(state, parsed, "module");
    if (modules_value == state.api.nilObj) return state.arena.alloc(ModuleSpec, 0) catch
        return state.fail("projectResolutionFailed", "out of memory while reading module compositions", .{});

    const count = try vectorSize(state, modules_value, "module");
    const modules = state.arena.alloc(ModuleSpec, count) catch
        return state.fail("projectResolutionFailed", "out of memory while reading module compositions", .{});
    for (0..count) |i| {
        const item = state.api.send(modules_value, state.api.intern("at:"), &.{state.api.mkSmallInt(@intCast(i))}, 1);
        if (!isMap(state.api, item)) return state.fail("manifestValueInvalid", "module entries must be TOML tables", .{});

        const entry = try duplicateString(state, try mapAt(state, item, "entry"), "module.entry");
        if (entry.len == 0 or std.fs.path.isAbsolute(entry) or !std.mem.endsWith(u8, entry, ".birnam"))
            return state.fail("manifestValueInvalid", "module.entry must be a non-empty project-relative .birnam path", .{});

        const files_value = try mapAt(state, item, "files");
        const files = try parseStringList(state, files_value, "module.files", false);
        for (files) |path| {
            if (path.len == 0 or std.fs.path.isAbsolute(path) or !std.mem.endsWith(u8, path, ".birnam"))
                return state.fail("manifestValueInvalid", "module.files must contain project-relative .birnam paths", .{});
            if (std.mem.eql(u8, path, entry))
                return state.fail("duplicateModulePart", "module entry {s} is also listed as a source part", .{entry});
        }
        for (files, 0..) |path, file_index| for (files[0..file_index]) |old| if (std.mem.eql(u8, old, path))
            return state.fail("duplicateModulePart", "module source part {s} is listed more than once", .{path});

        modules[i] = .{ .entry = entry, .files = files };
    }
    for (modules, 0..) |entry, i| {
        for (modules[0..i]) |old| {
            if (std.mem.eql(u8, old.entry, entry.entry))
                return state.fail("duplicateModuleComposition", "module entry {s} is declared more than once", .{entry.entry});
            for (entry.files) |path| for (old.files) |old_path| if (std.mem.eql(u8, old_path, path))
                return state.fail("duplicateModulePart", "module source part {s} belongs to more than one composition", .{path});
            for (entry.files) |path| if (std.mem.eql(u8, old.entry, path))
                return state.fail("duplicateModulePart", "module entry {s} is also a source part", .{old.entry});
            for (old.files) |path| if (std.mem.eql(u8, entry.entry, path))
                return state.fail("duplicateModulePart", "module entry {s} is also a source part", .{entry.entry});
        }
    }
    return modules;
}

fn parseRunConfig(state: *State, parsed: Oop) FailureError!RunConfig {
    const run_value = try mapAt(state, parsed, "run");
    if (run_value == state.api.nilObj) return .{
        .repl = state.arena.dupe(u8, "none") catch return state.fail("projectResolutionFailed", "out of memory while reading run config", .{}),
        .socket_bind = state.arena.dupe(u8, "127.0.0.1") catch return state.fail("projectResolutionFailed", "out of memory while reading run config", .{}),
        .socket_port = 4400,
    };
    if (!isMap(state.api, run_value)) return state.fail("manifestValueInvalid", "run must be a TOML table", .{});

    const repl_value = try mapAt(state, run_value, "repl");
    const repl = if (repl_value == state.api.nilObj)
        state.arena.dupe(u8, "none") catch return state.fail("projectResolutionFailed", "out of memory while reading run config", .{})
    else
        try duplicateString(state, repl_value, "run.repl");
    if (!std.mem.eql(u8, repl, "none") and !std.mem.eql(u8, repl, "tty") and !std.mem.eql(u8, repl, "socket"))
        return state.fail("manifestValueInvalid", "run.repl must be one of none, tty, or socket", .{});

    const bind_value = try mapAt(state, run_value, "socket_bind");
    const socket_bind = if (bind_value == state.api.nilObj)
        state.arena.dupe(u8, "127.0.0.1") catch return state.fail("projectResolutionFailed", "out of memory while reading run config", .{})
    else
        try duplicateString(state, bind_value, "run.socket_bind");
    if (socket_bind.len == 0) return state.fail("manifestValueInvalid", "run.socket_bind must not be empty", .{});

    const port_value = try mapAt(state, run_value, "socket_port");
    const socket_port = if (port_value == state.api.nilObj) 4400 else blk: {
        if (state.api.isSmallInt(port_value) == 0)
            return state.fail("manifestValueInvalid", "run.socket_port must be an integer from 0 to 65535", .{});
        const port = state.api.smallIntValue(port_value);
        if (port < 0 or port > 65535)
            return state.fail("manifestValueInvalid", "run.socket_port must be an integer from 0 to 65535", .{});
        break :blk port;
    };
    return .{ .repl = repl, .socket_bind = socket_bind, .socket_port = socket_port };
}

fn sha256Hex(state: *State, bytes: []const u8) []const u8 {
    var digest: [32]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(bytes, &digest, .{});
    return state.arena.dupe(u8, &std.fmt.bytesToHex(digest, .lower)) catch @panic("project: oom");
}

fn hasDotPathComponent(path: []const u8) bool {
    var parts = std.mem.splitScalar(u8, path, '/');
    while (parts.next()) |part| if (std.mem.eql(u8, part, ".") or std.mem.eql(u8, part, "..")) return true;
    return false;
}

/// Hash the literal files referenced by embedFile:. The source digest exposed
/// by project resolution includes this value, so changing an asset invalidates
/// the owning dylib even when its Birnam source is unchanged.
fn embeddedDigest(state: *State, codegen: Oop, source: []const u8, project_root: []const u8, source_path: []const u8) FailureError![]const u8 {
    const files = state.api.send(codegen, state.api.intern("embeddedFilesOf:"), &.{state.api.mkString(source.ptr, source.len)}, 1);
    const count = try vectorSize(state, files, "embedded files");
    var hasher = std.crypto.hash.sha2.Sha256.init(.{});
    const add = struct {
        fn field(h: *std.crypto.hash.sha2.Sha256, value: []const u8) void {
            var len: [8]u8 = undefined;
            std.mem.writeInt(u64, &len, value.len, .big);
            h.update(&len);
            h.update(value);
        }
    }.field;
    add(&hasher, "birnam.embedded-inputs.v1");
    for (0..count) |i| {
        const item = state.api.send(files, state.api.intern("at:"), &.{state.api.mkSmallInt(@intCast(i))}, 1);
        const relative = try duplicateString(state, item, "embedded file path");
        if (relative.len == 0 or std.fs.path.isAbsolute(relative) or hasDotPathComponent(relative))
            return state.fail("embeddedPathInvalid", "embedFile: path {s} in {s} must be a non-empty relative path without '.' or '..'", .{ relative, source_path });
        const full = try joinPath(state, project_root, relative);
        const bytes = std.fs.cwd().readFileAlloc(state.arena, full, max_source_bytes) catch
            return state.fail("embeddedFileMissing", "could not read embedFile: path {s} in {s}", .{ relative, source_path });
        add(&hasher, relative);
        add(&hasher, bytes);
    }
    var digest: [32]u8 = undefined;
    hasher.final(&digest);
    return state.arena.dupe(u8, &std.fmt.bytesToHex(digest, .lower)) catch @panic("project: oom");
}

fn sourceDigestWithEmbedded(state: *State, source_digest: []const u8, assets_digest: []const u8) []const u8 {
    var hasher = std.crypto.hash.sha2.Sha256.init(.{});
    hasher.update("birnam.source-with-embedded-inputs.v1");
    var source_len: [8]u8 = undefined;
    std.mem.writeInt(u64, &source_len, source_digest.len, .big);
    hasher.update(&source_len);
    hasher.update(source_digest);
    var assets_len: [8]u8 = undefined;
    std.mem.writeInt(u64, &assets_len, assets_digest.len, .big);
    hasher.update(&assets_len);
    hasher.update(assets_digest);
    var digest: [32]u8 = undefined;
    hasher.final(&digest);
    return state.arena.dupe(u8, &std.fmt.bytesToHex(digest, .lower)) catch @panic("project: oom");
}

fn parseManifest(state: *State, manifest_path: []const u8, bytes: []const u8) FailureError!Manifest {
    const toml = global(state, "Toml") orelse
        return state.fail("manifestMalformed", "Toml is not available while reading {s}", .{manifest_path});
    const parsed = state.api.send(toml, state.api.intern("parse:"), &.{state.api.mkString(bytes.ptr, bytes.len)}, 1);
    if (isError(state.api, parsed)) return state.fail("manifestMalformed", "could not parse {s} as TOML", .{manifest_path});
    if (!isMap(state.api, parsed)) return state.fail("manifestMalformed", "{s} did not produce a TOML table", .{manifest_path});

    const name_value = try mapAt(state, parsed, "name");
    const name = if (name_value == state.api.nilObj)
        ""
    else
        try duplicateString(state, name_value, "name");

    const version_value = try mapAt(state, parsed, "version");
    const version = if (version_value == state.api.nilObj)
        null
    else
        try duplicateString(state, version_value, "version");

    const main_value = try mapAt(state, parsed, "main");
    const main = if (main_value == state.api.nilObj)
        null
    else
        try duplicateString(state, main_value, "main");
    if (main) |main_path| if (std.fs.path.isAbsolute(main_path))
        return state.fail("manifestValueInvalid", "main must be project-relative", .{});

    const roots_value = try mapAt(state, parsed, "source_roots");
    const source_roots = if (roots_value == state.api.nilObj) blk: {
        const defaults = [_][]const u8{"src"};
        break :blk try duplicateStringSlice(state, &defaults);
    } else try parseStringList(state, roots_value, "source_roots", false);
    if (source_roots.len == 0) return state.fail("manifestValueInvalid", "source_roots must not be empty", .{});
    for (source_roots) |source_root| if (std.fs.path.isAbsolute(source_root))
        return state.fail("manifestValueInvalid", "source_roots must be project-relative", .{});

    const load_value = try mapAt(state, parsed, "load");
    const loads = try parseStringList(state, load_value, "load", true);
    const run_config = try parseRunConfig(state, parsed);
    const wrapped = try parseWrapped(state, parsed);
    const modules = try parseModuleSpecs(state, parsed);

    const dep_value = try mapAt(state, parsed, "dependencies");
    var dependencies_list: std.ArrayList(Dependency) = .{};
    if (dep_value != state.api.nilObj) {
        if (!isMap(state.api, dep_value)) return state.fail("manifestValueInvalid", "dependencies must be a TOML table", .{});
        const keys = state.api.send(dep_value, state.api.intern("keys"), &.{}, 0);
        const key_count = try vectorSize(state, keys, "dependencies");
        for (0..key_count) |i| {
            const key = state.api.send(keys, state.api.intern("at:"), &.{state.api.mkSmallInt(@intCast(i))}, 1);
            const dep_name = try duplicateString(state, key, "dependency name");
            const spec = try mapAt(state, dep_value, dep_name);
            const path_value = try mapAt(state, spec, "path");
            const dep_path = try duplicateString(state, path_value, "dependency path");
            if (std.fs.path.isAbsolute(dep_path))
                return state.fail("dependencyOriginInvalid", "dependency #{s} uses an absolute path", .{dep_name});
            dependencies_list.append(state.arena, .{ .name = dep_name, .path = dep_path }) catch
                return state.fail("projectResolutionFailed", "out of memory while reading dependencies", .{});
        }
    }
    const dependencies = dependencies_list.toOwnedSlice(state.arena) catch
        return state.fail("projectResolutionFailed", "out of memory while reading dependencies", .{});
    std.sort.heap(Dependency, dependencies, {}, struct {
        fn lessThan(_: void, a: Dependency, b: Dependency) bool {
            return std.mem.order(u8, a.name, b.name) == .lt;
        }
    }.lessThan);

    return .{
        .name = name,
        .version = version,
        .main = main,
        .source_roots = source_roots,
        .loads = loads,
        .modules = modules,
        .dependencies = dependencies,
        .run = run_config,
        .wrapped = wrapped,
        .digest = sha256Hex(state, bytes),
    };
}

fn joinPath(state: *State, left: []const u8, right: []const u8) FailureError![]const u8 {
    return std.fs.path.join(state.arena, &.{ left, right }) catch
        return state.fail("projectResolutionFailed", "out of memory while joining paths", .{});
}

fn canonicalPath(state: *State, path: []const u8, kind: []const u8) FailureError![]const u8 {
    return std.fs.cwd().realpathAlloc(state.arena, path) catch
        return state.fail(kind, "could not resolve path {s}", .{path});
}

fn readManifest(state: *State, root: []const u8) FailureError!struct { path: []const u8, bytes: []u8 } {
    const path = try joinPath(state, root, "birnam.toml");
    const bytes = std.fs.cwd().readFileAlloc(state.arena, path, max_source_bytes) catch
        return state.fail("dependencyOriginInvalid", "project {s} has no readable birnam.toml", .{root});
    return .{ .path = path, .bytes = bytes };
}

fn collectSourceFiles(state: *State, root: []const u8, files: *std.ArrayList([]const u8)) FailureError!void {
    var dir = std.fs.cwd().openDir(root, .{ .iterate = true }) catch
        return state.fail("dependencyOriginInvalid", "source root {s} is not readable", .{root});
    defer dir.close();

    var iterator = dir.iterate();
    while (true) {
        const entry = iterator.next() catch
            return state.fail("sourceMalformed", "could not enumerate source root {s}", .{root});
        const item = entry orelse break;
        const path = try joinPath(state, root, item.name);
        switch (item.kind) {
            .directory => try collectSourceFiles(state, path, files),
            .file => if (std.mem.endsWith(u8, item.name, ".birnam")) {
                files.append(state.arena, path) catch
                    return state.fail("projectResolutionFailed", "out of memory while indexing sources", .{});
            },
            else => {},
        }
    }
}

fn metadataName(state: *State, metadata: Oop) FailureError![]const u8 {
    const name = state.api.send(metadata, state.api.intern("at:"), &.{state.api.mkSmallInt(1)}, 1);
    if (!isSymbol(state.api, name)) return state.fail("sourceMalformed", "module metadata has a non-symbol name", .{});
    return state.arena.dupe(u8, std.mem.span(state.api.symbolName(name))) catch
        return state.fail("projectResolutionFailed", "out of memory while reading module metadata", .{});
}

fn metadataRequires(state: *State, metadata: Oop) FailureError![]const []const u8 {
    const requires = state.api.send(metadata, state.api.intern("at:"), &.{state.api.mkSmallInt(2)}, 1);
    const n = try vectorSize(state, requires, "module requires");
    const out = state.arena.alloc([]const u8, n) catch
        return state.fail("projectResolutionFailed", "out of memory while reading module dependencies", .{});
    for (0..n) |i| {
        const name = state.api.send(requires, state.api.intern("at:"), &.{state.api.mkSmallInt(@intCast(i))}, 1);
        if (!isSymbol(state.api, name)) return state.fail("sourceMalformed", "module requires contains a non-symbol", .{});
        out[i] = state.arena.dupe(u8, std.mem.span(state.api.symbolName(name))) catch
            return state.fail("projectResolutionFailed", "out of memory while reading module dependencies", .{});
    }
    return out;
}

fn metadataString(state: *State, metadata: Oop, index: i64, label: []const u8) FailureError![]const u8 {
    const value = state.api.send(metadata, state.api.intern("at:"), &.{state.api.mkSmallInt(index)}, 1);
    return duplicateString(state, value, label);
}

fn containsPath(paths: []const []const u8, path: []const u8) bool {
    for (paths) |candidate| if (std.mem.eql(u8, candidate, path)) return true;
    return false;
}

fn composeSource(state: *State, paths: []const []const u8) FailureError![]const u8 {
    const module = global(state, "Module") orelse
        return state.fail("moduleCompositionUnavailable", "Module composeSources: is not available while resolving sources", .{});
    const vector_type = state.api.import("Vector") orelse
        return state.fail("moduleCompositionUnavailable", "Vector is not available while resolving sources", .{});
    var source_paths = state.api.send(vector_type, state.api.intern("new"), &.{}, 0);
    for (paths) |path| {
        const path_value = state.api.mkString(path.ptr, path.len);
        source_paths = state.api.send(source_paths, state.api.intern("add:"), &.{path_value}, 1);
    }
    const result = state.api.send(module, state.api.intern("composeSources:"), &.{source_paths}, 1);
    if (isError(state.api, result))
        return state.fail("sourceReadFailed", "could not compose module source at {s}", .{paths[0]});
    if (!isString(state.api, result))
        return state.fail("sourceReadFailed", "module composition at {s} did not return source text", .{paths[0]});
    const bytes = stringBytes(state.api, result);
    if (bytes.len > max_source_bytes)
        return state.fail("sourceTooLarge", "composed module source exceeds {d} bytes at {s}", .{ max_source_bytes, paths[0] });
    return state.arena.dupe(u8, bytes) catch
        return state.fail("projectResolutionFailed", "out of memory while recording composed module source", .{});
}

fn moduleMetadata(state: *State, codegen: Oop, source: []const u8, path: []const u8) FailureError!Oop {
    const metadata = state.api.send(codegen, state.api.intern("moduleMetadataOf:"), &.{state.api.mkString(source.ptr, source.len)}, 1);
    if (metadata == state.api.nilObj)
        return state.fail("sourceMalformed", "module entry {s} has no valid static module declaration", .{path});
    if (isError(state.api, metadata))
        return state.fail("sourceMalformed", "module entry {s} has an invalid static module declaration", .{path});
    return metadata;
}

fn rejectPartDeclaration(state: *State, codegen: Oop, path: []const u8) FailureError!void {
    const source = try composeSource(state, &.{path});
    const metadata = state.api.send(codegen, state.api.intern("moduleMetadataOf:"), &.{state.api.mkString(source.ptr, source.len)}, 1);
    if (isError(state.api, metadata))
        return state.fail("sourceMalformed", "module source part {s} has an invalid static module declaration", .{path});
    if (metadata != state.api.nilObj)
        return state.fail("modulePartDeclaration", "module source part {s} must not declare a module; put Module named:requires: in the entry", .{path});
}

fn appendIndexedSource(state: *State, project: *Project, codegen: Oop, sources: *std.ArrayList(Source), paths: []const []const u8) FailureError!void {
    const source = try composeSource(state, paths);
    const metadata = try moduleMetadata(state, codegen, source, paths[0]);
    const name = try metadataName(state, metadata);
    for (sources.items) |old| if (std.mem.eql(u8, old.name, name))
        return state.fail("duplicateModuleProvider", "module #{s} is provided by both {s} and {s}", .{ name, old.path, paths[0] });
    const requires = try metadataRequires(state, metadata);
    const metadata_source_digest = try metadataString(state, metadata, 3, "source digest");
    const assets_digest = try embeddedDigest(state, codegen, source, project.root, paths[0]);
    const source_digest = sourceDigestWithEmbedded(state, metadata_source_digest, assets_digest);
    const source_paths = state.arena.alloc([]const u8, paths.len) catch
        return state.fail("projectResolutionFailed", "out of memory while recording module source paths", .{});
    for (paths, 0..) |path, i| source_paths[i] = path;
    sources.append(state.arena, .{
        .name = name,
        .path = paths[0],
        .sources = source_paths,
        .source = source,
        .project_root = project.root,
        .requires = requires,
        .source_digest = source_digest,
        .revision_digest = try metadataString(state, metadata, 4, "revision digest"),
    }) catch return state.fail("projectResolutionFailed", "out of memory while indexing sources", .{});
}

fn indexProject(state: *State, project: *Project) FailureError!void {
    var files: std.ArrayList([]const u8) = .{};
    for (project.manifest.source_roots) |root_name| {
        const root = try joinPath(state, project.root, root_name);
        try collectSourceFiles(state, root, &files);
    }
    sortStrings(files.items);

    const codegen = state.api.import("Codegen") orelse
        return state.fail("projectResolutionFailed", "Codegen is not available for source indexing", .{});

    var sources: std.ArrayList(Source) = .{};
    var explicit_paths: std.ArrayList([]const u8) = .{};
    for (project.manifest.modules) |spec| {
        const paths = state.arena.alloc([]const u8, 1 + spec.files.len) catch
            return state.fail("projectResolutionFailed", "out of memory while preparing module composition", .{});
        paths[0] = try joinPath(state, project.root, spec.entry);
        if (containsPath(explicit_paths.items, paths[0]))
            return state.fail("duplicateModulePart", "module source {s} belongs to more than one composition", .{paths[0]});
        explicit_paths.append(state.arena, paths[0]) catch
            return state.fail("projectResolutionFailed", "out of memory while tracking module composition", .{});
        for (spec.files, 0..) |file, i| {
            paths[i + 1] = try joinPath(state, project.root, file);
            if (containsPath(explicit_paths.items, paths[i + 1]))
                return state.fail("duplicateModulePart", "module source {s} belongs to more than one composition", .{paths[i + 1]});
            explicit_paths.append(state.arena, paths[i + 1]) catch
                return state.fail("projectResolutionFailed", "out of memory while tracking module composition", .{});
            try rejectPartDeclaration(state, codegen, paths[i + 1]);
        }
        try appendIndexedSource(state, project, codegen, &sources, paths);
    }

    for (files.items) |path| {
        if (containsPath(explicit_paths.items, path)) continue;
        const source = try composeSource(state, &.{path});
        const metadata = state.api.send(codegen, state.api.intern("moduleMetadataOf:"), &.{state.api.mkString(source.ptr, source.len)}, 1);
        if (metadata == state.api.nilObj) continue; // legacy or runtime-dynamic source
        if (isError(state.api, metadata)) return state.fail("sourceMalformed", "source {s} has no valid static module declaration", .{path});
        try appendIndexedSource(state, project, codegen, &sources, &.{path});
    }
    project.sources = sources.toOwnedSlice(state.arena) catch
        return state.fail("projectResolutionFailed", "out of memory while indexing sources", .{});
}

fn findProject(state: *State, root: []const u8) ?*Project {
    for (state.projects.items) |project| if (std.mem.eql(u8, project.root, root)) return project;
    return null;
}

fn loadProjectAt(state: *State, root_path: []const u8) FailureError!*Project {
    const root = try canonicalPath(state, root_path, "dependencyOriginInvalid");
    if (findProject(state, root)) |project| return project;

    const project = state.arena.create(Project) catch
        return state.fail("projectResolutionFailed", "out of memory while creating a project", .{});
    project.* = undefined;
    state.projects.append(state.arena, project) catch
        return state.fail("projectResolutionFailed", "out of memory while tracking projects", .{});

    const manifest_file = try readManifest(state, root);
    project.root = root;
    project.manifest_path = manifest_file.path;
    project.manifest = try parseManifest(state, manifest_file.path, manifest_file.bytes);
    project.sources = &.{};
    try indexProject(state, project);
    return project;
}

fn loadDependencyClosure(state: *State, project: *Project) FailureError!void {
    for (project.manifest.dependencies) |dependency| {
        const dependency_root = try joinPath(state, project.root, dependency.path);
        const dependency_project = try loadProjectAt(state, dependency_root);
        try loadDependencyClosure(state, dependency_project);
    }
}

fn observedRun(state: *State, run_config: RunConfig) FailureError!Oop {
    var run_map = try newMap(state);
    run_map = try mapPutString(state, run_map, "repl", run_config.repl);
    run_map = try mapPutString(state, run_map, "socketBind", run_config.socket_bind);
    run_map = try mapPut(state, run_map, "socketPort", state.api.mkSmallInt(run_config.socket_port));
    return run_map;
}

fn observedWrapped(state: *State, wrapped: []const Wrapped) FailureError!Oop {
    var result = try newVector(state);
    for (wrapped) |entry| {
        var item = try newMap(state);
        item = try mapPutString(state, item, "name", entry.name);
        var commands = try newVector(state);
        for (entry.build) |command| {
            var argv = try newVector(state);
            for (command) |argument| argv = try vectorAdd(state, argv, state.api.mkString(argument.ptr, argument.len));
            commands = try vectorAdd(state, commands, argv);
        }
        item = try mapPut(state, item, "build", commands);
        result = try vectorAdd(state, result, item);
    }
    return result;
}

fn observedProject(state: *State, project: *Project, root: *Project) FailureError!Oop {
    var result = try newMap(state);
    result = try mapPutString(state, result, "root", project.root);
    result = try mapPutString(state, result, "manifest", project.manifest_path);
    result = try mapPutString(state, result, "name", project.manifest.name);
    result = try mapPut(state, result, "main", state.api.nilObj);
    result = try mapPutStringVector(state, result, "loads", project.manifest.loads);
    result = try mapPut(state, result, "run", try observedRun(state, project.manifest.run));
    result = try mapPut(state, result, "wrapped", try observedWrapped(state, project.manifest.wrapped));

    var dependencies = try newVector(state);
    for (project.manifest.dependencies) |dependency| {
        const dependency_root = try joinPath(state, project.root, dependency.path);
        const dependency_project = try loadProjectAt(state, dependency_root);
        var item = try newMap(state);
        item = try mapPutString(state, item, "name", dependency.name);
        item = try mapPutString(state, item, "root", dependency_project.root);
        dependencies = try vectorAdd(state, dependencies, item);
    }
    result = try mapPut(state, result, "dependencies", dependencies);

    var sources = try newVector(state);
    for (project.sources) |source| {
        var item = try newMap(state);
        item = try mapPutString(state, item, "name", source.name);
        item = try mapPutString(state, item, "path", source.path);
        item = try mapPutStringVector(state, item, "sources", source.sources);
        item = try mapPutString(state, item, "projectRoot", source.project_root);
        item = try mapPutString(state, item, "source", source.source);
        item = try mapPutStringVector(state, item, "requires", source.requires);
        item = try mapPutString(state, item, "sourceDigest", source.source_digest);
        item = try mapPutString(state, item, "revisionDigest", source.revision_digest);
        sources = try vectorAdd(state, sources, item);
    }
    result = try mapPut(state, result, "sources", sources);

    if (project == root) if (project.manifest.main) |main| {
        const main_path = try joinPath(state, project.root, main);
        if (std.fs.cwd().access(main_path, .{})) |_| {
            result = try mapPutString(state, result, "main", main_path);
        } else |_| {
            return state.fail("entrySourceMissing", "main source {s} does not exist", .{main_path});
        }
    };
    return result;
}

fn projectObservation(state: *State, root: *Project) FailureError!Oop {
    try loadDependencyClosure(state, root);
    var projects = try newVector(state);
    for (state.projects.items) |project| projects = try vectorAdd(state, projects, try observedProject(state, project, root));
    var snapshot = try newMap(state);
    snapshot = try mapPutString(state, snapshot, "root", root.root);
    snapshot = try mapPut(state, snapshot, "projects", projects);
    return snapshot;
}

fn nearestManifestRoot(state: *State, path: []const u8) FailureError![]const u8 {
    var current = try canonicalPath(state, path, "projectNotFound");
    while (true) {
        const manifest = try joinPath(state, current, "birnam.toml");
        if (std.fs.cwd().access(manifest, .{})) |_| return current else |err| switch (err) {
            error.FileNotFound => {},
            else => return state.fail("projectNotFound", "could not inspect {s}", .{manifest}),
        }
        const parent = std.fs.path.dirname(current) orelse return state.fail("projectNotFound", "no birnam.toml found above {s}", .{path});
        if (std.mem.eql(u8, parent, current)) return state.fail("projectNotFound", "no birnam.toml found above {s}", .{path});
        current = parent;
    }
}

fn digestResolutionModules(state: *State, root: *Project, modules: []const Module) []const u8 {
    var hasher = std.crypto.hash.sha2.Sha256.init(.{});
    const add = struct {
        fn field(h: *std.crypto.hash.sha2.Sha256, value: []const u8) void {
            var len: [8]u8 = undefined;
            std.mem.writeInt(u64, &len, value.len, .big);
            h.update(&len);
            h.update(value);
        }
    }.field;
    add(&hasher, "birnam.project-resolution.v1");
    add(&hasher, root.root);
    add(&hasher, root.manifest.digest);
    for (modules) |module| {
        add(&hasher, module.name);
        add(&hasher, module.path);
        for (module.sources) |path| add(&hasher, path);
        add(&hasher, module.source_digest);
        for (module.requires) |dependency| add(&hasher, dependency);
    }
    var digest: [32]u8 = undefined;
    hasher.final(&digest);
    return state.arena.dupe(u8, &std.fmt.bytesToHex(digest, .lower)) catch @panic("project: oom");
}

fn policyFailure(state: *State, value: Oop) FailureError {
    const kind_value = state.api.send(value, state.api.intern("kind"), &.{}, 0);
    const message_value = state.api.send(value, state.api.intern("message"), &.{}, 0);
    const rendered_value = state.api.send(value, state.api.intern("printString"), &.{}, 0);
    const kind = if (isSymbol(state.api, kind_value))
        state.arena.dupe(u8, std.mem.span(state.api.symbolName(kind_value))) catch
            return state.fail("projectPolicyFailed", "could not read project policy failure kind", .{})
    else
        "projectPolicyFailed";
    const message = if (isString(state.api, message_value))
        state.arena.dupe(u8, stringBytes(state.api, message_value)) catch
            return state.fail("projectPolicyFailed", "could not read project policy failure message", .{})
    else if (isString(state.api, rendered_value))
        state.arena.dupe(u8, stringBytes(state.api, rendered_value)) catch
            return state.fail("projectPolicyFailed", "could not render project policy failure", .{})
    else
        "project policy returned an unclassified failure";
    return state.fail(kind, "{s}", .{message});
}

fn policyStringVector(state: *State, value: Oop, field: []const u8) FailureError![]const []const u8 {
    const count = try vectorSize(state, value, field);
    const result = state.arena.alloc([]const u8, count) catch
        return state.fail("projectPolicyFailed", "out of memory while reading policy field {s}", .{field});
    for (0..count) |i| {
        const item = state.api.send(value, state.api.intern("at:"), &.{state.api.mkSmallInt(@intCast(i))}, 1);
        result[i] = try duplicateString(state, item, field);
    }
    return result;
}

fn policyRunConfig(state: *State, value: Oop) FailureError!RunConfig {
    if (!isMap(state.api, value)) return state.fail("projectPolicyFailed", "policy run value is not a map", .{});
    const repl = try duplicateString(state, try mapAt(state, value, "repl"), "run.repl");
    const bind = try duplicateString(state, try mapAt(state, value, "socketBind"), "run.socketBind");
    const port_value = try mapAt(state, value, "socketPort");
    if (state.api.isSmallInt(port_value) == 0)
        return state.fail("projectPolicyFailed", "policy run socketPort is not an integer", .{});
    return .{ .repl = repl, .socket_bind = bind, .socket_port = state.api.smallIntValue(port_value) };
}

fn policyModule(state: *State, value: Oop) FailureError!Module {
    if (!isMap(state.api, value)) return state.fail("projectPolicyFailed", "policy module entry is not a map", .{});
    const sources = try policyStringVector(state, try mapAt(state, value, "sources"), "module.sources");
    const requires = try policyStringVector(state, try mapAt(state, value, "requires"), "module.requires");
    return .{
        .name = try duplicateString(state, try mapAt(state, value, "name"), "module.name"),
        .path = try duplicateString(state, try mapAt(state, value, "path"), "module.path"),
        .sources = sources,
        .source = try duplicateString(state, try mapAt(state, value, "source"), "module.source"),
        .project_root = try duplicateString(state, try mapAt(state, value, "projectRoot"), "module.projectRoot"),
        .requires = requires,
        .source_digest = try duplicateString(state, try mapAt(state, value, "sourceDigest"), "module.sourceDigest"),
        .revision_digest = try duplicateString(state, try mapAt(state, value, "revisionDigest"), "module.revisionDigest"),
    };
}

fn resolutionFromPolicy(state: *State, root: *Project, value: Oop) FailureError!Resolution {
    if (isError(state.api, value)) return policyFailure(state, value);
    if (!isMap(state.api, value)) return state.fail("projectPolicyFailed", "ProjectPolicy did not return a resolution map", .{});
    const module_values = try mapAt(state, value, "modules");
    const module_count = try vectorSize(state, module_values, "policy modules");
    const modules = state.arena.alloc(Module, module_count) catch
        return state.fail("projectPolicyFailed", "out of memory while reading policy modules", .{});
    for (0..module_count) |i| {
        const item = state.api.send(module_values, state.api.intern("at:"), &.{state.api.mkSmallInt(@intCast(i))}, 1);
        modules[i] = try policyModule(state, item);
    }
    const load_roots = try policyStringVector(state, try mapAt(state, value, "loadRoots"), "policy loadRoots");
    const main_value = try mapAt(state, value, "main");
    const main_path = if (main_value == state.api.nilObj) null else try duplicateString(state, main_value, "main");
    return .{
        .project_root = try duplicateString(state, try mapAt(state, value, "projectRoot"), "projectRoot"),
        .manifest_path = try duplicateString(state, try mapAt(state, value, "manifest"), "manifest"),
        .project_name = try duplicateString(state, try mapAt(state, value, "projectName"), "projectName"),
        .main_path = main_path,
        .load_roots = load_roots,
        .run = try policyRunConfig(state, try mapAt(state, value, "run")),
        .modules = modules,
        .wrapped = root.manifest.wrapped,
        .digest = digestResolutionModules(state, root, modules),
    };
}

fn run(state: *State, working_dir: []const u8) FailureError!Resolution {
    const root_path = try nearestManifestRoot(state, working_dir);
    const root = try loadProjectAt(state, root_path);
    const snapshot = try projectObservation(state, root);
    const policy = global(state, "ProjectPolicy") orelse
        return state.fail("projectPolicyUnavailable", "ProjectPolicy is not loaded", .{});
    const result = state.api.send(policy, state.api.intern("resolveSnapshot:"), &.{snapshot}, 1);
    return resolutionFromPolicy(state, root, result);
}

pub fn resolve(api: *IdApi, allocator: std.mem.Allocator, working_dir: []const u8) Result {
    var arena = std.heap.ArenaAllocator.init(allocator);
    var state = State{ .arena = arena.allocator(), .api = api };
    const resolution = run(&state, working_dir) catch {
        return .{
            .arena = arena,
            .resolution = null,
            .diagnostic = .{ .kind = state.diagnostic_kind, .message = state.diagnostic_message },
        };
    };
    return .{ .arena = arena, .resolution = resolution, .diagnostic = null };
}

pub fn writeReport(result: *const Result, writer: anytype) !void {
    if (result.diagnostic) |diagnostic| {
        try writer.print("{s}: {s}\n", .{ diagnostic.kind, diagnostic.message });
        return;
    }
    const resolution = result.resolution.?;
    try writer.print("project: {s}\n", .{resolution.project_root});
    try writer.print("manifest: {s}\n", .{resolution.manifest_path});
    if (resolution.project_name.len != 0) try writer.print("name: {s}\n", .{resolution.project_name});
    if (resolution.main_path) |main| try writer.print("main: {s}\n", .{main});
    if (resolution.wrapped.len != 0) {
        try writer.writeAll("wrapped:\n");
        for (resolution.wrapped) |wrapped| try writer.print("  {s} -> lib/lib{s}.dylib\n", .{ wrapped.name, wrapped.name });
    }
    try writer.writeAll("module-order:\n");
    if (resolution.modules.len == 0) try writer.writeAll("  (none)\n");
    for (resolution.modules) |module| {
        try writer.print("  #{s} <- {s}", .{ module.name, module.path });
        if (module.sources.len > 1) {
            try writer.writeAll(" +");
            for (module.sources[1..]) |path| try writer.print(" {s}", .{path});
        }
        if (module.requires.len != 0) {
            try writer.writeAll(" requires");
            for (module.requires, 0..) |dependency, i| {
                if (i == 0) {
                    try writer.writeAll(" #");
                } else {
                    try writer.writeAll(", #");
                }
                try writer.writeAll(dependency);
            }
        }
        try writer.writeAll("\n");
    }
    try writer.print("resolution-digest: {s}\n", .{resolution.digest});
}

test "project path sorting is byte deterministic" {
    var paths = [_][]const u8{ "z/src.birnam", "a/src.birnam", "m/src.birnam" };
    sortStrings(&paths);
    try std.testing.expectEqualStrings("a/src.birnam", paths[0]);
    try std.testing.expectEqualStrings("m/src.birnam", paths[1]);
    try std.testing.expectEqualStrings("z/src.birnam", paths[2]);
}
