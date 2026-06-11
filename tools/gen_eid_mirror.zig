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

const header =
    \\//! GENERATED FILE — DO NOT EDIT.
    \\//!
    \\//! Mirror of src/kernel/kernel_eids.zig, regenerated on every build by
    \\//! tools/gen_eid_mirror.zig. Change the kernel source, never this file.
    \\
    \\
;

pub fn main() !void {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const args = try std.process.argsAlloc(a);
    if (args.len < 2) {
        std.debug.print("usage: gen_eid_mirror <output-path.zig>\n", .{});
        return error.MissingOutputPath;
    }

    // Reflect over the kernel EID declarations and format each at RUNTIME.
    // (Formatting at comptime via comptimePrint blows the eval-branch quota
    // for ~200 constants — and runtime is simpler anyway: the generator only
    // exists to run on the build host.)
    var parts = std.ArrayList([]const u8){};
    try parts.append(a, header);
    inline for (comptime std.meta.declarations(kernel_eids)) |decl| {
        const value = @field(kernel_eids, decl.name);
        // The kernel file is all `pub const : u64`; guard anyway so a future
        // non-EID helper decl can't break generation.
        if (@TypeOf(value) == u64) {
            try parts.append(a, try std.fmt.allocPrint(
                a,
                "pub const {s}: u64 = {d};\n",
                .{ decl.name, value },
            ));
        }
    }
    const content = try std.mem.concat(a, u8, parts.items);

    var file = try std.fs.cwd().createFile(args[1], .{});
    defer file.close();
    try file.writeAll(content);
}
