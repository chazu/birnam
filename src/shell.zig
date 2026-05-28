const std = @import("std");
const posix = std.posix;

// -- Module registry --------------------------------------------------------

const MAX_MODULES = 16;
const MAX_EXPORTS = 32;

const ModuleInfo = struct {
    lib: ?std.DynLib = null,
    path: [*:0]const u8 = "",
    init_fn: ?*const fn (*ShellContext) callconv(.c) void = null,
};

const ExportEntry = struct {
    name: ?[*:0]const u8 = null,
    value: ?*anyopaque = null,
};

const ShellContext = extern struct {
    load_module: *const fn ([*:0]const u8) callconv(.c) bool,
    set_repl_handler: *const fn (?*const fn ([*:0]const u8) callconv(.c) void) callconv(.c) void,
    log: *const fn ([*:0]const u8) callconv(.c) void,
    export_value: *const fn ([*:0]const u8, ?*anyopaque) callconv(.c) void,
    import_value: *const fn ([*:0]const u8) callconv(.c) ?*anyopaque,
};

var module_infos: [MAX_MODULES]ModuleInfo = [_]ModuleInfo{.{}} ** MAX_MODULES;
var module_count: usize = 0;
var repl_handler: ?*const fn ([*:0]const u8) callconv(.c) void = null;

var export_table: [MAX_EXPORTS]ExportEntry = [_]ExportEntry{.{}} ** MAX_EXPORTS;
var export_count: usize = 0;

var shell_context = ShellContext{
    .load_module = &shellLoadModule,
    .set_repl_handler = &shellSetReplHandler,
    .log = &shellLog,
    .export_value = &shellExportValue,
    .import_value = &shellImportValue,
};

// -- Shell callbacks --------------------------------------------------------

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

fn shellSetReplHandler(handler: ?*const fn ([*:0]const u8) callconv(.c) void) callconv(.c) void {
    repl_handler = handler;
}

fn shellExportValue(name: [*:0]const u8, value: ?*anyopaque) callconv(.c) void {
    const key = std.mem.span(name);
    // Update existing entry if name matches
    for (export_table[0..export_count]) |*entry| {
        if (entry.name) |n| {
            if (std.mem.eql(u8, std.mem.span(n), key)) {
                entry.value = value;
                return;
            }
        }
    }
    // Add new entry
    if (export_count < MAX_EXPORTS) {
        export_table[export_count] = .{ .name = name, .value = value };
        export_count += 1;
    } else {
        shellLog("error: export table full");
    }
}

fn shellImportValue(name: [*:0]const u8) callconv(.c) ?*anyopaque {
    const key = std.mem.span(name);
    for (export_table[0..export_count]) |entry| {
        if (entry.name) |n| {
            if (std.mem.eql(u8, std.mem.span(n), key)) {
                return entry.value;
            }
        }
    }
    return null;
}

fn shellLoadModule(path: [*:0]const u8) callconv(.c) bool {
    if (module_count >= MAX_MODULES) {
        shellLog("error: module registry full");
        return false;
    }

    var lib = std.DynLib.open(std.mem.span(path)) catch {
        stderrPrint("[birnam] error: could not load '{s}'\n", .{path});
        return false;
    };

    const init_fn = lib.lookup(*const fn (*ShellContext) callconv(.c) void, "birnam_module_init") orelse {
        stderrPrint("[birnam] error: '{s}' has no birnam_module_init\n", .{path});
        lib.close();
        return false;
    };

    module_infos[module_count] = .{
        .lib = lib,
        .path = path,
        .init_fn = init_fn,
    };
    module_count += 1;

    // Call init
    init_fn(&shell_context);

    return true;
}

fn reloadModule(info: *ModuleInfo) bool {
    // Close old handle
    if (info.lib) |*lib| {
        lib.close();
        info.lib = null;
    }

    // Reopen
    var lib = std.DynLib.open(std.mem.span(info.path)) catch {
        shellLog("error: reload failed - could not reopen library");
        return false;
    };

    const init_fn = lib.lookup(*const fn (*ShellContext) callconv(.c) void, "birnam_module_init") orelse {
        shellLog("error: reload failed - no birnam_module_init");
        lib.close();
        return false;
    };

    info.lib = lib;
    info.init_fn = init_fn;

    // Re-init
    init_fn(&shell_context);
    return true;
}

