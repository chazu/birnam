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
const fact_store = @import("fact_store.zig");
const strings = @import("strings.zig");
const boxed = @import("boxed.zig");

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
    \\extern int birnam_is_falsey(Ref v);
    \\extern int birnam_is_error(Ref v);
    \\extern Ref birnam_make_block(Ref method_eid, Ref self, Ref *caps, unsigned long n);
    \\extern Ref birnam_invoke_block(Ref block, Ref *args, unsigned long n);
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

/// The capture spec of the block method at `pool[idx]` (which parent locals it
/// captures, in order). Empty when there is no store, the pool entry is not an
/// eid, or the method has no `:method-captures-ptr` fact (captureless block).
fn blockSpecs(store: ?*const fact_store.FactStore, pool: []const Ref, idx: usize) []const vm.CaptureSpec {
    const s = store orelse return &.{};
    if (idx >= pool.len or !ref_mod.isEid(pool[idx])) return &.{};
    const spec_ref = s.currentValue(ref_mod.untagEid(pool[idx]), kernel_eids.ATTR_METHOD_CAPTURES_PTR) orelse return &.{};
    return vm.captureSpecsFromRef(spec_ref);
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
/// **Supported:** pushes, locals, pop, self, true/false/nil, SEND_0/1/N, RETURN,
/// control flow (JUMP/BRANCH → goto/labels), and **blocks** (MAKE_BLOCK creates a
/// block closure capturing the named parent locals; INVOKE_BLOCK calls one) — the
/// block BODY stays interpreted, only its creation/invocation is lowered. Only
/// `RETURN_FROM_METHOD` (a block's `^` non-local return) returns
/// `Error.UnsupportedOpcode`, so the caller declines + keeps the interpreter for
/// methods using it — incremental rollout, never miscompiles.
/// `store` (optional) lets MAKE_BLOCK read a block method's `:method-captures-ptr`
/// spec to know which parent locals to capture; pass the live store for the JIT
/// path. With a null store, a MAKE_BLOCK is lowered as captureless.
pub fn emitMethod(
    allocator: std.mem.Allocator,
    fn_name: []const u8,
    code: []const u8,
    pool: []const Ref,
    store: ?*const fact_store.FactStore,
) Error![:0]u8 {
    var b = std.ArrayList(u8){};
    errdefer b.deinit(allocator);
    const w = b.writer(allocator);
    try w.writeAll(RUNTIME_PRELUDE);
    try emitBodyTo(allocator, w, fn_name, code, pool, store);
    return b.toOwnedSliceSentinel(allocator, 0);
}

/// Emit just the C function (no prelude) for one method to `w`. Shared by
/// `emitMethod` (one method + its own prelude) and `emitUnit` (many methods over
/// a single shared prelude — the mode-3 ship path).
fn emitBodyTo(
    allocator: std.mem.Allocator,
    w: anytype,
    fn_name: []const u8,
    code: []const u8,
    pool: []const Ref,
    store: ?*const fact_store.FactStore,
) Error!void {
    // Pre-scan: max local slot (sizes L[]) + the set of jump-TARGET pcs (where C
    // labels must be emitted). A target = (pc + 3) + i16-offset, matching the VM's
    // `applyJump(pc + 3, off)` for JUMP / BRANCH_IF_FALSE / BRANCH_IF_ERROR.
    var max_local: usize = 0;
    var targets = std.AutoHashMap(usize, void).init(allocator);
    defer targets.deinit();
    {
        var pc: usize = 0;
        while (pc < code.len) {
            const op = std.meta.intToEnum(vm.Opcode, code[pc]) catch return Error.BadBytecode;
            switch (op) {
                .PUSH_LOCAL, .STORE_LOCAL => {
                    const slot = bytecode.readU8(code, pc + 1);
                    if (slot > max_local) max_local = slot;
                },
                .JUMP, .BRANCH_IF_FALSE, .BRANCH_IF_ERROR => {
                    const off = bytecode.readI16(code, pc + 1);
                    const target: usize = @intCast(@as(i64, @intCast(pc + 3)) + off);
                    targets.put(target, {}) catch return Error.OutOfMemory;
                },
                .MAKE_BLOCK => {
                    // A captured parent_slot may exceed the slots reached by
                    // PUSH/STORE_LOCAL (e.g. a param only captured, never used);
                    // grow L[] to cover it.
                    for (blockSpecs(store, pool, bytecode.readU16(code, pc + 1))) |sp| {
                        if (sp.parent_slot > max_local) max_local = sp.parent_slot;
                    }
                },
                else => {},
            }
            pc += vm.instructionSize(op);
        }
    }

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
        // Emit a label for any instruction that is a jump target.
        if (targets.contains(pc)) try w.print("L{d}: ;\n", .{pc});
        const op = std.meta.intToEnum(vm.Opcode, code[pc]) catch return Error.BadBytecode;
        switch (op) {
            .JUMP => {
                const off = bytecode.readI16(code, pc + 1);
                const t: usize = @intCast(@as(i64, @intCast(pc + 3)) + off);
                try w.print("  goto L{d};\n", .{t});
            },
            .BRANCH_IF_FALSE => {
                const off = bytecode.readI16(code, pc + 1);
                const t: usize = @intCast(@as(i64, @intCast(pc + 3)) + off);
                try w.print("  {{ Ref _v = S[--sp]; if (birnam_is_falsey(_v)) goto L{d}; }}\n", .{t});
            },
            .BRANCH_IF_ERROR => {
                const off = bytecode.readI16(code, pc + 1);
                const t: usize = @intCast(@as(i64, @intCast(pc + 3)) + off);
                // Peek TOS: error → leave it + jump; else pop and fall through.
                try w.print("  if (birnam_is_error(S[sp-1])) goto L{d}; else sp--;\n", .{t});
            },
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
            .MAKE_BLOCK => {
                const idx = bytecode.readU16(code, pc + 1);
                if (idx >= pool.len or !ref_mod.isEid(pool[idx])) return Error.BadBytecode;
                const bm = ref_mod.untagEid(pool[idx]);
                const specs = blockSpecs(store, pool, idx);
                if (specs.len == 0) {
                    try w.print("  S[sp++] = birnam_make_block({d}UL, recv, 0, 0);\n", .{bm});
                } else {
                    // Capture the named parent locals (in spec order) into a temp.
                    try w.writeAll("  { Ref _c[] = {");
                    for (specs, 0..) |sp, i| {
                        if (i != 0) try w.writeAll(",");
                        try w.print(" L[{d}]", .{sp.parent_slot});
                    }
                    try w.print(" }}; S[sp++] = birnam_make_block({d}UL, recv, _c, {d}); }}\n", .{ bm, specs.len });
                }
            },
            .INVOKE_BLOCK => {
                const n = bytecode.readU8(code, pc + 1);
                // block at S[sp-n-1]; args at S[sp-n .. sp].
                try w.print("  {{ Ref _r = birnam_invoke_block(S[sp-{d}], &S[sp-{d}], {d}UL); sp -= {d}; S[sp++] = _r; }}\n", .{ n + 1, n, n, n + 1 });
            },
            // A block's `^` non-local return into a JIT'd (frameless) method is
            // not lowered — decline so the caller keeps the interpreter.
            .RETURN_FROM_METHOD => return Error.UnsupportedOpcode,
        }
        pc += vm.instructionSize(op);
    }

    // A branch can target one-past-the-end (jump-to-end); land it here.
    if (targets.contains(code.len)) try w.print("L{d}: ;\n", .{code.len});
    // A body that falls off the end returns nil (the interpreter's implicit ^self
    // is handled by the compiler emitting a trailing RETURN; this is a safety net).
    try w.print("  return {d}UL;\n}}\n", .{ref_mod.NIL});
}

