//! RRB-Tree microbenchmark vs std.ArrayList.
//!
//! Sanity-only. Expect RRB 2-5x slower per op than mutable ArrayList
//! for nth/set; comparable for push (both amortized). Goal: not
//! catastrophically wrong. Build with: zig build bench.

const std = @import("std");
const rrb = @import("rrb");

const U64Vec = rrb.Rrb(u64);

fn nowNs() i128 {
    return std.time.nanoTimestamp();
}

fn benchRrbPush(allocator: std.mem.Allocator, n: u64) !i128 {
    var arena = std.heap.ArenaAllocator.init(allocator);
    defer arena.deinit();
    var v = U64Vec.empty(arena.allocator());
    const t0 = nowNs();
    var i: u64 = 0;
    while (i < n) : (i += 1) v = try v.push(i);
    const t1 = nowNs();
    std.mem.doNotOptimizeAway(&v);
    return t1 - t0;
}

fn benchRrbNth(allocator: std.mem.Allocator, n: u64) !i128 {
    var arena = std.heap.ArenaAllocator.init(allocator);
    defer arena.deinit();
    var v = U64Vec.empty(arena.allocator());
    var i: u64 = 0;
    while (i < n) : (i += 1) v = try v.push(i);
    var sink: u64 = 0;
    const t0 = nowNs();
    i = 0;
    while (i < n) : (i += 1) sink +%= v.nth(i);
    const t1 = nowNs();
    std.mem.doNotOptimizeAway(&sink);
    return t1 - t0;
}

fn benchRrbSet(allocator: std.mem.Allocator, n: u64) !i128 {
    var arena = std.heap.ArenaAllocator.init(allocator);
    defer arena.deinit();
    var v = U64Vec.empty(arena.allocator());
    var i: u64 = 0;
    while (i < n) : (i += 1) v = try v.push(0);
    const t0 = nowNs();
    i = 0;
    while (i < n) : (i += 1) v = try v.set(i, i);
    const t1 = nowNs();
    std.mem.doNotOptimizeAway(&v);
    return t1 - t0;
}

fn benchStdPush(allocator: std.mem.Allocator, n: u64) !i128 {
    var v = std.ArrayList(u64){};
    defer v.deinit(allocator);
    const t0 = nowNs();
    var i: u64 = 0;
    while (i < n) : (i += 1) try v.append(allocator, i);
    const t1 = nowNs();
    std.mem.doNotOptimizeAway(&v);
    return t1 - t0;
}

fn benchStdNth(allocator: std.mem.Allocator, n: u64) !i128 {
    var v = std.ArrayList(u64){};
    defer v.deinit(allocator);
    var i: u64 = 0;
    while (i < n) : (i += 1) try v.append(allocator, i);
    var sink: u64 = 0;
    const t0 = nowNs();
    i = 0;
    while (i < n) : (i += 1) sink +%= v.items[i];
    const t1 = nowNs();
    std.mem.doNotOptimizeAway(&sink);
    return t1 - t0;
}

fn benchStdSet(allocator: std.mem.Allocator, n: u64) !i128 {
    var v = std.ArrayList(u64){};
    defer v.deinit(allocator);
    var i: u64 = 0;
    while (i < n) : (i += 1) try v.append(allocator, 0);
    const t0 = nowNs();
    i = 0;
    while (i < n) : (i += 1) v.items[i] = i;
    const t1 = nowNs();
    std.mem.doNotOptimizeAway(&v);
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

    std.debug.print("\nRRB-Tree vs std.ArrayList (ReleaseFast)\n", .{});
    std.debug.print("========================================\n", .{});

    for (sizes) |n| {
        std.debug.print("\n[N = {d}]\n", .{n});
        const rp = try benchRrbPush(allocator, n);
        report("RRB push", n, rp);
        const sp = try benchStdPush(allocator, n);
        report("std append", n, sp);
        const rn = try benchRrbNth(allocator, n);
        report("RRB nth", n, rn);
        const sn = try benchStdNth(allocator, n);
        report("std index", n, sn);
        const rs = try benchRrbSet(allocator, n);
        report("RRB set", n, rs);
        const ss = try benchStdSet(allocator, n);
        report("std set", n, ss);

        const ratio_push = @as(f64, @floatFromInt(rp)) / @as(f64, @floatFromInt(sp));
        const ratio_nth = @as(f64, @floatFromInt(rn)) / @as(f64, @floatFromInt(sn));
        const ratio_set = @as(f64, @floatFromInt(rs)) / @as(f64, @floatFromInt(ss));
        std.debug.print("  ratio: push/append = {d:.2}x   nth/index = {d:.2}x   set/set = {d:.2}x\n", .{ ratio_push, ratio_nth, ratio_set });
    }
}
