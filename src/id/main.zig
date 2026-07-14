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
const idapi = @import("idapi.zig");
const build_opts = @import("build_opts");

const USAGE =
    \\birnam — a Piumarta object system that compiles to native (docs/30).
    \\
    \\Usage:
    \\  birnam                     Start the interactive REPL (the default).
    \\  birnam <file.birnam>       Compile and run a script, then exit.
    \\  birnam <file.image>        Boot a saved image (maps compiled code; runs
    \\                             `main`, else drops into the REPL).
    \\  birnam --repl              Start the REPL explicitly (aliases: -i, repl).
    \\  birnam --listen <port> [file]
    \\                             Start a socket REPL on <port> (alias: --serve).
    \\                             If FILE is given it is run first, so the listener
    \\                             attaches to its state. Run from the repo root so
    \\                             modules/ is visible. Blocks; Ctrl-C to stop.
    \\  birnam --lsp               Run the birnam language server on stdio (LSP).
    \\                             Point lsp-mode / eglot at this. Resolves the
    \\                             modules/ dir relative to the executable, so it
    \\                             works launched from any project directory.
    \\  birnam doc <mod> [sel]     Print documentation for a module (like `go doc`):
    \\                             its method docstrings, or one selector's doc.
    \\  birnam docgen [out.html]   Generate the full HTML doc reference (every birnam
    \\                             module + every Zig floor file) to a file.
    \\  birnam init                Write a starter birnam.toml (the recognised keys,
    \\                             commented) into the current directory. Refuses to
    \\                             overwrite an existing one.
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
    \\                    default search (next to the exe, then ../lib, then
    \\                    zig-out/lib).
    \\  BIRNAM_ID_DYLIB   Path to the id-floor dylib (libbirnam_id.dylib).
    \\  BIRNAM_NO_STDLIB  If set, skip the self-hosted stdlib (bare Zig floor).
    \\  BIRNAM_GC_PRECISE If set, use the precise (layout-descriptor) GC.
    \\
;

const IdApi = idapi.IdApi;
const Oop = idapi.Oop;

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
    const id_path = if (std.posix.getenv("BIRNAM_ID_DYLIB")) |d|
        try a.dupe(u8, d)
    else blk: {
        const dir = try resolveLibDir(a);
        defer a.free(dir);
        break :blk try std.fs.path.join(a, &.{ dir, "libbirnam_id.dylib" });
    };
    defer a.free(id_path);

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
    var lib = try std.DynLib.open(id_path);
    const id_init = lib.lookup(idapi.IdInitFn, "id_init") orelse return error.NoIdInit;
    const api = id_init();
    defer if (host_isolate_shutdown) |shutdown| shutdown();

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
        // whole invocation to `[Cli run:dir:]` (modules/cli.birnam) — usage
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
        return runScript(a, api, sp);
    }

    // Bare invocation drops into the REPL — the documented default.
    return runRepl(a, api, false);
}

/// The language stack a script needs, in dependency order. Loaded quietly
/// through the id api; codegen resolves globals through the store, so the store
/// comes first.
const script_modules = [_][]const u8{
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
// Zig primitives. Each source is embedded so it is ALWAYS available as the boot
// floor; the precompiled dylib (stage-1 build) is preferred when present. Loaded
// by default, skipped whole with BIRNAM_NO_STDLIB=1 (the bare Zig floor as an
// option). `toml` follows `stdlib` — `Toml parse:` calls stdlib methods
// (`collect:`/`asInteger`) at RUN time, so stdlib must be up before Toml is used.
const STDLIB_SRC = @embedFile("stdlib.birnam");
const TOML_SRC = @embedFile("toml.birnam");
const CONFIG_SRC = @embedFile("config.birnam");
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
    .{ .name = "isolate", .src = ISOLATE_SRC },
};

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
            return;
        }
    }
    // Fallback: the declared table order (also correct — it's dependency-ordered).
    for (self_hosted_modules) |sh| {
        loadSelfHosted(a, api, codegen, sh.name, sh.src);
        markLoaded(api, sh.name);
    }
}