/// Mode-3 ship codegen: emit a single C translation unit (one shared prelude +
/// one `Ref bm_<eid>(...)` per method) for the given method eids. Methods whose
/// bytecode the codegen can't lower are skipped and reported in `skipped`. The
/// unit is compiled by `zig cc -O2` into a frozen dylib (freeze_native.zig) and
/// loaded back — the same codegen as the libtcc live path, a different compiler.
pub const Unit = struct { c_src: [:0]u8, emitted: []u64, skipped: usize };

pub fn emitUnit(
    allocator: std.mem.Allocator,
    store: *const fact_store.FactStore,
    eids: []const u64,
) Error!Unit {
    var b = std.ArrayList(u8){};
    errdefer b.deinit(allocator);
    var emitted = std.ArrayList(u64){};
    errdefer emitted.deinit(allocator);
    const w = b.writer(allocator);
    try w.writeAll(RUNTIME_PRELUDE);

    var skipped: usize = 0;
    for (eids) |eid| {
        const bc = store.currentValue(eid, kernel_eids.ATTR_BYTECODE_STREAM) orelse {
            skipped += 1;
            continue;
        };
        if (!strings.isString(bc)) {
            skipped += 1;
            continue;
        }
        var pool = std.ArrayList(Ref){};
        defer pool.deinit(allocator);
        if (store.currentValue(eid, kernel_eids.ATTR_CONSTANT_POOL)) |pr| {
            if (boxed.isRrbRef(pr)) {
                const rrb = boxed.untagRrbRef(pr);
                for (0..rrb.count()) |i| try pool.append(allocator, rrb.nth(i));
            }
        }
        var name_buf: [32]u8 = undefined;
        const name = std.fmt.bufPrint(&name_buf, "bm_{d}", .{eid}) catch unreachable;
        // Emit to a temp first — a decline can leave a partial function, so only
        // append a method to the unit once its whole body emitted cleanly.
        var tmp = std.ArrayList(u8){};
        defer tmp.deinit(allocator);
        emitBodyTo(allocator, tmp.writer(allocator), name, strings.bytes(bc), pool.items, store) catch |e| switch (e) {
            error.UnsupportedOpcode => {
                skipped += 1;
                continue;
            },
            else => return e,
        };
        try w.writeAll(tmp.items);
        try emitted.append(allocator, eid);
    }
    return .{
        .c_src = try b.toOwnedSliceSentinel(allocator, 0),
        .emitted = try emitted.toOwnedSlice(allocator),
        .skipped = skipped,
    };
}

