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
const bytecode = @import("bytecode.zig");
const vm = @import("vm.zig");
const ref_mod = @import("ref.zig");
const kernel_eids = @import("kernel_eids.zig");

const Ref = ref_mod.Ref;

/// The runtime entry points generated bodies may reference. Emitted as a C
/// prelude; satisfied at JIT time via `Jit.addSymbol("birnam_send0", &send.birnamSend0)`
/// (etc.) or by the linker for the ship path.
pub const RUNTIME_PRELUDE =
    \\typedef unsigned long Ref;
    \\extern Ref birnam_send0(Ref recv, Ref sel);
    \\extern Ref birnam_send1(Ref recv, Ref sel, Ref a1);
    \\extern Ref birnam_send2(Ref recv, Ref sel, Ref a1, Ref a2);
    \\extern Ref birnam_sendN(Ref recv, Ref sel, Ref *args, unsigned long n);
    \\
;

pub const Error = error{ UnsupportedOpcode, BadBytecode, OutOfMemory };

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

/// Lower a compiled **bytecode method body** to a C function. Emits a stack
/// machine: `Ref S[]` operand stack + `Ref L[]` locals (args copied in), one
/// translated statement per opcode. The generated body calls the runtime send
/// entry points (`birnam_sendN` family) and embeds all compile-time-known Refs
/// (smallints, true/false/nil, constant-pool entries) as `unsigned long`
/// literals — so the only runtime dependency is the send path.
///
/// Signature of the emitted function: `Ref <fn_name>(Ref recv, Ref *args,
/// unsigned long n_args)` — matching how a method closure is invoked.
///
/// **Subset (this cut):** straight-line bodies — pushes, locals, pop, self,
/// true/false/nil, and SEND_0/1/N + RETURN. Control flow (JUMP/BRANCH), blocks
/// (MAKE_BLOCK/INVOKE_BLOCK), and non-local return return `Error.UnsupportedOpcode`
/// so the caller cleanly declines methods the codegen can't yet handle (and falls
/// back to the interpreter) — incremental rollout, never miscompiles.
pub fn emitMethod(
    allocator: std.mem.Allocator,
    fn_name: []const u8,
    code: []const u8,
    pool: []const Ref,
) Error![:0]u8 {
    // Pre-scan: max local slot referenced (sizes the L[] array). Args also live
    // in locals, so the array must hold at least n_args — handled at call time by
    // a generous floor.
    var max_local: usize = 0;
    {
        var pc: usize = 0;
        while (pc < code.len) {
            const op = std.meta.intToEnum(vm.Opcode, code[pc]) catch return Error.BadBytecode;
            switch (op) {
                .PUSH_LOCAL, .STORE_LOCAL => {
                    const slot = bytecode.readU8(code, pc + 1);
                    if (slot > max_local) max_local = slot;
                },
                else => {},
            }
            pc += vm.instructionSize(op);
        }
    }

    var b = std.ArrayList(u8){};
    errdefer b.deinit(allocator);
    const w = b.writer(allocator);

    try w.writeAll(RUNTIME_PRELUDE);
    // Stack + locals sized generously; a body pushes ≤1 per opcode so code.len is
    // a safe stack ceiling, and locals = max referenced slot + a 16-arg headroom.
    try w.print(
        \\Ref {s}(Ref recv, Ref *args, unsigned long n_args) {{
        \\  Ref S[{d}]; Ref L[{d}]; long sp = 0;
        \\  for (unsigned long i = 0; i < n_args; i++) L[i] = args[i];
        \\
    , .{ fn_name, code.len + 4, max_local + 16 });

    var pc: usize = 0;
    while (pc < code.len) {
        const op = std.meta.intToEnum(vm.Opcode, code[pc]) catch return Error.BadBytecode;
        switch (op) {
            .PUSH_CONST => {
                const idx = bytecode.readU16(code, pc + 1);
                if (idx >= pool.len) return Error.BadBytecode;
                try w.print("  S[sp++] = {d}UL;\n", .{pool[idx]});
            },
            .PUSH_SMALLINT_I32 => {
                const n = bytecode.readI32(code, pc + 1);
                const r = ref_mod.mkSmallInt(@as(i64, n)) orelse return Error.BadBytecode;
                try w.print("  S[sp++] = {d}UL;\n", .{r});
            },
            .PUSH_LOCAL => try w.print("  S[sp++] = L[{d}];\n", .{bytecode.readU8(code, pc + 1)}),
            .STORE_LOCAL => try w.print("  L[{d}] = S[--sp];\n", .{bytecode.readU8(code, pc + 1)}),
            .POP => try w.writeAll("  sp--;\n"),
            .LOAD_SELF => try w.writeAll("  S[sp++] = recv;\n"),
            .LOAD_CONST_TRUE => try w.print("  S[sp++] = {d}UL;\n", .{ref_mod.mkEid(kernel_eids.TRUE_EID)}),
            .LOAD_CONST_FALSE => try w.print("  S[sp++] = {d}UL;\n", .{ref_mod.mkEid(kernel_eids.FALSE_EID)}),
            .LOAD_CONST_NIL => try w.print("  S[sp++] = {d}UL;\n", .{ref_mod.NIL}),
            .SEND_0 => {
                const sel = bytecode.readU32(code, pc + 1);
                try w.print("  S[sp-1] = birnam_send0(S[sp-1], {d}UL);\n", .{sel});
            },
            .SEND_1 => {
                const sel = bytecode.readU32(code, pc + 1);
                try w.print("  S[sp-2] = birnam_send1(S[sp-2], {d}UL, S[sp-1]); sp--;\n", .{sel});
            },
            .SEND_N => {
                const sel = bytecode.readU32(code, pc + 1);
                const n = bytecode.readU8(code, pc + 5);
                // recv at S[sp-n-1]; args at S[sp-n .. sp].
                try w.print("  {{ Ref _r = birnam_sendN(S[sp-{d}], {d}UL, &S[sp-{d}], {d}UL); sp -= {d}; S[sp++] = _r; }}\n", .{ n + 1, sel, n, n, n + 1 });
            },
            .RETURN => try w.writeAll("  return S[sp-1];\n"),
            // Not yet lowered — decline so the caller keeps the interpreter.
            .JUMP, .BRANCH_IF_FALSE, .BRANCH_IF_ERROR, .MAKE_BLOCK, .INVOKE_BLOCK, .RETURN_FROM_METHOD => return Error.UnsupportedOpcode,
        }
        pc += vm.instructionSize(op);
    }

    // A body that falls off the end returns nil (the interpreter's implicit ^self
    // is handled by the compiler emitting a trailing RETURN; this is a safety net).
    try w.print("  return {d}UL;\n}}\n", .{ref_mod.NIL});

    return b.toOwnedSliceSentinel(allocator, 0);
}

test "codegen: unary-send C contains the prelude, name, and selector" {
    const a = std.testing.allocator;
    const src = try emitUnarySend(a, "m_squared", 12345);
    defer a.free(src);
    try std.testing.expect(std.mem.indexOf(u8, src, "birnam_send0") != null);
    try std.testing.expect(std.mem.indexOf(u8, src, "Ref m_squared(Ref recv)") != null);
    try std.testing.expect(std.mem.indexOf(u8, src, "12345") != null);
}
