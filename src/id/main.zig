//! The siphon — birnam's bare entry point (the Zig analog of `~/dev/c/birnam`'s
//! `system.c` siphon @ d35caee). It does ONE raw `dlopen` of the id dylib to get
//! the id API (`id_init`), and from then on does *everything through the API* —
//! including re-importing the id layer itself (`api.import("Object")`) and
//! re-loading the id dylib through the API's own `dlopen`. Module loading and the
//! object namespace are part of the id system, so the floor is reflective: the
//! siphon can re-import even id.
//!
//! No eids, no fact store.

const std = @import("std");
const artifact_preflight = @import("artifact_preflight.zig");
const runtime_resources = @import("runtime_resources.zig");
const idapi = @import("idapi.zig");
const project = @import("project.zig");
const project_build = @import("project_build.zig");
const project_check = @import("project_check.zig");
const build_opts = @import("build_opts");

const USAGE =
    \\birnam — a Piumarta object system that compiles to native (docs/30).
    \\
    \\Usage:
    \\  birnam                     Start the interactive REPL (the default).
    \\  birnam <file.birnam>       Compile and run a script, then exit.
    \\  birnam <artifact.dylib>    Inspect and activate a self-described compiled
    \\                             root and only its declared dependency closure.
    \\  birnam <file.image>        Boot a saved image (maps compiled code; runs
    \\                             `main`, else drops into the REPL).
    \\  birnam --repl              Start the REPL explicitly (aliases: -i, repl).
    \\  birnam --listen <port> [file]
    \\                             Start a socket REPL on <port> (alias: --serve).
    \\                             If FILE is given it is run first, so the listener
    \\                             attaches to its state. Run from the repo root so
    \\                             the configured script path is unambiguous.
    \\                             Blocks; Ctrl-C to stop.
    \\  birnam --lsp               Run the birnam language server on stdio (LSP).
    \\                             Point lsp-mode at this. Resolves the
    \\                             loads its compiled module closure relative to
    \\                             the executable, so it
    \\                             works launched from any project directory.
    \\  birnam doc <mod> [sel]     Print documentation for a module (like `go doc`):
    \\                             its method docstrings, or one selector's doc.
    \\  birnam docgen [out.html]   Generate the full HTML doc reference (every birnam
    \\                             module + Rust floor and Zig oracle) to a file.
    \\  birnam init                Write a starter birnam.toml (the recognised keys,
    \\                             commented) into the current directory. Refuses to
    \\                             overwrite an existing one.
    \\  birnam check [directory]   Resolve the nearest source project and print its
    \\                             deterministic dependency-first module graph.
    \\  birnam check --help       Print check command documentation and exit.
    \\  birnam build [directory]  Compile the resolved source project and publish
    \\                             a complete project-local build directory.
    \\  birnam build --help       Print build command documentation and exit.
    \\  birnam install [directory] Install the latest project build into the
    \\                             runtime library directory.
    \\  birnam install --help     Print install command documentation and exit.
    \\  birnam run [directory] [options] [-- args...]
    \\                             Run a built project main; default is no REPL.
    \\  birnam run --help          Print run command documentation and exit.
    \\  birnam test [directory]    Run each discovered project test in a fresh
    \\                             process against the selected project build.
    \\  birnam test --help         Print test command documentation and exit.
    \\  birnam wrap <name>         Scaffold wrapped/<name>/<name>.birnam — a
    \\                             barebones Alien-based C-library wrapper to edit.
    \\  birnam --bake <img> <exe>  Bake an image into a standalone executable.
    \\  birnam --rebuild <img>     Boot an image by replaying its source, not the
    \\                             mapped `.code` section (recovery / rebuild).
    \\  birnam --version           Print the version and exit.
    \\  birnam --help              Print this help and exit (alias: -h).
    \\
    \\Environment:
    \\  BIRNAM_LIB_DIR    Directory holding the module dylibs. Overrides the
    \\                    executable-relative runtime lib directory.
    \\  BIRNAM_ID_DYLIB   Path to the id-floor dylib (libbirnam_id.dylib).
    \\  BIRNAM_NO_STDLIB  If set, skip the self-hosted stdlib (bare native runtime).
    \\  BIRNAM_GC_PRECISE If set, use the precise (layout-descriptor) GC.
    \\
;

const IdApi = idapi.IdApi;
const Oop = idapi.Oop;
const FloorIdentity = *const fn () callconv(.c) [*:0]const u8;

// The project adapter is host-side because it owns filesystem traversal and the
// existing Toml/Codegen seams. ProjectPolicy owns the semantic resolution over
// its value snapshot. The LSP itself remains authored in Birnam; this bridge
// gives it one immutable native snapshot per workspace lookup rather than a
// second manifest parser or a second dependency graph.
var project_bridge_api: ?*IdApi = null;
// A project run already owns one immutable resolution for its full lifetime.
// Keep only a borrowed pointer while `runProject` is active so in-image tools
// can request the current project without repeating filesystem/TOML/CST work.
// The bridge still materializes a fresh Birnam value map per request, so no Oop
// is hidden from the collector and explicit non-empty URIs retain normal lookup.
var active_project_resolution: ?*const project.Resolution = null;
var active_project_build: ?*const project_build.Build = null;

fn hexNibble(c: u8) ?u8 {
    return switch (c) {
        '0'...'9' => c - '0',
        'a'...'f' => c - 'a' + 10,
        'A'...'F' => c - 'A' + 10,
        else => null,
    };
}

/// Convert a file URI (or a plain path used by tests/tools) into a filesystem
/// path. LSP clients normally send file:/// URIs; percent decoding belongs at
/// this host boundary so the Birnam LSP never needs URL/path policy.
fn projectPathFromUri(a: std.mem.Allocator, raw: []const u8) ![]u8 {
    if (raw.len == 0) return std.fs.cwd().realpathAlloc(a, ".");
    var input = raw;
    var is_file_uri = false;
    if (std.mem.startsWith(u8, raw, "file://")) {
        is_file_uri = true;
        input = raw[7..];
        if (std.mem.startsWith(u8, input, "localhost/")) input = input[9..];
    } else if (std.mem.indexOf(u8, raw, "://") != null) {
        return error.UnsupportedProjectUri;
    }

    var path: std.ArrayList(u8) = .{};
    errdefer path.deinit(a);
    var i: usize = 0;
    while (i < input.len) : (i += 1) {
        if (is_file_uri and input[i] == '%' and i + 2 < input.len) {
            const hi = hexNibble(input[i + 1]) orelse return error.InvalidProjectUri;
            const lo = hexNibble(input[i + 2]) orelse return error.InvalidProjectUri;
            try path.append(a, (hi << 4) | lo);
            i += 2;
        } else {
            try path.append(a, input[i]);
        }
    }
    if (path.items.len == 0) return std.fs.cwd().realpathAlloc(a, ".");
    return path.toOwnedSlice(a);
}

/// `project.resolve` starts from a directory. A document URI names a file, so
/// use its parent unless the path itself contains a manifest (the root-URI
/// case). This also handles unsaved files: their parent is still discoverable.
fn projectWorkingDirectory(a: std.mem.Allocator, raw: []const u8) ![]u8 {
    const path = try projectPathFromUri(a, raw);
    defer a.free(path);
    const manifest = try std.fs.path.join(a, &.{ path, "birnam.toml" });
    defer a.free(manifest);
    if (std.fs.cwd().access(manifest, .{})) |_| return a.dupe(u8, path) else |_| {}
    if (std.fs.path.dirname(path)) |parent| return a.dupe(u8, parent);
    return a.dupe(u8, path);
}

fn nativeMap(api: *IdApi) ?Oop {
    const map = api.import("Map") orelse return null;
    const value = api.send(map, api.intern("new"), &.{}, 0);
    return if (value == null or value == api.nilObj) null else value;
}

fn nativeMapPut(api: *IdApi, map: Oop, key: []const u8, value: Oop) ?Oop {
    const key_oop = api.mkString(key.ptr, key.len);
    const updated = api.send(map, api.intern("at:put:"), &[_]Oop{ key_oop, value }, 2);
    return if (updated == null or updated == api.nilObj) null else updated;
}

fn nativeVector(api: *IdApi) ?Oop {
    const vector = api.import("Vector") orelse return null;
    const value = api.send(vector, api.intern("new"), &.{}, 0);
    return if (value == null or value == api.nilObj) null else value;
}

fn nativeVectorAdd(api: *IdApi, vector: Oop, value: Oop) ?Oop {
    const updated = api.send(vector, api.intern("add:"), &[_]Oop{value}, 1);
    return if (updated == null or updated == api.nilObj) null else updated;
}

fn nativeStringVector(api: *IdApi, values: []const []const u8) ?Oop {
    var vector = nativeVector(api) orelse return null;
    for (values) |value| {
        const string = api.mkString(value.ptr, value.len);
        vector = nativeVectorAdd(api, vector, string) orelse return null;
    }
    return vector;
}

fn projectNativeFailure(api: *IdApi, diagnostic: project.Diagnostic) ?Oop {
    var map = nativeMap(api) orelse return null;
    map = nativeMapPut(api, map, "ok", api.falseObj) orelse return null;
    map = nativeMapPut(api, map, "kind", api.mkString(diagnostic.kind.ptr, diagnostic.kind.len)) orelse return null;
    map = nativeMapPut(api, map, "message", api.mkString(diagnostic.message.ptr, diagnostic.message.len)) orelse return null;
    return map;
}

fn projectNativeSuccess(api: *IdApi, resolution: *const project.Resolution, build: ?*const project_build.Build) ?Oop {
    var modules = nativeVector(api) orelse return null;
    for (resolution.modules) |module| {
        var item = nativeMap(api) orelse return null;
        item = nativeMapPut(api, item, "name", api.mkString(module.name.ptr, module.name.len)) orelse return null;
        item = nativeMapPut(api, item, "path", api.mkString(module.path.ptr, module.path.len)) orelse return null;
        item = nativeMapPut(api, item, "sources", nativeStringVector(api, module.sources) orelse return null) orelse return null;
        item = nativeMapPut(api, item, "projectRoot", api.mkString(module.project_root.ptr, module.project_root.len)) orelse return null;
        item = nativeMapPut(api, item, "requires", nativeStringVector(api, module.requires) orelse return null) orelse return null;
        item = nativeMapPut(api, item, "sourceDigest", api.mkString(module.source_digest.ptr, module.source_digest.len)) orelse return null;
        item = nativeMapPut(api, item, "revisionDigest", api.mkString(module.revision_digest.ptr, module.revision_digest.len)) orelse return null;
        modules = nativeVectorAdd(api, modules, item) orelse return null;
    }

    var map = nativeMap(api) orelse return null;
    map = nativeMapPut(api, map, "ok", api.trueObj) orelse return null;
    map = nativeMapPut(api, map, "projectRoot", api.mkString(resolution.project_root.ptr, resolution.project_root.len)) orelse return null;
    map = nativeMapPut(api, map, "manifest", api.mkString(resolution.manifest_path.ptr, resolution.manifest_path.len)) orelse return null;
    map = nativeMapPut(api, map, "projectName", api.mkString(resolution.project_name.ptr, resolution.project_name.len)) orelse return null;
    map = nativeMapPut(api, map, "main", if (resolution.main_path) |path|
        api.mkString(path.ptr, path.len)
    else
        api.nilObj) orelse return null;
    map = nativeMapPut(api, map, "digest", api.mkString(resolution.digest.ptr, resolution.digest.len)) orelse return null;
    map = nativeMapPut(api, map, "buildRoot", if (build) |selected|
        api.mkString(selected.output_root.ptr, selected.output_root.len)
    else
        api.nilObj) orelse return null;
    map = nativeMapPut(api, map, "profile", if (build) |selected|
        api.mkString(selected.profile.ptr, selected.profile.len)
    else
        api.nilObj) orelse return null;
    map = nativeMapPut(api, map, "modules", modules) orelse return null;
    return map;
}

fn mProjectSnapshotForUri(_: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    const api = project_bridge_api orelse return null;
    if (n < 1) return api.nilObj;
    if (api.vtableOf(args[0]) != api.string_vt) return projectNativeFailure(api, .{
        .kind = "projectUriInvalid",
        .message = "project URI must be a String",
    }) orelse api.nilObj;
    var uri_len: usize = 0;
    const uri = api.stringBytes(args[0], &uri_len);
    const raw = uri[0..uri_len];
    if (raw.len == 0) if (active_project_resolution) |resolution|
        return projectNativeSuccess(api, resolution, active_project_build) orelse api.nilObj;
    const working_dir = projectWorkingDirectory(std.heap.c_allocator, raw) catch return projectNativeFailure(api, .{
        .kind = "projectUriInvalid",
        .message = "could not decode workspace URI",
    }) orelse api.nilObj;
    defer std.heap.c_allocator.free(working_dir);

    var result = project.resolve(api, std.heap.c_allocator, working_dir);
    defer result.deinit();
    return if (result.diagnostic) |diagnostic|
        projectNativeFailure(api, diagnostic) orelse api.nilObj
    else
        projectNativeSuccess(api, &result.resolution.?, null) orelse api.nilObj;
}

/// Host-only path decoding for the LSP's authored-source index. URI policy
/// stays beside the resolver so Birnam does not grow an editor-specific URI
/// parser; this answers the decoded path without reading or evaluating it.
fn mProjectPathForUri(_: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    const api = project_bridge_api orelse return null;
    if (n < 1) return api.nilObj;
    if (api.vtableOf(args[0]) != api.string_vt) return api.nilObj;
    var uri_len: usize = 0;
    const uri = api.stringBytes(args[0], &uri_len);
    const path = projectPathFromUri(std.heap.c_allocator, uri[0..uri_len]) catch return api.nilObj;
    defer std.heap.c_allocator.free(path);
    return api.mkString(path.ptr, path.len);
}

fn installProjectBridge(api: *IdApi) !void {
    if (project_bridge_api != null) return;
    const object = api.import("Object") orelse return error.NoObject;
    const project_object = api.send(object, api.intern("clone"), &.{}, 0);
    if (project_object == null) return error.NoProjectBridge;
    api.method(api.vtableOf(project_object), api.intern("snapshotForUri:"), &mProjectSnapshotForUri);
    api.method(api.vtableOf(project_object), api.intern("pathForUri:"), &mProjectPathForUri);
    _ = api.exportObj("Project", project_object);
    if (api.import("Globals")) |globals| {
        _ = api.send(globals, api.intern("at:put:"), &[_]Oop{ api.intern("Project"), project_object }, 2);
    }
    project_bridge_api = api;
}

