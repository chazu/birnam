//! birnam → C codegen (mode-1 backend, docs/10a) — the *seed*.
//!
//! Lowers a code-cell to a C function that orchestrates the birnam runtime "as a
//! library": it takes/returns `Ref`s (u64) and performs work by calling the
//! runtime's C-ABI send entry points (`birnam_send0/1/2`, `send.zig`). The
//! generated C is fed to `jit.Jit` (libtcc) for live mode 1, or to `zig cc` for
//! the mode-3 ship; the resulting fn-ptr is installed as a method closure via the
//! relink-on-load spine (W1).
//!
//! This first cut emits the two simplest method shapes — a unary self-send and a
//! one-arg keyword self-send — enough to prove the codegen → JIT → real-dispatch
//! loop end-to-end. Lowering real bytecode bodies (locals, control flow, literals)
//! builds on this same "emit C that calls the runtime" foundation.

const std = @import("std");

/// The runtime entry points generated bodies may reference. Emitted as a C
/// prelude; satisfied at JIT time via `Jit.addSymbol("birnam_send0", &send.birnamSend0)`
/// (etc.) or by the linker for the ship path.
pub const RUNTIME_PRELUDE =
    \\typedef unsigned long Ref;
    \\extern Ref birnam_send0(Ref recv, Ref sel);
    \\extern Ref birnam_send1(Ref recv, Ref sel, Ref a1);
    \\extern Ref birnam_send2(Ref recv, Ref sel, Ref a1, Ref a2);
    \\
;

/// Emit `Ref <fn_name>(Ref recv) { return birnam_send0(recv, <sel_eid>); }` — a
/// method whose body is a single unary self-send (e.g. `^ self squared`).
/// Caller owns the returned null-terminated C source.
pub fn emitUnarySend(
    allocator: std.mem.Allocator,
    fn_name: []const u8,
    sel_eid: u64,
) ![:0]u8 {
    return std.fmt.allocPrintSentinel(allocator, "{s}Ref {s}(Ref recv) {{ return birnam_send0(recv, {d}); }}\n", .{ RUNTIME_PRELUDE, fn_name, sel_eid }, 0);
}

/// Emit `Ref <fn_name>(Ref recv, Ref a1) { return birnam_send1(recv, <sel_eid>, a1); }`
/// — a method body that is a single one-arg keyword self-send (e.g. `^ self + x`).
pub fn emitKeywordSend1(
    allocator: std.mem.Allocator,
    fn_name: []const u8,
    sel_eid: u64,
) ![:0]u8 {
    return std.fmt.allocPrintSentinel(allocator, "{s}Ref {s}(Ref recv, Ref a1) {{ return birnam_send1(recv, {d}, a1); }}\n", .{ RUNTIME_PRELUDE, fn_name, sel_eid }, 0);
}

test "codegen: unary-send C contains the prelude, name, and selector" {
    const a = std.testing.allocator;
    const src = try emitUnarySend(a, "m_squared", 12345);
    defer a.free(src);
    try std.testing.expect(std.mem.indexOf(u8, src, "birnam_send0") != null);
    try std.testing.expect(std.mem.indexOf(u8, src, "Ref m_squared(Ref recv)") != null);
    try std.testing.expect(std.mem.indexOf(u8, src, "12345") != null);
}
