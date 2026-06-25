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
const dylib_search = @import("dylib_search.zig");
const config_mod = @import("config.zig");
const lsp = @import("lsp.zig");
const bopts = @import("build_opts");

/// Default module set when neither argv nor birnam.toml names any (and
/// `--bare` was not given). peg before lang — order matters.
const default_modules = [_][]const u8{ "peg", "lang", "json", "stdlib", "net" };

// Search dirs for the FFI `Module load:` bare-name resolver (process-lifetime).
var g_resolver_dirs: []const []const u8 = &.{};
var g_resolver_buf: ?[]const u8 = null;

/// Resolve a bare dylib name through the module search path. Registered on
/// `ffi_module.dylib_resolver` so `[Module load: "liblinenoise.dylib"]` finds
/// the lib in the known module dir (cwd-independent). The result is held until
/// the next call (the kernel copies it immediately).
fn dylibResolverFn(name_ptr: [*]const u8, name_len: usize, out_len: *usize) callconv(.c) ?[*]const u8 {
    const pa = std.heap.page_allocator;
    const name = name_ptr[0..name_len];
    const resolved = dylib_search.resolveWith(pa, name, g_resolver_dirs) orelse return null;
    if (g_resolver_buf) |b| pa.free(b);
    g_resolver_buf = resolved;
    out_len.* = resolved.len;
    return resolved.ptr;
}

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
    if (std.mem.eql(u8, line, ":help") or std.mem.eql(u8, line, ":h") or std.mem.eql(u8, line, "?")) {
        printHelp();
        return true;
    }
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
        // Prune/GC sweep entities by reachability and do NOT scan the live VM
        // value stack — only safe when control is back in the shell (empty
        // stack). When invoked through the `[language command:]` bridge (e.g.
        // the linenoise REPL routing a `:` line mid-loop), the running loop's
        // frames/blocks are live; sweeping them hangs/crashes the REPL. Refuse.
        if (g_repl_bridge_active) {
            stderrPrint("[birnam] :prune skipped — only runs at the shell boundary, not inside a running REPL loop\n", .{});
            return true;
        }
        const stats = prune.pruneReachable(store, table, allocator) catch {
            stderrPrint("[birnam] :prune failed\n", .{});
            return true;
        };
        stdoutPrint("[birnam] pruned ({d} facts kept, {d} dropped, {d} entities swept)\n", .{ stats.kept, stats.dropped_facts, stats.swept_entities });
        return true;
    }
    if (std.mem.eql(u8, line, ":gc")) {
        if (g_repl_bridge_active) {
            stderrPrint("[birnam] :gc skipped — only runs at the shell boundary, not inside a running REPL loop\n", .{});
            return true;
        }
        // Heap reclaim (distinct from :prune's logical reclaim). Sweeps the
        // boxes arena. Best run after :prune so dropped facts no longer pin
        // their boxes (Phase 15 §Q2 current-values gate).
        const st = gc.collectBoxes(store, table, allocator);
        if (st.skipped) {
            stdoutPrint("[birnam] :gc skipped (no arena registry or FFI in flight)\n", .{});
        } else {
            stdoutPrint("[birnam] gc ({d} boxes freed/{d} kept, {d} closures freed/{d} kept, {d} wrappers freed/{d} kept, {d} strings freed/{d} kept)\n", .{ st.boxes_freed, st.boxes_kept, st.closures_freed, st.closures_kept, st.wrappers_freed, st.wrappers_kept, st.strings_freed, st.strings_kept });
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
    // `:retain current|all|since <basis>|last <n>` — Phase 15.R R2 retention
    // policy. Opt into time-travel beyond "now". Shell links the kernel so it
    // calls store.setRetention directly (no module ABI surface needed).
    if (std.mem.startsWith(u8, line, ":retain ")) {
        const arg = std.mem.trim(u8, line[8..], " ");
        if (std.mem.eql(u8, arg, "current")) {
            store.setRetention(.current_only);
            stdoutPrint("[birnam] retention: current-only\n", .{});
        } else if (std.mem.eql(u8, arg, "all")) {
            store.setRetention(.keep_all);
            stdoutPrint("[birnam] retention: keep-all\n", .{});
        } else if (std.mem.startsWith(u8, arg, "since ")) {
            const n = std.fmt.parseInt(u64, std.mem.trim(u8, arg[6..], " "), 10) catch {
                stderrPrint("[birnam] :retain since <basis> — bad number\n", .{});
                return true;
            };
            store.setRetention(.{ .keep_since_basis = n });
            stdoutPrint("[birnam] retention: keep-since basis {d}\n", .{n});
        } else if (std.mem.startsWith(u8, arg, "last ")) {
            const n = std.fmt.parseInt(u64, std.mem.trim(u8, arg[5..], " "), 10) catch {
                stderrPrint("[birnam] :retain last <n> — bad number\n", .{});
                return true;
            };
            store.setRetention(.{ .keep_last_n = n });
            stdoutPrint("[birnam] retention: keep-last {d} txs\n", .{n});
        } else {
            stderrPrint("[birnam] :retain current|all|since <basis>|last <n>\n", .{});
        }
        return true;
    }
    // `:excise <eid>` — Phase 15.R R3. Hard-delete an entity (current + history,
    // incl. retained archives). Refuses if a live snapshot pins an epoch.
    if (std.mem.startsWith(u8, line, ":excise ")) {
        const eid = std.fmt.parseInt(u64, std.mem.trim(u8, line[8..], " "), 10) catch {
            stderrPrint("[birnam] :excise <eid> — bad number\n", .{});
            return true;
        };
        store.exciseEntity(eid, allocator) catch |e| {
            stderrPrint("[birnam] :excise failed: {s}\n", .{@errorName(e)});
            return true;
        };
        stdoutPrint("[birnam] excised entity {d}\n", .{eid});
        return true;
    }
    // `:asof <basis> <eid> <attr-eid>` — Phase 15.R R1/R2. Exact time-travel
    // point read: snapshot as of `basis`, read (eid, attr). The held db-value /
    // `[db asOf: t]` message-send sugar + GC-auto-release is the follow-on.
    if (std.mem.startsWith(u8, line, ":asof ")) {
        var it = std.mem.tokenizeScalar(u8, std.mem.trim(u8, line[6..], " "), ' ');
        const basis = std.fmt.parseInt(u64, it.next() orelse "", 10) catch {
            stderrPrint("[birnam] :asof <basis> <eid> <attr-eid>\n", .{});
            return true;
        };
        const eid = std.fmt.parseInt(u64, it.next() orelse "", 10) catch {
            stderrPrint("[birnam] :asof <basis> <eid> <attr-eid>\n", .{});
            return true;
        };
        const attr = std.fmt.parseInt(u64, it.next() orelse "", 10) catch {
            stderrPrint("[birnam] :asof <basis> <eid> <attr-eid>\n", .{});
            return true;
        };
        const snap = store.snapshotAsOf(basis) catch |e| {
            stderrPrint("[birnam] :asof failed: {s}\n", .{@errorName(e)});
            return true;
        };
        defer snap.deinit(allocator);
        if (snap.get(eid, attr)) |v| {
            stdoutPrint("[birnam] asof basis {d}: ({d}, {d}) = ref 0x{x}\n", .{ snap.basis_t, eid, attr, v });
        } else {
            stdoutPrint("[birnam] asof basis {d}: ({d}, {d}) = (absent)\n", .{ snap.basis_t, eid, attr });
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

// Shell state exposed to a birnam-authored REPL via the `[language command:]`
// primitive (installed at handoff). A birnam REPL loop owns input + eval, but
// the `:` image/session commands (`:in`/`:save`/`:load`/`:gc`/`:prune`/`:freeze`/
// `:asof`/`:help`) live in the shell's `handleImageCommand` — so we route them
// back through it. These are set once, just before handing off, and live for
// the process.
var g_repl_alloc: ?std.mem.Allocator = null;
var g_repl_store: ?*fact_store.FactStore = null;
var g_repl_table: ?*object_table.ObjectTable = null;
var g_repl_interner: ?*interner_mod.Interner = null;
var g_repl_manager: ?*const module.Manager = null;
/// True while a `:` command runs via the `[language command:]` bridge — i.e.
/// nested inside a running birnam REPL loop (linenoise). Stack-sweeping
/// commands (:prune/:gc) refuse in this state.
var g_repl_bridge_active: bool = false;

/// `[language command: aString]` — run a shell `:` command through
/// `handleImageCommand`; answers `true` if it was a recognised command,
/// `false` otherwise (so the birnam REPL knows to fall through to `evalLine:`).
fn shellCommandImpl(s: *send_mod.Send, args: [*]const ref_mod.Ref, n_args: usize) callconv(.c) ref_mod.Ref {
    _ = s;
    _ = n_args;
    const arg = args[0];
    if (!strings.isString(arg)) return ref_mod.mkEid(kernel_eids.FALSE_EID);
    // Mark nested-in-REPL so stack-unsafe commands (:prune/:gc) refuse.
    g_repl_bridge_active = true;
    defer g_repl_bridge_active = false;
    const handled = handleImageCommand(
        strings.bytes(arg),
        g_repl_alloc orelse return ref_mod.mkEid(kernel_eids.FALSE_EID),
        g_repl_store.?,
        g_repl_table.?,
        g_repl_interner.?,
        g_repl_manager.?,
    );
    return ref_mod.mkEid(if (handled) kernel_eids.TRUE_EID else kernel_eids.FALSE_EID);
}

/// Install `command:` on the `language` prototype + record the shell state it
/// needs, then return — the caller drives the birnam REPL loop.
fn installReplBridge(
    api: id_api_mod.IdApi,
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
    manager: *const module.Manager,
) void {
    g_repl_alloc = allocator;
    g_repl_store = store;
    g_repl_table = table;
    g_repl_interner = interner;
    g_repl_manager = manager;
    const sel = api.intern("command:", 8);
    const clo = api.make_closure(@ptrCast(&shellCommandImpl), ref_mod.NIL);
    _ = api.assert_fact(kernel_eids.LANGUAGE_PROTO_VT, sel, ref_mod.mkEid(clo), kernel_eids.M_USER);
}

/// Resolve a global binding's value by name (walks the GLOBALS scope binding
/// list). Used to reach `linenoise` after filing in its wrapper.
fn resolveGlobal(api: id_api_mod.IdApi, name_eid: u64) ?ref_mod.Ref {
    var head = api.query_eavt(kernel_eids.GLOBALS_SCOPE_EID, kernel_eids.ATTR_SCOPE_FIRST_BINDING);
    while (ref_mod.isEid(head)) {
        const b = ref_mod.untagEid(head);
        const nm = api.query_eavt(b, kernel_eids.ATTR_BIND_NAME);
        if (ref_mod.isEid(nm) and ref_mod.untagEid(nm) == name_eid) {
            return api.query_eavt(b, kernel_eids.ATTR_BIND_VALUE);
        }
        head = api.query_eavt(b, kernel_eids.ATTR_BIND_NEXT);
    }
    return null;
}

/// Process one REPL input line: dispatch (shell `:` command, else evaluate via
/// the lang handler) + the per-line maintenance the loop runs at its quiescent
/// boundary (FFI resolve, auto-prune, auto-gc, epoch spill). Shared by the
/// built-in stdin loop and the linenoise loop. Returns false to stop the loop.
fn processReplLine(
    l: []const u8,
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
    manager: *const module.Manager,
) bool {
    if (handleImageCommand(l, allocator, store, table, interner, manager)) return true;
    if (repl_handler) |handler| {
        var buf: [4096]u8 = undefined;
        if (l.len < buf.len) {
            @memcpy(buf[0..l.len], l);
            buf[l.len] = 0;
            handler(@ptrCast(buf[0 .. l.len + 1].ptr));
        }
        // Bind any FFI wraps this line declared (resolveModule is idempotent).
        ffi_module.resolveAllPending(allocator, store, table) catch {};
        // Quiescent line boundary — the safe point to sweep per-line garbage.
        if (prune.maybePrune(store, table, allocator) catch null) |pst| {
            stderrPrint("[birnam] auto-pruned ({d} facts dropped, {d} entities swept)\n", .{ pst.dropped_facts, pst.swept_entities });
        }
        if (gc.maybeCollect(store, table, allocator)) |st| {
            if (!st.skipped and (st.boxes_freed + st.closures_freed + st.wrappers_freed + st.strings_freed > 0)) {
                stderrPrint("[birnam] auto-gc ({d} boxes, {d} closures, {d} wrappers, {d} strings freed)\n", .{ st.boxes_freed, st.closures_freed, st.wrappers_freed, st.strings_freed });
            }
        }
        const spilled = store.maybeSpillEpochs(".", 2, allocator) catch 0;
        if (spilled > 0) stderrPrint("[birnam] cold-tier: spilled {d} epoch(s) to disk\n", .{spilled});
    } else {
        if (std.mem.eql(u8, l, "quit") or std.mem.eql(u8, l, "exit")) return false;
        stderrPrint("[birnam] no repl handler registered\n", .{});
    }
    return true;
}

/// Hand the REPL to the linenoise line reader, but the SHELL owns the loop: it
/// files in the linenoise wrapper, then reads each line via `[linenoise
/// prompt:]` and runs the SAME `processReplLine` body as the built-in loop — so
/// `:prune`/`:gc` and auto-maintenance run at the quiescent boundary (the line
/// editing is the only thing delegated to birnam). Returns true once the loop
/// ends (caller exits); false if it couldn't hand off (caller falls back).
fn runBirnamRepl(
    name: []const u8,
    cfg_dir: []const u8,
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
    manager: *const module.Manager,
) bool {
    if (!std.mem.eql(u8, name, "linenoise")) {
        stderrPrint("[birnam] repl: unknown repl '{s}' — using built-in loop\n", .{name});
        return false;
    }
    const wrapper = std.fs.path.join(allocator, &.{ cfg_dir, "wrapped/linenoise/linenoise.birnam" }) catch return false;
    std.fs.cwd().access(wrapper, .{}) catch {
        stderrPrint("[birnam] repl: missing {s} — using built-in loop\n", .{wrapper});
        return false;
    };

    const api = id_api_mod.populate();
    // Keep `[language command:]` available for birnam code (the shell-owned loop
    // doesn't use it, but the guard makes nested :prune/:gc safe).
    installReplBridge(api, allocator, store, table, interner, manager);

    // File in the wrapper: `Module load:` (bare name → module search path) +
    // `wrap:` decls; the commit dlopens the lib + installs the FFI methods.
    var buf1: [4096]u8 = undefined;
    const c1 = std.fmt.bufPrint(&buf1, ":in {s}", .{wrapper}) catch return false;
    _ = handleImageCommand(c1, allocator, store, table, interner, manager);

    // Resolve `linenoise` (bound by the wrapper). If the dylib didn't load, the
    // wraps are unresolved — fall back to the built-in loop.
    const ln = resolveGlobal(api, api.intern("linenoise", 9)) orelse {
        stderrPrint("[birnam] repl: linenoise not available (run `birnam wrap linenoise`) — using built-in loop\n", .{});
        return false;
    };
    const sel_prompt = api.intern("prompt:", 7);
    const sel_hadd = api.intern("historyAdd:", 11);
    if (ref_mod.mkSmallInt(1000)) |n| _ = api.send1(ln, api.intern("historyMaxLen:", 14), n);

    stdoutPrint("[birnam] linenoise REPL — line editing + history (up/down recalls, :quit to exit)\n", .{});

    while (true) {
        const prompt_ref = api.make_string("birnam> ", 8);
        const line_ref = api.send1(ln, sel_prompt, prompt_ref);
        if (line_ref == ref_mod.NIL) break; // ^D / EOF
        if (!strings.isString(line_ref)) continue;
        const l = strings.bytes(line_ref);
        if (l.len == 0) continue;
        _ = api.send1(ln, sel_hadd, line_ref);
        if (std.mem.eql(u8, l, ":quit") or std.mem.eql(u8, l, ":exit")) break;
        if (!processReplLine(l, allocator, store, table, interner, manager)) break;
    }
    stderrPrint("[birnam] goodbye\n", .{});
    return true;
}

var shell_context = id_api_mod.ShellContext{
    .log = &shellLog,
    .set_repl_handler = &shellSetReplHandler,
};

/// One-line startup banner: version + the two things a newcomer needs.
fn printBanner() void {
    stdoutPrint("birnam {s} — type :help for commands. Evaluate with brackets: [3 + 4]\n", .{bopts.version});
}

/// `birnam --help` — CLI usage. The shell-internal command reference is `:help`
/// once the REPL is up; this covers invocation (flags, subcommands, env).
fn printUsage() void {
    stdoutPrint(
        \\birnam {s} — relational fact-store language + shell
        \\
        \\Usage:
        \\  birnam [modules…] [file.birnam …] [flags]   start the REPL / run scripts
        \\  birnam <subcommand> […]
        \\
        \\With no module given, loads the default set (peg lang) or whatever a
        \\discovered birnam.toml names. Modules are bare names (lang) or dylib paths.
        \\
        \\Flags:
        \\  -e, --eval EXPR        evaluate one expression, then exit
        \\  -i, --interactive      after scripts/-e, stay in the REPL
        \\      --bare             load no modules (suppress defaults + config)
        \\      --lsp              run as a Language Server over stdio (JSON-RPC)
        \\      --overrides LIST   module overrides, comma-separated slot=impl
        \\                         (e.g. repl=linenoise,json=jsmn)
        \\  -h, --help             show this help
        \\  -V, --version          print the version
        \\      --                 end flags; remaining args are the program's argv
        \\
        \\Subcommands:
        \\  init                   scaffold birnam.toml + playground.birnam here
        \\  wrap [name]            build first-party wrapped C libs (birnam.toml [[wrapped]])
        \\  --aot=<static|dynamic> ahead-of-time compile an image (see --aot usage)
        \\
        \\Environment:
        \\  BIRNAM_DYLIB_PATH        exact module dirs (':'-separated)
        \\  BIRNAM_PATH              install roots; modules sought under <root>/lib
        \\  BIRNAM_MODULE_OVERRIDES  same format as --overrides
        \\
        \\Files:
        \\  birnam.toml            project config (modules, dylib_paths, image,
        \\                         [module_overrides], [[wrapped]]); cwd-upward.
        \\
        \\In the REPL, `:help` lists the interactive commands.
        \\
    , .{bopts.version});
}

/// REPL command reference (`:help`). Covers both the shell-handled commands
/// (image / file-in / heap) and the lang-handled ones.
fn printHelp() void {
    stdoutPrint(
        \\birnam commands:
        \\  :help, :h, ?         show this
        \\  :in  <file>          evaluate a .birnam file (atomic)
        \\  :in! <file>          evaluate a file, continue past errors
        \\  :save <file.bim>     snapshot the whole image to disk
        \\  :load <file.bim>     restore an image
        \\  :prune               drop unreachable facts (logical reclaim)
        \\  :gc                  reclaim heap (run after :prune)
        \\  :retention <policy>  current | all | since <t> | last <n>
        \\  :reset               clear the top-level scope bindings
        \\  :eval-mode [m]       bytecode | treewalk (bare: show current)
        \\  :freeze <out>        emit a wrapper dylib re-establishing FFI wraps
        \\  ^D                   quit
        \\
        \\Evaluate expressions with brackets: [3 + 4], [x squared], let x := 10.
        \\Shell flags: birnam [modules…] [file.birnam] [-e EXPR] [-i] [--bare]
        \\
    , .{});
}

/// Feed one line of source to the lang REPL handler (used by `-e EXPR`). The
/// handler wants a null-terminated string; the result prints via the lang log.
fn evalReplLine(allocator: std.mem.Allocator, line: []const u8) void {
    const h = repl_handler orelse {
        stderrPrint("[birnam] -e: no repl handler (is lang loaded?)\n", .{});
        return;
    };
    const z = allocator.allocSentinel(u8, line.len, 0) catch return;
    defer allocator.free(z);
    @memcpy(z, line);
    h(@ptrCast(z.ptr));
}

// -- Signal handling (kept from Phase 0) ------------------------------------

var reload_requested: bool = false;

fn sighupHandler(_: c_int) callconv(.c) void {
    reload_requested = true;
}

fn installSignalHandler() void {
    const act = posix.Sigaction{
        .handler = .{ .handler = sighupHandler },
        // Portable empty signal mask: `sa_mask` is an integer on macOS/BSD but
        // a struct (`[N]c_ulong`) on Linux, so a bare `0` won't compile there.
        .mask = std.mem.zeroes(posix.sigset_t),
        .flags = 0,
    };
    posix.sigaction(posix.SIG.HUP, &act, null);
}

// -- `birnam --aot` driver ---------------------------------------------------

/// Compile a user `.bim` into a standalone executable. Usage:
///   birnam --aot=<static|dynamic> [--strip=malleable] [--strip=history] \
///          <image.bim> [--entry=<selector>[:<recv>]] -o <out>
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
        stderrPrint("[birnam] --aot: no input image (give a .bim path)\n", .{});
        return;
    };
    const out_path = out orelse {
        stderrPrint("[birnam] --aot: no output (-o <path>)\n", .{});
        return;
    };
    if (strip_history) stdoutPrint("[birnam] --strip=history: freezing store read-only (no tx-writer / history)\n", .{});

    // Stage the image at the per-app embed path (gitignored; not the fixture).
    const embed_rel = ".aot_app.bim";
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

// -- `birnam init` -----------------------------------------------------------

const init_birnam_toml =
    \\# birnam project config — discovered by walking cwd upward.
    \\modules = ["peg", "lang"]
    \\# dylib_paths = ["zig-out/lib"]   # extra dirs to find module dylibs
    \\# image = "session.bim"           # snapshot auto-loaded on a bare `birnam`
    \\
;

const init_playground =
    \\// playground.birnam — run: birnam playground.birnam   (or :in from the REPL)
    \\// statements are separated by ';' when filed in.
    \\[3 + 4];
    \\let greeting := "hello, birnam";
    \\let x := 10;
    \\[x squared];
    \\
;

/// `birnam init` — scaffold a project in the cwd. Writes `birnam.toml` and a
/// starter `playground.birnam`, skipping any that already exist (never
/// clobbers). Pure file I/O — no kernel boot.
fn runInit() void {
    writeIfAbsent("birnam.toml", init_birnam_toml);
    writeIfAbsent("playground.birnam", init_playground);
    stdoutPrint("Next: `birnam` (resumes this project) or `birnam playground.birnam`.\n", .{});
}

/// `birnam wrap [name]` — build the `[[wrapped]]` C libraries from the
/// discovered `birnam.toml` into dylibs via the system `cc` (NOT zig: authoring
/// a wrapped library never requires a zig toolchain). With no arg, builds every
/// declared lib; with a name, just that one. Each `.dylib` is then loadable from
/// a birnam wrapper via `Module load:` + the FFI `wrap:` surface.
fn runWrap(allocator: std.mem.Allocator, only: ?[]const u8) void {
    const cfg = config_mod.discover(allocator) orelse {
        stderrPrint("[birnam] wrap: no birnam.toml found (walk cwd upward). Declare a [[wrapped]] entry.\n", .{});
        return;
    };
    if (cfg.wrapped.len == 0) {
        stderrPrint("[birnam] wrap: birnam.toml has no [[wrapped]] entries.\n", .{});
        return;
    }
    var built: usize = 0;
    var matched: usize = 0;
    for (cfg.wrapped) |lib| {
        if (only) |n| if (!std.mem.eql(u8, n, lib.name)) continue;
        matched += 1;
        if (buildWrapped(allocator, lib)) built += 1;
    }
    if (only != null and matched == 0) {
        stderrPrint("[birnam] wrap: no [[wrapped]] entry named '{s}'.\n", .{only.?});
        return;
    }
    stdoutPrint("[birnam] wrap: built {d}/{d} librar{s}.\n", .{ built, matched, if (matched == 1) "y" else "ies" });
}

/// Compile one `[[wrapped]]` entry: `cc <cflags> <-dynamiclib|-shared> -fPIC
/// <sources…> -o <output>`. Returns true on a clean (exit 0) build.
fn buildWrapped(allocator: std.mem.Allocator, lib: config_mod.WrappedLib) bool {
    if (lib.sources.len == 0) {
        stderrPrint("[birnam] wrap: '{s}' has no sources.\n", .{lib.name});
        return false;
    }
    const output = lib.output orelse {
        stderrPrint("[birnam] wrap: '{s}' has no resolvable output path.\n", .{lib.name});
        return false;
    };
    // Ensure the output directory exists.
    if (std.fs.path.dirname(output)) |d| std.fs.cwd().makePath(d) catch {};

    var args = std.ArrayList([]const u8){};
    defer args.deinit(allocator);
    args.append(allocator, "cc") catch return false;
    // Platform shared-lib flag: macOS wants -dynamiclib, ELF wants -shared.
    args.append(allocator, if (@import("builtin").target.os.tag == .macos) "-dynamiclib" else "-shared") catch return false;
    args.append(allocator, "-fPIC") catch return false;
    for (lib.cflags) |f| args.append(allocator, f) catch return false;
    for (lib.sources) |s| args.append(allocator, s) catch return false;
    args.append(allocator, "-o") catch return false;
    args.append(allocator, output) catch return false;

    stdoutPrint("[birnam] wrap: building {s} -> {s}\n", .{ lib.name, output });
    var child = std.process.Child.init(args.items, allocator);
    child.stdout_behavior = .Inherit;
    child.stderr_behavior = .Inherit;
    child.spawn() catch |e| {
        stderrPrint("[birnam] wrap: cannot run cc ({s}). Is a C compiler installed?\n", .{@errorName(e)});
        return false;
    };
    const term = child.wait() catch return false;
    if (term == .Exited and term.Exited == 0) {
        if (lib.wrapper) |w| stdoutPrint("[birnam] wrap: ok — file in the wrapper with `:in {s}`\n", .{w});
        return true;
    }
    stderrPrint("[birnam] wrap: '{s}' build failed.\n", .{lib.name});
    return false;
}

fn writeIfAbsent(path: []const u8, contents: []const u8) void {
    if (std.fs.cwd().access(path, .{})) {
        stdoutPrint("  exists, skipped: {s}\n", .{path});
        return;
    } else |_| {}
    const f = std.fs.cwd().createFile(path, .{}) catch {
        stderrPrint("[birnam] init: could not write {s}\n", .{path});
        return;
    };
    defer f.close();
    f.writeAll(contents) catch {
        stderrPrint("[birnam] init: write failed for {s}\n", .{path});
        return;
    };
    stdoutPrint("  created: {s}\n", .{path});
}

// -- Main --------------------------------------------------------------------

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    // `birnam --aot=… <image.bim> -o <out>` is a build-time driver, not a
    // REPL session — branch before booting the kernel.
    {
        const argv = try std.process.argsAlloc(allocator);
        defer std.process.argsFree(allocator, argv);
        for (argv[1..]) |arg| {
            if (std.mem.eql(u8, arg, "--help") or std.mem.eql(u8, arg, "-h")) {
                printUsage();
                return;
            }
            if (std.mem.eql(u8, arg, "--version") or std.mem.eql(u8, arg, "-V")) {
                stdoutPrint("birnam {s}\n", .{bopts.version});
                return;
            }
        }
        for (argv[1..]) |arg| {
            if (std.mem.startsWith(u8, arg, "--aot")) return runAotDriver(allocator, argv[1..]);
        }
        if (argv.len > 1 and std.mem.eql(u8, argv[1], "init")) return runInit();
        if (argv.len > 1 and std.mem.eql(u8, argv[1], "wrap")) {
            runWrap(allocator, if (argv.len > 2) argv[2] else null);
            // Skip the deferred GPA leak check: `discover` allocates the config
            // strings for the process lifetime (never freed by design, like the
            // REPL path). The OS reclaims on exit.
            std.process.exit(0);
        }
    }

    stderrPrint("[birnam] shell starting\n", .{});

    installSignalHandler();

    // -- Boot the kernel -----------------------------------------------------
    const store = try allocator.create(fact_store.FactStore);
    store.* = fact_store.FactStore.init(allocator);
    try store.enableIndexArena(); // Phase 15.I: index nodes → freeable epoch arena

    const table = try allocator.create(object_table.ObjectTable);
    table.* = object_table.ObjectTable.init(allocator);

    // Phase 15.1b: live process gets the segmented arena registry. String
    // boxes route through the `strings` arena (GC-enumerable); index maps +
    // interner key dups stay on the kernel allocator.
    try arenas.init(allocator);
    gc.resetAutoTrigger();

    const interner = try allocator.create(interner_mod.Interner);
    interner.* = interner_mod.Interner.init(allocator);
    interner.setStringAllocator(arenas.strings());

    _ = try bootstrap.boot(allocator, store, table, interner);
    id_api_mod.setInterner(interner);

    const api = id_api_mod.populate();

    var manager = module.Manager.init(allocator, &shell_context, api);
    defer manager.deinit();

    // -- Resolve the module set ----------------------------------------------
    // Args before `--` are module names (bare like `lang`, or full dylib
    // paths); everything after `--` is the program's argument list, exposed to
    // birnam as `System arguments`. `--bare` suppresses the default set.
    //
    // Precedence for which modules to load:
    //   1. modules named on argv (explicit override)
    //   2. `modules` from a discovered birnam.toml
    //   3. the default set (peg + lang), unless `--bare`
    // Module names + config strings are needed only through module loading and
    // the snapshot auto-load — never during the REPL. Allocate them in a
    // short-lived arena freed before the loop, so they don't show as leaks at
    // exit. (Program args are separate — they back `System arguments` and must
    // live for the process, so they stay on the main allocator.)
    var setup_arena = std.heap.ArenaAllocator.init(allocator);
    const sa = setup_arena.allocator();

    // Args ending in `.birnam` are scripts to run (file-in then exit, unless
    // `-i`); `-e/--eval EXPR` evaluates one expression then exits; everything
    // else before `--` is a module name.
    var args = std.process.args();
    _ = args.next(); // skip argv[0]
    var argv_modules = std.ArrayList([]const u8){};
    var scripts = std.ArrayList([]const u8){};
    var prog_args = std.ArrayList([]const u8){};
    var seen_dashdash = false;
    var bare = false;
    var interactive = false;
    var lsp_mode = false;
    var overrides_flag: ?[]const u8 = null;
    var eval_expr: ?[]const u8 = null;
    var boot_image: ?[]const u8 = null;
    while (args.next()) |arg| {
        if (seen_dashdash) {
            try prog_args.append(allocator, try allocator.dupe(u8, arg));
            continue;
        }
        if (std.mem.eql(u8, arg, "--")) {
            seen_dashdash = true;
            continue;
        }
        if (std.mem.eql(u8, arg, "--bare")) {
            bare = true;
            continue;
        }
        if (std.mem.eql(u8, arg, "--lsp")) {
            lsp_mode = true;
            continue;
        }
        if (std.mem.eql(u8, arg, "--overrides")) {
            if (args.next()) |v| overrides_flag = try sa.dupe(u8, v);
            continue;
        }
        if (std.mem.eql(u8, arg, "-i") or std.mem.eql(u8, arg, "--interactive")) {
            interactive = true;
            continue;
        }
        if (std.mem.eql(u8, arg, "-e") or std.mem.eql(u8, arg, "--eval")) {
            if (args.next()) |e| eval_expr = try sa.dupe(u8, e);
            continue;
        }
        if (std.mem.eql(u8, arg, "--boot-image")) {
            if (args.next()) |p| boot_image = try sa.dupe(u8, p);
            continue;
        }
        if (std.mem.endsWith(u8, arg, ".birnam")) {
            try scripts.append(sa, try sa.dupe(u8, arg));
            continue;
        }
        try argv_modules.append(sa, try sa.dupe(u8, arg));
    }
    // Register program args (slice outlives the process). Backs `System arguments`.
    id_api_mod.setArgs(try prog_args.toOwnedSlice(allocator));

    // Discover birnam.toml (cwd upward). Best-effort; null when none found.
    const cfg = config_mod.discover(sa);
    if (cfg) |c| stderrPrint("[birnam] config: {s}/birnam.toml\n", .{c.dir});
    const extra_dirs: []const []const u8 = if (cfg) |c| c.dylib_paths else &.{};

    // Register the FFI bare-name dylib resolver (process-lifetime copy of the
    // search dirs, since `extra_dirs` lives on the setup arena freed before the
    // REPL). Lets `Module load:` of a bare name resolve through the module
    // search path — the "known place" — regardless of cwd.
    {
        const pa = std.heap.page_allocator;
        if (pa.alloc([]const u8, extra_dirs.len)) |dirs| {
            for (extra_dirs, 0..) |d, i| dirs[i] = pa.dupe(u8, d) catch d;
            g_resolver_dirs = dirs;
        } else |_| {}
        ffi_module.dylib_resolver = &dylibResolverFn;
    }

    // Merge module overrides from all three sources, lowest precedence first so
    // `overrideFor` (last-wins) yields: argv > env > toml > default. The "repl"
    // slot is consumed below; other slots are available for modules that
    // declare an override point.
    var overrides_list = std.ArrayList(config_mod.Override){};
    if (cfg) |c| overrides_list.appendSlice(sa, c.overrides) catch {};
    if (std.process.getEnvVarOwned(sa, "BIRNAM_MODULE_OVERRIDES")) |env_str| {
        if (config_mod.parseOverrideList(sa, env_str) catch null) |evs| overrides_list.appendSlice(sa, evs) catch {};
    } else |_| {}
    if (overrides_flag) |s| {
        if (config_mod.parseOverrideList(sa, s) catch null) |avs| overrides_list.appendSlice(sa, avs) catch {};
    }
    const overrides = overrides_list.items;

    // Pick the source of module names per the precedence above.
    // `--boot-image` overrides everything: load only the native (Zig) prefix
    // — the parser/compiler — then replay the image, which carries the compiled
    // pure-birnam modules (json/stdlib) as M_USER facts. This skips recompiling
    // them from source every boot. The native prefix must init in the SAME
    // order the image was saved with, so its CODE eids land below the image's
    // M_USER eids (eid coordination). net is a Zig module and is NOT yet
    // re-inited here (first cut — mirrors AOT-static; sockets unavailable).
    const native_prefix = [_][]const u8{ "peg", "lang" };
    const from_argv = argv_modules.items.len > 0;
    const module_names: []const []const u8 = if (boot_image != null)
        &native_prefix
    else if (from_argv)
        argv_modules.items
    else if (bare)
        &.{}
    else if (cfg != null and cfg.?.modules != null)
        cfg.?.modules.?
    else
        &default_modules;

    // -- Load modules (each name resolved through the dylib search path) ------
    var any_module = false;
    var lang_eid: u64 = 0; // captured for `--lsp` — the LANG_VT entity that answers parseExpr:
    for (module_names) |name| {
        const resolved = dylib_search.resolveWith(allocator, name, extra_dirs) orelse {
            stderrPrint("[birnam] module '{s}' not found (set BIRNAM_PATH/BIRNAM_DYLIB_PATH, dylib_paths, or stage it nearby)\n", .{name});
            continue;
        };
        defer allocator.free(resolved);
        const path_z = try allocator.dupeZ(u8, resolved);
        defer allocator.free(path_z);
        if (try manager.load(path_z)) |mod_eid| {
            stderrPrint("[birnam] loaded {s} (eid {d})\n", .{ name, mod_eid });
            if (std.mem.eql(u8, name, "lang")) lang_eid = mod_eid;
            any_module = true;
        } else {
            stderrPrint("[birnam] could not load {s}\n", .{resolved});
        }
    }
    if (!any_module) {
        stderrPrint("[birnam] no modules loaded — pass one or more module names, or add a birnam.toml\n", .{});
    }

    // Boot-image replay: after the native prefix is up, `:load` the image. It
    // reuses the proven post-load path (loadFromFile + rematerializeClosures +
    // resolveAllPending), so the compiled birnam modules come back as live
    // bytecode methods with fresh in-process fn-ptr closures — no recompile.
    if (boot_image) |bimg| {
        var buf: [4096]u8 = undefined;
        const cmd = std.fmt.bufPrint(&buf, ":load {s}", .{bimg}) catch "";
        if (cmd.len > 0) {
            stderrPrint("[birnam] boot-image: replaying {s}\n", .{bimg});
            _ = handleImageCommand(cmd, allocator, store, table, interner, &manager);
        }
    }

    // Auto-load a project snapshot named by birnam.toml — but only when the
    // module set came from config/default (an explicit argv invocation is a
    // custom session and shouldn't drag the project image in). `--bare` opts
    // out entirely. Skipped under `--boot-image` (already replayed above).
    if (!from_argv and !bare and boot_image == null) {
        if (cfg) |c| {
            if (c.image) |img| {
                var buf: [4096]u8 = undefined;
                const cmd = std.fmt.bufPrint(&buf, ":load {s}", .{img}) catch "";
                if (cmd.len > 0) _ = handleImageCommand(cmd, allocator, store, table, interner, &manager);
            }
        }
    }
    // -- W1 Stage-0 diagnostic (docs/19c) ------------------------------------
    // `BIRNAM_RESIDUE_DUMP=1` characterizes what a full-fact image would carry
    // (what the M_USER write filter drops, and the irreducible native-ptr
    // residue) against the real full-module boot, then exits. Off by default.
    if (std.process.getEnvVarOwned(sa, "BIRNAM_RESIDUE_DUMP")) |_| {
        const report = image.characterizeResidue(allocator, store, interner) catch "";
        stderrPrint("{s}", .{report});
        // Stage 1 end-to-end: write a full (v7) image of the live system and
        // report the writer counts (facts_written ≈ total − residue, skipped =
        // residue). Confirms the full writer on the real fact-set.
        if (image.writeFull(allocator, store, table.next_eid.load(.monotonic), .{}, interner, table)) |res| {
            defer allocator.free(res.bytes);
            stderrPrint("[W1 Stage 1] writeFull: {d} facts written, {d} skipped (residue), {d} bytes\n", .{ res.facts_written, res.skipped_pointer_values, res.bytes.len });
        } else |e| stderrPrint("[W1 Stage 1] writeFull failed: {any}\n", .{e});
        std.process.exit(0);
    } else |_| {}

    // `BIRNAM_SAVE_FULL=<path>` writes a v7 full image of the live system and
    // exits — for W1 Stage-2/3 testing (produce a self-contained image, then
    // load it via --boot-image). Off by default.
    if (std.process.getEnvVarOwned(sa, "BIRNAM_SAVE_FULL")) |path| {
        const mf = image.Manifest{};
        if (image.writeFull(allocator, store, table.next_eid.load(.monotonic), mf, interner, table)) |res| {
            defer allocator.free(res.bytes);
            std.fs.cwd().writeFile(.{ .sub_path = path, .data = res.bytes }) catch |e| {
                stderrPrint("[W1] save-full failed: {any}\n", .{e});
                std.process.exit(1);
            };
            stderrPrint("[W1] saved full image: {s} ({d} facts, {d} bytes)\n", .{ path, res.facts_written, res.bytes.len });
        } else |e| stderrPrint("[W1] writeFull failed: {any}\n", .{e});
        std.process.exit(0);
    } else |_| {}

    // -- LSP server mode -----------------------------------------------------
    // `birnam --lsp` becomes a Language Server over stdio instead of a REPL.
    // stdout is reserved for the JSON-RPC protocol (all shell chatter is on
    // stderr), so the loop must take over before the banner/prompt path.
    if (lsp_mode) {
        setup_arena.deinit();
        lsp.serve(allocator, api, lang_eid) catch |e| {
            stderrPrint("[birnam] --lsp: {s}\n", .{@errorName(e)});
        };
        std.process.exit(0);
    }

    // -- Script / eval bridge ------------------------------------------------
    // `birnam foo.birnam` file-ins each script; `-e EXPR` evaluates one
    // expression. Both run after modules load and then exit (run-and-exit),
    // unless `-i` was given (then fall through into the REPL — load defs, then
    // explore them live).
    const run_and_exit = (scripts.items.len > 0 or eval_expr != null) and !interactive;
    if (scripts.items.len > 0 or eval_expr != null) {
        // Baseline the auto-prune growth counter against the post-boot,
        // post-module-load high-water mark, so the inter-form prune that
        // `fileIn:` triggers measures the SCRIPT's growth — not the fixed
        // kernel+grammar+prelude+module init. (The REPL path baselines at
        // line 1118; a run-and-exit script never reaches it.)
        prune.resetAutoPrune(table);
        for (scripts.items) |path| {
            var buf: [4096]u8 = undefined;
            const cmd = std.fmt.bufPrint(&buf, ":in {s}", .{path}) catch continue;
            _ = handleImageCommand(cmd, allocator, store, table, interner, &manager);
        }
        if (eval_expr) |expr| evalReplLine(allocator, expr);
    }

    // Birnam-authored REPL handoff. A plain interactive session (no scripts,
    // no -e, not --bare) with `repl = "<name>"` in birnam.toml hands the loop to
    // that birnam REPL instead of the built-in stdin loop. Must run while `cfg`
    // (on setup_arena) is still alive. On success the birnam loop runs until
    // EOF, then we exit; if it can't hand off (unknown name / not built) we fall
    // through to the built-in loop.
    // Only hand off on an interactive terminal: line editing needs a TTY, and a
    // piped/redirected stdin (scripts, tests, heredocs) stays on the predictable
    // built-in loop. So `repl = "linenoise"` enhances interactive use without
    // changing non-interactive behaviour.
    if (!run_and_exit and !bare and std.posix.isatty(std.posix.STDIN_FILENO)) {
        if (config_mod.overrideFor(overrides, "repl")) |repl_name| {
            // The birnam REPL's assets are resolved relative to the config dir;
            // fall back to cwd when the override came from env/flag with no toml.
            const base_dir = if (cfg) |c| c.dir else ".";
            if (runBirnamRepl(repl_name, base_dir, allocator, store, table, interner, &manager)) {
                std.process.exit(0);
            }
        }
    }

    setup_arena.deinit(); // argv/config strings done — release before the REPL
    if (run_and_exit) std.process.exit(0);

    // Baseline the auto-prune growth counter AFTER boot + module loads, so it
    // triggers on user/session churn, not the fixed kernel+grammar+prelude init.
    prune.resetAutoPrune(table);

    printBanner();

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
            if (!processReplLine(l, allocator, store, table, interner, &manager)) break;
        } else break; // EOF
    }

    stderrPrint("[birnam] goodbye\n", .{});

    // Exit without running the `defer gpa.deinit()` leak check. The shell has
    // no teardown path for its kernel / interner / module state — that state is
    // process-lifetime by design, so the GeneralPurposeAllocator would report
    // thousands of "leaks" on every exit (pure noise, especially for an
    // installed binary). The OS reclaims everything; skipping the check also
    // avoids walking the whole allocation set on the way out.
    std.process.exit(0);
}
