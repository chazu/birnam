//! HAMT microbenchmark vs std.AutoHashMap.
//!
//! Sanity-only. Expect HAMT 2-5x slower per op than mutable hashmap
//! (we pay for persistence + path-copy on every assoc). Goal: not
//! catastrophically wrong. Build with: zig build bench.

const std = @import("std");
const hamt = @import("hamt");

const U64Map = hamt.Hamt(u64, u64, hamt.U64Ctx);

fn nowNs() i128 {
    return std.time.nanoTimestamp();
}

fn benchHamtInsert(allocator: std.mem.Allocator, n: u64) !i128 {
    var arena = std.heap.ArenaAllocator.init(allocator);
    defer arena.deinit();
    var m = U64Map.empty(arena.allocator(), .{});
    const t0 = nowNs();
    var i: u64 = 0;
    while (i < n) : (i += 1) m = try m.assoc(i, i);
    const t1 = nowNs();
    std.mem.doNotOptimizeAway(&m);
    return t1 - t0;
}

fn benchHamtLookup(allocator: std.mem.Allocator, n: u64) !i128 {
    var arena = std.heap.ArenaAllocator.init(allocator);
    defer arena.deinit();
    var m = U64Map.empty(arena.allocator(), .{});
    var i: u64 = 0;
    while (i < n) : (i += 1) m = try m.assoc(i, i);
    var sink: u64 = 0;
    const t0 = nowNs();
    i = 0;
    while (i < n) : (i += 1) sink +%= m.get(i).?;
    const t1 = nowNs();
    std.mem.doNotOptimizeAway(&sink);
    return t1 - t0;
}

fn benchStdInsert(allocator: std.mem.Allocator, n: u64) !i128 {
    var m = std.AutoHashMap(u64, u64).init(allocator);
    defer m.deinit();
    const t0 = nowNs();
    var i: u64 = 0;
    while (i < n) : (i += 1) try m.put(i, i);
    const t1 = nowNs();
    std.mem.doNotOptimizeAway(&m);
    return t1 - t0;
}

fn benchStdLookup(allocator: std.mem.Allocator, n: u64) !i128 {
    var m = std.AutoHashMap(u64, u64).init(allocator);
    defer m.deinit();
    var i: u64 = 0;
    while (i < n) : (i += 1) try m.put(i, i);
    var sink: u64 = 0;
    const t0 = nowNs();
    i = 0;
    while (i < n) : (i += 1) sink +%= m.get(i).?;
    const t1 = nowNs();
    std.mem.doNotOptimizeAway(&sink);
    return t1 - t0;
}

fn report(label: []const u8, n: u64, ns: i128) void {
    const ns_per_op = @as(f64, @floatFromInt(ns)) / @as(f64, @floatFromInt(n));
    std.debug.print("  {s:<22} N={d:>7}  total={d:>10}ns  per-op={d:>7.1}ns\n", .{ label, n, ns, ns_per_op });
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const sizes = [_]u64{ 1_000, 10_000, 100_000 };

    std.debug.print("HAMT vs std.AutoHashMap (ReleaseFast)\n", .{});
    std.debug.print("======================================\n", .{});

    for (sizes) |n| {
        std.debug.print("\n[N = {d}]\n", .{n});
        const hi = try benchHamtInsert(allocator, n);
        report("HAMT assoc", n, hi);
        const si = try benchStdInsert(allocator, n);
        report("std put", n, si);
        const hl = try benchHamtLookup(allocator, n);
        report("HAMT get", n, hl);
        const sl = try benchStdLookup(allocator, n);
        report("std get", n, sl);

        const ratio_ins = @as(f64, @floatFromInt(hi)) / @as(f64, @floatFromInt(si));
        const ratio_lkp = @as(f64, @floatFromInt(hl)) / @as(f64, @floatFromInt(sl));
        std.debug.print("  ratio: assoc/put = {d:.2}x   get/get = {d:.2}x\n", .{ ratio_ins, ratio_lkp });
    }
}