/// Resolve the directory holding the runtime module dylibs. Tries, in order:
/// `$BIRNAM_LIB_DIR`, then next to this executable (`<exedir>` and `<exedir>/../lib`
/// — so a baked exe placed beside its dylibs just works), then `zig-out/lib` (the
/// dev tree). The first directory that actually holds `libbirnam_store.dylib` wins;
/// falls back to `zig-out/lib`. Caller frees the returned slice.
fn resolveLibDir(a: std.mem.Allocator) ![]u8 {
    if (std.posix.getenv("BIRNAM_LIB_DIR")) |d| return a.dupe(u8, d);
    const probe = "libbirnam_store.dylib";
    if (std.fs.selfExeDirPathAlloc(a)) |exedir| {
        defer a.free(exedir);
        const parent_lib = try std.fs.path.join(a, &.{ exedir, "..", "lib" });
        defer a.free(parent_lib);
        for ([_][]const u8{ exedir, parent_lib }) |cand| {
            const p = try std.fs.path.join(a, &.{ cand, probe });
            defer a.free(p);
            if (std.fs.cwd().access(p, .{})) |_| {
                return a.dupe(u8, cand); // cand is exedir/parent_lib, both freed after we dupe
            } else |_| {}
        }
    } else |_| {}
    return a.dupe(u8, "zig-out/lib");
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
        "store",      "peg",   "lang",       "tcc",   "codegen", "prelude",
        "collection", "error", "relational", "alien", "image",
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
    _ = api.send(codegen, api.intern("evalPersisting:"), &[_]Oop{api.mkString(src.ptr, src.len)}, 1);

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

// The subcommands whose handlers live in birnam (modules/cli.birnam).
const cli_subcommands = [_][]const u8{ "--listen", "--serve", "--lsp", "doc", "docgen", "init", "wrap" };

/// A token `runCli` handles — a subcommand implemented in birnam. Recognized
/// here, then handed over whole: `[Cli run: #["doc" "time"] dir: "<modules>"]`.
/// Cli drives the same public module API any birnam program can call for itself —
/// `[Doc cli:]`, `[Doc buildSite:modules:zig:]`, `[Lsp serveStdio]`,
/// `[Repl serveOn:]` — so the CLI is one more client of the libraries, not their
/// owner; only the boot + GC-anchored driver frame stays Zig.
fn isCliSubcommand(tok: []const u8) bool {
    for (cli_subcommands) |c| if (std.mem.eql(u8, tok, c)) return true;
    return false;
}

/// Append S to BUF as a birnam string literal (`\` escapes `"` and `\`), so an
/// arbitrary path or CLI argument survives the trip through boot source intact.
fn appendBirnamString(buf: *std.ArrayList(u8), a: std.mem.Allocator, s: []const u8) !void {
    try buf.append(a, '"');
    for (s) |c| {
        if (c == '"' or c == '\\') try buf.append(a, '\\');
        try buf.append(a, c);
    }
    try buf.append(a, '"');
}

/// Run one birnam-side subcommand: boot the image, resolve modules/, load the
/// dispatcher, and hand it argv. `[Cli run:dir:]` answers the process status —
/// 0 is success (usage displays included), anything else exits 1. A leftover
/// `[Codegen lastError]` (a module that failed to PARSE during dispatch) is
/// reported on stderr, so `--lsp`'s stdout stays protocol-only.
fn runCli(a: std.mem.Allocator, api: *IdApi, argv: []const []const u8) !void {
    try loadStack(a, api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);

    loadStdlib(a, api, codegen);

    const mdir = try resolveModulesDir(a);
    defer a.free(mdir);
    // Export the resolved modules dir so a loaded module's relative dep-loads
    // ("modules/x.birnam") resolve from this (possibly foreign) working directory.
    const mdirZ = try a.dupeZ(u8, mdir);
    defer a.free(mdirZ);
    _ = setenv("BIRNAM_MODULES_DIR", mdirZ.ptr, 1);

    // One boot form: load the dispatcher, hand it argv + the modules dir.
    var boot: std.ArrayList(u8) = .{};
    defer boot.deinit(a);
    const cli_path = try std.fmt.allocPrint(a, "{s}/cli.birnam", .{mdir});
    defer a.free(cli_path);
    try boot.appendSlice(a, "[System load: ");
    try appendBirnamString(&boot, a, cli_path);
    try boot.appendSlice(a, "]; [Cli run: #[");
    for (argv) |arg| {
        try appendBirnamString(&boot, a, arg);
        try boot.append(a, ' ');
    }
    try boot.appendSlice(a, "] dir: ");
    try appendBirnamString(&boot, a, mdir);
    try boot.appendSlice(a, "]");

    const res = api.send(codegen, api.intern("evalPersisting:"), &[_]Oop{api.mkString(boot.items.ptr, boot.items.len)}, 1);
    const ok = api.send(res, api.intern("="), &[_]Oop{api.mkSmallInt(0)}, 1);
    if (ok == api.trueObj) return;
    const err = api.send(codegen, api.intern("lastError"), &[_]Oop{}, 0);
    if (err != api.nilObj) {
        std.fs.File.stderr().writeAll("birnam: subcommand failed:\n") catch {};
        outErrorString(api, err);
    }
    std.process.exit(1);
}

// libc setenv (we link libc) — set $BIRNAM_MODULES_DIR for child module loads.
extern "c" fn setenv(name: [*:0]const u8, value: [*:0]const u8, overwrite: c_int) c_int;

/// Locate the source `modules/` directory independently of the current working
/// directory, so `--lsp` (launched by an editor from an arbitrary project) still
/// finds json.birnam/lsp.birnam. Probes `$BIRNAM_MODULES_DIR`, then candidates
/// relative to the executable (installed layout: <prefix>/share/birnam/modules;
/// dev layout: <repo>/zig-out/bin/birnam → <repo>/modules), then falls back to
/// `./modules`.
fn resolveModulesDir(a: std.mem.Allocator) ![]u8 {
    if (std.posix.getenv("BIRNAM_MODULES_DIR")) |d| return a.dupe(u8, d);
    const probe = "json.birnam";
    if (std.fs.selfExeDirPathAlloc(a)) |exedir| {
        defer a.free(exedir);
        const c1 = try std.fs.path.join(a, &.{ exedir, "..", "..", "modules" });
        defer a.free(c1);
        const c2 = try std.fs.path.join(a, &.{ exedir, "..", "modules" });
        defer a.free(c2);
        const c3 = try std.fs.path.join(a, &.{ exedir, "modules" });
        defer a.free(c3);
        const installed = try std.fs.path.join(a, &.{ exedir, "..", "share", "birnam", "modules" });
        defer a.free(installed);
        for ([_][]const u8{ installed, c1, c2, c3 }) |cand| {
            const p = try std.fs.path.join(a, &.{ cand, probe });
            defer a.free(p);
            if (std.fs.cwd().access(p, .{})) |_| return a.dupe(u8, cand) else |_| {}
        }
    } else |_| {}
    return a.dupe(u8, "modules");
}

/// Print a `[Codegen lastError]` String (a `line:col: message` block) to stderr.
fn outErrorString(api: *IdApi, s: Oop) void {
    var len: usize = 0;
    const p = api.stringBytes(s, &len);
    std.fs.File.stderr().writeAll(p[0..len]) catch {};
    std.fs.File.stderr().writeAll("\n") catch {};
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
    const r = api.send(codegen, api.intern("compile:output:"), &[_]Oop{
        api.mkString(src.ptr, src.len), api.mkString(out_path.ptr, out_path.len),
    }, 2);
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

    if (!booted) loadStdlib(a, api, codegen);

    const globals = api.import("Globals") orelse return error.NoGlobals;
    var repl = api.send(globals, api.intern("at:"), &[_]Oop{api.intern("Repl")}, 1);
    // Not already in the image: load the module (checked BEFORE the session so a
    // load failure can't be confused with a user's stray in-session lastError).
    if (repl == api.nilObj) {
        const mdir = try resolveModulesDir(a);
        defer a.free(mdir);
        const mdirZ = try a.dupeZ(u8, mdir);
        defer a.free(mdirZ);
        _ = setenv("BIRNAM_MODULES_DIR", mdirZ.ptr, 1);

        var boot: std.ArrayList(u8) = .{};
        defer boot.deinit(a);
        try boot.appendSlice(a, "[System load: ");
        const repl_path = try std.fmt.allocPrint(a, "{s}/repl.birnam", .{mdir});
        defer a.free(repl_path);
        try appendBirnamString(&boot, a, repl_path);
        try boot.appendSlice(a, "]");
        _ = api.send(codegen, api.intern("evalPersisting:"), &[_]Oop{api.mkString(boot.items.ptr, boot.items.len)}, 1);

        repl = api.send(globals, api.intern("at:"), &[_]Oop{api.intern("Repl")}, 1);
        if (repl == api.nilObj) {
            std.fs.File.stderr().writeAll("birnam: could not load the REPL (modules/repl.birnam):\n") catch {};
            const err = api.send(codegen, api.intern("lastError"), &[_]Oop{}, 0);
            if (err != api.nilObj) outErrorString(api, err);
            std.process.exit(1);
        }
    }
    const initialized = api.send(repl, api.intern("consoleHostInit"), &[_]Oop{}, 0);
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
    const ready = api.send(repl, api.intern("consoleHostReady"), &[_]Oop{}, 0);
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