const CHECK_USAGE =
    \\Usage: birnam check [directory] [--format human|json]
    \\
    \\Resolve the nearest birnam.toml and compile-check every module in a private
    \\temporary directory. The command is read-only: it does not evaluate
    \\project modules, load compiled units, or publish build output.
    \\
    \\Arguments:
    \\  directory       Directory to search from; defaults to the current directory.
    \\
    \\Options:
    \\  --format FMT    Output format: human (default) or json.
    \\  -h, --help      Show this help and exit.
    \\
    \\Examples:
    \\  birnam check
    \\  birnam check ./path/to/project
    \\  birnam check --help
    \\
;

const BUILD_USAGE =
    \\Usage: birnam build [directory] [--profile debug|release]
    \\
    \\Resolve and compile the nearest birnam.toml source project. Build outputs
    \\are staged under .birnam/build and published only after every module and
    \\build record succeeds. It does not run or activate the project. Use one
    \\[[module]] table to compose an entry source and optional ordered files
    \\into one compiled dylib.
    \\
    \\Module example:
    \\  [[module]]
    \\  entry = "src/app.birnam"
    \\  files = ["src/app-extra.birnam"]
    \\
    \\Arguments:
    \\  directory       Directory to search from; defaults to the current directory.
    \\
    \\Options:
    \\  --profile NAME  Select debug (default) or release publication.
    \\  -h, --help      Show this help and exit.
    \\
    \\Examples:
    \\  birnam build
    \\  birnam build ./path/to/project
    \\  birnam build --help
    \\
;

const INSTALL_USAGE =
    \\Usage: birnam install [directory] [options]
    \\
    \\Install the nearest project's last successful build into the runtime
    \\library directory. This includes both libbirnam_<name>.dylib modules and
    \\lib<name>.dylib libraries declared by [[wrapped]].
    \\
    \\Arguments:
    \\  directory       Directory to search from; defaults to the current directory.
    \\
    \\Options:
    \\  --build         Build and publish before installing.
    \\  --profile NAME  Select debug (default) or release publication.
    \\  -h, --help      Print this command documentation.
    \\
    \\The destination is BIRNAM_LIB_DIR when set, otherwise the default runtime
    \\library directory used by birnam (normally <prefix>/lib).
    \\
    \\Examples:
    \\  birnam install
    \\  birnam install --build
    \\  BIRNAM_LIB_DIR=/opt/birnam/lib birnam install ./app
    \\
;

const RUN_USAGE =
    \\Usage: birnam run [directory] [options] [-- args...]
    \\
    \\Run the nearest project using its last successful build. The default is
    \\no-repl: resolve, load compiled modules, evaluate main, then exit.
    \\
    \\Arguments:
    \\  directory       Directory to search from; defaults to the current directory.
    \\
    \\Options:
    \\  --build         Build and publish before running.
    \\  --profile NAME  Select debug (default) or release publication.
    \\  --no-repl       Evaluate main and exit (the default).
    \\  --repl          Start the terminal REPL after main (same as --repl=tty).
    \\  --repl=tty      Start the terminal REPL after main.
    \\  --repl=socket   Start the socket REPL after main.
    \\  --repl-bind IP  Override the socket bind address/host.
    \\  --repl-port N   Override the socket port (0..65535).
    \\  -h, --help      Show this help and exit.
    \\
    \\Arguments after `--` are exposed to main in `ProjectArgs` as a Vector of
    \\Strings. Command-line settings override the [run] table in birnam.toml.
    \\
    \\Examples:
    \\  birnam run --build
    \\  birnam run ./app --repl=tty
    \\  birnam run --repl=socket --repl-bind 127.0.0.1 --repl-port 4400
    \\
;

const TEST_USAGE =
    \\Usage: birnam test [directory] [options]
    \\
    \\Run every configured or discovered project test in its own process.
    \\Tests consume the selected successful project build; --build publishes it
    \\first. Each test source registers one or more TestSuite values and Birnam
    \\runs those suites with ConsoleRunner.
    \\
    \\Discovery:
    \\  [test]
    \\  sources = ["tests/app-test.birnam"]
    \\
    \\When [test].sources is absent, every .birnam file below test/ and tests/
    \\is discovered in deterministic path order.
    \\
    \\Options:
    \\  --build         Build and publish before testing.
    \\  --profile NAME  Select debug (default) or release publication.
    \\  -h, --help      Show this help and exit.
    \\
;

const ReplMode = enum { none, tty, socket };
const OutputFormat = enum { human, json };

const ProjectCheckOptions = struct {
    directory: []const u8,
    format: OutputFormat,
};

const ProjectBuildOptions = struct {
    directory: []const u8,
    profile: []const u8,
};

const ProjectRunOptions = struct {
    directory: []const u8,
    build: bool,
    profile: []const u8,
    repl: ?ReplMode,
    socket_bind: ?[]const u8,
    socket_port: ?i64,
    arguments: []const []const u8,
};

const ProjectInstallOptions = struct {
    directory: []const u8,
    build: bool,
    profile: []const u8,
};

const ProjectTestOptions = struct {
    directory: []const u8,
    build: bool,
    profile: []const u8,
};

fn parseReplMode(value: []const u8) ?ReplMode {
    if (std.mem.eql(u8, value, "none")) return .none;
    if (std.mem.eql(u8, value, "tty")) return .tty;
    if (std.mem.eql(u8, value, "socket")) return .socket;
    return null;
}

fn projectProfile(value: []const u8) ?[]const u8 {
    if (std.mem.eql(u8, value, "debug") or std.mem.eql(u8, value, "release")) return value;
    return null;
}

fn parseProjectCheckOptions(args: *std.process.ArgIterator) ProjectCheckOptions {
    var directory: []const u8 = ".";
    var has_directory = false;
    var format: OutputFormat = .human;
    while (args.next()) |token| {
        if (std.mem.eql(u8, token, "--help") or std.mem.eql(u8, token, "-h")) {
            if (args.next() != null) checkUsageError("unexpected argument after --help", .{});
            out(CHECK_USAGE);
            std.process.exit(0);
        }
        if (std.mem.eql(u8, token, "--format")) {
            const value = args.next() orelse checkUsageError("--format requires human or json", .{});
            format = if (std.mem.eql(u8, value, "human")) .human else if (std.mem.eql(u8, value, "json")) .json else checkUsageError("--format must be human or json", .{});
            continue;
        }
        if (std.mem.startsWith(u8, token, "--format=")) {
            const value = token["--format=".len..];
            format = if (std.mem.eql(u8, value, "human")) .human else if (std.mem.eql(u8, value, "json")) .json else checkUsageError("--format must be human or json", .{});
            continue;
        }
        if (std.mem.startsWith(u8, token, "-")) checkUsageError("unknown option '{s}'", .{token});
        if (has_directory) checkUsageError("expected at most one project directory", .{});
        directory = token;
        has_directory = true;
    }
    return .{ .directory = directory, .format = format };
}

fn parseProjectBuildOptions(args: *std.process.ArgIterator) ProjectBuildOptions {
    var directory: []const u8 = ".";
    var has_directory = false;
    var profile: []const u8 = "debug";
    while (args.next()) |token| {
        if (std.mem.eql(u8, token, "--help") or std.mem.eql(u8, token, "-h")) {
            if (args.next() != null) buildUsageError("unexpected argument after --help", .{});
            out(BUILD_USAGE);
            std.process.exit(0);
        }
        if (std.mem.eql(u8, token, "--profile")) {
            profile = projectProfile(args.next() orelse buildUsageError("--profile requires debug or release", .{})) orelse
                buildUsageError("--profile must be debug or release", .{});
            continue;
        }
        if (std.mem.startsWith(u8, token, "--profile=")) {
            profile = projectProfile(token["--profile=".len..]) orelse
                buildUsageError("--profile must be debug or release", .{});
            continue;
        }
        if (std.mem.startsWith(u8, token, "-")) buildUsageError("unknown option '{s}'", .{token});
        if (has_directory) buildUsageError("expected at most one project directory", .{});
        directory = token;
        has_directory = true;
    }
    return .{ .directory = directory, .profile = profile };
}

fn parseProjectRunOptions(a: std.mem.Allocator, args: *std.process.ArgIterator) ProjectRunOptions {
    var directory: []const u8 = ".";
    var has_directory = false;
    var build = false;
    var profile: []const u8 = "debug";
    var repl: ?ReplMode = null;
    var socket_bind: ?[]const u8 = null;
    var socket_port: ?i64 = null;
    var arguments: std.ArrayList([]const u8) = .{};
    defer arguments.deinit(a);

    while (args.next()) |raw| {
        const token: []const u8 = raw;
        if (std.mem.eql(u8, token, "--")) {
            while (args.next()) |argument| arguments.append(a, argument) catch runUsageError("out of memory while reading arguments", .{});
            break;
        }
        if (std.mem.eql(u8, token, "--help") or std.mem.eql(u8, token, "-h")) {
            out(RUN_USAGE);
            std.process.exit(0);
        }
        if (std.mem.eql(u8, token, "--build")) {
            build = true;
            continue;
        }
        if (std.mem.eql(u8, token, "--profile")) {
            profile = projectProfile(args.next() orelse runUsageError("--profile requires debug or release", .{})) orelse
                runUsageError("--profile must be debug or release", .{});
            continue;
        }
        if (std.mem.startsWith(u8, token, "--profile=")) {
            profile = projectProfile(token["--profile=".len..]) orelse
                runUsageError("--profile must be debug or release", .{});
            continue;
        }
        if (std.mem.eql(u8, token, "--no-repl")) {
            repl = .none;
            continue;
        }
        if (std.mem.eql(u8, token, "--repl")) {
            repl = .tty;
            continue;
        }
        if (std.mem.startsWith(u8, token, "--repl=")) {
            repl = parseReplMode(token["--repl=".len..]) orelse
                runUsageError("--repl must be one of none, tty, or socket", .{});
            continue;
        }
        if (std.mem.eql(u8, token, "--repl-bind")) {
            socket_bind = args.next() orelse runUsageError("--repl-bind requires an address or host", .{});
            continue;
        }
        if (std.mem.startsWith(u8, token, "--repl-bind=")) {
            socket_bind = token["--repl-bind=".len..];
            if (socket_bind.?.len == 0) runUsageError("--repl-bind requires an address or host", .{});
            continue;
        }
        if (std.mem.eql(u8, token, "--repl-port")) {
            const value = args.next() orelse runUsageError("--repl-port requires an integer", .{});
            socket_port = std.fmt.parseInt(i64, value, 10) catch
                runUsageError("--repl-port must be an integer from 0 to 65535", .{});
            if (socket_port.? < 0 or socket_port.? > 65535)
                runUsageError("--repl-port must be an integer from 0 to 65535", .{});
            continue;
        }
        if (std.mem.startsWith(u8, token, "--repl-port=")) {
            socket_port = std.fmt.parseInt(i64, token["--repl-port=".len..], 10) catch
                runUsageError("--repl-port must be an integer from 0 to 65535", .{});
            if (socket_port.? < 0 or socket_port.? > 65535)
                runUsageError("--repl-port must be an integer from 0 to 65535", .{});
            continue;
        }
        if (std.mem.startsWith(u8, token, "-"))
            runUsageError("unknown option '{s}'", .{token});
        if (has_directory) runUsageError("expected at most one project directory", .{});
        directory = token;
        has_directory = true;
    }
    const owned_arguments = arguments.toOwnedSlice(a) catch runUsageError("out of memory while reading arguments", .{});
    return .{
        .directory = directory,
        .build = build,
        .profile = profile,
        .repl = repl,
        .socket_bind = socket_bind,
        .socket_port = socket_port,
        .arguments = owned_arguments,
    };
}

fn parseProjectInstallOptions(a: std.mem.Allocator, args: *std.process.ArgIterator) ProjectInstallOptions {
    _ = a;
    var directory: []const u8 = ".";
    var has_directory = false;
    var build = false;
    var profile: []const u8 = "debug";
    while (args.next()) |token| {
        if (std.mem.eql(u8, token, "--help") or std.mem.eql(u8, token, "-h")) {
            if (args.next() != null) installUsageError("unexpected argument after --help", .{});
            out(INSTALL_USAGE);
            std.process.exit(0);
        }
        if (std.mem.eql(u8, token, "--build")) {
            build = true;
            continue;
        }
        if (std.mem.eql(u8, token, "--profile")) {
            profile = projectProfile(args.next() orelse installUsageError("--profile requires debug or release", .{})) orelse
                installUsageError("--profile must be debug or release", .{});
            continue;
        }
        if (std.mem.startsWith(u8, token, "--profile=")) {
            profile = projectProfile(token["--profile=".len..]) orelse
                installUsageError("--profile must be debug or release", .{});
            continue;
        }
        if (std.mem.startsWith(u8, token, "-"))
            installUsageError("unknown option '{s}'", .{token});
        if (has_directory) installUsageError("expected at most one project directory", .{});
        directory = token;
        has_directory = true;
    }
    return .{ .directory = directory, .build = build, .profile = profile };
}

fn parseProjectTestOptions(args: *std.process.ArgIterator) ProjectTestOptions {
    var directory: []const u8 = ".";
    var has_directory = false;
    var build = false;
    var profile: []const u8 = "debug";
    while (args.next()) |token| {
        if (std.mem.eql(u8, token, "--help") or std.mem.eql(u8, token, "-h")) {
            if (args.next() != null) testUsageError("unexpected argument after --help", .{});
            out(TEST_USAGE);
            std.process.exit(0);
        }
        if (std.mem.eql(u8, token, "--build")) {
            build = true;
            continue;
        }
        if (std.mem.eql(u8, token, "--profile")) {
            profile = projectProfile(args.next() orelse testUsageError("--profile requires debug or release", .{})) orelse
                testUsageError("--profile must be debug or release", .{});
            continue;
        }
        if (std.mem.startsWith(u8, token, "--profile=")) {
            profile = projectProfile(token["--profile=".len..]) orelse
                testUsageError("--profile must be debug or release", .{});
            continue;
        }
        if (std.mem.startsWith(u8, token, "-")) testUsageError("unknown option '{s}'", .{token});
        if (has_directory) testUsageError("expected at most one project directory", .{});
        directory = token;
        has_directory = true;
    }
    return .{ .directory = directory, .build = build, .profile = profile };
}

