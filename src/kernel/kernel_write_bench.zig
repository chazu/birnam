//! Fact-WRITE microbenchmark — the memory-growing class, fenced carefully.
//!
//! Writes are the dangerous benchmark: every assert/retract path-copies 5 HAMT
//! indexes + appends never-freed history, so a naive high-iteration write loop
//! grows memory without bound (this is what crashed the machine in the first
//! benchmarking attempt, and is ADR-perf-1 finding #1).
//!
//! Safety mechanisms here:
//!   1. CappedAllocator — a real allocator (forwards free to the child, so prune
//!      reclamation works) WITH a hard outstanding-bytes ceiling. Past the
//!      ceiling, alloc returns null → the store gets error.OutOfMemory and the
//!      bench aborts CLEAN instead of taking the machine down. (FixedBuffer
//!      can't be used here: it's bump-only and can't free, so prune wouldn't
//!      reclaim.) macOS ignores ulimit -v, so this in-process cap is the wall.
//!   2. Every bench prints peak outstanding bytes. Start with a SMALL count,
//!      watch the peak, scale up only while peak stays well under the ceiling.
//!   3. One bench per process invocation (argv mode) on a fresh boot, so peaks
//!      never compound across benches.
//!
//! Run (always under timeout):
//!   zig build write-bench
//!   timeout 30 ./zig-out/bin/kernel_write_bench distinct 10000
//!   timeout 30 ./zig-out/bin/kernel_write_bench churn    10000
//!   timeout 30 ./zig-out/bin/kernel_write_bench assign   10000
//!   timeout 30 ./zig-out/bin/kernel_write_bench churn-pruned 100000
//!
//! Modes:
//!   distinct       — assert [e_i, A, v] for fresh e_i. "Adding data" (index growth).
//!   churn          — assert [E, A, i] repeatedly (cardinality-one supersede).
//!                    "Mutating one slot"; history grows. No reclaim.
//!   assign         — retract old + assert new on [E, A]. The `:=` pattern
//!                    (lang's mutateBinding does exactly this). 2 store ops/iter.
//!   churn-pruned   — like churn but compact() every PRUNE_EVERY iters to prove
//!                    history reclamation keeps memory bounded across a long run.

const std = @import("std");
const ref_mod = @import("ref.zig");
const fact_store_mod = @import("fact_store.zig");
const object_table_mod = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const siphon = @import("siphon.zig");
const kernel_eids = @import("kernel_eids.zig");

const Ref = ref_mod.Ref;

const CEILING: usize = 512 * 1024 * 1024; // hard wall on outstanding bytes
const PRUNE_EVERY: u64 = 10_000;
const A_X: u64 = 250;
const E_BASE: u64 = 1_000_000;

// -- Capped allocator: real free + hard outstanding-bytes ceiling ----------

const Capped = struct {
    child: std.mem.Allocator,
    ceiling: usize,
    outstanding: usize = 0,
    peak: usize = 0,
    hit_cap: bool = false,

    fn alloc(ctx: *anyopaque, len: usize, a: std.mem.Alignment, ra: usize) ?[*]u8 {
        const self: *Capped = @ptrCast(@alignCast(ctx));
        if (self.outstanding + len > self.ceiling) {
            self.hit_cap = true;
            return null;
        }
        const p = self.child.rawAlloc(len, a, ra) orelse return null;
        self.outstanding += len;
        if (self.outstanding > self.peak) self.peak = self.outstanding;
        return p;
    }
    fn resize(ctx: *anyopaque, mem: []u8, a: std.mem.Alignment, new_len: usize, ra: usize) bool {
        const self: *Capped = @ptrCast(@alignCast(ctx));
        if (new_len > mem.len and self.outstanding + (new_len - mem.len) > self.ceiling) return false;
        if (!self.child.rawResize(mem, a, new_len, ra)) return false;
        self.outstanding = self.outstanding - mem.len + new_len;
        if (self.outstanding > self.peak) self.peak = self.outstanding;
        return true;
    }
    fn remap(ctx: *anyopaque, mem: []u8, a: std.mem.Alignment, new_len: usize, ra: usize) ?[*]u8 {
        const self: *Capped = @ptrCast(@alignCast(ctx));
        if (new_len > mem.len and self.outstanding + (new_len - mem.len) > self.ceiling) {
            self.hit_cap = true;
            return null;
        }
        const p = self.child.rawRemap(mem, a, new_len, ra) orelse return null;
        self.outstanding = self.outstanding - mem.len + new_len;
        if (self.outstanding > self.peak) self.peak = self.outstanding;
        return p;
    }
    fn free(ctx: *anyopaque, mem: []u8, a: std.mem.Alignment, ra: usize) void {
        const self: *Capped = @ptrCast(@alignCast(ctx));
        self.child.rawFree(mem, a, ra);
        self.outstanding -= mem.len;
    }
    fn allocator(self: *Capped) std.mem.Allocator {
        return .{ .ptr = self, .vtable = &.{ .alloc = alloc, .resize = resize, .remap = remap, .free = free } };
    }
};

fn nowNs() i128 {
    return std.time.nanoTimestamp();
}

