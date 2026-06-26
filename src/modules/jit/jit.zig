//! The libtcc mode-1 backend as a LOADABLE BIRNAM MODULE (docs/10a). Nothing here
//! is "below" the IdApi: the compiler (libtcc + codegen + manager) lives entirely
//! in this dylib and reaches the kernel only through `abi` (the IdApi) — reading a
//! method's bytecode/pool/captures, installing/reverting the native closure, and
//! draining the hotness queue. The generated C calls the host's `birnam_*` runtime
//! entry points, resolved via `dlsym(RTLD_DEFAULT, …)` (they are `@export`ed from
//! the shell). Loaded like any other module; registers a maintenance hook so hot
//! methods auto-compile at the REPL's quiescent boundary.

const std = @import("std");
const abi = @import("module_abi");
const ke = @import("kernel_eids");
const c = @cImport({
    @cInclude("libtcc.h");
});

const Ref = abi.Ref;

var g_api: *const abi.IdApi = undefined;
var g_log: *const fn ([*:0]const u8) callconv(.c) void = undefined;

// -- Bytecode format (mirror of vm.zig / bytecode.zig — stable, version-gated) --

const Opcode = enum(u8) {
    PUSH_CONST = 0,
    PUSH_SMALLINT_I32 = 1,
    PUSH_LOCAL = 2,
    STORE_LOCAL = 3,
    POP = 4,
    LOAD_SELF = 5,
    LOAD_CONST_TRUE = 6,
    LOAD_CONST_FALSE = 7,
    LOAD_CONST_NIL = 8,
    SEND_0 = 9,
    SEND_1 = 10,
    SEND_N = 11,
    RETURN = 12,
    RETURN_FROM_METHOD = 13,
    JUMP = 14,
    BRANCH_IF_FALSE = 15,
    MAKE_BLOCK = 16,
    INVOKE_BLOCK = 17,
    BRANCH_IF_ERROR = 18,
};

fn operandSize(op: Opcode) usize {
    return switch (op) {
        .POP, .LOAD_SELF, .LOAD_CONST_TRUE, .LOAD_CONST_FALSE, .LOAD_CONST_NIL, .RETURN => 0,
        .PUSH_LOCAL, .STORE_LOCAL, .RETURN_FROM_METHOD => 1,
        .PUSH_CONST, .MAKE_BLOCK, .JUMP, .BRANCH_IF_FALSE, .BRANCH_IF_ERROR => 2,
        .INVOKE_BLOCK => 3,
        .PUSH_SMALLINT_I32 => 4,
        .SEND_0, .SEND_1 => 6,
        .SEND_N => 7,
    };
}
fn instrSize(op: Opcode) usize {
    return 1 + operandSize(op);
}
fn rdU8(b: []const u8, pc: usize) u8 {
    return b[pc];
}
fn rdU16(b: []const u8, pc: usize) u16 {
    return std.mem.readInt(u16, b[pc..][0..2], .little);
}
fn rdI16(b: []const u8, pc: usize) i16 {
    return std.mem.readInt(i16, b[pc..][0..2], .little);
}
fn rdU32(b: []const u8, pc: usize) u32 {
    return std.mem.readInt(u32, b[pc..][0..4], .little);
}
fn rdI32(b: []const u8, pc: usize) i32 {
    return std.mem.readInt(i32, b[pc..][0..4], .little);
}

// -- libtcc wrapper ----------------------------------------------------------

pub const Jit = struct {
    state: *c.TCCState,
    relocated: bool = false,

    fn init() ?Jit {
        const s = c.tcc_new() orelse return null;
        _ = c.tcc_set_output_type(s, c.TCC_OUTPUT_MEMORY);
        return .{ .state = s };
    }
    fn deinit(self: *Jit) void {
        c.tcc_delete(self.state);
    }
    fn addSymbol(self: *Jit, name: [*:0]const u8, ptr: *const anyopaque) bool {
        return c.tcc_add_symbol(self.state, name, ptr) >= 0;
    }
    fn compile(self: *Jit, src: [*:0]const u8) bool {
        return c.tcc_compile_string(self.state, src) != -1;
    }
    fn relocate(self: *Jit) bool {
        if (c.tcc_relocate(self.state) < 0) return false;
        self.relocated = true;
        return true;
    }
    fn getSym(self: *Jit, name: [*:0]const u8) ?*anyopaque {
        return c.tcc_get_symbol(self.state, name);
    }
};

