//! Build-time generator for the kernel-EID mirror.
//!
//! `src/kernel/kernel_eids.zig` is the ONE definition of birnam's well-known
//! EIDs. Dylib modules can't share kernel *types* across the C-ABI boundary,
//! but the EID *numbers* are just constants — so today each module hand-copies
//! the ones it needs (`const AST_LITERAL_VT: u64 = 105;` …). That mirror rots
//! silently the moment the kernel renumbers an EID.
//!
//! This program reflects over the real `kernel_eids` declarations and emits a
//! dependency-free Zig file of `pub const NAME: u64 = N;` lines. build.zig runs
//! it and feeds the result to each module as `@import("kernel_eids")`, so there
//! is a single source of truth and a kernel rename becomes a module *compile
//! error* (fail-loud) instead of a wrong number baked into a dylib.
//!
//! Why generate rather than let modules `@import` kernel_eids.zig directly:
//!   - The emitted file is pure constants. Even if kernel_eids.zig later grows
//!     imports or non-EID helpers, what a module compiles against stays
//!     dependency-free and safe to bake into a standalone dylib.
//!   - Modules depend on a build artifact named `kernel_eids`, not on the
//!     kernel/ source-tree layout.
//!
//! Usage: `gen_eid_mirror <output-path.zig>` (driven by build.zig).

const std = @import("std");
const kernel_eids = @import("kernel_eids");

/// Emitted at comptime by reflecting over every `pub const : u64` in the kernel.
const mirror_source = blk: {
    var out: []const u8 =
        \\//! GENERATED FILE — DO NOT EDIT.
        \\//!
        \\//! Mirror of src/kernel/kernel_eids.zig, regenerated on every build by
        \\//! tools/gen_eid_mirror.zig. Change the kernel source, never this file.
        \\
        \\
    ;
    for (std.meta.declarations(kernel_eids)) |decl| {
        const value = @field(kernel_eids, decl.name);
        // The kernel file is all `pub const : u64`; guard anyway so a future
        // non-EID helper decl can't break generation.
        if (@TypeOf(value) == u64) {
            out = out ++ std.fmt.comptimePrint(
                "pub const {s}: u64 = {d};\n",
                .{ decl.name, value },
            );
        }
    }
    break :blk out;
};

pub fn main() !void {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();

    const args = try std.process.argsAlloc(arena.allocator());
    if (args.len < 2) {
        std.debug.print("usage: gen_eid_mirror <output-path.zig>\n", .{});
        return error.MissingOutputPath;
    }

    var file = try std.fs.cwd().createFile(args[1], .{});
    defer file.close();
    try file.writeAll(mirror_source);
}