test "codegen: unary-send C contains the prelude, name, and selector" {
    const a = std.testing.allocator;
    const src = try emitUnarySend(a, "m_squared", 12345);
    defer a.free(src);
    try std.testing.expect(std.mem.indexOf(u8, src, "birnam_send0") != null);
    try std.testing.expect(std.mem.indexOf(u8, src, "Ref m_squared(Ref recv)") != null);
    try std.testing.expect(std.mem.indexOf(u8, src, "12345") != null);
}

test "codegen: control-flow opcodes lower to labels + goto + predicate calls" {
    const a = std.testing.allocator;
    // Hand-built body exercising every branch op (the compiler currently emits
    // BRANCH_IF_ERROR; JUMP/BRANCH_IF_FALSE are lowered for completeness).
    // Targets land on instruction boundaries (= where labels are emitted):
    //   0: LOAD_SELF                 (1)
    //   1: BRANCH_IF_FALSE +3        (3) after=4 -> target 7
    //   4: JUMP +4                   (3) after=7 -> target 11 (end)
    //   7: LOAD_CONST_NIL            (1)
    //   8: BRANCH_IF_ERROR +0        (3) after=11 -> target 11 (end)
    //  11: RETURN                    (1)
    const code = [_]u8{
        @intFromEnum(vm.Opcode.LOAD_SELF),
        @intFromEnum(vm.Opcode.BRANCH_IF_FALSE), 3, 0,
        @intFromEnum(vm.Opcode.JUMP),            4, 0,
        @intFromEnum(vm.Opcode.LOAD_CONST_NIL),
        @intFromEnum(vm.Opcode.BRANCH_IF_ERROR), 0, 0,
        @intFromEnum(vm.Opcode.RETURN),
    };
    const src = try emitMethod(a, "cf", &code, &.{}, null);
    defer a.free(src);
    try std.testing.expect(std.mem.indexOf(u8, src, "L7:") != null); // branch-if-false target
    try std.testing.expect(std.mem.indexOf(u8, src, "goto L11;") != null); // jump-to-end
    try std.testing.expect(std.mem.indexOf(u8, src, "birnam_is_falsey") != null);
    try std.testing.expect(std.mem.indexOf(u8, src, "birnam_is_error") != null);
    try std.testing.expect(std.mem.indexOf(u8, src, "L11:") != null); // jump-to-end label
}

test "codegen: a captureless MAKE_BLOCK lowers to birnam_make_block" {
    const a = std.testing.allocator;
    // pool[0] = mkEid(7) (the block method); null store → captureless.
    const code = [_]u8{ @intFromEnum(vm.Opcode.MAKE_BLOCK), 0, 0, @intFromEnum(vm.Opcode.RETURN) };
    const src = try emitMethod(a, "b", &code, &.{ref_mod.mkEid(7)}, null);
    defer a.free(src);
    try std.testing.expect(std.mem.indexOf(u8, src, "birnam_make_block(7UL, recv, 0, 0)") != null);
}

test "codegen: declines a block's non-local return (RETURN_FROM_METHOD)" {
    const a = std.testing.allocator;
    const code = [_]u8{ @intFromEnum(vm.Opcode.RETURN_FROM_METHOD), 0 };
    try std.testing.expectError(Error.UnsupportedOpcode, emitMethod(a, "r", &code, &.{}, null));
}
