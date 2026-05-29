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