// -- exe bake: a self-booting executable = this runtime + an appended image ---
//
// The standalone-exe bake (docs/40 §6): rather than an OS-level dual-format file
// (given up — dyld/glibc fight it), a "runnable app" is a COPY of the birnam
// runtime with an image appended after a trailer. Trailing bytes are ignored by
// both the Mach-O and ELF loaders, so the copy still executes as birnam; on
// startup it notices the trailer and boots the embedded image (auto-running
// `main`). The image format and boot path are unchanged — this only packages.
//
//   file layout:  [ birnam binary ][ image bytes ][ u64 img_len ][ magic ]
//
// The magic MUST stay in sync with image.zig's `bake_magic` (its `bakeExe:`).
const BAKE_MAGIC = "BIRNMEXE";
const BAKE_TRAILER = 16; // u64 length + 8-byte magic

/// Read this executable's own bytes (its path from `selfExePath`).
fn selfExeBytes(a: std.mem.Allocator) ![]u8 {
    const self_path = try std.fs.selfExePathAlloc(a);
    defer a.free(self_path);
    return std.fs.cwd().readFileAlloc(a, self_path, 1 << 30);
}

/// The image sub-slice a bake trailer points to inside `bytes` (a whole exe file),
/// or null if there is no valid trailer — a plain birnam, or bytes too short /
/// a bad length. Pure: no I/O, so it is unit-testable.
fn bakeTrailerImage(bytes: []const u8) ?[]const u8 {
    if (bytes.len < BAKE_TRAILER) return null;
    if (!std.mem.eql(u8, bytes[bytes.len - 8 ..], BAKE_MAGIC)) return null;
    const img_len = std.mem.readInt(u64, bytes[bytes.len - BAKE_TRAILER ..][0..8], .little);
    if (img_len == 0 or img_len + BAKE_TRAILER > bytes.len) return null;
    const start = bytes.len - BAKE_TRAILER - @as(usize, @intCast(img_len));
    return bytes[start .. bytes.len - BAKE_TRAILER];
}

/// The embedded image appended to this exe (the bake trailer), or null for a plain
/// birnam (no trailer). Caller frees the returned slice.
fn embeddedImageBytes(a: std.mem.Allocator) !?[]u8 {
    const bytes = try selfExeBytes(a);
    defer a.free(bytes);
    const img = bakeTrailerImage(bytes) orelse return null;
    return try a.dupe(u8, img);
}

/// `birnam --bake <image> <out-exe>` — append an existing `.image` to a copy of
/// this runtime, producing a self-booting executable. (The in-session form is
/// `[System bakeExe: path]`, which bakes the running system directly.)
fn bakeExeCli(a: std.mem.Allocator, image_path: []const u8, out_path: []const u8) !void {
    const self_bytes = try selfExeBytes(a);
    defer a.free(self_bytes);
    const img_bytes = try std.fs.cwd().readFileAlloc(a, image_path, 1 << 30);
    defer a.free(img_bytes);
    const f = try std.fs.cwd().createFile(out_path, .{ .mode = 0o755 });
    defer f.close();
    try f.writeAll(self_bytes);
    try f.writeAll(img_bytes);
    var lb: [8]u8 = undefined;
    std.mem.writeInt(u64, &lb, img_bytes.len, .little);
    try f.writeAll(&lb);
    try f.writeAll(BAKE_MAGIC);
    out("[bake] wrote ");
    out(out_path);
    out("\n");
}

/// Boot an image carried as bytes (the exe-bake path): write it to a per-process
/// temp `.image` and run it through the ordinary image boot (so mapped code,
/// replay, global re-derivation, and the `main`-entry convention all apply).
fn runImageBytes(a: std.mem.Allocator, api: *IdApi, bytes: []const u8, rebuild: bool) !void {
    var pbuf: [64]u8 = undefined;
    const tmp = try std.fmt.bufPrint(&pbuf, "/tmp/birnam-baked-{d}.image", .{std.c.getpid()});
    {
        const f = try std.fs.cwd().createFile(tmp, .{});
        defer f.close();
        try f.writeAll(bytes);
    }
    defer std.fs.cwd().deleteFile(tmp) catch {};
    return runImage(a, api, tmp, rebuild);
}

fn verifySelectedFloorDigest(path: []const u8) void {
    const expected = std.posix.getenv("BIRNAM_ID_FLOOR_SHA256") orelse return;
    if (expected.len == 0) return;
    const file = if (std.fs.path.isAbsolute(path))
        std.fs.openFileAbsolute(path, .{}) catch @panic("selected floor digest: open failed")
    else
        std.fs.cwd().openFile(path, .{}) catch @panic("selected floor digest: open failed");
    defer file.close();
    var hasher = std.crypto.hash.sha2.Sha256.init(.{});
    var buffer: [16 * 1024]u8 = undefined;
    while (true) {
        const count = file.read(&buffer) catch @panic("selected floor digest: read failed");
        if (count == 0) break;
        hasher.update(buffer[0..count]);
    }
    var digest: [std.crypto.hash.sha2.Sha256.digest_length]u8 = undefined;
    hasher.final(&digest);
    const actual = std.fmt.bytesToHex(digest, .lower);
    if (!std.mem.eql(u8, &actual, expected)) {
        std.debug.panic("selected floor digest mismatch: expected {s}, got {s}", .{
            expected,
            &actual,
        });
    }
}

fn verifySelectedFloorIdentity(lib: *std.DynLib) void {
    const expected = std.posix.getenv("BIRNAM_ID_FLOOR_IDENTITY") orelse return;
    if (expected.len == 0) return;
    const identity = lib.lookup(FloorIdentity, "birnam_id_floor_identity") orelse
        @panic("selected floor is missing birnam_id_floor_identity");
    const actual = std.mem.span(identity());
    if (!std.mem.eql(u8, actual, expected)) {
        std.debug.panic("selected floor identity mismatch: expected {s}, got {s}", .{
            expected,
            actual,
        });
    }
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const a = gpa.allocator();

    var args = try std.process.argsWithAllocator(a);
    defer args.deinit();
    _ = args.next();
    var script_path = args.next(); // optional: a .birnam file to compile + run
    // `--help` / `--version` are pure tooling ops — answer before touching any
    // dylib, so they work from anywhere even without the modules installed.
    if (script_path) |sp| {
        if (std.mem.eql(u8, sp, "--help") or std.mem.eql(u8, sp, "-h")) {
            out(USAGE);
            return;
        }
        if (std.mem.eql(u8, sp, "--version") or std.mem.eql(u8, sp, "-V")) {
            out("birnam " ++ build_opts.version ++ "\n");
            return;
        }
    }

    // `check --help` and malformed check invocations are pure CLI operations:
    // answer them before resolving or loading any runtime dylib, just like the
    // top-level help/version paths above.
    var project_check_options: ?ProjectCheckOptions = null;
    var project_build_options: ?ProjectBuildOptions = null;
    var project_run_options: ?ProjectRunOptions = null;
    var project_install_options: ?ProjectInstallOptions = null;
    var project_test_options: ?ProjectTestOptions = null;
    if (script_path) |sp| {
        if (std.mem.eql(u8, sp, "check")) {
            project_check_options = parseProjectCheckOptions(&args);
        } else if (std.mem.eql(u8, sp, "build")) {
            project_build_options = parseProjectBuildOptions(&args);
        } else if (std.mem.eql(u8, sp, "run")) {
            project_run_options = parseProjectRunOptions(a, &args);
        } else if (std.mem.eql(u8, sp, "install")) {
            project_install_options = parseProjectInstallOptions(a, &args);
        } else if (std.mem.eql(u8, sp, "test")) {
            project_test_options = parseProjectTestOptions(&args);
        }
    }
    // `--rebuild foo.image` forces the replay path (rebuild code from the image's
    // own source) instead of mapping the compiled `.code` section.
    var rebuild = false;
    if (script_path) |sp| {
        if (std.mem.eql(u8, sp, "--rebuild")) {
            rebuild = true;
            script_path = args.next();
        }
    }
    // The id dylib: an explicit `$BIRNAM_ID_DYLIB`, else libbirnam_id.dylib in the
    // resolved lib dir (exe-relative, so a baked exe boots from any cwd).
    var runtime = try runtime_resources.resolve(a);
    defer runtime.deinit();
    const id_path = try a.dupe(u8, runtime.id_dylib);
    defer a.free(id_path);
    verifySelectedFloorDigest(id_path);

    // `--bake <image> <out-exe>` — tooling op, no runtime needed yet.
    if (script_path) |sp| {
        if (std.mem.eql(u8, sp, "--bake")) {
            const img = args.next() orelse {
                // Missing operand: usage is a help display, not a crash — exit
                // cleanly so a `!void` return doesn't print `error: X` + a trace.
                out("usage: birnam --bake <image> <out-exe>\n");
                return;
            };
            const outp = args.next() orelse {
                out("usage: birnam --bake <image> <out-exe>\n");
                return;
            };
            return bakeExeCli(a, img, outp);
        }
    }

    // The ONE irreducible raw load — open the id dylib, find `id_init`, call it
    // to get the id API table. Everything else goes through the api.
    var lib = std.DynLib.open(id_path) catch |err|
        runtimeResourceUnavailable("runtime dylib", id_path, err);
    verifySelectedFloorIdentity(&lib);
    const id_init = lib.lookup(idapi.IdInitFn, "id_init") orelse return error.NoIdInit;
    const api = id_init();
    defer if (host_isolate_shutdown) |shutdown| shutdown();

    // Private subprocess entry used by `birnam test`. Each invocation receives
    // one absolute test source and therefore starts with a fresh heap, Store,
    // module generation, and project artifact activation.
    if (script_path) |sp| {
        if (std.mem.eql(u8, sp, "--project-test-worker")) {
            const directory = args.next() orelse return error.InvalidProjectTestWorkerArguments;
            const profile = args.next() orelse return error.InvalidProjectTestWorkerArguments;
            const test_path = args.next() orelse return error.InvalidProjectTestWorkerArguments;
            if (args.next() != null or projectProfile(profile) == null)
                return error.InvalidProjectTestWorkerArguments;
            return runProjectTestWorker(a, api, directory, profile, test_path);
        }
    }

    // The first project workflow is intentionally read-only and source-only.
    // It boots the parser/TOML stack, then resolves without evaluating a project
    // source or calling Module load:.
    if (script_path) |sp| {
        if (std.mem.eql(u8, sp, "check")) {
            return runProjectCheck(a, api, project_check_options.?) catch |err| checkCommandError(err);
        }
        if (std.mem.eql(u8, sp, "build")) {
            return runProjectBuild(a, api, project_build_options.?) catch |err| buildCommandError(err);
        }
        if (std.mem.eql(u8, sp, "run")) {
            const options = project_run_options.?;
            defer a.free(options.arguments);
            return runProject(a, api, options) catch |err| runCommandError(err);
        }
        if (std.mem.eql(u8, sp, "install")) {
            const options = project_install_options.?;
            return runProjectInstall(a, api, options) catch |err| installCommandError(err);
        }
        if (std.mem.eql(u8, sp, "test")) {
            const options = project_test_options.?;
            return runProjectTest(a, api, options) catch |err| testCommandError(err);
        }
    }

    // Worker routing deliberately precedes baked-image discovery: a baked
    // coordinator executable must still recover the explicit worker image and
    // must never dispatch its embedded application's main entry.
    if (script_path) |sp| {
        if (std.mem.eql(u8, sp, "--worker")) {
            const image = args.next() orelse return error.InvalidWorkerArguments;
            const receiver = args.next() orelse return error.InvalidWorkerArguments;
            const selector = args.next() orelse return error.InvalidWorkerArguments;
            const fd_flag = args.next() orelse return error.InvalidWorkerArguments;
            if (!std.mem.eql(u8, fd_flag, "--channel-fd")) return error.InvalidWorkerArguments;
            const fd_text = args.next() orelse return error.InvalidWorkerArguments;
            const fd = try std.fmt.parseInt(c_int, fd_text, 10);
            if (args.next() != null) return error.InvalidWorkerArguments;
            return runWorker(a, api, image, receiver, selector, fd);
        }
    }

    // Baked exe: if this binary carries an appended image, boot it — this IS the
    // app. A plain birnam has no trailer and falls through to normal handling.
    if (try embeddedImageBytes(a)) |img| {
        defer a.free(img);
        return runImageBytes(a, api, img, rebuild);
    }

    // Script mode: `birnam path.birnam` compiles + runs the file, quietly.
    // REPL mode: `birnam --repl` (or `-i` / `repl`) starts an interactive loop.
    if (script_path) |sp| {
        if (std.mem.eql(u8, sp, "--repl") or std.mem.eql(u8, sp, "-i") or std.mem.eql(u8, sp, "repl"))
            return runRepl(a, api, false);
        // The birnam-side subcommands (doc/docgen/init/wrap/--lsp/--listen):
        // recognize the token, collect the rest of argv verbatim, and hand the
        // whole invocation to the compiled Cli module — usage
        // checks included.
        if (isCliSubcommand(sp)) {
            var argv: std.ArrayList([]const u8) = .{};
            defer argv.deinit(a);
            try argv.append(a, sp);
            while (args.next()) |x| try argv.append(a, x);
            return runCli(a, api, argv.items);
        }
        if (std.mem.eql(u8, sp, "--compile-module")) {
            const msrc = args.next() orelse {
                out("usage: birnam --compile-module <src.birnam> <out.dylib>\n");
                return;
            };
            const mout = args.next() orelse {
                out("usage: birnam --compile-module <src.birnam> <out.dylib>\n");
                return;
            };
            return runCompileModule(a, api, msrc, mout);
        }
        if (std.mem.endsWith(u8, sp, ".image"))
            return runImage(a, api, sp, rebuild);
        if (std.mem.endsWith(u8, sp, ".dylib"))
            return runCompiledArtifact(a, api, sp);
        return runScript(a, api, sp);
    }

    // Bare invocation drops into the REPL — the documented default.
    return runRepl(a, api, false);
}

/// The language stack a script needs, in dependency order. Loaded quietly
/// through the id api; codegen resolves globals through the store, so the store
/// comes first.
const script_modules = [_][]const u8{
    "libbirnam_id_artifact",
    "libbirnam_store",
    "libbirnam_id_peg",
    "libbirnam_id_lang",
    "libbirnam_id_tcc",
    "libbirnam_id_codegen",
    "libbirnam_id_prelude",
    "libbirnam_id_collection",
    "libbirnam_id_error",
    "libbirnam_id_isolate",
    "libbirnam_id_relational",
    "libbirnam_id_alien",
    "libbirnam_id_image",
};

