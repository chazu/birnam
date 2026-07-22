//! Direct loader probe for the diagnostic-only R1 Rust dylib.

const std = @import("std");

const CrateIdentity = *const fn () callconv(.c) [*:0]const u8;

pub fn main() !void {
    var args = std.process.args();
    _ = args.next();
    const path = args.next() orelse return error.MissingDylibPath;
    if (args.next() != null) return error.UnexpectedArgument;

    var dylib = try std.DynLib.open(path);
    defer dylib.close();

    const identity = dylib.lookup(
        CrateIdentity,
        "birnam_id_rust_crate_identity",
    ) orelse return error.MissingRustCrateIdentity;
    if (!std.mem.eql(u8, std.mem.span(identity()), "rust-id-shell-v1")) {
        return error.WrongRustCrateIdentity;
    }

    if (dylib.lookup(*const anyopaque, "id_init") != null) {
        return error.PrematureIdInitExport;
    }

    std.debug.print("rust-floor-shell: diagnostic identity present; id_init absent\n", .{});
}
