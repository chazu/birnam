//! Shell entry point.
//!
//! Boots the kernel, builds the IdApi, manages loaded modules via the
//! Phase 5 module system. Each `birnam` invocation:
//!   1. Allocates FactStore + ObjectTable + Interner.
//!   2. Runs bootstrap.boot to wire kernel vtables, selectors, mcache,
//!      send/bind, and the rule registry.
//!   3. Populates IdApi from the kernel side.
//!   4. Loads each module path passed on the command line, in order, via
//!      the Module Manager (assigns each its own EID for `m`-column tagging).
//!   5. Enters a stdin line loop. Each line is handed to the dylib-registered
//!      REPL handler (if any) or echoed as "no repl handler registered."
//!
//! Multiple module args are supported. Order matters: a REPL module should
//! come after any module it intends to inspect, because the REPL handler
//! is registered at REPL init.

const std = @import("std");
const posix = std.posix;

const fact_store = @import("kernel/fact_store.zig");
const object_table = @import("kernel/object_table.zig");
const interner_mod = @import("kernel/interner.zig");
const arenas = @import("kernel/arenas.zig");
const bootstrap = @import("kernel/bootstrap.zig");
const id_api_mod = @import("kernel/id_api.zig");
const module = @import("kernel/module.zig");
const image = @import("kernel/image.zig");
const prune = @import("kernel/prune.zig");
const gc = @import("kernel/gc.zig");
const ffi_module = @import("kernel/ffi_module.zig");
const freeze = @import("kernel/freeze.zig");
const kernel_eids = @import("kernel/kernel_eids.zig");
const send_mod = @import("kernel/send.zig");
const strings = @import("kernel/strings.zig");
const ref_mod = @import("kernel/ref.zig");

// -- Image save/load commands ------------------------------------------------