// -- kqueue file watcher ----------------------------------------------------

const Kqueue = struct {
    kq: posix.fd_t,

    fn create() ?Kqueue {
        const kq = posix.kqueue() catch return null;
        return .{ .kq = kq };
    }

    fn watchFile(self: *Kqueue, path: [*:0]const u8) void {
        const fd = posix.openZ(path, .{ .ACCMODE = .RDONLY }, 0) catch return;
        var changelist = [_]posix.Kevent{.{
            .ident = @intCast(fd),
            .filter = posix.system.EVFILT.VNODE,
            .flags = posix.system.EV.ADD | posix.system.EV.CLEAR,
            .fflags = posix.system.NOTE.WRITE | posix.system.NOTE.RENAME,
            .data = 0,
            .udata = 0,
        }};
        _ = posix.kevent(self.kq, &changelist, &.{}, null) catch {};
    }

    fn poll(self: *Kqueue) bool {
        var events: [4]posix.Kevent = undefined;
        const timeout = posix.timespec{ .sec = 0, .nsec = 100_000_000 }; // 100ms
        const n = posix.kevent(self.kq, &.{}, &events, &timeout) catch return false;
        return n > 0;
    }

    fn deinit(self: *Kqueue) void {
        posix.close(self.kq);
    }
};

// -- Signal handling --------------------------------------------------------

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

// -- Main -------------------------------------------------------------------

pub fn main() !void {
    stderrPrint("[birnam] shell starting\n", .{});

    installSignalHandler();

    // Parse args: first arg is module path, default to ./zig-out/lib/libbirnam_id.dylib
    var args = std.process.args();
    _ = args.next(); // skip argv[0]
    const module_path: [*:0]const u8 = if (args.next()) |p| p else "./zig-out/lib/libbirnam_id.dylib";

    // Load the initial module
    if (!shellLoadModule(module_path)) {
        stderrPrint("[birnam] fatal: could not load initial module\n", .{});
        std.process.exit(1);
    }

    // Set up file watcher
    var kq = Kqueue.create();
    if (kq) |*watcher| {
        watcher.watchFile(module_path);
    }

    stderrPrint("[birnam] entering main loop (^C to quit, SIGHUP to reload)\n", .{});

    // Main loop
    const reader = std.fs.File.stdin().deprecatedReader();
    var buf: [4096]u8 = undefined;

    while (true) {
        // Check for SIGHUP reload
        if (reload_requested) {
            reload_requested = false;
            stderrPrint("[birnam] reloading modules...\n", .{});
            for (module_infos[0..module_count]) |*m| {
                _ = reloadModule(m);
            }
        }

        // Check for file changes
        if (kq) |*watcher| {
            if (watcher.poll()) {
                stderrPrint("[birnam] file change detected, reloading...\n", .{});
                for (module_infos[0..module_count]) |*m| {
                    _ = reloadModule(m);
                }
            }
        }

        // Prompt
        stdoutPrint("birnam> ", .{});

        const line = reader.readUntilDelimiterOrEof(&buf, '\n') catch continue;
        if (line) |l| {
            if (repl_handler) |handler| {
                // Null-terminate for C callback
                if (l.len < buf.len) {
                    buf[l.len] = 0;
                    handler(@ptrCast(buf[0 .. l.len + 1].ptr));
                }
            } else {
                if (std.mem.eql(u8, l, "quit") or std.mem.eql(u8, l, "exit")) break;
                stderrPrint("[birnam] no repl handler registered\n", .{});
            }
        } else {
            break; // EOF
        }
    }

    // Cleanup
    if (kq) |*watcher| watcher.deinit();
    stderrPrint("[birnam] goodbye\n", .{});
}
