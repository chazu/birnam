//! Child-process caller for test-only Rust FFI abort entrypoints.

const std = @import("std");

const AbortFn = *const fn () callconv(.c) void;

pub fn main() !void {
    var args = std.process.args();
    _ = args.next();
    const dylib_path = args.next() orelse return error.MissingDylibPath;
    const symbol = args.next() orelse return error.MissingSymbol;
    if (args.next() != null) return error.UnexpectedArgument;

    var dylib = try std.DynLib.open(dylib_path);
    defer dylib.close();
    const trigger = dylib.lookup(AbortFn, symbol) orelse
        return error.MissingAbortTestHook;
    trigger();
    return error.AbortHookReturned;
}
