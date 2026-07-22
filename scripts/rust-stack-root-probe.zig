//! ReleaseSafe foreign caller that keeps a newly allocated Oop in C's x19
//! across gcCollect. The Zig caller never receives the object identity.

const std = @import("std");
const idapi = @import("idapi");

extern fn birnam_hold_new_object_and_collect(
    vtable: idapi.Oop,
    allocate: *const fn (idapi.Oop, usize) callconv(.c) idapi.Oop,
    collect: *const fn () callconv(.c) usize,
) void;

var trace_calls: usize = 0;

fn trace(_: idapi.Oop, _: idapi.MarkFn) callconv(.c) void {
    trace_calls += 1;
}

fn require(condition: bool, failure: anyerror) !void {
    if (!condition) return failure;
}

pub fn main() !void {
    var args = std.process.args();
    _ = args.next();
    const dylib_path = args.next() orelse return error.MissingDylibPath;
    if (args.next() != null) return error.UnexpectedArgument;

    var dylib = try std.DynLib.open(dylib_path);
    defer dylib.close();
    const init = dylib.lookup(idapi.IdInitFn, "id_init") orelse return error.MissingIdInit;
    const api = init();
    api.gcSetAuto(0);
    const vtable = api.proto(api.object_vt);
    api.setTraceHook(vtable, trace);

    trace_calls = 0;
    api.gcSetStackBottom(0);
    birnam_hold_new_object_and_collect(vtable, api.alloc, api.gcCollect);
    try require(trace_calls == 0, error.MissingAnchorDidNotRefuseMarking);

    trace_calls = 0;
    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    birnam_hold_new_object_and_collect(vtable, api.alloc, api.gcCollect);
    try require(trace_calls >= 1, error.RegisterRootWasNotMarked);
    stack_anchor = api.gcLiveCount();
    std.mem.doNotOptimizeAway(&stack_anchor);
    std.debug.print("rust-stack-root: optimized C x19 root reached its trace hook\n", .{});
}