/// Intercept `:save <path>` / `:load <path>` REPL commands. Returns true if
/// the line was an image command (handled here, not forwarded to a module).
/// The shell owns store/table/manager, so it is the natural home: it can
/// build the manifest from the loaded-module set and drive Approach-B
/// closure rematerialization on load.
fn handleImageCommand(
    line: []const u8,
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
    manager: *const module.Manager,
) bool {
    if (std.mem.startsWith(u8, line, ":save ")) {
        const path = std.mem.trim(u8, line[6..], " ");
        // Manifest = the currently-loaded module set (abi_hash placeholder
        // until a real ABI-hash mechanism lands; verification is a TODO).
        const deps = allocator.alloc(image.ModuleDep, manager.modules.items.len) catch {
            stderrPrint("[birnam] :save — out of memory\n", .{});
            return true;
        };
        defer allocator.free(deps);
        for (manager.modules.items, 0..) |m, i| deps[i] = .{ .name = m.name, .abi_hash = 0 };
        const manifest = image.Manifest{ .modules = deps };

        const res = image.saveToFile(allocator, path, store, table.next_eid.load(.monotonic), manifest, interner) catch {
            stderrPrint("[birnam] :save failed for {s}\n", .{path});
            return true;
        };
        allocator.free(res.bytes);
        stdoutPrint("[birnam] saved {s} ({d} facts, {d} modules)\n", .{ path, res.facts_written, deps.len });
        return true;
    }
    if (std.mem.eql(u8, line, ":prune")) {
        const stats = prune.pruneReachable(store, table, allocator) catch {
            stderrPrint("[birnam] :prune failed\n", .{});
            return true;
        };
        stdoutPrint("[birnam] pruned ({d} facts kept, {d} dropped, {d} entities swept)\n", .{ stats.kept, stats.dropped_facts, stats.swept_entities });
        return true;
    }
    if (std.mem.eql(u8, line, ":gc")) {
        // Heap reclaim (distinct from :prune's logical reclaim). Sweeps the
        // boxes arena. Best run after :prune so dropped facts no longer pin
        // their boxes (Phase 15 §Q2 current-values gate).
        const st = gc.collectBoxes(store, table, allocator);
        if (st.skipped) {
            stdoutPrint("[birnam] :gc skipped (no arena registry or FFI in flight)\n", .{});
        } else {
            stdoutPrint("[birnam] gc ({d} boxes freed/{d} kept, {d} closures freed/{d} kept, {d} wrappers freed/{d} kept)\n", .{ st.boxes_freed, st.boxes_kept, st.closures_freed, st.closures_kept, st.wrappers_freed, st.wrappers_kept });
        }
        return true;
    }
    if (std.mem.startsWith(u8, line, ":load ")) {
        const path = std.mem.trim(u8, line[6..], " ");
        const loaded = image.loadFromFile(allocator, path, store, interner) catch {
            stderrPrint("[birnam] :load failed for {s}\n", .{path});
            return true;
        };
        // Restore the eid allocator high-water mark, then rebuild fn-ptr
        // closures for loaded bytecode methods against this process's kernel.
        const cur = table.next_eid.load(.monotonic);
        if (loaded.next_eid > cur) table.next_eid.store(loaded.next_eid, .monotonic);
        const n = image.rematerializeClosures(allocator, store, table) catch 0;
        // Re-link FFI wraps by name in THIS process: re-dlopen each module +
        // re-dlsym its declarative wraps (persisted :ffi-addr would be a stale
        // pointer from the saving process).
        ffi_module.resolveAllPending(allocator, store, table) catch {};
        stdoutPrint("[birnam] loaded {s} ({d} facts, {d} methods rebuilt)\n", .{ path, loaded.facts_loaded, n });
        return true;
    }
    // `:in <path>` (atomic) / `:in! <path>` (continue-on-error). Both delegate
    // to the lang `language` prototype's fileIn: — the parser lives there. The
    // shell just drives the send (it links the kernel send path directly).
    if (std.mem.startsWith(u8, line, ":in! ") or std.mem.startsWith(u8, line, ":in ")) {
        const bang = line[3] == '!';
        const path = std.mem.trim(u8, line[(if (bang) @as(usize, 5) else 4)..], " ");
        const path_ref = strings.mkString(allocator, path) catch {
            stderrPrint("[birnam] :in — out of memory\n", .{});
            return true;
        };
        const proto = ref_mod.mkEid(kernel_eids.LANGUAGE_PROTO);
        if (bang) {
            const sel = interner.str_to_eid.get("fileIn:onError:") orelse {
                stderrPrint("[birnam] :in! unavailable (lang not loaded)\n", .{});
                return true;
            };
            const cont = interner.intern(store, table, "continue") catch return true;
            _ = send_mod.send2(proto, sel, path_ref, ref_mod.mkEid(cont));
        } else {
            const sel = interner.str_to_eid.get("fileIn:") orelse {
                stderrPrint("[birnam] :in unavailable (lang not loaded)\n", .{});
                return true;
            };
            _ = send_mod.send1(proto, sel, path_ref);
        }
        return true;
    }
    // `:freeze <outbase>` — emit a standalone wrapper module dylib that, when
    // loaded after peg + lang, re-establishes the image's FFI wraps. Writes
    // <outbase>.birnam + <outbase>.zig + birnam_module_abi.zig, then compiles.
    if (std.mem.startsWith(u8, line, ":freeze ")) {
        const outbase = std.mem.trim(u8, line[8..], " ");
        const dir_path = std.fs.path.dirname(outbase) orelse ".";
        const base = std.fs.path.basename(outbase);
        var dir = std.fs.cwd().openDir(dir_path, .{}) catch {
            stderrPrint("[birnam] :freeze — cannot open dir {s}\n", .{dir_path});
            return true;
        };
        defer dir.close();
        const stats = freeze.writeArtifacts(allocator, store, interner, dir, base) catch {
            stderrPrint("[birnam] :freeze failed\n", .{});
            return true;
        };
        stdoutPrint("[birnam] froze {d} module(s), {d} wrap(s) -> {s}.birnam + {s}.zig\n", .{ stats.modules, stats.wraps, base, base });
        const term = freeze.compile(allocator, dir_path, base) catch {
            stdoutPrint("[birnam] (compile skipped — run: zig build-lib {s}.zig -dynamic --name {s})\n", .{ base, base });
            return true;
        };
        if (term == .Exited and term.Exited == 0) {
            stdoutPrint("[birnam] compiled lib{s}.dylib\n", .{base});
        } else {
            stdoutPrint("[birnam] compile failed; artifacts written\n", .{});
        }
        return true;
    }
    return false;
}

