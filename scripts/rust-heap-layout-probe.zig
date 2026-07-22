//! External C-layout fixture for R2.1 Rust heap blocks.

const std = @import("std");

const Oop = ?*anyopaque;
const LayoutCallback = *const fn ([*]u8, Oop, usize, usize, Oop) callconv(.c) c_int;
const ProbeLayout = *const fn (usize, Oop, LayoutCallback) callconv(.c) c_int;

var callback_count: usize = 0;

fn inspectLayout(
    base: [*]u8,
    payload: Oop,
    requested_size: usize,
    reported_size: usize,
    vtable: Oop,
) callconv(.c) c_int {
    const payload_pointer = payload orelse return 0;
    const base_address = @intFromPtr(base);
    const payload_address = @intFromPtr(payload_pointer);
    const expected_total = (8 + requested_size + 7) & ~@as(usize, 7);

    if (base_address % 16 != 0 or payload_address % 8 != 0) return 0;
    if (payload_address - base_address != 8) return 0;
    if (reported_size != expected_total - 8) return 0;

    const header: *Oop = @ptrFromInt(payload_address - 8);
    if (header.* != vtable) return 0;

    const bytes: [*]u8 = @ptrCast(payload_pointer);
    for (bytes[0..reported_size]) |byte| {
        if (byte != 0) return 0;
    }
    for (bytes[0..reported_size], 0..) |*byte, index| {
        byte.* = @truncate(index *% 29 +% 0x5a);
    }
    for (bytes[0..reported_size], 0..) |byte, index| {
        if (byte != @as(u8, @truncate(index *% 29 +% 0x5a))) return 0;
    }

    callback_count += 1;
    return 1;
}

pub fn main() !void {
    var args = std.process.args();
    _ = args.next();
    const dylib_path = args.next() orelse return error.MissingDylibPath;
    if (args.next() != null) return error.UnexpectedArgument;

    var dylib = try std.DynLib.open(dylib_path);
    defer dylib.close();
    const probe = dylib.lookup(ProbeLayout, "birnam_id_test_heap_layout") orelse
        return error.MissingHeapLayoutHook;

    var marker: u64 = 0;
    const vtable: Oop = @ptrCast(&marker);
    var requested_size: usize = 0;
    while (requested_size <= 257) : (requested_size += 1) {
        if (probe(requested_size, vtable, inspectLayout) != 1)
            return error.HeapLayoutRejected;
    }
    for ([_]usize{ 511, 512, 513, 4095, 4096, 4097 }) |extra_size| {
        if (probe(extra_size, vtable, inspectLayout) != 1)
            return error.HeapLayoutRejected;
    }
    if (callback_count != 264) return error.WrongCallbackCount;

    std.debug.print("rust-heap-layout: 264 external block probes passed\n", .{});
}
