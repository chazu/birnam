//! Phase 11.9 — tail-call probe.
//!
//! ADR-11 #4: VM dispatch lowers each opcode handler to a tail-call into
//! the next handler via `@call(.always_tail, ...)`. The probe runs a
//! deeply recursive two-handler chain that, without tail-call elimination,
//! would overflow the C stack long before completing.
//!
//! CI gate: if `.always_tail` fails to lower to `jmp` on the target arch,
//! this test crashes with stack overflow. Failure = port-blocker per
//! ADR-11 addendum #7 (no fallback dispatch).
//!
//! Depth chosen well above any plausible C stack size (16 MB / 32 bytes
//! per frame = ~500k frames). 1M iterations is comfortably past that.

const std = @import("std");
const testing = std.testing;

const PROBE_DEPTH: u32 = 1_000_000;

const ProbeState = struct {
    count: u32 = 0,
};

const Handler = *const fn (*ProbeState) callconv(.c) void;

fn handlerA(s: *ProbeState) callconv(.c) void {
    s.count += 1;
    if (s.count >= PROBE_DEPTH) return;
    @call(.always_tail, handlerB, .{s});
}

fn handlerB(s: *ProbeState) callconv(.c) void {
    s.count += 1;
    if (s.count >= PROBE_DEPTH) return;
    @call(.always_tail, handlerA, .{s});
}

test "tail-call probe: 1M-depth handler chain does not overflow stack" {
    var s = ProbeState{};
    handlerA(&s);
    try testing.expectEqual(PROBE_DEPTH, s.count);
}

test "tail-call probe: dispatch table indirection still tail-calls" {
    // Stronger check: dispatch through a runtime-indexed function-pointer
    // table, mirroring the VM's actual dispatch shape. If lowering
    // degrades through the indirection, this overflows.
    const table: [2]Handler = .{ handlerA, handlerB };
    var s = ProbeState{};
    table[0](&s);
    try testing.expectEqual(PROBE_DEPTH, s.count);
}