// -- Shell callbacks ---------------------------------------------------------

fn stderrPrint(comptime fmt: []const u8, args: anytype) void {
    const w = std.fs.File.stderr().deprecatedWriter();
    w.print(fmt, args) catch {};
}

fn stdoutPrint(comptime fmt: []const u8, args: anytype) void {
    const w = std.fs.File.stdout().deprecatedWriter();
    w.print(fmt, args) catch {};
}

fn shellLog(msg: [*:0]const u8) callconv(.c) void {
    stderrPrint("[birnam] {s}\n", .{msg});
}

var repl_handler: ?*const fn ([*:0]const u8) callconv(.c) void = null;

fn shellSetReplHandler(handler: ?*const fn ([*:0]const u8) callconv(.c) void) callconv(.c) void {
    repl_handler = handler;
}

var shell_context = id_api_mod.ShellContext{
    .log = &shellLog,
    .set_repl_handler = &shellSetReplHandler,
};

// -- Signal handling (kept from Phase 0) ------------------------------------

var reload_requested: bool = false;

fn sighupHandler(_: c_int) callconv(.c) void {
    reload_requested = true;
}

fn installSignalHandler() void {
    const act = posix.Sigaction{
        .handler = .{ .handler = sighupHandler },
        .mask = 0,
        .flags = 0,
    };
    posix.sigaction(posix.SIG.HUP, &act, null);
}

// -- `birnam --aot` driver ---------------------------------------------------

