//! Mode-3 ship backend (docs/10a): the SAME birnam→C codegen as the libtcc live
//! path, compiled by `zig cc -O2` into a FROZEN, optimized dylib and loaded back.
//! Where libtcc binds the runtime entry points via `addSymbol`, a frozen dylib is
//! built with `-undefined dynamic_lookup` and resolves them at load against the
//! host's exported `birnam_*` symbols (see send.zig / dispatch.zig `@export`s).
//! On load, each `bm_<eid>` is `dlsym`'d and installed as a trampoline closure —
//! relink-by-name, the optimized counterpart of the in-memory JIT.

const std = @import("std");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const codegen = @import("codegen.zig");
const jit_method = @import("jit_method.zig");

pub const FreezeResult = struct {
    /// The method eids whose bodies lowered + were compiled into the dylib.
    emitted: []u64,
    /// Methods skipped (no bytecode / un-lowerable).
    skipped: usize,
};

/// Emit a C unit for `eids`, write it to `<dir>/<base>.c`, and `zig cc -O2
/// -shared` it into `<dir>/lib<base>.dylib`. Caller frees `result.emitted`.
pub fn freeze(
    allocator: std.mem.Allocator,
    store: *const fact_store.FactStore,
    eids: []const u64,
    dir: []const u8,
    base: []const u8,
) !FreezeResult {
    const unit = try codegen.emitUnit(allocator, store, eids);
    defer allocator.free(unit.c_src);
    errdefer allocator.free(unit.emitted);

    const c_rel = try std.fmt.allocPrint(allocator, "{s}.c", .{base});
    defer allocator.free(c_rel);
    {
        const c_path = try std.fs.path.join(allocator, &.{ dir, c_rel });
        defer allocator.free(c_path);
        try std.fs.cwd().writeFile(.{ .sub_path = c_path, .data = unit.c_src });
    }

    const dylib_rel = try std.fmt.allocPrint(allocator, "lib{s}.dylib", .{base});
    defer allocator.free(dylib_rel);

    // Same codegen, optimized compiler. `-undefined dynamic_lookup` defers the
    // birnam_* externs to load-time resolution against the host.
    var child = std.process.Child.init(&.{
        "zig",                "cc",     "-shared", "-O2",
        "-o",                 dylib_rel, c_rel,    "-Wl,-undefined,dynamic_lookup",
    }, allocator);
    child.cwd = dir;
    child.stdout_behavior = .Ignore;
    child.stderr_behavior = .Inherit;
    try child.spawn();
    const term = try child.wait();
    if (term != .Exited or term.Exited != 0) return error.ZigCcFailed;

    return .{ .emitted = unit.emitted, .skipped = unit.skipped };
}

/// `dlopen` a frozen dylib and install each `bm_<eid>` (for `eids`) as a native
/// method (registry + trampoline closure). Returns how many installed.
pub fn load(
    allocator: std.mem.Allocator,
    table: *object_table.ObjectTable,
    dylib_path: []const u8,
    eids: []const u64,
) !usize {
    const path_z = try allocator.dupeZ(u8, dylib_path);
    defer allocator.free(path_z);
    const handle = std.c.dlopen(path_z, .{ .LAZY = true, .GLOBAL = true }) orelse return error.DlopenFailed;

    var installed: usize = 0;
    for (eids) |eid| {
        var name_buf: [40]u8 = undefined;
        const sym = std.fmt.bufPrintZ(&name_buf, "bm_{d}", .{eid}) catch continue;
        const addr = std.c.dlsym(handle, sym) orelse continue;
        const f: jit_method.MethodFn = @ptrFromInt(@intFromPtr(addr));
        const idx = jit_method.registerFn(f) orelse continue;
        jit_method.installTrampoline(allocator, table, eid, idx) catch continue;
        installed += 1;
    }
    return installed;
}
