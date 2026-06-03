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
const bootstrap = @import("kernel/bootstrap.zig");
const id_api_mod = @import("kernel/id_api.zig");
const module = @import("kernel/module.zig");
const image = @import("kernel/image.zig");
const prune = @import("kernel/prune.zig");
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

// -- Main --------------------------------------------------------------------

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    stderrPrint("[birnam] shell starting\n", .{});

    installSignalHandler();

    // -- Boot the kernel -----------------------------------------------------
    const store = try allocator.create(fact_store.FactStore);
    store.* = fact_store.FactStore.init(allocator);

    const table = try allocator.create(object_table.ObjectTable);
    table.* = object_table.ObjectTable.init(allocator);

    const interner = try allocator.create(interner_mod.Interner);
    interner.* = interner_mod.Interner.init(allocator);

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