/// compact() keep-predicate: retain every current fact (history still collapses
/// to current-only, which is the reclamation we want to measure).
fn keepAll(_: void, _: u64, _: u64, _: u64, _: Ref) bool {
    return true;
}

fn out(comptime fmt: []const u8, args: anytype) void {
    var buf: [256]u8 = undefined;
    _ = std.fs.File.stdout().write(std.fmt.bufPrint(&buf, fmt, args) catch return) catch {};
}

fn report(cap: *Capped, name: []const u8, ops: u64, ns: i128) void {
    const per = @as(f64, @floatFromInt(ns)) / @as(f64, @floatFromInt(ops));
    out("{s:<24} {d:>9.1} ns/op   peak {d:>6.1} MiB / {d} MiB{s}\n", .{
        name, per, @as(f64, @floatFromInt(cap.peak)) / (1024.0 * 1024.0),
        cap.ceiling / (1024 * 1024), if (cap.hit_cap) "  [HIT CAP — aborted]" else "",
    });
}

pub fn main() !void {
    var mode: []const u8 = "distinct";
    var count: u64 = 10_000;
    {
        var args = std.process.args();
        _ = args.next();
        if (args.next()) |m| mode = m;
        if (args.next()) |c| count = std.fmt.parseInt(u64, c, 10) catch 10_000;
    }
    if (count > 10_000_000) count = 10_000_000; // hard count cap too

    // Child = GeneralPurposeAllocator — what the real shell/aot binaries use, so
    // the numbers reflect production. (page_allocator would syscall per node and
    // measure mmap overhead, not the fact store.) Capped wraps it for the ceiling.
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    var cap = Capped{ .child = gpa.allocator(), .ceiling = CEILING };
    const a = cap.allocator();

    var store = fact_store_mod.FactStore.init(a);
    // Match production (shell/aot): route index nodes to a freeable epoch arena
    // so compact() actually reclaims the old index instead of leaking it. Without
    // this, compact rebuilds but the old HAMT roots have no deinit → churn-pruned
    // grows unbounded (observed: hit the 512 MiB cap at ~100k).
    try store.enableIndexArena();
    var table = object_table_mod.ObjectTable.init(a);
    var interner = interner_mod.Interner.init(a);
    _ = try siphon.boot(a, &store, &table, &interner);

    out("write_bench mode={s} count={d} ceiling={d} MiB\n", .{ mode, count, CEILING / (1024 * 1024) });

    const M = kernel_eids.M_USER;
    const v = ref_mod.mkSmallInt(7).?;

    if (std.mem.eql(u8, mode, "distinct")) {
        const t0 = nowNs();
        var i: u64 = 0;
        while (i < count) : (i += 1) {
            _ = store.assert(E_BASE + i, A_X, v, M) catch break;
            if (cap.hit_cap) break;
        }
        const t1 = nowNs();
        report(&cap, "distinct assert", i, t1 - t0);
    } else if (std.mem.eql(u8, mode, "churn")) {
        const t0 = nowNs();
        var i: u64 = 0;
        while (i < count) : (i += 1) {
            _ = store.assert(E_BASE, A_X, ref_mod.mkSmallInt(@intCast(i & 0x3FFFFFFF)).?, M) catch break;
            if (cap.hit_cap) break;
        }
        const t1 = nowNs();
        report(&cap, "churn assert (1 slot)", i, t1 - t0);
    } else if (std.mem.eql(u8, mode, "assign")) {
        // The `:=` pattern: retract current value, assert new. mutateBinding
        // does exactly this. Seed an initial value first.
        _ = try store.assert(E_BASE, A_X, ref_mod.mkSmallInt(0).?, M);
        const t0 = nowNs();
        var i: u64 = 0;
        var prev = ref_mod.mkSmallInt(0).?;
        while (i < count) : (i += 1) {
            const next = ref_mod.mkSmallInt(@intCast((i + 1) & 0x3FFFFFFF)).?;
            _ = store.retract(E_BASE, A_X, prev, M) catch break;
            _ = store.assert(E_BASE, A_X, next, M) catch break;
            prev = next;
            if (cap.hit_cap) break;
        }
        const t1 = nowNs();
        report(&cap, "assign (retract+assert)", i, t1 - t0);
    } else if (std.mem.eql(u8, mode, "churn-pruned")) {
        // Same as churn but compact() periodically to collapse history and prove
        // memory stays bounded across a long mutation run.
        const t0 = nowNs();
        var i: u64 = 0;
        while (i < count) : (i += 1) {
            _ = store.assert(E_BASE, A_X, ref_mod.mkSmallInt(@intCast(i & 0x3FFFFFFF)).?, M) catch break;
            if (cap.hit_cap) break;
            if (i != 0 and i % PRUNE_EVERY == 0) {
                // Keep all current facts; compact collapses history to current-only.
                _ = store.compact(a, {}, keepAll) catch {};
            }
        }
        const t1 = nowNs();
        report(&cap, "churn+prune (1 slot)", i, t1 - t0);
    } else {
        out("unknown mode '{s}' (distinct|churn|assign|churn-pruned)\n", .{mode});
        return;
    }
}