/// Compile a user `.brnimg` into a standalone executable. Usage:
///   birnam --aot=<static|dynamic> [--strip=malleable] [--strip=history] \
///          <image.brnimg> [--entry=<selector>[:<recv>]] -o <out>
/// Mechanism (M1/M4): copy the image to a per-app embed path, run the
/// parameterized `zig build aot-build` with the strip/entry flags, then copy
/// the resulting binary to <out>. Must run from the repo root (needs build.zig
/// + src/ + a `zig` on PATH). Static modes only for now.
fn runAotDriver(gpa: std.mem.Allocator, args: [][:0]u8) !void {
    var arena = std.heap.ArenaAllocator.init(gpa);
    defer arena.deinit();
    const allocator = arena.allocator();
    var aot_mode: []const u8 = "static";
    var strip_malleable = false;
    var strip_history = false;
    var input: ?[]const u8 = null;
    var out: ?[]const u8 = null;
    var entry_sel: []const u8 = "";
    var entry_recv: i64 = 0;
    var entry_recv_name: []const u8 = "";

    var i: usize = 0;
    while (i < args.len) : (i += 1) {
        const a = args[i];
        if (std.mem.startsWith(u8, a, "--aot=")) {
            aot_mode = a["--aot=".len..];
        } else if (std.mem.eql(u8, a, "--strip=malleable")) {
            strip_malleable = true;
        } else if (std.mem.eql(u8, a, "--strip=history")) {
            strip_history = true;
        } else if (std.mem.startsWith(u8, a, "--entry=")) {
            const spec = a["--entry=".len..];
            if (std.mem.indexOfScalar(u8, spec, ':')) |c| {
                entry_sel = spec[0..c];
                const recv_spec = spec[c + 1 ..];
                // recv is either a bare integer (SmallInt) or an identifier
                // (resolved as a top-level binding in the baked image).
                if (std.fmt.parseInt(i64, recv_spec, 10)) |n| {
                    entry_recv = n;
                } else |_| {
                    entry_recv_name = recv_spec;
                }
            } else entry_sel = spec;
        } else if (std.mem.eql(u8, a, "-o")) {
            i += 1;
            if (i < args.len) out = args[i];
        } else if (!std.mem.startsWith(u8, a, "-")) {
            input = a;
        }
    }

    if (std.fs.cwd().access("src/aot_main.zig", .{})) |_| {} else |_| {
        stderrPrint("[birnam] --aot must run from the repo root (src/aot_main.zig not found)\n", .{});
        return;
    }
    const is_dynamic = std.mem.eql(u8, aot_mode, "dynamic");
    const img_path = input orelse {
        stderrPrint("[birnam] --aot: no input image (give a .brnimg path)\n", .{});
        return;
    };
    const out_path = out orelse {
        stderrPrint("[birnam] --aot: no output (-o <path>)\n", .{});
        return;
    };
    if (strip_history) stdoutPrint("[birnam] --strip=history: freezing store read-only (no tx-writer / history)\n", .{});

    // Stage the image at the per-app embed path (gitignored; not the fixture).
    const embed_rel = ".aot_app.brnimg";
    std.fs.cwd().copyFile(img_path, std.fs.cwd(), "src/" ++ embed_rel, .{}) catch {
        stderrPrint("[birnam] --aot: cannot read image {s}\n", .{img_path});
        return;
    };
    defer std.fs.cwd().deleteFile("src/" ++ embed_rel) catch {};

    // Build the parameterized AOT exe.
    var argv = std.ArrayList([]const u8){};
    defer argv.deinit(allocator);
    try argv.appendSlice(allocator, &.{ "zig", "build", "aot-build", "-Daot-embed=" ++ embed_rel, "-Daot-name=birnam-aot-app" });
    if (strip_malleable) try argv.appendSlice(allocator, &.{ "-Daot-strip-malleable=true", "-Daot-strip-ast=true" });
    if (is_dynamic) try argv.append(allocator, "-Daot-dynamic=true");
    if (strip_history) try argv.append(allocator, "-Daot-strip-history=true");
    if (entry_sel.len > 0) {
        try argv.append(allocator, try std.fmt.allocPrint(allocator, "-Daot-entry-selector={s}", .{entry_sel}));
        if (entry_recv_name.len > 0) {
            try argv.append(allocator, try std.fmt.allocPrint(allocator, "-Daot-entry-recv-name={s}", .{entry_recv_name}));
        } else {
            try argv.append(allocator, try std.fmt.allocPrint(allocator, "-Daot-entry-recv={d}", .{entry_recv}));
        }
    }

    stdoutPrint("[birnam] --aot={s}{s}: building {s} -> {s}\n", .{ if (is_dynamic) "dynamic" else "static", if (strip_malleable) " --strip=malleable" else "", img_path, out_path });
    var child = std.process.Child.init(argv.items, allocator);
    child.stdout_behavior = .Inherit;
    child.stderr_behavior = .Inherit;
    try child.spawn();
    const term = try child.wait();
    if (!(term == .Exited and term.Exited == 0)) {
        stderrPrint("[birnam] --aot: zig build failed\n", .{});
        return;
    }

    // Copy the built binary to the user's -o path (preserve exec bit).
    std.fs.cwd().copyFile("zig-out/bin/birnam-aot-app", std.fs.cwd(), out_path, .{ .override_mode = 0o755 }) catch {
        stderrPrint("[birnam] --aot: built ok but could not write {s}\n", .{out_path});
        return;
    };
    stdoutPrint("[birnam] --aot: wrote {s}\n", .{out_path});

    // Dynamic mode: stage the module dylibs beside the binary so the bundle is
    // portable (the exe resolves them exe-relative + via $BIRNAM_DYLIB_PATH).
    if (is_dynamic) stageDylibs(allocator, "src/" ++ embed_rel, out_path);
}

/// Copy each module dylib named in the staged image's manifest next to
/// `out_path`. Source is the saved path if it still exists, else
/// `zig-out/lib/<basename>`, else `<basename>` in cwd. Best-effort: a missing
/// source warns (the user may stage it manually / set BIRNAM_DYLIB_PATH).
fn stageDylibs(allocator: std.mem.Allocator, staged_img: []const u8, out_path: []const u8) void {
    const f = std.fs.cwd().openFile(staged_img, .{}) catch return;
    defer f.close();
    const bytes = f.readToEndAlloc(allocator, std.math.maxInt(usize)) catch return;
    const manifest = image.peekManifest(allocator, bytes) catch {
        stderrPrint("[birnam] --aot: could not read image manifest for dylib staging\n", .{});
        return;
    };
    const out_dir = std.fs.path.dirname(out_path) orelse ".";
    for (manifest.modules) |dep| {
        const base = std.fs.path.basename(dep.name);
        const dst = std.fs.path.join(allocator, &.{ out_dir, base }) catch continue;
        // Candidate sources in priority order.
        const lib_rel = std.fs.path.join(allocator, &.{ "zig-out", "lib", base }) catch base;
        const candidates = [_][]const u8{ dep.name, lib_rel, base };
        var staged = false;
        for (candidates) |src| {
            std.fs.cwd().copyFile(src, std.fs.cwd(), dst, .{}) catch continue;
            stdoutPrint("[birnam] --aot: staged {s}\n", .{base});
            staged = true;
            break;
        }
        if (!staged) stderrPrint("[birnam] --aot: WARNING could not stage {s} (place it beside the binary or set BIRNAM_DYLIB_PATH)\n", .{base});
    }
}