/// Bind the host runtime entry points (resolved by dlsym — they're @exported from
/// the shell) so the generated C resolves.
fn bindRuntime(j: *Jit) bool {
    const names = [_][*:0]const u8{
        "birnam_send0",     "birnam_send1",      "birnam_send2",     "birnam_sendN",
        "birnam_is_falsey", "birnam_is_error",   "birnam_make_block", "birnam_invoke_block",
    };
    for (names) |n| {
        const sym = std.c.dlsym(@ptrFromInt(@as(usize, @bitCast(@as(isize, -2)))), n) orelse return false; // RTLD_DEFAULT
        if (!j.addSymbol(n, sym)) return false;
    }
    return true;
}

const PRELUDE =
    \\typedef unsigned long Ref;
    \\extern Ref birnam_send0(Ref,Ref);
    \\extern Ref birnam_send1(Ref,Ref,Ref);
    \\extern Ref birnam_send2(Ref,Ref,Ref,Ref);
    \\extern Ref birnam_sendN(Ref,Ref,Ref*,unsigned long);
    \\extern int birnam_is_falsey(Ref);
    \\extern int birnam_is_error(Ref);
    \\extern Ref birnam_make_block(Ref,Ref,Ref*,unsigned long);
    \\extern Ref birnam_invoke_block(Ref,Ref*,unsigned long);
    \\
;

const CodegenError = error{ Declined, Bad, OutOfMemory };

