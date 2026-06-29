//! src/id/codegen.zig — rung 3c (part 2): the birnam → C emitter.
//!
//! Walks the birnam CST (from `Lang`) and emits a Method-ABI C function, then
//! compiles it with `Tcc` and runs it. Everything through the object model: it
//! `api.import("Lang")` + `api.import("Tcc")` and drives them via `api.send`.
//!
//! MVP scope: integer literals and binary arithmetic sends `[a op b]`
//! (op ∈ + - * /), recursively. Values are SmallInts; tag/untag are inlined C
//! macros, so the emitted C is self-contained — no IdApi callback yet. Sends to
//! objects, variables, blocks, and comparison/boolean ops come next (they need
//! the emitted C to call back into libid via the IdApi — a pre-relocate symbol
//! hook on Tcc).
//!
//! Facade: `[Codegen eval: aString]` → the value of the program's first
//! expression (a SmallInt), or nil if parsing/codegen fails.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

var g_api: *idapi.IdApi = undefined;
var lang: Oop = undefined;
var tcc: Oop = undefined;

pub var codegen_vt: Oop = null;
pub var codegen_obj: Oop = null;

// Cached rule-name symbols (CST node labels), set in bringUp.
var r_expr: Oop = null;
var r_primary: Oop = null;
var r_message: Oop = null;
var r_number: Oop = null;
var r_send: Oop = null;
var r_binaryMsg: Oop = null;

var src_bytes: []const u8 = &.{}; // the source for the current eval (for spans)

// -- CST access (via the peg CstNode message api) ----------------------------

fn s0(rcv: Oop, name: [*:0]const u8) Oop {
    return g_api.send(rcv, g_api.intern(name), &[_]Oop{}, 0);
}
fn s1(rcv: Oop, name: [*:0]const u8, a: Oop) Oop {
    return g_api.send(rcv, g_api.intern(name), &[_]Oop{a}, 1);
}
fn ruleOf(node: Oop) Oop {
    return s0(node, "rule");
}
fn nchild(node: Oop) usize {
    return @intCast(g_api.smallIntValue(s0(node, "childCount")));
}
fn child(node: Oop, i: usize) Oop {
    return s1(node, "childAt:", g_api.mkSmallInt(@intCast(i)));
}
fn spanStart(node: Oop) usize {
    return @intCast(g_api.smallIntValue(s0(node, "start")));
}
fn spanEnd(node: Oop) usize {
    return @intCast(g_api.smallIntValue(s0(node, "end")));
}
fn text(node: Oop) []const u8 {
    return src_bytes[spanStart(node)..spanEnd(node)];
}

/// Descend through single-child wrapper rules (expr / primary / message) to the
/// meaningful node beneath.
fn skip(node: Oop) Oop {
    var nd = node;
    while (true) {
        const r = ruleOf(nd);
        if ((r == r_expr or r == r_primary or r == r_message) and nchild(nd) == 1) {
            nd = child(nd, 0);
        } else return nd;
    }
}

// -- emission ----------------------------------------------------------------

fn put(buf: *std.ArrayList(u8), s: []const u8) void {
    buf.appendSlice(ca, s) catch @panic("codegen: oom");
}

/// Emit a C expression of type `void*` (a tagged SmallInt) for `node0`. Returns
/// false on anything outside the MVP subset.
fn emit(buf: *std.ArrayList(u8), node0: Oop) bool {
    const node = skip(node0);
    const r = ruleOf(node);
    if (r == r_number) {
        put(buf, "MKV(");
        put(buf, text(node)); // the integer literal text (incl. any leading '-')
        put(buf, "L)");
        return true;
    }
    if (r == r_send) {
        const recv = child(node, 0);
        const msg = skip(child(node, 1));
        if (ruleOf(msg) != r_binaryMsg) return false; // MVP: only binary arithmetic
        const op = text(child(msg, 0)); // the binop char
        if (op.len != 1) return false;
        switch (op[0]) {
            '+', '-', '*', '/' => {},
            else => return false,
        }
        put(buf, "MKV( GETV(");
        if (!emit(buf, recv)) return false;
        put(buf, ") ");
        put(buf, op);
        put(buf, " GETV(");
        if (!emit(buf, child(msg, 1))) return false;
        put(buf, ") )");
        return true;
    }
    return false; // ident / string / block / keyword / unary — not yet
}

// -- [Codegen eval: src] -----------------------------------------------------

fn mEval(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const src = a[0];
    var len: usize = 0;
    const p = g_api.stringBytes(src, &len);
    src_bytes = p[0..len];

    const cst = s1(lang, "parse:", src);
    if (cst == g_api.nilObj) return g_api.nilObj;

    // program → statements → statement → (expr | letBinding); take the first.
    if (nchild(cst) < 1) return g_api.nilObj;
    const statements = child(cst, 0);
    if (nchild(statements) < 1) return g_api.nilObj;
    const statement = child(statements, 0);
    if (nchild(statement) < 1) return g_api.nilObj;
    const expr = child(statement, 0);

    var buf: std.ArrayList(u8) = .{};
    defer buf.deinit(ca);
    put(&buf,
        \\#define GETV(x) (((long)(x))>>1)
        \\#define MKV(x) ((void*)(((long)(x))<<1|1L))
        \\void* __ev(void* self, void** args, long n){ (void)self;(void)args;(void)n; return
    );
    put(&buf, " ");
    if (!emit(&buf, expr)) return g_api.nilObj;
    put(&buf, "; }");

    const c_src = g_api.mkString(buf.items.ptr, buf.items.len);
    const unit = s1(tcc, "compile:", c_src);
    if (unit == g_api.nilObj) return g_api.nilObj;

    const vt = g_api.proto(g_api.object_vt);
    const sel = g_api.intern("__ev");
    const fname = "__ev";
    _ = g_api.send(unit, g_api.intern("install:as:on:"), &[_]Oop{ g_api.mkString(fname, fname.len), sel, vt }, 3);
    const inst = g_api.alloc(vt, 0);
    return g_api.send(inst, sel, &[_]Oop{}, 0);
}

// -- bringup / module entrypoint --------------------------------------------

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    lang = api.import("Lang") orelse @panic("codegen: Lang not loaded");
    tcc = api.import("Tcc") orelse @panic("codegen: Tcc not loaded");

    r_expr = api.intern("expr");
    r_primary = api.intern("primary");
    r_message = api.intern("message");
    r_number = api.intern("number");
    r_send = api.intern("send");
    r_binaryMsg = api.intern("binaryMsg");

    const reloading = api.isExported("Codegen") != 0;
    if (reloading) {
        codegen_vt = api.import("CodegenVt");
        codegen_obj = api.import("Codegen");
    } else {
        codegen_vt = api.proto(api.object_vt);
        codegen_obj = api.alloc(codegen_vt, 0);
    }

    api.method(codegen_vt, api.intern("eval:"), &mEval);

    if (!reloading) {
        _ = api.exportObj("CodegenVt", codegen_vt);
        _ = api.exportObj("Codegen", codegen_obj);
    }
}

fn moduleInit(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}
comptime {
    if (!@import("builtin").is_test) @export(&moduleInit, .{ .name = "module_init" });
}
