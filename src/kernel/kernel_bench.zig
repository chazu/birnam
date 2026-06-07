//! Kernel-primitive microbenchmark — SAFE BY CONSTRUCTION.
//!
//! Context: an earlier attempt to benchmark by running million-iteration
//! fact-mutation loops in the live REPL exhausted system memory and crashed
//! the machine. Root cause: every assert/retract appends never-freed history +
//! path-copies 5 HAMT indexes, so high-volume MUTATION grows memory without
//! bound. (That is itself ADR-perf-1 finding #1.)
//!
//! This harness is deliberately constrained so it CANNOT do that:
//!
//!   1. Hard memory ceiling. All allocation goes through a FixedBufferAllocator
//!      over a single up-front slice (default 256 MiB). The process never asks
//!      the OS for more, so a runaway hits `error.OutOfMemory` and exits clean
//!      instead of taking the machine down. (macOS ignores `ulimit -v`, so the
//!      cap must live in-process.)
//!   2. Phase 1 measures only NON-GROWING primitives — warm dispatch and field
//!      reads — whose hot loops allocate nothing (results are immediates /
//!      already-stored Refs). Memory stays flat regardless of iteration count.
//!   3. Fact-WRITE benchmarking (the dangerous, memory-growing class) is NOT
//!      here. It needs bounded batches + prune-in-loop and is a separate, later
//!      step (Phase 2).
//!
//! Run: `zig build bench` (ReleaseFast). Iteration count: argv[1] (default 1e6).
//! Always launch under `timeout` as a belt-and-suspenders wall-clock kill.

const std = @import("std");
const ref_mod = @import("ref.zig");
const fact_store_mod = @import("fact_store.zig");
const object_table_mod = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const bootstrap = @import("bootstrap.zig");
const send_mod = @import("send.zig");
const kernel_eids = @import("kernel_eids.zig");

const Ref = ref_mod.Ref;

/// Hard ceiling for ALL bench allocation. Boot needs only a few MiB; the flat
/// loops allocate ~nothing. 256 MiB is generous headroom AND a firm wall: if a
/// bench unexpectedly grows, it OOMs here, not on the OS.
const ARENA_BYTES: usize = 256 * 1024 * 1024;

fn nowNs() i128 {
    return std.time.nanoTimestamp();
}

fn report(name: []const u8, iters: u64, ns: i128, baseline_ns: ?i128) void {
    const per = @as(f64, @floatFromInt(ns)) / @as(f64, @floatFromInt(iters));
    var buf: [256]u8 = undefined;
    const line = if (baseline_ns) |b| blk: {
        const bper = @as(f64, @floatFromInt(b)) / @as(f64, @floatFromInt(iters));
        const ratio = per / bper;
        break :blk std.fmt.bufPrint(&buf, "{s:<32} {d:>8.2} ns/op   (native {d:>6.2} ns/op, {d:>6.1}x)\n", .{ name, per, bper, ratio }) catch return;
    } else std.fmt.bufPrint(&buf, "{s:<32} {d:>8.2} ns/op\n", .{ name, per }) catch return;
    _ = std.fs.File.stdout().write(line) catch {};
}