// The self-hosted layer — birnam defining more of its own library on top of the
// native primitives. Each source is embedded so it is ALWAYS available as the boot
// floor; the precompiled dylib (stage-1 build) is preferred when present. Loaded
// by default, skipped whole with BIRNAM_NO_STDLIB=1 (the bare native runtime as an
// option). `toml` follows `stdlib` — `Toml parse:` calls stdlib methods
// (`collect:`/`asInteger`) at RUN time, so stdlib must be up before Toml is used.
const STDLIB_SRC = @embedFile("stdlib.birnam");
const TOML_SRC = @embedFile("toml.birnam");
const CONFIG_SRC = @embedFile("config.birnam");
const APP_CONFIG_SRC = @embedFile("app-config.birnam");
const PROJECT_POLICY_SRC = @embedFile("project-policy.birnam");
const COMPILED_UNIT_SRC = @embedFile("compiled-unit.birnam");
const ISOLATE_SRC = @embedFile("isolate.birnam");
// The module MANAGER itself (docs/44 Phase 5b) — self-hosted like the rest, but
// loaded FIRST and by name (never through the scan/loadOrder flow it implements).
const MODULES_SRC = @embedFile("modules.birnam");

/// Bring up one self-hosted module by `name`: PREFER the precompiled artifact
/// (`libbirnam_<name>.dylib`) — `[Codegen load:]` installs its methods with no
/// recompile — and FALL BACK to evaluating the embedded `src` when that dylib is
/// absent, stale, or ABI-mismatched (`load:` answers nil). Source is the floor;
/// the dylib is the optimization. (Stage 3's `Modules` manager will generalize
/// this into a dependency-ordered load; for now it's a direct helper.)
fn loadSelfHosted(a: std.mem.Allocator, api: *IdApi, codegen: Oop, name: []const u8, src: []const u8) void {
    const force_source = std.posix.getenv("BIRNAM_FORCE_SOURCE") != null;
    if (!force_source) {
        if (resolveLibDir(a)) |dir| {
            defer a.free(dir);
            const fname = std.fmt.allocPrint(a, "libbirnam_{s}.dylib", .{name}) catch null;
            if (fname) |fn_| {
                defer a.free(fn_);
                if (std.fs.path.join(a, &.{ dir, fn_ })) |p| {
                    defer a.free(p);
                    // Only attempt the load if the file is actually there (a missing
                    // file is the common dev case; skip the dlopen-error noise).
                    if (std.fs.cwd().access(p, .{})) |_| {
                        const r = api.send(codegen, api.intern("load:"), &[_]Oop{api.mkString(p.ptr, p.len)}, 1);
                        const is_error = api.isExported("Error") != 0 and
                            api.send(r, api.intern("isError"), &.{}, 0) == api.trueObj;
                        if (r != api.nilObj and !is_error) return; // installed from the precompiled dylib
                    } else |_| {}
                } else |_| {}
            }
        } else |_| {}
    }
    const source = api.mkString(src.ptr, src.len);
    const name_z = a.dupeZ(u8, name) catch return;
    defer a.free(name_z);
    const recorded = api.send(codegen, api.intern("recordModuleSource:named:"), &[_]Oop{ source, api.intern(name_z.ptr) }, 2);
    if (api.isExported("Error") != 0 and api.send(recorded, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj) return;
    _ = api.send(codegen, api.intern("eval:"), &[_]Oop{source}, 1);
}

const SelfHosted = struct { name: [:0]const u8, src: []const u8 };
// The self-hosted layer as a SET — the LOAD ORDER is derived from each module's own
// `requires` declaration (topo-sorted below), not this list's order. To add a
// self-hosted module: embed its source, add a row here, and declare its `requires`
// in its source; the sort places it correctly.
const self_hosted_modules = [_]SelfHosted{
    .{ .name = "stdlib", .src = STDLIB_SRC },
    .{ .name = "compiledUnit", .src = COMPILED_UNIT_SRC },
    .{ .name = "toml", .src = TOML_SRC },
    .{ .name = "config", .src = CONFIG_SRC },
    .{ .name = "appConfig", .src = APP_CONFIG_SRC },
    .{ .name = "isolate", .src = ISOLATE_SRC },
};

/// Project policy is loaded on demand. It is a build/check decision module,
/// not part of the ordinary image generation set: keeping it lazy preserves
/// image capture for interactive/runtime-only sessions while project commands
/// still get the precompiled Birnam implementation (or its embedded source
/// fallback).
fn loadProjectPolicy(a: std.mem.Allocator, api: *IdApi, codegen: Oop) void {
    if (birnamGlobal(api, "ProjectPolicy") != null) return;
    loadSelfHosted(a, api, codegen, "projectPolicy", PROJECT_POLICY_SRC);
    markLoaded(api, "projectPolicy");
}

/// Scan a self-hosted module's source for its `[Module … requires: #[ #a #b ]]`
/// declaration and build a Vector of the dependency-name symbols. The declaration
/// is the module's first statement (a fixed convention; `requires:` never appears
/// before it in a comment), so this reads the requires WITHOUT loading the module —
/// the pre-load knowledge a dependency-ordered boot needs. Absent/empty → an empty
/// Vector (a leaf). The module's own runtime declaration re-asserts the same edges.
fn scanRequires(api: *IdApi, src: []const u8) Oop {
    const vector = api.import("Vector") orelse return api.nilObj;
    var v = api.send(vector, api.intern("new"), &[_]Oop{}, 0);
    const rk = std.mem.indexOf(u8, src, "requires:") orelse return v;
    const br = std.mem.indexOfPos(u8, src, rk, "#[") orelse return v;
    var i = br + 2;
    while (i < src.len and src[i] != ']') {
        if (src[i] == '#') {
            const start = i + 1;
            var j = start;
            while (j < src.len and (std.ascii.isAlphanumeric(src[j]) or src[j] == '_')) j += 1;
            if (j > start) {
                var nbuf: [64]u8 = undefined;
                const len = @min(j - start, nbuf.len - 1);
                @memcpy(nbuf[0..len], src[start..][0..len]);
                nbuf[len] = 0;
                v = api.send(v, api.intern("add:"), &[_]Oop{api.intern(@ptrCast(&nbuf))}, 1);
            }
            i = j;
        } else i += 1;
    }
    return v;
}

/// Install the self-hosted module named `nameSym` (from the embedded table), or do
/// nothing if the name isn't self-hosted (a kernel dependency in the sort order —
/// already loaded).
fn installSelfHostedByName(a: std.mem.Allocator, api: *IdApi, codegen: Oop, nameSym: Oop) void {
    const nm = std.mem.span(api.symbolName(nameSym));
    for (self_hosted_modules) |sh| {
        if (std.mem.eql(u8, nm, sh.name)) {
            loadSelfHosted(a, api, codegen, sh.name, sh.src);
            markLoaded(api, sh.name);
            return;
        }
    }
}

/// Record a boot-installed self-hosted module as `#loaded` in the Module graph, so a
/// later `[Module load:]` of it (e.g. as another module's dependency) is a no-op.
fn markLoaded(api: *IdApi, name: [:0]const u8) void {
    if (birnamGlobal(api, "Module")) |mm|
        _ = api.send(mm, api.intern("markLoaded:"), &[_]Oop{api.intern(name.ptr)}, 1);
}

/// Bring up the whole self-hosted layer, unless opted out. The load ORDER is a
/// topological sort of the modules' `requires` graph (§stage-3): each source's
/// requires is scanned + registered into the `Module` manager, then `[Module
/// loadOrder:]` decides the sequence — so a module always loads after its deps,
/// derived from the sources rather than hardcoded. Falls back to the declared table
/// order if the manager is unavailable or reports a cycle.
fn loadStdlib(a: std.mem.Allocator, api: *IdApi, codegen: Oop) void {
    if (std.posix.getenv("BIRNAM_NO_STDLIB") != null) return;
    // The manager FIRST, hardcoded (docs/44 Phase 5b): it defines the
    // `named:requires:`/`loadOrder:`/`markLoaded:` protocol this very function —
    // and every self-hosted source's opening statement — sequences the layer
    // with, so it cannot ride the flow it implements. Same dylib-else-embedded-
    // source floor as the rest; it stays out of the scanned set below (no deps
    // by construction, and its body contains the literal "named:requires:" the
    // first-substring scan would misread).
    loadSelfHosted(a, api, codegen, "modules", MODULES_SRC);
    const mm = birnamGlobal(api, "Module");
    if (mm != null) refreshModuleManager(a, api, mm.?);
    const vector = api.import("Vector");
    if (mm != null and vector != null) {
        const m = mm.?;
        // Register each module's scanned requires, and collect the name set.
        var names = api.send(vector.?, api.intern("new"), &[_]Oop{}, 0);
        for (self_hosted_modules) |sh| {
            _ = api.send(m, api.intern("named:requires:"), &[_]Oop{ api.intern(sh.name.ptr), scanRequires(api, sh.src) }, 2);
            names = api.send(names, api.intern("add:"), &[_]Oop{api.intern(sh.name.ptr)}, 1);
        }
        const order = api.send(m, api.intern("loadOrder:"), &[_]Oop{names}, 1);
        // loadOrder answers a Vector, or a #circularDependency error → fall back.
        const is_error = api.isExported("Error") != 0 and
            api.send(order, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj;
        if (order != api.nilObj and !is_error) {
            const n = api.smallIntValue(api.send(order, api.intern("size"), &[_]Oop{}, 0));
            var k: i64 = 0;
            while (k < n) : (k += 1)
                installSelfHostedByName(a, api, codegen, api.send(order, api.intern("at:"), &[_]Oop{api.mkSmallInt(k)}, 1));
            configureRuntimeNativeDir(a, api);
            return;
        }
    }
    // Fallback: the declared table order (also correct — it's dependency-ordered).
    for (self_hosted_modules) |sh| {
        loadSelfHosted(a, api, codegen, sh.name, sh.src);
        markLoaded(api, sh.name);
    }
    configureRuntimeNativeDir(a, api);
}

/// Resolve the directory holding runtime dylibs through the single runtime
/// resource contract. `BIRNAM_LIB_DIR` remains an explicit escape hatch.
fn resolveLibDir(a: std.mem.Allocator) ![]u8 {
    var resources = try runtime_resources.resolve(a);
    defer resources.deinit();
    return a.dupe(u8, resources.lib_dir);
}

/// Load the full language stack quietly through the id api (store first, since
/// codegen resolves globals through it).
fn loadStack(a: std.mem.Allocator, api: *IdApi) !void {
    const lib_dir = try resolveLibDir(a);
    defer a.free(lib_dir);
    for (script_modules) |m| {
        const ps = try std.fmt.allocPrint(a, "{s}/{s}.dylib", .{ lib_dir, m });
        defer a.free(ps);
        const p = try a.dupeZ(u8, ps);
        defer a.free(p);
        _ = try loadModule(api, p.ptr, null);
    }
    try resolveIsolateFunctions(a, api);
    // The whole type universe now exists: make the MOP mirror total (docs/42 §D1) —
    // every native selector reflects through the fact store, alongside user methods.
    if (api.import("Store")) |st| {
        _ = api.send(st, api.intern("backfillMirror"), &[_]Oop{}, 0);
    }
    // The module MANAGER is no longer registered here: it is birnam now
    // (modules.birnam), brought up as loadStdlib's first act — the kernel tier is
    // represented in its graph there (registerKernelModules), once it exists.
}

/// The Module manager singleton — a birnam-minted global (`[Globals at: #Module
/// put: …]` in modules.birnam), NOT a C-level export, so it is looked up through
/// Globals. Null when the self-hosted layer (or the manager itself) isn't up.
fn birnamGlobal(api: *IdApi, name: [:0]const u8) ?Oop {
    const globals = api.import("Globals") orelse return null;
    const r = api.send(globals, api.intern("at:"), &[_]Oop{api.intern(name.ptr)}, 1);
    return if (r == api.nilObj) null else r;
}

/// Represent the kernel modules in the manager's graph: registered, kind #kernel,
/// no requires — the manager KNOWS they exist (so a self-hosted module may
/// `requires:` one and `loadOrder:` treats it as a satisfied leaf) but does NOT
/// load them; they are already up. Names mirror script_modules. Also points the
/// manager at the freshly-resolved dylib dir so `[Module load: #x]` resolves
/// `<libDir>/libbirnam_x.dylib` — re-sent at every boot (images included), so a
/// booted image loads from ITS tree, not the one it was saved in.
fn refreshModuleManager(a: std.mem.Allocator, api: *IdApi, mm: Oop) void {
    const kernel = [_][:0]const u8{
        "artifact", "store",      "peg",   "lang",       "tcc",   "codegen",
        "prelude",  "collection", "error", "relational", "alien", "image",
    };
    for (kernel) |name| _ = api.send(mm, api.intern("kernel:"), &[_]Oop{api.intern(name.ptr)}, 1);
    if (resolveLibDir(a)) |lib_dir| {
        defer a.free(lib_dir);
        _ = api.send(mm, api.intern("libDir:"), &[_]Oop{api.mkString(lib_dir.ptr, lib_dir.len)}, 1);
    } else |_| {}
}

/// Compile + run a birnam script file. Loads the full stack quietly, then
/// `[Codegen evalPersisting: source]`. The script does its own I/O via `System`.
/// Top-level `let`s PERSIST to the Globals store (as in the REPL) — so a script
/// that binds `main` (or any global) leaves it in the store for `[System
/// saveImage:]` to capture, and records its source for image re-derivation
/// (docs/40 §1 entry-point convention, §6 source-backed block globals).
fn runScript(a: std.mem.Allocator, api: *IdApi, path: []const u8) !void {
    try loadStack(a, api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    // Anchor the conservative GC's stack scan here: every frame the eval (and the
    // native code it runs) pushes lives BELOW this local, so `[System gc]` from
    // within the script can find all stack-held roots. Without this, gcCollect is
    // a no-op (it refuses to sweep when it cannot find stack roots).
    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);

    loadStdlib(a, api, codegen);

    const src = try std.fs.cwd().readFileAlloc(a, path, 16 << 20);
    defer a.free(src);
    _ = api.send(codegen, api.intern("evalPersisting:from:"), &[_]Oop{ api.mkString(src.ptr, src.len), api.mkString(path.ptr, path.len) }, 2);

    // Surface a compile failure instead of exiting quietly on a program that
    // silently became nil (a syntax error or an out-of-subset construct).
    const err = api.send(codegen, api.intern("lastError"), &[_]Oop{}, 0);
    if (err != api.nilObj) {
        out("birnam: compile error in ");
        out(path);
        out(":\n");
        outErrorString(api, err);
        std.process.exit(1);
    }
}

/// Inspect and activate an explicitly selected compiled root. The host knows
/// only that this is a dynamic-library container; module identity, requirements,
/// native requirements, and capabilities come from the artifact's ID
/// descriptor. No project manifest or build receipt participates.
fn runCompiledArtifact(a: std.mem.Allocator, api: *IdApi, path: []const u8) !void {
    // The only pre-inspection load after libid: the tiny Artifact rung. It
    // returns an ID descriptor without Store, Codegen, Module, or artifact code.
    const runtime_lib = try resolveLibDir(a);
    defer a.free(runtime_lib);
    const inspector_path = try std.fs.path.join(a, &.{ runtime_lib, "libbirnam_id_artifact.dylib" });
    defer a.free(inspector_path);
    const inspector_path_z = try a.dupeZ(u8, inspector_path);
    defer a.free(inspector_path_z);
    _ = try loadModule(api, inspector_path_z.ptr, null);
    const artifact = api.import("Artifact") orelse return error.NoArtifactInspector;
    const absolute = try std.fs.cwd().realpathAlloc(a, path);
    defer a.free(absolute);
    const inspected = api.send(artifact, api.intern("inspect:"), &.{api.mkString(absolute.ptr, absolute.len)}, 1);
    if (inspected == api.nilObj) {
        const failure = api.send(artifact, api.intern("lastFailure"), &.{}, 0);
        std.fs.File.stderr().writeAll("birnam: artifact inspection failed") catch {};
        if (failure != api.nilObj and api.vtableOf(failure) == api.symbol_vt) {
            std.fs.File.stderr().writeAll(": ") catch {};
            std.fs.File.stderr().writeAll(std.mem.span(api.symbolName(failure))) catch {};
        }
        std.fs.File.stderr().writeAll("\n") catch {};
        return error.ArtifactInspectionFailed;
    }

    // The item-level descriptor protocol needs no Collection/Store rung. Walk
    // and reject the entire sibling closure before loading the language stack
    // or executing any artifact initializer.
    const directory = std.fs.path.dirname(absolute) orelse ".";
    var preflight = artifact_preflight.Preflight.init(a, api, artifact, directory);
    defer preflight.deinit();
    try preflight.walkRoot(inspected, null);

    try loadStack(a, api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);
    loadStdlib(a, api, codegen);

    const manager = birnamGlobal(api, "Module") orelse return error.NoModuleManager;
    _ = api.send(manager, api.intern("libDir:"), &.{api.mkString(directory.ptr, directory.len)}, 1);
    if (api.import("Alien")) |alien|
        _ = api.send(alien, api.intern("libraryDir:"), &.{api.mkString(directory.ptr, directory.len)}, 1);

    const result = api.send(manager, api.intern("loadArtifact:"), &.{api.mkString(absolute.ptr, absolute.len)}, 1);
    if (result == api.nilObj or apiErrorValue(api, result)) {
        std.fs.File.stderr().writeAll("birnam: artifact activation failed: ") catch {};
        if (result != api.nilObj) outErrorString(api, result);
        return error.ArtifactActivationFailed;
    }
}

/// `birnam check [directory]` — observe the nearest manifest and print the
/// source-only ProjectResolution. The host adapter uses Toml and Codegen's
/// full-CST metadata scanner; ProjectPolicy makes the resolution decisions
/// after the language floor is up. Neither side evaluates project source or
/// mutates Module/Store state.
fn runProjectCheck(a: std.mem.Allocator, api: *IdApi, options: ProjectCheckOptions) !void {
    try loadStack(a, api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);
    loadStdlib(a, api, codegen);
    loadProjectPolicy(a, api, codegen);

    var resolution_result = project.resolve(api, a, options.directory);
    defer resolution_result.deinit();
    if (resolution_result.diagnostic) |diagnostic| {
        if (options.format == .json) {
            var buffer: [4096]u8 = undefined;
            var writer = std.fs.File.stdout().writer(&buffer);
            try project_check.writeFailureJson(diagnostic, &writer.interface);
            try writer.interface.flush();
            std.process.exit(1);
        }
        checkDiagnosticError(diagnostic.kind, diagnostic.message);
    }

    var checked = try project_check.check(api, a, &resolution_result.resolution.?);
    defer checked.deinit();
    var stdout_buffer: [4096]u8 = undefined;
    var stdout_writer = std.fs.File.stdout().writer(&stdout_buffer);
    if (options.format == .json) {
        try project_check.writeJson(&checked, &stdout_writer.interface);
    } else {
        try project.writeReport(&resolution_result, &stdout_writer.interface);
        try project_check.writeHuman(&checked, &stdout_writer.interface);
    }
    stdout_writer.interface.flush() catch |err| return err;
    if (!checked.ok()) std.process.exit(1);
}

fn checkUsageError(comptime fmt: []const u8, args: anytype) noreturn {
    var buffer: [512]u8 = undefined;
    const message = std.fmt.bufPrint(&buffer, fmt, args) catch "invalid command arguments";
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam check: ") catch {};
    stderr.writeAll(message) catch {};
    stderr.writeAll("\n") catch {};
    stderr.writeAll(CHECK_USAGE) catch {};
    std.process.exit(2);
}

fn checkDiagnosticError(kind: []const u8, message: []const u8) noreturn {
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam check: ") catch {};
    stderr.writeAll(kind) catch {};
    stderr.writeAll(": ") catch {};
    stderr.writeAll(message) catch {};
    stderr.writeAll("\nusage: birnam check [directory]\n") catch {};
    stderr.writeAll("try 'birnam check --help' for more information\n") catch {};
    std.process.exit(1);
}

fn checkCommandError(err: anyerror) noreturn {
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam check: ") catch {};
    stderr.writeAll(@errorName(err)) catch {};
    stderr.writeAll("\nusage: birnam check [directory]\n") catch {};
    stderr.writeAll("try 'birnam check --help' for more information\n") catch {};
    std.process.exit(1);
}

/// `birnam build [directory]` — materialize the resolved source graph into a
/// complete project-local artifact directory. Compilation happens through the
/// existing Codegen artifact primitive; this command never loads the project
/// modules or activates their runtime behavior.
fn runProjectBuild(a: std.mem.Allocator, api: *IdApi, options: ProjectBuildOptions) !void {
    try loadStack(a, api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);
    loadStdlib(a, api, codegen);
    loadProjectPolicy(a, api, codegen);

    var resolution = project.resolve(api, a, options.directory);
    defer resolution.deinit();
    if (resolution.diagnostic) |diagnostic|
        buildDiagnosticError(diagnostic.kind, diagnostic.message);

    var result = project_build.build(api, a, &resolution.resolution.?, build_opts.version, options.profile);
    defer result.deinit();
    if (result.diagnostic) |diagnostic|
        buildDiagnosticError(diagnostic.kind, diagnostic.message);

    var stdout_buffer: [4096]u8 = undefined;
    var stdout_writer = std.fs.File.stdout().writer(&stdout_buffer);
    project_build.writeReport(&result, &stdout_writer.interface) catch |err| return err;
    stdout_writer.interface.flush() catch |err| return err;
}

/// Install a validated project publication into the runtime's module directory.
/// The build record is the authority for the exact artifact set; no source is
/// compiled and no project code is evaluated unless `--build` was requested.
fn installProjectBuild(a: std.mem.Allocator, build: *const project_build.Build, destination: []const u8) !void {
    try std.fs.cwd().makePath(destination);

    var token_value: u64 = undefined;
    std.crypto.random.bytes(std.mem.asBytes(&token_value));
    const token = try std.fmt.allocPrint(a, ".birnam-install-{x}", .{token_value});
    defer a.free(token);
    const stage = try std.fs.path.join(a, &.{ destination, token });
    defer a.free(stage);
    try std.fs.cwd().makePath(stage);
    defer std.fs.cwd().deleteTree(stage) catch {};

    const copyToStage = struct {
        fn run(allocator: std.mem.Allocator, stage_path: []const u8, build_root: []const u8, relative_path: []const u8) ![]u8 {
            const source = try std.fs.path.join(allocator, &.{ build_root, relative_path });
            defer allocator.free(source);
            const filename = std.fs.path.basename(relative_path);
            const staged = try std.fs.path.join(allocator, &.{ stage_path, filename });
            errdefer allocator.free(staged);
            try std.fs.cwd().copyFile(source, std.fs.cwd(), staged, .{});
            return staged;
        }
    }.run;

    var staged_paths: std.ArrayList([]u8) = .{};
    defer {
        for (staged_paths.items) |path| a.free(path);
        staged_paths.deinit(a);
    }
    for (build.artifacts) |artifact|
        try staged_paths.append(a, try copyToStage(a, stage, build.output_root, artifact.relative_path));
    for (build.native) |artifact|
        try staged_paths.append(a, try copyToStage(a, stage, build.output_root, artifact.relative_path));

    for (staged_paths.items) |staged| {
        const filename = std.fs.path.basename(staged);
        const installed = try std.fs.path.join(a, &.{ destination, filename });
        defer a.free(installed);
        try std.fs.cwd().rename(staged, installed);
    }
}

/// `birnam install [directory]` — copy the complete, validated project
/// publication into the same library directory used by the runtime loader.
fn runProjectInstall(a: std.mem.Allocator, api: *IdApi, options: ProjectInstallOptions) !void {
    try loadStack(a, api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);
    loadStdlib(a, api, codegen);
    loadProjectPolicy(a, api, codegen);

    var resolution = project.resolve(api, a, options.directory);
    defer resolution.deinit();
    if (resolution.diagnostic) |diagnostic|
        installDiagnosticError(diagnostic.kind, diagnostic.message);

    var result = if (options.build)
        project_build.build(api, a, &resolution.resolution.?, build_opts.version, options.profile)
    else
        project_build.validate(api, a, &resolution.resolution.?, build_opts.version, options.profile);
    defer result.deinit();
    if (result.diagnostic) |diagnostic|
        installDiagnosticError(diagnostic.kind, diagnostic.message);

    const destination = try resolveLibDir(a);
    defer a.free(destination);
    try installProjectBuild(a, &result.build.?, destination);

    out("installed: ");
    out(destination);
    out("\n");
    for (result.build.?.artifacts) |artifact| {
        out("  #");
        out(artifact.name);
        out(" -> ");
        out(std.fs.path.basename(artifact.relative_path));
        out("\n");
    }
    for (result.build.?.native) |artifact| {
        out("  ");
        out(artifact.name);
        out(" -> ");
        out(std.fs.path.basename(artifact.relative_path));
        out("\n");
    }
}

fn buildUsageError(comptime fmt: []const u8, args: anytype) noreturn {
    var buffer: [512]u8 = undefined;
    const message = std.fmt.bufPrint(&buffer, fmt, args) catch "invalid command arguments";
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam build: ") catch {};
    stderr.writeAll(message) catch {};
    stderr.writeAll("\n") catch {};
    stderr.writeAll(BUILD_USAGE) catch {};
    std.process.exit(2);
}

fn buildDiagnosticError(kind: []const u8, message: []const u8) noreturn {
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam build: ") catch {};
    stderr.writeAll(kind) catch {};
    stderr.writeAll(": ") catch {};
    stderr.writeAll(message) catch {};
    stderr.writeAll("\nusage: birnam build [directory]\n") catch {};
    stderr.writeAll("try 'birnam build --help' for more information\n") catch {};
    std.process.exit(1);
}

fn buildCommandError(err: anyerror) noreturn {
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam build: ") catch {};
    stderr.writeAll(@errorName(err)) catch {};
    stderr.writeAll("\nusage: birnam build [directory]\n") catch {};
    stderr.writeAll("try 'birnam build --help' for more information\n") catch {};
    std.process.exit(1);
}

fn installUsageError(comptime fmt: []const u8, args: anytype) noreturn {
    var buffer: [512]u8 = undefined;
    const message = std.fmt.bufPrint(&buffer, fmt, args) catch "invalid command arguments";
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam install: ") catch {};
    stderr.writeAll(message) catch {};
    stderr.writeAll("\n") catch {};
    stderr.writeAll(INSTALL_USAGE) catch {};
    std.process.exit(2);
}

fn installDiagnosticError(kind: []const u8, message: []const u8) noreturn {
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam install: ") catch {};
    stderr.writeAll(kind) catch {};
    stderr.writeAll(": ") catch {};
    stderr.writeAll(message) catch {};
    stderr.writeAll("\nusage: birnam install [directory] [--build]\n") catch {};
    stderr.writeAll("try 'birnam install --help' for more information\n") catch {};
    std.process.exit(1);
}

fn installCommandError(err: anyerror) noreturn {
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam install: ") catch {};
    stderr.writeAll(@errorName(err)) catch {};
    stderr.writeAll("\nusage: birnam install [directory] [--build]\n") catch {};
    stderr.writeAll("try 'birnam install --help' for more information\n") catch {};
    std.process.exit(1);
}

fn runUsageError(comptime fmt: []const u8, args: anytype) noreturn {
    var buffer: [512]u8 = undefined;
    const message = std.fmt.bufPrint(&buffer, fmt, args) catch "invalid command arguments";
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam run: ") catch {};
    stderr.writeAll(message) catch {};
    stderr.writeAll("\n") catch {};
    stderr.writeAll(RUN_USAGE) catch {};
    std.process.exit(2);
}

fn runDiagnosticError(kind: []const u8, message: []const u8) noreturn {
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam run: ") catch {};
    stderr.writeAll(kind) catch {};
    stderr.writeAll(": ") catch {};
    stderr.writeAll(message) catch {};
    stderr.writeAll("\nusage: birnam run [directory] [options] [-- args...]\n") catch {};
    stderr.writeAll("try 'birnam run --help' for more information\n") catch {};
    std.process.exit(1);
}

fn runCommandError(err: anyerror) noreturn {
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam run: ") catch {};
    stderr.writeAll(@errorName(err)) catch {};
    stderr.writeAll("\nusage: birnam run [directory] [options] [-- args...]\n") catch {};
    stderr.writeAll("try 'birnam run --help' for more information\n") catch {};
    std.process.exit(1);
}

fn testUsageError(comptime fmt: []const u8, args: anytype) noreturn {
    var buffer: [512]u8 = undefined;
    const message = std.fmt.bufPrint(&buffer, fmt, args) catch "invalid command arguments";
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam test: ") catch {};
    stderr.writeAll(message) catch {};
    stderr.writeAll("\n") catch {};
    stderr.writeAll(TEST_USAGE) catch {};
    std.process.exit(2);
}

fn testDiagnosticError(kind: []const u8, message: []const u8) noreturn {
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam test: ") catch {};
    stderr.writeAll(kind) catch {};
    stderr.writeAll(": ") catch {};
    stderr.writeAll(message) catch {};
    stderr.writeAll("\nusage: birnam test [directory] [options]\n") catch {};
    stderr.writeAll("try 'birnam test --help' for more information\n") catch {};
    std.process.exit(1);
}

fn testCommandError(err: anyerror) noreturn {
    const stderr = std.fs.File.stderr();
    stderr.writeAll("birnam test: ") catch {};
    stderr.writeAll(@errorName(err)) catch {};
    stderr.writeAll("\nusage: birnam test [directory] [options]\n") catch {};
    stderr.writeAll("try 'birnam test --help' for more information\n") catch {};
    std.process.exit(1);
}

fn apiErrorValue(api: *IdApi, value: Oop) bool {
    return value != null and api.isExported("Error") != 0 and
        api.send(value, api.intern("isError"), &.{}, 0) == api.trueObj;
}

/// Load one Birnam runtime module through the canonical compiled-unit manager.
/// The manager resolves the exact executable-relative dylib path and recursively
/// installs dependencies from compiler-embedded metadata.
fn loadRuntimeModule(a: std.mem.Allocator, api: *IdApi, name: [:0]const u8) !void {
    _ = a;
    const manager = birnamGlobal(api, "Module") orelse return error.NoModuleManager;
    const result = api.send(manager, api.intern("load:"), &[_]Oop{api.intern(name.ptr)}, 1);
    if (result == api.nilObj or apiErrorValue(api, result)) {
        if (result != api.nilObj) outErrorString(api, result);
        return error.RuntimeModuleFailed;
    }
}

/// Install a process-local host tool after image recovery. Unlike ordinary
/// Module loading, this does not claim that the tool belongs to the image's
/// sealed application generation; it is the compiled equivalent of the former
/// non-persisting source evaluation used for the REPL fallback.
fn loadHostRuntimeModule(a: std.mem.Allocator, api: *IdApi, name: [:0]const u8) !void {
    const codegen = api.import("Codegen") orelse return error.NoCodegen;
    const manager = birnamGlobal(api, "Module") orelse return error.NoModuleManager;
    const lib_dir = try resolveLibDir(a);
    defer a.free(lib_dir);
    const file_name = try std.fmt.allocPrint(a, "libbirnam_{s}.dylib", .{name});
    defer a.free(file_name);
    const path = try std.fs.path.join(a, &.{ lib_dir, file_name });
    defer a.free(path);
    const result = api.send(codegen, api.intern("loadHostCapability:"), &[_]Oop{api.mkString(path.ptr, path.len)}, 1);
    if (result == api.nilObj or apiErrorValue(api, result)) {
        if (result != api.nilObj) outErrorString(api, result);
        return error.RuntimeModuleFailed;
    }
    _ = api.send(manager, api.intern("markLoaded:"), &[_]Oop{api.intern(name.ptr)}, 1);
}

fn loadProjectArtifacts(api: *IdApi, build: *const project_build.Build) void {
    const inspector = api.import("Artifact") orelse
        runDiagnosticError("projectArtifactInspectionUnavailable", "Artifact inspection is not available");
    const manager = birnamGlobal(api, "Module") orelse
        runDiagnosticError("projectArtifactLoadFailed", "Module is not available");

    // Validate every selected output before activating the first one. The build
    // receipt selects exact paths; embedded descriptors remain runtime authority.
    for (build.artifacts) |artifact| {
        const path = std.fs.path.join(std.heap.c_allocator, &.{ build.output_root, artifact.relative_path }) catch
            runDiagnosticError("projectArtifactLoadFailed", "could not construct a project artifact path");
        defer std.heap.c_allocator.free(path);
        const description = api.send(inspector, api.intern("inspect:"), &[_]Oop{api.mkString(path.ptr, path.len)}, 1);
        if (description == api.nilObj) {
            const failure = api.send(inspector, api.intern("lastFailure"), &.{}, 0);
            const kind = if (failure != api.nilObj and api.vtableOf(failure) == api.symbol_vt)
                std.mem.span(api.symbolName(failure))
            else
                "projectArtifactInspectionFailed";
            runDiagnosticError(kind, std.fmt.allocPrint(std.heap.c_allocator, "could not inspect artifact for #{s} at {s}", .{ artifact.name, path }) catch "could not inspect project artifact");
        }
        const name_z = std.heap.c_allocator.dupeZ(u8, artifact.name) catch
            runDiagnosticError("projectArtifactLoadFailed", "out of memory while validating a project artifact");
        defer std.heap.c_allocator.free(name_z);
        if (api.send(description, api.intern("kind"), &.{}, 0) != api.intern("module"))
            runDiagnosticError("artifactKindMismatch", std.fmt.allocPrint(std.heap.c_allocator, "artifact for #{s} is not a module", .{artifact.name}) catch "project artifact is not a module");
        if (api.send(description, api.intern("identity"), &.{}, 0) != api.intern(name_z.ptr))
            runDiagnosticError("artifactIdentityMismatch", std.fmt.allocPrint(std.heap.c_allocator, "artifact identity does not match #{s}", .{artifact.name}) catch "project artifact identity mismatch");
    }

    // The publication order is dependency-first. Each explicit path therefore
    // reaches Module with its project dependencies already installed; runtime
    // dependencies remain rooted in the runtime module directory.
    for (build.artifacts) |artifact| {
        const path = std.fs.path.join(std.heap.c_allocator, &.{ build.output_root, artifact.relative_path }) catch
            runDiagnosticError("projectArtifactLoadFailed", "could not construct a project artifact path");
        defer std.heap.c_allocator.free(path);
        const name_z = std.heap.c_allocator.dupeZ(u8, artifact.name) catch
            runDiagnosticError("projectArtifactLoadFailed", "out of memory while loading a project artifact");
        defer std.heap.c_allocator.free(name_z);
        const result = api.send(manager, api.intern("load:path:"), &[_]Oop{ api.intern(name_z.ptr), api.mkString(path.ptr, path.len) }, 2);
        if (result == api.nilObj)
            runDiagnosticError("projectArtifactLoadFailed", std.fmt.allocPrint(std.heap.c_allocator, "could not load artifact for #{s} at {s}", .{ artifact.name, path }) catch "could not load project artifact");
        if (apiErrorValue(api, result)) {
            outErrorString(api, result);
            runDiagnosticError("projectArtifactLoadFailed", std.fmt.allocPrint(std.heap.c_allocator, "artifact load failed for #{s}", .{artifact.name}) catch "artifact load failed");
        }
    }
}

fn configureProjectNativeDir(api: *IdApi, build: *const project_build.Build) void {
    const lib_dir = std.fs.path.join(std.heap.c_allocator, &.{ build.output_root, "lib" }) catch
        runDiagnosticError("projectArtifactLoadFailed", "could not construct the project native library directory");
    defer std.heap.c_allocator.free(lib_dir);

    const alien = api.import("Alien") orelse
        runDiagnosticError("projectArtifactLoadFailed", "Alien is not available");
    const native_result = api.send(alien, api.intern("libraryDir:"), &[_]Oop{api.mkString(lib_dir.ptr, lib_dir.len)}, 1);
    if (native_result == api.nilObj or apiErrorValue(api, native_result))
        runDiagnosticError("projectArtifactLoadFailed", "could not configure the project native library directory");
}

/// The process-wide default for bare Alien dlopen names follows the same
/// resolved directory as Module and the kernel dylibs. A project run overrides
/// this with its project-local build/lib directory before loading project code.
fn configureRuntimeNativeDir(a: std.mem.Allocator, api: *IdApi) void {
    const alien = api.import("Alien") orelse return;
    const lib_dir = resolveLibDir(a) catch return;
    defer a.free(lib_dir);
    _ = api.send(alien, api.intern("libraryDir:"), &[_]Oop{api.mkString(lib_dir.ptr, lib_dir.len)}, 1);
}

fn installProjectArguments(api: *IdApi, arguments: []const []const u8) !void {
    const vector = api.import("Vector") orelse return error.NoVector;
    const globals = api.import("Globals") orelse return error.NoGlobals;
    var values = api.send(vector, api.intern("new"), &.{}, 0);
    for (arguments) |argument| {
        values = api.send(values, api.intern("add:"), &[_]Oop{api.mkString(argument.ptr, argument.len)}, 1);
    }
    _ = api.send(globals, api.intern("at:put:"), &[_]Oop{ api.intern("ProjectArgs"), values }, 2);
}

fn evalProjectSource(a: std.mem.Allocator, api: *IdApi, path: []const u8) !void {
    const codegen = api.import("Codegen") orelse return error.NoCodegen;
    const source = std.fs.cwd().readFileAlloc(a, path, 16 << 20) catch {
        runDiagnosticError("projectMainReadFailed", std.fmt.allocPrint(std.heap.c_allocator, "could not read project main {s}", .{path}) catch "could not read project main");
    };
    defer a.free(source);
    _ = api.send(codegen, api.intern("evalPersisting:from:"), &[_]Oop{ api.mkString(source.ptr, source.len), api.mkString(path.ptr, path.len) }, 2);
    const last_error = api.send(codegen, api.intern("lastError"), &.{}, 0);
    if (last_error != api.nilObj) {
        out("birnam: compile error in ");
        out(path);
        out(":\n");
        outErrorString(api, last_error);
        std.process.exit(1);
    }
}

fn runProjectSocketRepl(a: std.mem.Allocator, api: *IdApi, bind: []const u8, port: i64) !void {
    _ = a;
    const repl = birnamGlobal(api, "Repl") orelse return error.NoRepl;
    const result = api.send(repl, api.intern("serveOn:bind:"), &[_]Oop{
        api.mkSmallInt(port), api.mkString(bind.ptr, bind.len),
    }, 2);
    if (apiErrorValue(api, result)) {
        outErrorString(api, result);
        return error.SocketServeFailed;
    }
    return;
}

/// Bare REPL startup adopts a validated project publication only for a
/// configured project with a `main`. This preserves project-local runs while
/// keeping ordinary/image-save REPL startup free of the lazy policy module.
fn prepareBareProject(a: std.mem.Allocator, api: *IdApi, codegen: Oop) !void {
    const config = birnamGlobal(api, "Config") orelse return;
    const main_path = api.send(config, api.intern("main"), &.{}, 0);
    if (main_path == api.nilObj or api.vtableOf(main_path) != api.string_vt) return;

    loadProjectPolicy(a, api, codegen);
    var resolution_result = project.resolve(api, a, ".");
    defer resolution_result.deinit();
    if (resolution_result.diagnostic != null) return;

    var artifact_result = project_build.validate(api, a, &resolution_result.resolution.?, build_opts.version, "debug");
    defer artifact_result.deinit();
    if (artifact_result.diagnostic != null) return;
    configureProjectNativeDir(api, &artifact_result.build.?);
    loadProjectArtifacts(api, &artifact_result.build.?);
}

fn runProject(a: std.mem.Allocator, api: *IdApi, options: ProjectRunOptions) !void {
    try loadStack(a, api);
    // Project applications may host language tools, so preserve the same
    // read-only resolver bridge already installed for the LSP CLI. This binds
    // only the existing snapshot/path selectors; it adds no new runtime
    // primitive or project authority.
    try installProjectBridge(api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);
    loadStdlib(a, api, codegen);
    loadProjectPolicy(a, api, codegen);

    var resolution_result = project.resolve(api, a, options.directory);
    defer resolution_result.deinit();
    if (resolution_result.diagnostic) |diagnostic|
        runDiagnosticError(diagnostic.kind, diagnostic.message);
    const resolution = &resolution_result.resolution.?;
    active_project_resolution = resolution;
    defer active_project_resolution = null;
    const main_path = resolution.main_path orelse
        runDiagnosticError("projectMainMissing", "the project has no main entry; set main = \"...\" in birnam.toml");

    const mode_name = options.repl orelse parseReplMode(resolution.run.repl).?;
    // Load host tools from the runtime bundle before the manager is pointed at
    // the project's published lib directory.
    switch (mode_name) {
        .none => {},
        .tty => try loadRuntimeModule(a, api, "repl"),
        .socket => {
            try loadRuntimeModule(a, api, "socket");
            try loadRuntimeModule(a, api, "repl");
        },
    }

    var artifact_result = if (options.build)
        project_build.build(api, a, resolution, build_opts.version, options.profile)
    else
        project_build.validate(api, a, resolution, build_opts.version, options.profile);
    defer artifact_result.deinit();
    if (artifact_result.diagnostic) |diagnostic|
        runDiagnosticError(diagnostic.kind, diagnostic.message);
    active_project_build = &artifact_result.build.?;
    defer active_project_build = null;
    configureProjectNativeDir(api, &artifact_result.build.?);
    loadProjectArtifacts(api, &artifact_result.build.?);
    try installProjectArguments(api, options.arguments);
    try evalProjectSource(a, api, main_path);

    const bind = options.socket_bind orelse resolution.run.socket_bind;
    const port = options.socket_port orelse resolution.run.socket_port;
    if ((options.socket_bind != null or options.socket_port != null) and mode_name != .socket)
        runUsageError("socket bind and port options require --repl=socket", .{});
    switch (mode_name) {
        .none => return,
        .tty => return runReplSession(a, api, true, true),
        .socket => return runProjectSocketRepl(a, api, bind, port),
    }
}

fn runProjectTestWorker(a: std.mem.Allocator, api: *IdApi, directory: []const u8, profile: []const u8, test_path: []const u8) !void {
    try loadStack(a, api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);
    loadStdlib(a, api, codegen);
    loadProjectPolicy(a, api, codegen);

    var resolution_result = project.resolve(api, a, directory);
    defer resolution_result.deinit();
    if (resolution_result.diagnostic) |diagnostic|
        testDiagnosticError(diagnostic.kind, diagnostic.message);
    const resolution = &resolution_result.resolution.?;
    active_project_resolution = resolution;
    defer active_project_resolution = null;

    // Load the test harness from the runtime closure before project-native
    // requirements are selected. Project source is still evaluated only after
    // its complete compiled artifact set has passed validation and activation.
    try loadRuntimeModule(a, api, "test");
    var artifact_result = project_build.validate(api, a, resolution, build_opts.version, profile);
    defer artifact_result.deinit();
    if (artifact_result.diagnostic) |diagnostic|
        testDiagnosticError(diagnostic.kind, diagnostic.message);
    active_project_build = &artifact_result.build.?;
    defer active_project_build = null;
    configureProjectNativeDir(api, &artifact_result.build.?);
    loadProjectArtifacts(api, &artifact_result.build.?);
    try installProjectArguments(api, &.{});
    try evalProjectSource(a, api, test_path);

    const test_object = birnamGlobal(api, "Test") orelse
        testDiagnosticError("testHarnessUnavailable", "the Test runtime module did not publish Test");
    const runner = birnamGlobal(api, "ConsoleRunner") orelse
        testDiagnosticError("testHarnessUnavailable", "the Test runtime module did not publish ConsoleRunner");
    const suites = api.send(test_object, api.intern("suites"), &.{}, 0);
    const count = api.send(suites, api.intern("size"), &.{}, 0);
    if (api.isSmallInt(count) == 0 or api.smallIntValue(count) == 0)
        testDiagnosticError("testSuiteMissing", "the test source did not register a TestSuite");
    const result = api.send(test_object, api.intern("runAllSuitesWithRunner:"), &.{runner}, 1);
    if (api.send(result, api.intern("allPassed"), &.{}, 0) != api.trueObj) std.process.exit(1);
}

fn runProjectTest(a: std.mem.Allocator, api: *IdApi, options: ProjectTestOptions) !void {
    try loadStack(a, api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);
    loadStdlib(a, api, codegen);
    loadProjectPolicy(a, api, codegen);

    var resolution_result = project.resolve(api, a, options.directory);
    defer resolution_result.deinit();
    if (resolution_result.diagnostic) |diagnostic|
        testDiagnosticError(diagnostic.kind, diagnostic.message);
    const resolution = &resolution_result.resolution.?;
    if (resolution.test_sources.len == 0)
        testDiagnosticError("projectTestsNotFound", "no test sources were configured or discovered under test/ or tests/");

    // Prove or create the selected publication once before starting workers.
    // Workers validate it again immediately before activation, closing the
    // selection/use boundary without sharing any mutable image state.
    var artifact_result = if (options.build)
        project_build.build(api, a, resolution, build_opts.version, options.profile)
    else
        project_build.validate(api, a, resolution, build_opts.version, options.profile);
    defer artifact_result.deinit();
    if (artifact_result.diagnostic) |diagnostic|
        testDiagnosticError(diagnostic.kind, diagnostic.message);

    const self_path = try std.fs.selfExePathAlloc(a);
    defer a.free(self_path);
    var failures: usize = 0;
    for (resolution.test_sources) |test_path| {
        out("== ");
        out(test_path);
        out(" ==\n");
        const child = std.process.Child.run(.{
            .allocator = a,
            .argv = &.{ self_path, "--project-test-worker", resolution.project_root, options.profile, test_path },
            .max_output_bytes = 16 << 20,
        }) catch |err| {
            std.debug.print("birnam test: could not start {s}: {s}\n", .{ test_path, @errorName(err) });
            failures += 1;
            continue;
        };
        defer a.free(child.stdout);
        defer a.free(child.stderr);
        if (child.stdout.len != 0) std.fs.File.stdout().writeAll(child.stdout) catch {};
        if (child.stderr.len != 0) std.fs.File.stderr().writeAll(child.stderr) catch {};
        const passed = switch (child.term) {
            .Exited => |code| code == 0,
            else => false,
        };
        if (passed) {
            out("ok   ");
            out(test_path);
            out("\n");
        } else {
            out("FAIL ");
            out(test_path);
            out("\n");
            failures += 1;
        }
    }
    std.debug.print("birnam test: {d} source{s}, {d} failure{s}\n", .{
        resolution.test_sources.len,
        if (resolution.test_sources.len == 1) "" else "s",
        failures,
        if (failures == 1) "" else "s",
    });
    if (failures != 0) std.process.exit(1);
}

// The subcommands whose handlers live in birnam (modules/cli.birnam).
const cli_subcommands = [_][]const u8{ "--listen", "--serve", "--lsp", "doc", "docgen", "init", "wrap" };

/// A token `runCli` handles — a subcommand implemented in birnam. Recognized
/// here, then handed over whole through `[Cli run:source:rust:guides:]`.
/// Cli drives the same public module API any birnam program can call for itself —
/// `[Doc cli:]`, `[Doc buildSite:zig:rust:guides:]`, `[Lsp serveStdio]`,
/// `[Repl serveOn:]` — so the CLI is one more client of the libraries, not their
/// owner; only the boot + GC-anchored driver frame stays Zig.
fn isCliSubcommand(tok: []const u8) bool {
    for (cli_subcommands) |c| if (std.mem.eql(u8, tok, c)) return true;
    return false;
}

fn cliNeedsProjectPolicy(argv: []const []const u8) bool {
    for (argv) |arg| if (std.mem.eql(u8, arg, "--lsp")) return true;
    return false;
}

/// Run one Birnam-side subcommand: boot the image, load the compiled dispatcher,
/// and hand it argv plus native-source/guide resource paths.
/// 0 is success (usage displays included), anything else exits 1. A leftover
/// `[Codegen lastError]` (a module that failed to PARSE during dispatch) is
/// reported on stderr, so `--lsp`'s stdout stays protocol-only.
fn runCli(a: std.mem.Allocator, api: *IdApi, argv: []const []const u8) !void {
    try loadStack(a, api);
    try installProjectBridge(api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);

    loadStdlib(a, api, codegen);
    if (cliNeedsProjectPolicy(argv)) loadProjectPolicy(a, api, codegen);

    try loadRuntimeModule(a, api, "cli");
    const cli = birnamGlobal(api, "Cli") orelse return error.NoCli;
    const args_value = nativeStringVector(api, argv) orelse return error.NoVector;
    var resources = try runtime_resources.resolve(a);
    defer resources.deinit();
    const res = api.send(cli, api.intern("run:source:rust:guides:"), &[_]Oop{
        args_value,
        api.mkString(resources.source_dir.ptr, resources.source_dir.len),
        api.mkString(resources.rust_source_dir.ptr, resources.rust_source_dir.len),
        api.mkString(resources.guide_dir.ptr, resources.guide_dir.len),
    }, 4);
    const ok = api.send(res, api.intern("="), &[_]Oop{api.mkSmallInt(0)}, 1);
    if (ok == api.trueObj) return;
    const err = api.send(codegen, api.intern("lastError"), &[_]Oop{}, 0);
    if (err != api.nilObj) {
        std.fs.File.stderr().writeAll("birnam: subcommand failed:\n") catch {};
        outErrorString(api, err);
    }
    std.process.exit(1);
}

fn runtimeResourceUnavailable(kind: []const u8, path: []const u8, err: anyerror) noreturn {
    var buffer: [1024]u8 = undefined;
    var stderr = std.fs.File.stderr().writer(&buffer);
    stderr.interface.print("birnam: required {s} is unavailable: {s} ({s})\n", .{ kind, path, @errorName(err) }) catch {};
    stderr.interface.flush() catch {};
    std.process.exit(1);
}

fn writeStringOop(api: *IdApi, value: Oop) bool {
    if (value == null or api.vtableOf(value) != api.string_vt) return false;
    var len: usize = 0;
    const p = api.stringBytes(value, &len);
    std.fs.File.stderr().writeAll(p[0..len]) catch {};
    return true;
}

/// Print a compiler/runtime error without interpreting arbitrary object payload
/// bytes as a String. `[Codegen lastError]` is usually a String, while runtime
/// module and Alien failures are ordinary Error objects.
fn outErrorString(api: *IdApi, s: Oop) void {
    if (writeStringOop(api, s)) {
        std.fs.File.stderr().writeAll("\n") catch {};
        return;
    }

    if (s != null) {
        const rendered = api.send(s, api.intern("printString"), &.{}, 0);
        if (rendered != s and writeStringOop(api, rendered)) {
            std.fs.File.stderr().writeAll("\n") catch {};
            return;
        }
        if (apiErrorValue(api, s)) {
            const stderr = std.fs.File.stderr();
            stderr.writeAll("error(") catch {};
            const kind = api.send(s, api.intern("kind"), &.{}, 0);
            if (kind != null and api.vtableOf(kind) == api.symbol_vt)
                stderr.writeAll(std.mem.span(api.symbolName(kind))) catch {};
            const message = api.send(s, api.intern("message"), &.{}, 0);
            if (message != null and api.vtableOf(message) == api.string_vt) {
                stderr.writeAll(": ") catch {};
                _ = writeStringOop(api, message);
            }
            stderr.writeAll(")\n") catch {};
            return;
        }
    }
    std.fs.File.stderr().writeAll("birnam: non-string error value\n") catch {};
}

/// `birnam --compile-module <src.birnam> <out.dylib>` — AOT-compile a birnam-source
/// module to an INSTALLABLE dylib (the stage-1 build path): loading it later with
/// `[Codegen load:]` installs its methods with no recompile (artifact-mode
/// `defineMethod` lowering). Brings up the language stack but NOT the stdlib —
/// compiling `defineMethod` bodies needs only the front end (a body's sends are
/// late-bound, so the target methods need not exist yet), which is exactly what
/// lets the stdlib compile itself. Exits non-zero on a compile failure so the build
/// step fails loudly instead of emitting a broken artifact.
fn runCompileModule(a: std.mem.Allocator, api: *IdApi, src_path: []const u8, out_path: []const u8) !void {
    try loadStack(a, api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    const src = try std.fs.cwd().readFileAlloc(a, src_path, 16 << 20);
    defer a.free(src);
    const r = api.send(codegen, api.intern("compile:output:from:"), &[_]Oop{
        api.mkString(src.ptr, src.len), api.mkString(out_path.ptr, out_path.len), api.mkString(src_path.ptr, src_path.len),
    }, 3);
    if (r == api.nilObj) {
        out("birnam: could not compile module ");
        out(src_path);
        out(":\n");
        const err = api.send(codegen, api.intern("lastError"), &[_]Oop{}, 0);
        if (err != api.nilObj) outErrorString(api, err);
        std.process.exit(1);
    }
    std.debug.print("[stage1] compiled {s} -> {s}\n", .{ src_path, out_path });
}

/// Boot an image: `birnam foo.image` (docs/40). Bring up the language stack
/// (the native module vtables the image relinks against), map the image back in,
/// then honor the entry-point convention — if the global `main` is bound, send it
/// `value`; otherwise open the REPL with every restored binding live (docs/40 §1).
///
/// By default the image's COMPILED code is MAPPED (its methods installed with no
/// recompile). `[System loadImage:]` answers `true` when it did so; `false` for a
/// data-only image or an arch/ABI mismatch (or `--rebuild`), where we fall back to
/// re-evaluating the library from source (here: the stdlib; user-method replay is
/// slice 4's next step).
fn recoverImageRuntime(a: std.mem.Allocator, api: *IdApi, path: []const u8, rebuild: bool) !void {
    try loadStack(a, api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    // Heap facts may refer to lifecycle instances through the stable
    // CompiledUnitPrototype/CompiledUnitVt exports. Recreate that facade before
    // image decoding; mapped/replayed image code will refresh its methods later.
    // The module manager is the only prerequisite needed at this point: method
    // bodies can refer to stdlib protocol before that protocol is installed.
    loadSelfHosted(a, api, codegen, "modules", MODULES_SRC);
    // Restore the stdlib protocol before mapped image code evaluates ordinary
    // source documents. Those documents may use the documentation hooks and
    // other derived sends even though their compiled methods are carried by the
    // image itself.
    loadSelfHosted(a, api, codegen, "stdlib", STDLIB_SRC);
    loadSelfHosted(a, api, codegen, "compiledUnit", COMPILED_UNIT_SRC);
    // Map the image. NOTE: stdlib is NOT loaded up front — a mapped `.code` section
    // installs the whole library (stdlib + user) with no recompile, which is the
    // point. We only re-eval the stdlib if the image had no mappable code.
    const system = api.import("System") orelse return error.NoSystem;
    const path_str = api.mkString(path.ptr, path.len);
    const rebuild_arg: Oop = if (rebuild) api.trueObj else api.falseObj;
    const status = api.send(system, api.intern("loadImage:rebuild:"), &[_]Oop{ path_str, rebuild_arg }, 2);
    if (status == api.nilObj) {
        out("[image] failed to load ");
        out(path);
        out("\n");
        return error.ImageLoadFailed;
    }
    // Code not mapped (data-only image, arch/ABI mismatch, or --rebuild): this is
    // explicit source recovery, not the ordinary image path. Rebuild the whole
    // library from the image's recorded module and method sources; if an older
    // image replays nothing, fall back to the embedded stdlib so the REPL remains
    // usable.
    if (status == api.falseObj) {
        std.debug.print(
            "[image] compiled units were not mapped ({s}); recovering from source\n",
            .{if (rebuild) "requested rebuild" else "unavailable or incompatible"},
        );
        const replayed = api.send(codegen, api.intern("replayMethods"), &[_]Oop{}, 0);
        if (api.smallIntValue(replayed) == 0) loadStdlib(a, api, codegen);
    }

    // Re-install the Alien surface: native wrap trampolines and dlopen'd libraries
    // can't ride the image, so each successful send was recorded as replayable
    // source (`__aliens__` facts) and is re-evaluated here — after code resolution,
    // so the `on:` targets (restored module singletons) are bound; before the
    // checkpoint, so replay's (deduped) asserts are never journaled.
    _ = api.send(codegen, api.intern("replayAliens"), &[_]Oop{}, 0);

    // Re-derive source-backed block globals that were intentionally not encoded as
    // durable values (docs/40 §6). Their recorded source runs over the now-resolved
    // compiled layer; the entry-point `main` closure lands through this path.
    _ = api.send(codegen, api.intern("deriveImageGlobals"), &[_]Oop{}, 0);

    // The Module manager rode the image (it is birnam, docs/44 Phase 5b); its
    // libDir cell holds the SAVING tree's path. Re-point it at this boot's
    // resolved dir (and re-assert the kernel tier — idempotent facts) so
    // `[Module load: #x]` resolves against the tree the image runs in. Before
    // the checkpoint, so the refresh is never journaled.
    if (birnamGlobal(api, "Module")) |mm| refreshModuleManager(a, api, mm);
}

fn runImage(a: std.mem.Allocator, api: *IdApi, path: []const u8, rebuild: bool) !void {
    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);

    try recoverImageRuntime(a, api, path, rebuild);

    // Entry-point convention: `main` bound → run it; else drop into the REPL.
    if (api.import("Globals")) |globals| {
        const m = api.send(globals, api.intern("at:"), &[_]Oop{api.intern("main")}, 1);
        if (m != api.nilObj) {
            _ = api.send(m, api.intern("value"), &[_]Oop{}, 0);
            return;
        }
    }
    return runRepl(a, api, true); // booted: stack + stdlib already up (mapped/replayed)
}

// -- the REPL: an interactive tool for exploring the running system ----------

fn out(s: []const u8) void {
    std.fs.File.stdout().writeAll(s) catch {};
}

/// The console REPL lives in birnam — modules/repl.birnam owns the form reader,
/// the `:commands`, config-main, and the eval/print loop (`[Repl console]`); only
/// the boot + GC-anchored driver frame stays Zig, the same split as `runCli`.
/// `booted` = entered from a mapped image (`runImage`), where the stack and stdlib
/// are ALREADY up — re-loading them would re-JIT the whole library and replace
/// the restored generation's dispatch. A standalone `--repl` passes false to do
/// the one-time init itself.
fn runRepl(a: std.mem.Allocator, api: *IdApi, booted: bool) !void {
    return runReplSession(a, api, booted, false);
}

fn runReplSession(a: std.mem.Allocator, api: *IdApi, booted: bool, project_run: bool) !void {
    // repl.birnam rides the stdlib's string protocol, so no-stdlib mode has no
    // interactive loop — `birnam <file>` remains for bare-floor debugging.
    if (std.posix.getenv("BIRNAM_NO_STDLIB") != null) {
        std.fs.File.stderr().writeAll("birnam: the REPL requires the stdlib (unset BIRNAM_NO_STDLIB)\n") catch {};
        std.process.exit(1);
    }
    if (!booted) try loadStack(a, api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);

    if (!booted) {
        loadStdlib(a, api, codegen);
        // Load the runtime tool before a configured project's native-library
        // cutover. Project artifacts are consumed from their build record;
        // Module.libDir remains the Birnam runtime root.
        try loadRuntimeModule(a, api, "repl");
    }
    if (!booted) {
        configureRuntimeNativeDir(a, api);
        try prepareBareProject(a, api, codegen);
    }

    const globals = api.import("Globals") orelse return error.NoGlobals;
    var repl = api.send(globals, api.intern("at:"), &[_]Oop{api.intern("Repl")}, 1);
    // Not already in the image: load the compiled module before the session so
    // a load failure cannot be confused with an in-session compile error.
    if (repl == api.nilObj) {
        if (booted)
            try loadHostRuntimeModule(a, api, "repl")
        else
            try loadRuntimeModule(a, api, "repl");

        repl = api.send(globals, api.intern("at:"), &[_]Oop{api.intern("Repl")}, 1);
        if (repl == api.nilObj) {
            std.fs.File.stderr().writeAll("birnam: could not load the REPL compiled unit:\n") catch {};
            const err = api.send(codegen, api.intern("lastError"), &[_]Oop{}, 0);
            if (err != api.nilObj) outErrorString(api, err);
            std.process.exit(1);
        }
    }
    const initialized = api.send(repl, api.intern(if (project_run) "consoleHostInitNoConfig" else "consoleHostInit"), &[_]Oop{}, 0);
    if (api.send(initialized, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj) {
        outErrorString(api, initialized);
        return;
    }
    const generations = birnamGlobal(api, "DispatchGeneration") orelse return error.NoDispatchGeneration;
    const generation = api.send(generations, api.intern("initializeCurrent"), &[_]Oop{}, 0);
    if (api.send(generation, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj) {
        outErrorString(api, generation);
        return;
    }
    try resolveHostPhaseFunctions(a, api);
    if (host_enter_running.?() == 0) return error.InvalidRuntimePhase;
    const ready = api.send(repl, api.intern(if (project_run) "consoleHostReadyNoMain" else "consoleHostReady"), &[_]Oop{}, 0);
    if (api.send(ready, api.intern("isError"), &[_]Oop{}, 0) == api.trueObj) {
        outErrorString(api, ready);
        _ = api.send(repl, api.intern("consoleDone"), &[_]Oop{}, 0);
        return;
    }

    const intents = birnamGlobal(api, "EvolutionIntent") orelse return error.NoEvolutionIntent;
    while (true) {
        if (host_isolate_drain) |drain| drain();
        const step = api.send(repl, api.intern("hostStep"), &[_]Oop{}, 0);
        const tag = api.send(step, api.intern("at:"), &[_]Oop{api.mkSmallInt(0)}, 1);
        if (tag == api.intern("done")) break;
        if (tag != api.intern("evaluated")) continue;
        var value = api.send(step, api.intern("at:"), &[_]Oop{api.mkSmallInt(1)}, 1);
        if (api.send(intents, api.intern("isIntent:"), &[_]Oop{value}, 1) == api.trueObj) {
            if (host_begin_quiescence.?() == 0) return error.InvalidRuntimePhase;
            value = api.send(value, api.intern("fulfill"), &[_]Oop{}, 0);
            if (host_end_quiescence.?() == 0) return error.InvalidRuntimePhase;
        }
        _ = api.send(repl, api.intern("printHostResult:"), &[_]Oop{value}, 1);
        if (host_isolate_drain) |drain| drain();
    }
    _ = api.send(repl, api.intern("consoleDone"), &[_]Oop{}, 0);
}

const RTLD_LAZY: c_int = 1;

const HostPhaseFn = *const fn () callconv(.c) c_int;
const IsolateDriverFn = *const fn (*IdApi, c_int, Oop, Oop) callconv(.c) c_int;
const IsolateBootErrorFn = *const fn (*IdApi, c_int, [*:0]const u8, [*]const u8, usize) callconv(.c) void;
const IsolateDrainFn = *const fn () callconv(.c) void;
const IsolateShutdownFn = *const fn () callconv(.c) void;
var host_enter_running: ?HostPhaseFn = null;
var host_begin_quiescence: ?HostPhaseFn = null;
var host_end_quiescence: ?HostPhaseFn = null;
var host_isolate_driver: ?IsolateDriverFn = null;
var host_isolate_boot_error: ?IsolateBootErrorFn = null;
var host_isolate_drain: ?IsolateDrainFn = null;
var host_isolate_shutdown: ?IsolateShutdownFn = null;

fn resolveHostPhaseFunctions(a: std.mem.Allocator, api: *IdApi) !void {
    if (host_enter_running != null) return;
    const dir = try resolveLibDir(a);
    defer a.free(dir);
    const path = try std.fs.path.join(a, &.{ dir, "libbirnam_id_codegen.dylib" });
    defer a.free(path);
    const path_z = try a.dupeZ(u8, path);
    defer a.free(path_z);
    const handle = api.dlopen(path_z.ptr, RTLD_LAZY) orelse return error.NoCodegen;
    host_enter_running = @ptrCast(@alignCast(api.dlsym(handle, "birnam_codegen_enter_running") orelse return error.NoCodegen));
    host_begin_quiescence = @ptrCast(@alignCast(api.dlsym(handle, "birnam_codegen_begin_quiescence") orelse return error.NoCodegen));
    host_end_quiescence = @ptrCast(@alignCast(api.dlsym(handle, "birnam_codegen_end_quiescence") orelse return error.NoCodegen));
}

fn resolveIsolateFunctions(a: std.mem.Allocator, api: *IdApi) !void {
    if (host_isolate_driver != null) return;
    const dir = try resolveLibDir(a);
    defer a.free(dir);
    const path = try std.fs.path.join(a, &.{ dir, "libbirnam_id_isolate.dylib" });
    defer a.free(path);
    const path_z = try a.dupeZ(u8, path);
    defer a.free(path_z);
    const handle = api.dlopen(path_z.ptr, RTLD_LAZY) orelse return error.NoIsolate;
    host_isolate_driver = @ptrCast(@alignCast(api.dlsym(handle, "birnam_isolate_worker_driver") orelse return error.NoIsolate));
    host_isolate_boot_error = @ptrCast(@alignCast(api.dlsym(handle, "birnam_isolate_worker_boot_error") orelse return error.NoIsolate));
    host_isolate_drain = @ptrCast(@alignCast(api.dlsym(handle, "birnam_isolate_drain") orelse return error.NoIsolate));
    host_isolate_shutdown = @ptrCast(@alignCast(api.dlsym(handle, "birnam_isolate_shutdown") orelse return error.NoIsolate));
}

fn workerFail(api: *IdApi, fd: c_int, kind: [*:0]const u8, message: []const u8) noreturn {
    if (host_isolate_boot_error) |send_error| send_error(api, fd, kind, message.ptr, message.len);
    std.posix.close(fd);
    std.process.exit(1);
}

fn receiverHasSelector(api: *IdApi, receiver: Oop, selector: Oop) bool {
    var vt = api.vtableOf(receiver);
    while (vt != null) : (vt = api.vtableParent(vt)) {
        var i: usize = 0;
        while (api.vtableSelectorAt(vt, i)) |candidate| : (i += 1)
            if (candidate == selector) return true;
    }
    return false;
}

fn selectorHasArityOne(selector: []const u8) bool {
    const colons = std.mem.count(u8, selector, ":");
    if (colons != 0) return colons == 1;
    return selector.len != 0 and !std.ascii.isAlphabetic(selector[0]);
}

fn runWorker(a: std.mem.Allocator, api: *IdApi, image: []const u8, receiver_name: []const u8, selector_name: []const u8, fd: c_int) !void {
    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);

    recoverImageRuntime(a, api, image, false) catch |e| workerFail(api, fd, "imageRecoveryFailed", @errorName(e));
    const generations = birnamGlobal(api, "DispatchGeneration") orelse workerFail(api, fd, "noDispatchGeneration", "image has no DispatchGeneration");
    const current = api.send(generations, api.intern("current"), &.{}, 0);
    if (current == api.nilObj) workerFail(api, fd, "noCurrentGeneration", "image has no current generation");
    if (api.send(current, api.intern("isError"), &.{}, 0) == api.trueObj)
        workerFail(api, fd, "invalidCurrentGeneration", "current generation is invalid");
    const codegen = api.import("Codegen") orelse workerFail(api, fd, "noCodegen", "image has no Codegen");
    const digest = api.send(codegen, api.intern("currentGenerationDigest"), &.{}, 0);
    var digest_len: usize = 0;
    if (digest == api.nilObj or api.vtableOf(digest) != api.string_vt or api.stringBytes(digest, &digest_len)[0..digest_len].len != 64)
        workerFail(api, fd, "invalidCurrentGeneration", "current generation digest is invalid");
    if (!selectorHasArityOne(selector_name))
        workerFail(api, fd, "wrongArity", "worker selector must have arity one");
    const receiver_z = a.dupeZ(u8, receiver_name) catch workerFail(api, fd, "outOfMemory", "could not copy receiver name");
    defer a.free(receiver_z);
    const selector_z = a.dupeZ(u8, selector_name) catch workerFail(api, fd, "outOfMemory", "could not copy selector name");
    defer a.free(selector_z);
    const globals = api.import("Globals") orelse workerFail(api, fd, "noGlobals", "image has no Globals");
    var receiver = api.send(globals, api.intern("at:"), &[_]Oop{api.intern(receiver_z.ptr)}, 1);
    if (receiver == api.nilObj) receiver = api.import(receiver_z.ptr);
    if (receiver == api.nilObj) workerFail(api, fd, "receiverNotFound", "named worker receiver was not found");
    const selector = api.intern(selector_z.ptr);
    if (!receiverHasSelector(api, receiver, selector))
        workerFail(api, fd, "selectorNotFound", "worker receiver does not implement the selector");

    resolveHostPhaseFunctions(a, api) catch workerFail(api, fd, "noPhaseHooks", "could not resolve runtime phase hooks");
    if (host_enter_running.?() == 0) workerFail(api, fd, "invalidRuntimePhase", "could not enter running phase");
    const status = host_isolate_driver.?(api, fd, receiver, selector);
    std.posix.close(fd);
    if (status != 0) std.process.exit(1);
}

/// Load (or reload) a module THROUGH the id api's own `dlopen`/`dlsym`, threading
/// the opaque per-module `state` so arbitrary state survives a hot reload.
fn loadModule(api: *IdApi, path: [*:0]const u8, state: ?*anyopaque) !?*anyopaque {
    const h = api.dlopen(path, RTLD_LAZY) orelse return error.LoadFailed;
    const raw = api.dlsym(h, "module_init") orelse return error.NoModuleInit;
    const init: idapi.ModuleInitFn = @ptrCast(@alignCast(raw));
    return init(api, state);
}

// The form reader (bracket balance incl. string/comment rules) lives in
// modules/repl.birnam now; its vectors are locked in modules/cli-test.birnam and
// the console transcripts in scripts/repl-golden.sh.

test "bake: the exe trailer round-trips an image slice; plain bytes have none" {
    const expect = std.testing.expect;
    const a = std.testing.allocator;

    // Assemble a synthetic baked exe: [runtime bytes][image][u64 len][magic].
    const runtime = "MZ...a pretend runtime binary...";
    const image = "BIRNAMIM\x01\x00\x00\x00...pretend image payload...";
    var buf: std.ArrayList(u8) = .{};
    defer buf.deinit(a);
    try buf.appendSlice(a, runtime);
    try buf.appendSlice(a, image);
    var lb: [8]u8 = undefined;
    std.mem.writeInt(u64, &lb, image.len, .little);
    try buf.appendSlice(a, &lb);
    try buf.appendSlice(a, BAKE_MAGIC);

    // The trailer recovers exactly the image slice.
    const got = bakeTrailerImage(buf.items) orelse return error.NoTrailer;
    try std.testing.expectEqualStrings(image, got);

    // A plain binary (no trailer), an empty file, and a corrupt length → null.
    try expect(bakeTrailerImage(runtime) == null);
    try expect(bakeTrailerImage("") == null);
    var bad = try a.dupe(u8, buf.items);
    defer a.free(bad);
    std.mem.writeInt(u64, bad[bad.len - BAKE_TRAILER ..][0..8], 1 << 30, .little); // len past EOF
    try expect(bakeTrailerImage(bad) == null);
}