// -- Main --------------------------------------------------------------------

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    // `birnam --aot=… <image.brnimg> -o <out>` is a build-time driver, not a
    // REPL session — branch before booting the kernel.
    {
        const argv = try std.process.argsAlloc(allocator);
        defer std.process.argsFree(allocator, argv);
        for (argv[1..]) |arg| {
            if (std.mem.startsWith(u8, arg, "--aot")) return runAotDriver(allocator, argv[1..]);
        }
    }

    stderrPrint("[birnam] shell starting\n", .{});

    installSignalHandler();

    // -- Boot the kernel -----------------------------------------------------
    const store = try allocator.create(fact_store.FactStore);
    store.* = fact_store.FactStore.init(allocator);

    const table = try allocator.create(object_table.ObjectTable);
    table.* = object_table.ObjectTable.init(allocator);

    // Phase 15.1b: live process gets the segmented arena registry. String
    // boxes route through the `strings` arena (GC-enumerable); index maps +
    // interner key dups stay on the kernel allocator.
    try arenas.init(allocator);

    const interner = try allocator.create(interner_mod.Interner);
    interner.* = interner_mod.Interner.init(allocator);
    interner.setStringAllocator(arenas.strings());

    _ = try bootstrap.boot(allocator, store, table, interner);
    id_api_mod.setInterner(interner);

    const api = id_api_mod.populate();

    var manager = module.Manager.init(allocator, &shell_context, api);
    defer manager.deinit();

    // -- Load modules from argv ----------------------------------------------
    var args = std.process.args();
    _ = args.next(); // skip argv[0]
    var any_module = false;
    while (args.next()) |path_slice| {
        const path_z = try allocator.dupeZ(u8, path_slice);
        defer allocator.free(path_z);
        if (try manager.load(path_z)) |mod_eid| {
            stderrPrint("[birnam] loaded {s} (eid {d})\n", .{ path_z, mod_eid });
            any_module = true;
        } else {
            stderrPrint("[birnam] could not load {s}\n", .{path_z});
        }
    }
    if (!any_module) {
        stderrPrint("[birnam] no modules loaded — pass one or more .dylib paths\n", .{});
    }

    stderrPrint("[birnam] entering main loop (^D to quit)\n", .{});

    // -- REPL loop -----------------------------------------------------------
    const reader = std.fs.File.stdin().deprecatedReader();
    var buf: [4096]u8 = undefined;

    while (true) {
        if (reload_requested) {
            reload_requested = false;
            stderrPrint("[birnam] SIGHUP — reload requested (not implemented in this build)\n", .{});
        }

        stdoutPrint("birnam> ", .{});
        const line = reader.readUntilDelimiterOrEof(&buf, '\n') catch continue;
        if (line) |l| {
            if (handleImageCommand(l, allocator, store, table, interner, &manager)) continue;
            if (repl_handler) |handler| {
                if (l.len < buf.len) {
                    buf[l.len] = 0;
                    handler(@ptrCast(buf[0 .. l.len + 1].ptr));
                }
            } else {
                if (std.mem.eql(u8, l, "quit") or std.mem.eql(u8, l, "exit")) break;
                stderrPrint("[birnam] no repl handler registered\n", .{});
            }
        } else break; // EOF
    }

    stderrPrint("[birnam] goodbye\n", .{});
}
