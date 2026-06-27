//! The siphon — birnam's bare entry point, the Zig analog of `~/dev/c/birnam/
//! src/main.c`. It is *only* a loader: it `dlopen`s a module, finds its
//! `entrypoint`, and calls it with the shared `State`. It contains none of the
//! object system — `libid` is the FIRST module it loads, then peg, then up.
//!
//! No eids, no fact store. Those arrive much later, as modules on this floor.

const std = @import("std");
const state_mod = @import("state.zig");

const State = state_mod.State;
const Oop = state_mod.Oop;

/// Load a module: dlopen, find `entrypoint`, hand it the shared state. The dylib
/// stays open for the life of the process (no close) — it has registered objects
/// into the running system.
fn loadModule(path: []const u8, st: *State) !void {
    std.debug.print("[siphon] loading module {s}\n", .{path});
    var lib = std.DynLib.open(path) catch |e| {
        std.debug.print("[siphon] failed to open {s}: {}\n", .{ path, e });
        return e;
    };
    const ep = lib.lookup(state_mod.EntrypointFn, "entrypoint") orelse {
        std.debug.print("[siphon] no entrypoint in {s}\n", .{path});
        return error.NoEntrypoint;
    };
    ep(st);
}

pub fn main() !void {
    std.debug.print("[siphon] birnam\n", .{});

    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const a = gpa.allocator();

    // libid path: argv[1], else the default install location beside the siphon.
    var args = try std.process.argsWithAllocator(a);
    defer args.deinit();
    _ = args.next(); // exe
    const id_path = args.next() orelse "zig-out/lib/libbirnam_id.dylib";

    var st: State = .{};
    try loadModule(id_path, &st);

    // Proof: drive the object system entirely through the loaded module's state.
    const send = st.send.?;
    const intern = st.intern.?;

    // child = [object_vt delegated]
    const s_delegated = intern("delegated");
    const child = send(st.object_vt, s_delegated, &[_]Oop{}, 0);

    // [child addMethod: #ping with: <fn>]  — installed via send (metacircular)
    const s_addMethod = intern("addMethod");
    const s_ping = intern("ping");
    _ = send(child, s_addMethod, &[_]Oop{ s_ping, @constCast(@ptrCast(&pingImpl)) }, 2);

    // inst = [child allocate: 0];  [inst ping]
    const s_allocate = intern("allocate");
    const inst = send(child, s_allocate, &[_]Oop{@as(Oop, @ptrFromInt(0))}, 1);
    const r = send(inst, s_ping, &[_]Oop{}, 0);

    std.debug.print("[siphon] [inst ping] => {d}\n", .{@intFromPtr(r)});
    if (@intFromPtr(r) != 7) return error.UnexpectedResult;
    std.debug.print("[siphon] ok — the id object system is up, driven through the module ABI\n", .{});
}

fn pingImpl(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = args;
    _ = n;
    return @ptrFromInt(7);
}