pub fn main() !void {
    // -- argv: iteration count (bounded default) --------------------------
    var iters: u64 = 1_000_000;
    {
        var args = std.process.args();
        _ = args.next(); // exe
        if (args.next()) |a| iters = std.fmt.parseInt(u64, a, 10) catch 1_000_000;
    }
    // Cap the count too — no accidental 1e12.
    if (iters > 100_000_000) iters = 100_000_000;

    // -- hard-capped allocator -------------------------------------------
    const backing = try std.heap.page_allocator.alloc(u8, ARENA_BYTES);
    defer std.heap.page_allocator.free(backing);
    var fba = std.heap.FixedBufferAllocator.init(backing);
    const a = fba.allocator();

    // -- boot the kernel (no peg/lang/arenas/REPL) -----------------------
    var store = fact_store_mod.FactStore.init(a);
    var table = object_table_mod.ObjectTable.init(a);
    var interner = interner_mod.Interner.init(a);
    _ = try bootstrap.boot(a, &store, &table, &interner);

    var hdr: [128]u8 = undefined;
    _ = std.fs.File.stdout().write(std.fmt.bufPrint(&hdr, "kernel_bench: {d} iters, {d} MiB cap\n", .{ iters, ARENA_BYTES / (1024 * 1024) }) catch "") catch {};

    // A materialized user entity to read from / dispatch to. OBJECT_VT is a
    // real kernel vtable; SEL_VTABLE is installed on it (returns the vt eid).
    const E: u64 = 100_000;
    _ = try store.assert(E, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.OBJECT_VT), kernel_eids.M_USER);
    const A_X: u64 = 250;
    const val = ref_mod.mkSmallInt(42).?;
    _ = try store.assert(E, A_X, val, kernel_eids.M_USER);

    // ====================================================================
    // Bench 1 — field read: store.currentValue(e,a). Zero-alloc (scalar :one
    // returns the stored Ref). THE cost a method pays to read object state.
    // ====================================================================
    {
        var sink: u64 = 0;
        // warmup
        var w: u64 = 0;
        while (w < 10_000) : (w += 1) sink +%= (store.currentValue(E, A_X) orelse 0);
        const t0 = nowNs();
        var i: u64 = 0;
        while (i < iters) : (i += 1) sink +%= (store.currentValue(E, A_X) orelse 0);
        const t1 = nowNs();
        std.mem.doNotOptimizeAway(&sink);

        // Native baseline: AutoHashMap(u64,u64) get — what a "field read" is in
        // a mutable object model.
        var hm = std.AutoHashMap(u64, u64).init(a);
        try hm.put(A_X, val);
        var nsink: u64 = 0;
        const b0 = nowNs();
        i = 0;
        while (i < iters) : (i += 1) nsink +%= (hm.get(A_X) orelse 0);
        const b1 = nowNs();
        std.mem.doNotOptimizeAway(&nsink);
        report("field read (currentValue)", iters, t1 - t0, b1 - b0);
    }

    // ====================================================================
    // Bench 2 — warm dispatch: send0(E, SEL_VTABLE). Returns the vt eid
    // (immediate). Exercises vtableEidOf + mcache hit + objectVtableImpl.
    // Zero-alloc after the first auto-promote of E (one struct, then stable).
    // ====================================================================
    {
        var sink: u64 = 0;
        var w: u64 = 0;
        while (w < 10_000) : (w += 1) sink +%= send_mod.send0(ref_mod.mkEid(E), kernel_eids.SEL_VTABLE);
        const t0 = nowNs();
        var i: u64 = 0;
        while (i < iters) : (i += 1) sink +%= send_mod.send0(ref_mod.mkEid(E), kernel_eids.SEL_VTABLE);
        const t1 = nowNs();
        std.mem.doNotOptimizeAway(&sink);

        // Native baseline: a direct indirect call returning a constant.
        const Fn = struct {
            fn vt(x: u64) callconv(.c) u64 {
                return kernel_eids.OBJECT_VT ^ (x & 1); // depends on arg → not foldable
            }
        };
        // Force a runtime indirect call (else the optimizer inlines + folds the
        // whole loop to a constant, reporting a meaningless 0 ns/op baseline).
        var fp_slot: *const fn (u64) callconv(.c) u64 = &Fn.vt;
        std.mem.doNotOptimizeAway(&fp_slot);
        const fp = fp_slot;
        var nsink: u64 = 0;
        const b0 = nowNs();
        i = 0;
        while (i < iters) : (i += 1) nsink +%= fp(E);
        const b1 = nowNs();
        std.mem.doNotOptimizeAway(&nsink);
        report("warm dispatch (send0 vtable)", iters, t1 - t0, b1 - b0);
    }

    var tail: [96]u8 = undefined;
    _ = std.fs.File.stdout().write(std.fmt.bufPrint(&tail, "peak bytes: {d} / {d}\n", .{ fba.end_index, ARENA_BYTES }) catch "") catch {};
}
