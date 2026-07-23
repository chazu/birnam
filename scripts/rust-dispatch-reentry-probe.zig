//! External Zig Method that synchronously reenters the public Rust IdApi send.

const std = @import("std");

const Oop = ?*anyopaque;
const Method = *const fn (Oop, [*]const Oop, usize) callconv(.c) Oop;
const RunReentry = *const fn (Method) callconv(.c) usize;
const ActiveSend = *const fn (Oop, Oop, ?[*]const Oop, usize) callconv(.c) Oop;

var active_send: ?ActiveSend = null;

fn zigReentrant(receiver: Oop, args: [*]const Oop, count: usize) callconv(.c) Oop {
    if (count != 1) return null;
    const send = active_send orelse return null;
    // The null/zero pair must be preserved through the Rust dispatcher to the
    // inner Rust Method, which answers 4242 only when it receives that shape.
    return send(receiver, args[0], null, 0);
}

pub fn main() !void {
    var args = std.process.args();
    _ = args.next();
    const dylib_path = args.next() orelse return error.MissingDylibPath;
    if (args.next() != null) return error.UnexpectedArgument;

    var dylib = try std.DynLib.open(dylib_path);
    defer dylib.close();
    active_send = dylib.lookup(ActiveSend, "birnam_id_test_active_send") orelse
        return error.MissingActiveSendHook;
    const run_reentry = dylib.lookup(RunReentry, "birnam_id_test_dispatch_reentry") orelse
        return error.MissingDispatchReentryHook;

    if (run_reentry(zigReentrant) != 4242) return error.WrongReentrantResult;
    std.debug.print("rust-dispatch-reentry: Zig Method reentered public Rust IdApi.send\n", .{});
}