/// Lower a method's bytecode to a C function `<fn_name>(recv, args, n)`, writing
/// it (no prelude) to `w`. Reads :bytecode-stream / :constant-pool / capture specs
/// through the IdApi. Shared by `emitMethodOwned` (live JIT) + `emitUnit` (mode-3).
fn emitMethod(allocator: std.mem.Allocator, w: anytype, fn_name: []const u8, method_eid: u64) CodegenError!void {
    const api = g_api;
    const bc_ref = api.query_eavt(method_eid, ke.ATTR_BYTECODE_STREAM);
    if (!abi.isString(bc_ref)) return error.Bad;
    const code_ptr = api.string_ptr(bc_ref) orelse return error.Bad;
    const code = code_ptr[0..api.string_len(bc_ref)];

    // Constant pool (RRB) → local slice.
    var pool = std.ArrayList(Ref){};
    defer pool.deinit(allocator);
    const pool_ref = api.query_eavt(method_eid, ke.ATTR_CONSTANT_POOL);
    if (abi.isEid(pool_ref) == false and pool_ref != abi.NIL) {
        const n = api.rrb_len(pool_ref);
        var i: u64 = 0;
        while (i < n) : (i += 1) try pool.append(allocator, api.rrb_nth(pool_ref, i));
    }

    // Pre-scan: max local slot + jump targets.
    var max_local: usize = 0;
    var targets = std.AutoHashMap(usize, void).init(allocator);
    defer targets.deinit();
    {
        var pc: usize = 0;
        while (pc < code.len) {
            const op = std.meta.intToEnum(Opcode, code[pc]) catch return error.Bad;
            switch (op) {
                .PUSH_LOCAL, .STORE_LOCAL => {
                    const slot = rdU8(code, pc + 1);
                    if (slot > max_local) max_local = slot;
                },
                .JUMP, .BRANCH_IF_FALSE, .BRANCH_IF_ERROR => {
                    const t: usize = @intCast(@as(i64, @intCast(pc + 3)) + rdI16(code, pc + 1));
                    targets.put(t, {}) catch return error.OutOfMemory;
                },
                .MAKE_BLOCK => {
                    var sbuf: [256]u8 = undefined;
                    const idx = rdU16(code, pc + 1);
                    if (idx < pool.items.len and abi.isEid(pool.items[idx])) {
                        const np = api.method_capture_specs(abi.untagEid(pool.items[idx]), &sbuf, sbuf.len);
                        var k: usize = 0;
                        while (k < np) : (k += 1) {
                            if (sbuf[k * 2] > max_local) max_local = sbuf[k * 2];
                        }
                    }
                },
                else => {},
            }
            pc += instrSize(op);
        }
    }

    try w.print(
        \\Ref {s}(Ref recv, Ref *args, unsigned long n_args) {{
        \\  Ref S[{d}]; Ref L[{d}]; long sp = 0;
        \\  for (unsigned long i = 0; i < n_args; i++) L[i] = args[i];
        \\
    , .{ fn_name, code.len + 4, max_local + 16 });

    var pc: usize = 0;
    while (pc < code.len) {
        if (targets.contains(pc)) try w.print("L{d}: ;\n", .{pc});
        const op = std.meta.intToEnum(Opcode, code[pc]) catch return error.Bad;
        switch (op) {
            .PUSH_CONST => {
                const idx = rdU16(code, pc + 1);
                if (idx >= pool.items.len) return error.Bad;
                try w.print("  S[sp++] = {d}UL;\n", .{pool.items[idx]});
            },
            .PUSH_SMALLINT_I32 => {
                const r = abi.mkSmallInt(@as(i64, rdI32(code, pc + 1))) orelse return error.Bad;
                try w.print("  S[sp++] = {d}UL;\n", .{r});
            },
            .PUSH_LOCAL => try w.print("  S[sp++] = L[{d}];\n", .{rdU8(code, pc + 1)}),
            .STORE_LOCAL => try w.print("  L[{d}] = S[--sp];\n", .{rdU8(code, pc + 1)}),
            .POP => try w.writeAll("  sp--;\n"),
            .LOAD_SELF => try w.writeAll("  S[sp++] = recv;\n"),
            .LOAD_CONST_TRUE => try w.print("  S[sp++] = {d}UL;\n", .{abi.mkEid(ke.TRUE_EID)}),
            .LOAD_CONST_FALSE => try w.print("  S[sp++] = {d}UL;\n", .{abi.mkEid(ke.FALSE_EID)}),
            .LOAD_CONST_NIL => try w.print("  S[sp++] = {d}UL;\n", .{abi.NIL}),
            .SEND_0 => try w.print("  S[sp-1] = birnam_send0(S[sp-1], {d}UL);\n", .{rdU32(code, pc + 1)}),
            .SEND_1 => try w.print("  S[sp-2] = birnam_send1(S[sp-2], {d}UL, S[sp-1]); sp--;\n", .{rdU32(code, pc + 1)}),
            .SEND_N => {
                const n = rdU8(code, pc + 5);
                try w.print("  {{ Ref _r = birnam_sendN(S[sp-{d}], {d}UL, &S[sp-{d}], {d}UL); sp -= {d}; S[sp++] = _r; }}\n", .{ n + 1, rdU32(code, pc + 1), n, n, n + 1 });
            },
            .RETURN => try w.writeAll("  return S[sp-1];\n"),
            .JUMP => {
                const t: usize = @intCast(@as(i64, @intCast(pc + 3)) + rdI16(code, pc + 1));
                try w.print("  goto L{d};\n", .{t});
            },
            .BRANCH_IF_FALSE => {
                const t: usize = @intCast(@as(i64, @intCast(pc + 3)) + rdI16(code, pc + 1));
                try w.print("  {{ Ref _v = S[--sp]; if (birnam_is_falsey(_v)) goto L{d}; }}\n", .{t});
            },
            .BRANCH_IF_ERROR => {
                const t: usize = @intCast(@as(i64, @intCast(pc + 3)) + rdI16(code, pc + 1));
                try w.print("  if (birnam_is_error(S[sp-1])) goto L{d}; else sp--;\n", .{t});
            },
            .MAKE_BLOCK => {
                const idx = rdU16(code, pc + 1);
                if (idx >= pool.items.len or !abi.isEid(pool.items[idx])) return error.Bad;
                const bm = abi.untagEid(pool.items[idx]);
                var sbuf: [256]u8 = undefined;
                const np = api.method_capture_specs(bm, &sbuf, sbuf.len);
                if (np == 0) {
                    try w.print("  S[sp++] = birnam_make_block({d}UL, recv, 0, 0);\n", .{bm});
                } else {
                    try w.writeAll("  { Ref _c[] = {");
                    var k: usize = 0;
                    while (k < np) : (k += 1) {
                        if (k != 0) try w.writeAll(",");
                        try w.print(" L[{d}]", .{sbuf[k * 2]});
                    }
                    try w.print(" }}; S[sp++] = birnam_make_block({d}UL, recv, _c, {d}); }}\n", .{ bm, np });
                }
            },
            .INVOKE_BLOCK => {
                const n = rdU8(code, pc + 1);
                try w.print("  {{ Ref _r = birnam_invoke_block(S[sp-{d}], &S[sp-{d}], {d}UL); sp -= {d}; S[sp++] = _r; }}\n", .{ n + 1, n, n, n + 1 });
            },
            .RETURN_FROM_METHOD => return error.Declined,
        }
        pc += instrSize(op);
    }
    if (targets.contains(code.len)) try w.print("L{d}: ;\n", .{code.len});
    try w.print("  return {d}UL;\n}}\n", .{abi.NIL});
}

/// Live JIT: one method as a complete C translation unit (prelude + `m(...)`).
fn emitMethodOwned(allocator: std.mem.Allocator, method_eid: u64) CodegenError![:0]u8 {
    var b = std.ArrayList(u8){};
    errdefer b.deinit(allocator);
    const w = b.writer(allocator);
    try w.writeAll(PRELUDE);
    try emitMethod(allocator, w, "m", method_eid);
    return b.toOwnedSliceSentinel(allocator, 0);
}

/// Mode-3: many methods as ONE C unit (shared prelude + `bm_<eid>` each), declines
/// skipped. Returns the C source + the eids that lowered. Caller frees both.
const Unit = struct { c_src: [:0]u8, emitted: []u64 };
fn emitUnit(allocator: std.mem.Allocator, eids: []const u64) CodegenError!Unit {
    var b = std.ArrayList(u8){};
    errdefer b.deinit(allocator);
    var emitted = std.ArrayList(u64){};
    errdefer emitted.deinit(allocator);
    const w = b.writer(allocator);
    try w.writeAll(PRELUDE);
    for (eids) |eid| {
        var tmp = std.ArrayList(u8){};
        defer tmp.deinit(allocator);
        var name_buf: [32]u8 = undefined;
        const name = std.fmt.bufPrint(&name_buf, "bm_{d}", .{eid}) catch unreachable;
        emitMethod(allocator, tmp.writer(allocator), name, eid) catch |e| switch (e) {
            error.Declined, error.Bad => continue,
            else => return e,
        };
        try b.appendSlice(allocator, tmp.items);
        try emitted.append(allocator, eid);
    }
    return .{ .c_src = try b.toOwnedSliceSentinel(allocator, 0), .emitted = try emitted.toOwnedSlice(allocator) };
}

// -- Trampoline + registry (module-local; closure.data is a SmallInt index) ----

const MethodFn = *const fn (recv: Ref, args: [*]const Ref, n: usize) callconv(.c) Ref;
var g_registry: [4096]MethodFn = undefined;
var g_registry_len: usize = 0;

/// The trampoline installed as a method's native closure. `send.sendN` calls it
/// (its `.method` ≠ the bytecode dispatcher) with the `Send` whose `closure.data`
/// is our SmallInt registry index. Layout: the kernel passes `*Send`; we read the
/// closure pointer (2nd field is `data`) — but to avoid depending on Send's layout
/// we use the IdApi-installed closure whose data we set. We read it via the same
/// 3-word libid-closure shape the kernel uses: [vt][method][data].
fn trampoline(s: *anyopaque, args: [*]const Ref, n: usize) callconv(.c) Ref {
    // `s` is `*Send`; its layout begins with selector, receiver, … — we instead
    // recover recv + the registry index from the closure the kernel set. The
    // kernel's Send exposes `closure` and `receiver`; mirror just those two fields.
    const send: *const SendView = @ptrCast(@alignCast(s));
    const cl = send.closure orelse return abi.NIL;
    const data = cl.data;
    if (!abi.isSmallInt(data)) return abi.NIL;
    const idx: usize = @intCast(abi.untagSmallInt(data));
    if (idx >= g_registry_len) return abi.NIL;
    return g_registry[idx](send.receiver, args, n);
}

// Exact mirror of `send.Send` + `closure.Closure` (both extern structs) so the
// trampoline reads recv + the closure data the kernel handed it. Field order +
// types match send.zig/closure.zig; the C ABI pads identically.
const ClosureView = extern struct { method: *const anyopaque, data: Ref };
const SendView = extern struct {
    selector: u64,
    receiver: Ref,
    n_args: u8,
    state: Ref,
    closure: ?*const ClosureView,
};

// -- Manager (owns Jits per method eid) --------------------------------------

// `jit` is null for a mode-3 FROZEN method (the dylib owns the code, not a libtcc
// Jit); for a live JIT it's the owning Jit (deinit'd on demote/re-JIT).
const Native = struct { jit: ?Jit, idx: u64 };
var g_native: std.AutoHashMap(u64, Native) = undefined;

/// Register `f` + install the trampoline at `eid` as native; record it (with its
/// optional owning Jit). Shared by live promote + mode-3 loadFrozen.
fn installNativeFn(eid: u64, f: MethodFn, jit: ?Jit) bool {
    if (g_registry_len >= g_registry.len) return false;
    const idx = g_registry_len;
    g_registry[idx] = f;
    g_registry_len += 1;
    const data = abi.mkSmallInt(@intCast(idx)) orelse return false;
    if (!g_api.jit_install_native(eid, @ptrCast(&trampoline), data)) return false;
    g_native.put(eid, .{ .jit = jit, .idx = idx }) catch return false;
    return true;
}
var g_declined: std.AutoHashMap(u64, void) = undefined;
var g_alloc: std.mem.Allocator = undefined;

fn promote(eid: u64) bool {
    if (g_declined.contains(eid)) return false;
    if (g_native.contains(eid)) return true;
    const c_src = emitMethodOwned(g_alloc, eid) catch |e| {
        if (e == error.Declined) g_declined.put(eid, {}) catch {};
        return false;
    };
    defer g_alloc.free(c_src);
    var j = Jit.init() orelse return false;
    if (!bindRuntime(&j) or !j.compile(c_src.ptr) or !j.relocate()) {
        j.deinit();
        return false;
    }
    const sym = j.getSym("m") orelse {
        j.deinit();
        return false;
    };
    const f: MethodFn = @ptrFromInt(@intFromPtr(sym));
    if (!installNativeFn(eid, f, j)) {
        j.deinit();
        return false;
    }
    return true;
}

fn demote(eid: u64) void {
    if (g_native.fetchRemove(eid)) |kv| {
        if (kv.value.jit) |*j| @constCast(j).deinit(); // frozen methods have no Jit
        g_api.jit_revert_bytecode(eid);
    }
}

// -- Mode-3 ship: freeze native methods to an optimized dylib + load it back ---

/// Emit a C unit for `eids`, write `<dir>/<base>.c`, and `zig cc -O2 -shared` it
/// into `<dir>/lib<base>.dylib` (-undefined dynamic_lookup → the birnam_* runtime
/// entries resolve at load against the host). Returns the eids that lowered.
fn freeze(eids: []const u64, dir: []const u8, base: []const u8) ![]u64 {
    const unit = try emitUnit(g_alloc, eids);
    defer g_alloc.free(unit.c_src);
    errdefer g_alloc.free(unit.emitted);
    const c_rel = try std.fmt.allocPrint(g_alloc, "{s}.c", .{base});
    defer g_alloc.free(c_rel);
    const c_path = try std.fs.path.join(g_alloc, &.{ dir, c_rel });
    defer g_alloc.free(c_path);
    try std.fs.cwd().writeFile(.{ .sub_path = c_path, .data = unit.c_src });
    const dylib_rel = try std.fmt.allocPrint(g_alloc, "lib{s}.dylib", .{base});
    defer g_alloc.free(dylib_rel);
    var child = std.process.Child.init(&.{
        "zig", "cc", "-shared", "-O2", "-o", dylib_rel, c_rel, "-Wl,-undefined,dynamic_lookup",
    }, g_alloc);
    child.cwd = dir;
    child.stdout_behavior = .Ignore;
    child.stderr_behavior = .Inherit;
    try child.spawn();
    const term = try child.wait();
    if (term != .Exited or term.Exited != 0) return error.ZigCcFailed;
    return unit.emitted;
}

/// dlopen a frozen dylib and install each `bm_<eid>` as a native method.
fn loadFrozen(dylib_path: []const u8, eids: []const u64) usize {
    const pz = g_alloc.dupeZ(u8, dylib_path) catch return 0;
    defer g_alloc.free(pz);
    const handle = std.c.dlopen(pz, .{ .LAZY = true, .GLOBAL = true }) orelse return 0;
    var n: usize = 0;
    for (eids) |eid| {
        var nb: [40]u8 = undefined;
        const sym = std.fmt.bufPrintZ(&nb, "bm_{d}", .{eid}) catch continue;
        const addr = std.c.dlsym(handle, sym) orelse continue;
        const f: MethodFn = @ptrFromInt(@intFromPtr(addr));
        if (installNativeFn(eid, f, null)) n += 1;
    }
    return n;
}

// -- birnam-facing methods on the `Jit` proto --------------------------------

fn jitCompileAll(s: *anyopaque, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, args, n };
    // The kernel doesn't expose entity iteration to modules; instead, turn auto-JIT
    // ON so everything compiles as it runs hot. (A future ABI could add a sweep.)
    g_api.jit_set_auto(true);
    return abi.mkEid(ke.TRUE_EID);
}
fn jitDemoteAll(s: *anyopaque, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = .{ s, args, n };
    g_api.jit_set_auto(false);
    var keys = std.ArrayList(u64){};
    defer keys.deinit(g_alloc);
    var it = g_native.keyIterator();
    while (it.next()) |k| keys.append(g_alloc, k.*) catch {};
    for (keys.items) |eid| demote(eid);
    return abi.mkEid(ke.TRUE_EID);
}

fn argStr(args: [*]const Ref, n: usize, allocator: std.mem.Allocator) ?[]u8 {
    if (n < 1 or !abi.isString(args[0])) return null;
    const p = g_api.string_ptr(args[0]) orelse return null;
    return allocator.dupe(u8, p[0..g_api.string_len(args[0])]) catch null;
}

/// `[Jit freezeTo: "<base>"]` — mode-3 ship: compile all currently-native methods
/// into `lib<base>.dylib` (optimized, `zig cc -O2`) + write `<base>.manifest`
/// (their eids). Returns the method count (SmallInt). The SAME codegen as the live
/// JIT — there is only one.
fn jitFreezeTo(s: *anyopaque, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = s;
    const base = argStr(args, n, g_alloc) orelse return abi.mkSmallInt(0).?;
    defer g_alloc.free(base);
    var eids = std.ArrayList(u64){};
    defer eids.deinit(g_alloc);
    var it = g_native.keyIterator();
    while (it.next()) |k| eids.append(g_alloc, k.*) catch {};
    const emitted = freeze(eids.items, ".", base) catch return abi.mkSmallInt(0).?;
    defer g_alloc.free(emitted);
    // manifest: one eid per line.
    const man = std.fmt.allocPrint(g_alloc, "{s}.manifest", .{base}) catch return abi.mkSmallInt(0).?;
    defer g_alloc.free(man);
    var mb = std.ArrayList(u8){};
    defer mb.deinit(g_alloc);
    for (emitted) |e| mb.writer(g_alloc).print("{d}\n", .{e}) catch {};
    std.fs.cwd().writeFile(.{ .sub_path = man, .data = mb.items }) catch {};
    return abi.mkSmallInt(@intCast(emitted.len)) orelse abi.mkSmallInt(0).?;
}

/// `[Jit loadFrozen: "<base>"]` — load a frozen dylib (+ its manifest) and install
/// each method native-from-dylib. Returns the count installed.
fn jitLoadFrozen(s: *anyopaque, args: [*]const Ref, n: usize) callconv(.c) Ref {
    _ = s;
    const base = argStr(args, n, g_alloc) orelse return abi.mkSmallInt(0).?;
    defer g_alloc.free(base);
    const man = std.fmt.allocPrint(g_alloc, "{s}.manifest", .{base}) catch return abi.mkSmallInt(0).?;
    defer g_alloc.free(man);
    const text = std.fs.cwd().readFileAlloc(g_alloc, man, 1 << 20) catch return abi.mkSmallInt(0).?;
    defer g_alloc.free(text);
    var eids = std.ArrayList(u64){};
    defer eids.deinit(g_alloc);
    var lines = std.mem.tokenizeScalar(u8, text, '\n');
    while (lines.next()) |ln| {
        const e = std.fmt.parseInt(u64, std.mem.trim(u8, ln, " \r"), 10) catch continue;
        eids.append(g_alloc, e) catch {};
    }
    const dylib = std.fmt.allocPrint(g_alloc, "lib{s}.dylib", .{base}) catch return abi.mkSmallInt(0).?;
    defer g_alloc.free(dylib);
    return abi.mkSmallInt(@intCast(loadFrozen(dylib, eids.items))) orelse abi.mkSmallInt(0).?;
}

// -- maintenance hook: drain hot methods + compile them ----------------------

fn maintenance() callconv(.c) void {
    var buf: [512]u64 = undefined;
    const got = g_api.jit_drain_hot(&buf, buf.len);
    for (buf[0..got]) |eid| _ = promote(eid);
}

// -- module init -------------------------------------------------------------

const build_opts = @import("build_opts");

pub fn moduleInitImpl(ctx: *abi.ShellContext, api: *const abi.IdApi, module_eid: u64) callconv(.c) void {
    g_api = api;
    g_log = ctx.log;
    g_alloc = std.heap.c_allocator;
    g_native = std.AutoHashMap(u64, Native).init(g_alloc);
    g_declined = std.AutoHashMap(u64, void).init(g_alloc);
    ctx.log("[jit] initializing");

    // `Jit` proto with compileAll / demoteAll (native methods, like Net).
    const jit_vt = api.intern("jit:Jit", 7);
    _ = api.assert_fact(jit_vt, api.attr_vtable, abi.mkEid(api.vtable_vt_eid), module_eid);
    _ = api.assert_fact(jit_vt, api.attr_delegate, abi.mkEid(api.object_vt_eid), module_eid);
    _ = api.bind_native_method(jit_vt, api.intern("compileAll", 10), @ptrCast(&jitCompileAll), abi.NIL, module_eid);
    _ = api.bind_native_method(jit_vt, api.intern("demoteAll", 9), @ptrCast(&jitDemoteAll), abi.NIL, module_eid);
    _ = api.bind_native_method(jit_vt, api.intern("freezeTo:", 9), @ptrCast(&jitFreezeTo), abi.NIL, module_eid);
    _ = api.bind_native_method(jit_vt, api.intern("loadFrozen:", 11), @ptrCast(&jitLoadFrozen), abi.NIL, module_eid);
    const jit_proto = api.intern("jit:JitProto", 12);
    _ = api.assert_fact(jit_proto, api.attr_vtable, abi.mkEid(jit_vt), module_eid);
    _ = api.assert_fact(module_eid, api.intern("Jit", 3), abi.mkEid(jit_proto), module_eid);
    api.bind_global(api.intern("Jit", 3), abi.mkEid(jit_proto));

    // Auto-JIT: register the quiescent maintenance hook.
    ctx.set_maintenance_hook(&maintenance);
    ctx.log("[jit] ready");
}

comptime {
    if (build_opts.export_init) @export(&moduleInitImpl, .{ .name = "birnam_module_init" });
}
