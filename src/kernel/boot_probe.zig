//! W7.0 boot-path instrumentation — THROWAWAY (docs/31 §W7.0).
//!
//! Tallies bootstrap fact-writes by attribute and detects facts that are
//! *written then read back before module load* — the only ordering-sensitive
//! writes, i.e. docs/30 §8 bucket 2 (the metaobject graph). Everything else is
//! deferrable. Wire via `FactStore.debug_on_write` / `debug_on_read`; call
//! `markMount()` the instant `siphon.boot` returns (that is the module-load
//! boundary). Self-contained (page_allocator, std-only) so it couples nothing —
//! `fact_store.zig` only gains two nullable fn-ptr fields, no import of this.
//!
//! Delete this file + the two `FactStore` fields + their two call sites once
//! W7.0 has produced its numbers.

const std = @import("std");

const alloc = std.heap.page_allocator;

const EA = struct { e: u64, a: u64 };
const Entry = struct { a: u64, n: u32 };

var enabled: bool = false;
var mounted: bool = false;
var inited: bool = false;
var total_writes: u64 = 0;
var total_rbm: u64 = 0;
var write_counts: std.AutoHashMap(u64, u32) = undefined;
var written: std.AutoHashMap(EA, void) = undefined;
var rbm_counts: std.AutoHashMap(u64, u32) = undefined;

/// Arm the probe and clear prior state. Call before `siphon.boot`.
pub fn reset() void {
    if (inited) {
        write_counts.deinit();
        written.deinit();
        rbm_counts.deinit();
    }
    write_counts = std.AutoHashMap(u64, u32).init(alloc);
    written = std.AutoHashMap(EA, void).init(alloc);
    rbm_counts = std.AutoHashMap(u64, u32).init(alloc);
    inited = true;
    enabled = true;
    mounted = false;
    total_writes = 0;
    total_rbm = 0;
}

/// Wired into `FactStore.debug_on_write` — fires on every `assert`.
pub fn onWrite(e: u64, a: u64) void {
    if (!enabled) return;
    total_writes += 1;
    bump(&write_counts, a);
    written.put(.{ .e = e, .a = a }, {}) catch {};
}

/// Wired into `FactStore.debug_on_read` — fires on every `currentValue`.
/// A read that HITS a fact written earlier in this boot (and before mount) is
/// the ordering-sensitive signal we are hunting.
pub fn onRead(e: u64, a: u64) void {
    if (!enabled or mounted) return;
    if (written.contains(.{ .e = e, .a = a })) {
        total_rbm += 1;
        bump(&rbm_counts, a);
    }
}

/// The module-load boundary: reads after this no longer count as pre-mount.
pub fn markMount() void {
    mounted = true;
}

/// Disarm (e.g. after `dump`) so later store activity is not tallied.
pub fn stop() void {
    enabled = false;
}

fn bump(m: *std.AutoHashMap(u64, u32), a: u64) void {
    const gop = m.getOrPut(a) catch return;
    if (!gop.found_existing) gop.value_ptr.* = 0;
    gop.value_ptr.* += 1;
}

pub fn writeCount(a: u64) u32 {
    return write_counts.get(a) orelse 0;
}
pub fn rbmCount(a: u64) u32 {
    return rbm_counts.get(a) orelse 0;
}
pub fn totalWrites() u64 {
    return total_writes;
}
pub fn totalRbm() u64 {
    return total_rbm;
}
pub fn distinctRbmAttrs() u32 {
    return rbm_counts.count();
}

fn gtByN(_: void, x: Entry, y: Entry) bool {
    return x.n > y.n;
}

fn dumpMap(title: []const u8, m: *std.AutoHashMap(u64, u32)) void {
    var entries = std.ArrayList(Entry){};
    defer entries.deinit(alloc);
    var it = m.iterator();
    while (it.next()) |e| entries.append(alloc, .{ .a = e.key_ptr.*, .n = e.value_ptr.* }) catch {};
    std.mem.sort(Entry, entries.items, {}, gtByN);
    std.debug.print("  {s}:\n", .{title});
    for (entries.items) |e| std.debug.print("    attr {d:>3} : {d}\n", .{ e.a, e.n });
}

pub fn dump() void {
    std.debug.print("\n=== W7.0 BOOT PROBE (docs/31 §W7.0) ===\n", .{});
    std.debug.print("total fact-writes during boot: {d}\n", .{total_writes});
    dumpMap("writes by attr (desc)", &write_counts);
    std.debug.print("pre-mount reads that HIT a boot-written fact: {d} (across {d} attrs)\n", .{ total_rbm, rbm_counts.count() });
    dumpMap("READ-BEFORE-MOUNT by attr (desc) = the ordering-sensitive set", &rbm_counts);
    std.debug.print("========================================\n", .{});
}
