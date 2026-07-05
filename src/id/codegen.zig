//! src/id/codegen.zig — rung 3c (part 2): the birnam → C emitter.
//!
//! Walks the birnam CST (from `Lang`) and emits a Method-ABI C function, then
//! compiles it with `Tcc` and runs it. Everything through the object model: it
//! `api.import("Lang")` + `api.import("Tcc")` and drives them via `api.send`.
//!
//! Covers the whole birnam grammar:
//!   - integer + string literals (`birnam_mkString`);
//!   - EVERY send — unary `[r sel]`, binary `[r op a]` (including arithmetic
//!     `+ - * /`), and keyword `[r k: a m: b]` — lowered to a real dispatch through
//!     libid via the `birnam_send`/`birnam_intern` runtime shims. Arithmetic
//!     therefore dispatches to the SmallInt protocol (the prelude) and honors its
//!     error flow — `[10 / 0]` is a #zeroDivide value, an error receiver propagates
//!     — rather than being inlined as raw C math that would untag past those (the
//!     former F7 codegen gap, now closed);
//!   - variables: `let x := e` → a C local `b_x`; identifiers resolve to it;
//!   - multi-statement programs (`a; b; c`) → the last statement's value;
//!   - blocks `[:a :b | body]` → file-scope C functions with the Method ABI
//!     (params in `args[]`, captured free vars in the instance payload); the
//!     literal is a statement-expression that builds the block's vtable and
//!     captures. Invoking a block is just an ordinary `value`/`value:`… send.
//! All recursive: receivers, arguments, let RHSs, and block bodies nest freely.
//!
//! Uses the STAGED Tcc path so the runtime shims are bound before relocation:
//! `[Tcc new]` → `[unit compile:]` → `[unit bindRuntime]` → `[unit relocate]`.
//!
//! Facade: `[Codegen eval: aString]` → the value of the program (its last
//! statement), or nil if parsing/codegen fails.
//!
//! Beyond the JIT, `[Codegen compile: src output: path]` writes a RELOCATABLE
//! shared library (docs/40 §3a, slice 1 of the images arc): the `birnam_*` shims
//! stay undefined externals resolved by the dynamic loader at dlopen (tcc.zig +
//! this module export them as dynamic symbols), and the baked `__g<i>` global
//! handles become static slots a per-unit `__birnam_unit_init` hook re-derives
//! by name at load. `[Codegen load: path]` dlopens such an artifact, runs the
//! hook, and evaluates its `__ev` entry.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

var g_api: *idapi.IdApi = undefined;
var lang: Oop = undefined;
var tcc: Oop = undefined;
var g_store: Oop = undefined; // the well-known store (for global resolution)
var g_globals_entity: Oop = null; // the reserved entity globals hang off of
var sel_currentValue: Oop = null;
var sel_addSymbol: Oop = null;
var sel_assert: Oop = null;
var sel_addMethodOn: Oop = null;
var method_blk_counter: usize = 0; // unique keys for stashed method blocks
var method_src_counter: usize = 0; // unique keys for recorded method sources
var g_methods_entity: Oop = null; // the reserved entity method-source facts hang off of
var g_globalsrc_entity: Oop = null; // the reserved entity top-level global-source facts hang off of
var artifact_method_counter: usize = 0; // per-emit ids for installable artifact methods

// The `__ev` entry-point harness vtable. Minted once, reused every eval: each
// eval reinstalls its fresh `__ev` method (install:as:on: REPLACES an existing
// selector's impl — id.zig vtableAddMethod), so a single permanent vtable serves
// all evals instead of leaking one per call. Its `__ev` slot always points at the
// current eval's code by the time we send, and nothing invokes `__ev` between evals.
var ev_harness_vt: Oop = null;

pub var codegen_vt: Oop = null;
pub var codegen_obj: Oop = null;
pub var reflect_vt: Oop = null;
pub var reflect_obj: Oop = null; // the Reflect facade (exported "Reflect")

// Cached rule-name symbols (CST node labels), set in bringUp.
var r_expr: Oop = null;
var r_primary: Oop = null;
var r_message: Oop = null;
var r_number: Oop = null;
var r_float: Oop = null;
var r_send: Oop = null;
var r_binaryMsg: Oop = null;
var r_unaryMsg: Oop = null;
var r_keywordMsg: Oop = null;
var r_ident: Oop = null;
var r_string: Oop = null;
var r_symbol: Oop = null;
var r_letBinding: Oop = null;
var r_block: Oop = null;
var r_statements: Oop = null;
var r_vectorLit: Oop = null;
var r_mapLit: Oop = null;
var r_mapEntry: Oop = null;
var r_logicVar: Oop = null;

var src_bytes: []const u8 = &.{}; // the source for the current eval (for spans)

// -- diagnostics -------------------------------------------------------------
// A compile failure (bad syntax or an out-of-subset construct) otherwise makes a
// whole program silently evaluate to nil, with no clue where. We capture the FIRST
// failure per eval — its message and source offset — format it (line:col + the
// offending line + a caret) at capture time (while `src_bytes` is still valid), and
// surface it via `[Codegen lastError]` so the CLI/REPL can print it.
var g_err_active: bool = false;
var g_err_text: ?[]u8 = null; // owned, formatted; replaced each capture

fn clearError() void {
    g_err_active = false;
}

/// Record a compile failure at byte `off` in the current source. Keeps the FIRST
/// (deepest/earliest) note per eval — inner emitters fail first, so that node is the
/// most specific. Formats immediately: `line:col: <msg>` then the source line and a
/// caret under `off`.
fn noteError(msg: []const u8, off: usize) void {
    if (g_err_active) return;
    g_err_active = true;

    // line:col (1-based) + the bounds of the offending line.
    var line: usize = 1;
    var line_start: usize = 0;
    var i: usize = 0;
    const clamped = if (off <= src_bytes.len) off else src_bytes.len;
    while (i < clamped) : (i += 1) {
        if (src_bytes[i] == '\n') {
            line += 1;
            line_start = i + 1;
        }
    }
    const col = clamped - line_start + 1;
    var line_end = line_start;
    while (line_end < src_bytes.len and src_bytes[line_end] != '\n') line_end += 1;
    const line_text = src_bytes[line_start..line_end];

    var b: std.ArrayList(u8) = .{};
    putFmt(&b, "{d}:{d}: {s}\n", .{ line, col, msg });
    b.appendSlice(ca, line_text) catch @panic("codegen: oom");
    b.append(ca, '\n') catch @panic("codegen: oom");
    // Caret: a run of spaces to the column, then '^'. Tabs in the prefix stay tabs
    // so the caret lines up under a tab-indented source line.
    var c: usize = line_start;
    while (c < clamped) : (c += 1) b.append(ca, if (src_bytes[c] == '\t') '\t' else ' ') catch @panic("codegen: oom");
    b.append(ca, '^') catch @panic("codegen: oom");

    if (g_err_text) |old| ca.free(old);
    g_err_text = b.toOwnedSlice(ca) catch @panic("codegen: oom");
}

/// `[Codegen lastError]` — the formatted message from the last eval's compile
/// failure, or nil if it compiled. Cleared at the start of every eval.
fn mLastError(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    if (!g_err_active) return g_api.nilObj;
    const t = g_err_text orelse return g_api.nilObj;
    return g_api.mkString(t.ptr, t.len);
}

// Baked global handles (docs/36 §3 v2, the safe layer). A global reference's
// VALUE stays late-bound through the store (the single source of truth), but its
// interned SELECTOR is immutable, so we resolve it ONCE at compile time and bake
// its address as an extern symbol — turning each runtime read from an O(symbols)
// intern scan into a constant the linker patches. `baked_globals` collects the
// distinct symbols referenced in the current compile (their index is the `__g<i>`
// suffix); `global_memo` dedups by symbol identity. Both reset per eval.
var baked_globals: std.ArrayList(Oop) = .{};
var global_memo: std.AutoHashMapUnmanaged(Oop, usize) = .{};

// Artifact (relocatable) emission mode — docs/40 §3a, slice 1. The JIT bakes a
// `__g<i>` handle as an extern the symbol window patches to this process's
// interned-symbol address; an artifact written to disk can't carry that, so in
// artifact mode each `__g<i>` is instead a static slot the unit's
// `__birnam_unit_init` hook fills by interning the NAME at load time. Set for
// the duration of one `emitProgram(…, artifact=true)`.
var artifact_mode: bool = false;

/// Resolve `name` to its interned symbol (once), assign it a stable `__g<i>` slot,
/// and return that index. Keyed by symbol identity, so the same global referenced
/// many times bakes a single handle.
fn bakeGlobal(name: []const u8) usize {
    const z = ca.allocSentinel(u8, name.len, 0) catch @panic("codegen: oom");
    defer ca.free(z);
    @memcpy(z[0..name.len], name);
    const sym = g_api.intern(z.ptr);
    if (global_memo.get(sym)) |id| return id;
    const id = baked_globals.items.len;
    baked_globals.append(ca, sym) catch @panic("codegen: oom");
    global_memo.put(ca, sym, id) catch @panic("codegen: oom");
    return id;
}

/// Emit a reference to the global `name`: `birnam_global_b((void*)__g<i>)` — a
/// store read keyed by the baked symbol (no runtime intern). Replaces the v1
/// `birnam_global("name")` everywhere a global is read. In artifact mode the
/// handle is a static slot (filled by `__birnam_unit_init`) rather than a
/// linker-patched extern, so the read is `birnam_global_b(__g<i>)`.
fn putGlobalRef(buf: *std.ArrayList(u8), name: []const u8) void {
    if (artifact_mode) {
        putFmt(buf, "birnam_global_b(__g{d})", .{bakeGlobal(name)});
    } else {
        putFmt(buf, "birnam_global_b((void*)__g{d})", .{bakeGlobal(name)});
    }
}

// Lexical environment for the current eval: birnam variable names currently in
// scope (from `let` / block params / block captures). Each is emitted as a C
// local `b_<name>`. Slices point into `src_bytes`. Reset at the start of each
// eval; saved/swapped when emitting a block body (its own scope).
var bound_names: std.ArrayList([]const u8) = .{};

// File-scope C function definitions for blocks, accumulated during emission and
// emitted (in post-order: inner blocks first) before `__ev`. A monotonic counter
// names each block's function. Both reset per eval.
var fn_defs: std.ArrayList(u8) = .{};
var blk_counter: usize = 0;

fn isBound(name: []const u8) bool {
    return listContains(bound_names.items, name);
}
/// `nil`/`true`/`false` are LITERALS, not identifiers — they emit a runtime
/// singleton (`birnam_nil/true/false()`), so they can't silently resolve to nil (an
/// unbound global reads nil), be shadowed, or be captured as a block free variable.
/// Returns the shim call for `name`, or null if `name` is an ordinary identifier.
/// (docs/design/41 §B8 — the fix for literal true/false not being real Booleans.)
fn literalSingleton(name: []const u8) ?[]const u8 {
    if (std.mem.eql(u8, name, "nil")) return "birnam_nil()";
    if (std.mem.eql(u8, name, "true")) return "birnam_true()";
    if (std.mem.eql(u8, name, "false")) return "birnam_false()";
    return null;
}
fn listContains(items: []const []const u8, name: []const u8) bool {
    for (items) |b| if (std.mem.eql(u8, b, name)) return true;
    return false;
}
fn setAdd(list: *std.ArrayList([]const u8), name: []const u8) void {
    if (!listContains(list.items, name)) list.append(ca, name) catch @panic("codegen: oom");
}

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

/// `put` with formatting — formats straight into the buffer (no fixed-size
/// scratch buffer to overflow).
fn putFmt(buf: *std.ArrayList(u8), comptime fmt: []const u8, args: anytype) void {
    const s = std.fmt.allocPrint(ca, fmt, args) catch @panic("codegen: oom");
    defer ca.free(s);
    buf.appendSlice(ca, s) catch @panic("codegen: oom");
}

// -- the global namespace runtime (docs/36) ----------------------------------
//
// Store-coupled, so it lives HERE (codegen depends on the store) rather than in
// tcc.zig (which stays store-free). Registered per-compile via the staged Tcc
// `addSymbol:value:` window, alongside tcc's floor `bindRuntime`.

/// `birnam_global("name")` — resolve a global reference through its binding:
/// the Globals store (non-nil wins, late-bound), else a module facade / seeded
/// type in the id env, else nil (undeclared / forward reference).
fn rtGlobal(name: [*:0]const u8) callconv(.c) Oop {
    const sym = g_api.intern(name);
    // The store reports absence as Zig null (≠ the `nilObj` singleton); a present
    // binding (even one bound to nil) is non-null and wins.
    const cur = g_api.send(g_store, sel_currentValue, &[_]Oop{ g_globals_entity, sym }, 2);
    if (cur != null) return cur;
    if (g_api.isExported(name) != 0) return g_api.import(name);
    return g_api.nilObj;
}

/// `birnam_global_b(sym)` — the baked-handle read: `sym` is an already-interned
/// selector (resolved at compile time, address patched in by the linker), so this
/// skips the runtime intern that `rtGlobal` does. Same late-bound resolution
/// otherwise: the Globals store wins, else a module facade / seeded type, else nil.
fn rtGlobalB(sym: Oop) callconv(.c) Oop {
    const cur = g_api.send(g_store, sel_currentValue, &[_]Oop{ g_globals_entity, sym }, 2);
    if (cur != null) return cur;
    const name = g_api.symbolName(sym);
    if (g_api.isExported(name) != 0) return g_api.import(name);
    return g_api.nilObj;
}

/// `birnam_global_set("name", value)` — bind a global by asserting it onto the
/// Globals entity (the same store cell `birnam_global` reads). The write side of
/// the namespace, emitted for top-level `let`s in persisting (REPL) mode so a
/// binding survives to the next evaluation.
fn rtGlobalSet(name: [*:0]const u8, value: Oop) callconv(.c) void {
    _ = g_api.send(g_store, sel_assert, &[_]Oop{ g_globals_entity, g_api.intern(name), value }, 3);
}

/// `birnam_record_global_src("name", src, n)` — record the SOURCE of a top-level
/// global `let` binding onto the reserved `__globalsrc__` entity, keyed by name (a
/// String leaf, so it survives an image directly). This is the portable truth an
/// image boot re-derives block-valued (and any un-serializable) globals from
/// (docs/40 §4/§6: blocks-in-globals persist where their source is in the store).
/// Emitted alongside `birnam_global_set` for every top-level `let` in persist mode.
fn rtRecordGlobalSrc(name: [*:0]const u8, src: [*]const u8, n: c_long) callconv(.c) void {
    const s = g_api.mkString(src, @intCast(n));
    _ = g_api.send(g_store, sel_assert, &[_]Oop{ g_globalsrc_entity, g_api.intern(name), s }, 3);
}

// Like tcc.zig's floor shims, the global-namespace shims are also real dynamic
// symbols (docs/40 §3a, slice 1): an emitted artifact leaves them undefined and
// the dynamic loader resolves them here at dlopen. The JIT path keeps patching
// the same addresses through the pre-relocate symbol window (`addShim`).
comptime {
    @export(&rtGlobal, .{ .name = "birnam_global" });
    @export(&rtGlobalB, .{ .name = "birnam_global_b" });
    @export(&rtGlobalSet, .{ .name = "birnam_global_set" });
    @export(&rtRecordGlobalSrc, .{ .name = "birnam_record_global_src" });
}

/// Register a codegen-owned shim into a unit's pre-relocate symbol window.
fn addShim(unit: Oop, name: []const u8, fnptr: *const anyopaque) void {
    const name_str = g_api.mkString(name.ptr, name.len);
    const addr: i64 = @intCast(@intFromPtr(fnptr));
    _ = g_api.send(unit, sel_addSymbol, &[_]Oop{ name_str, g_api.mkSmallInt(addr) }, 2);
}

// -- define-method (docs/36 §7) ----------------------------------------------
//
// `[Reflect defineMethod: #sel on: aType as: aBlock]`. A block's `self` is the
// block object (for captures), but a method's `self` is the receiver — so we
// can't install a block's fn directly. We compile a tiny per-define trampoline
// (via Tcc) that forwards [receiver, args…] to the block as a value-send. The
// block is reached through a uniquely-named global (reuse, not a new mechanism),
// and the method is installed through the store's dispatch mirror (queryable).

/// The number of message arguments a selector takes: keyword → colon count;
/// otherwise unary (alnum) → 0, binary (operator) → 1.
fn selectorArity(sel: Oop) usize {
    const name = g_api.symbolName(sel);
    var colons: usize = 0;
    var i: usize = 0;
    while (name[i] != 0) : (i += 1) {
        if (name[i] == ':') colons += 1;
    }
    if (colons > 0) return colons;
    const c0 = name[0];
    const is_letter = (c0 >= 'a' and c0 <= 'z') or (c0 >= 'A' and c0 <= 'Z') or c0 == '_';
    return if (is_letter) 0 else 1;
}

/// Staged-compile a single C function and return its native address as an Oop
/// (raw fn pointer), or nil. The TccState (holding the code) is intentionally
/// kept alive by the installed method; no GC yet, so it simply persists.
fn compileFn(c_src_bytes: []const u8, fn_name: []const u8) Oop {
    const c_src = g_api.mkString(c_src_bytes.ptr, c_src_bytes.len);
    const unit = s0(tcc, "new");
    if (unit == g_api.nilObj) return g_api.nilObj;
    if (s1(unit, "compile:", c_src) == g_api.nilObj) return g_api.nilObj;
    _ = s0(unit, "bindRuntime");
    addShim(unit, "birnam_global", @ptrCast(&rtGlobal));
    if (s0(unit, "relocate") == g_api.nilObj) return g_api.nilObj;
    const addr_si = s1(unit, "symbol:", g_api.mkString(fn_name.ptr, fn_name.len));
    if (addr_si == g_api.nilObj) return g_api.nilObj;
    const addr: usize = @intCast(g_api.smallIntValue(addr_si));
    return @ptrFromInt(addr);
}

/// `[Reflect defineMethod: #sel on: aType as: aBlock]` — install aBlock as a
/// method on aType under #sel. The block receives the receiver as its first
/// parameter, then the message arguments.
fn mDefineMethod(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return installMethod(args[0], args[1], args[2]);
}

/// `[Reflect defineMethod: #sel on: T as: blk source: srcString]` — the source-
/// carrying form codegen lowers `defineMethod:on:as:` to. Records the definition's
/// source as a store fact (the portable truth for image save/replay, docs/40 §4)
/// keyed by a fresh `__msrc_N` on the reserved methods entity — a String leaf, so
/// it survives an image directly — then installs the method exactly as the 3-arg
/// form does.
fn mDefineMethodSourced(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const id = method_src_counter;
    method_src_counter += 1;
    var keybuf: [40]u8 = undefined;
    const key = std.fmt.bufPrintZ(&keybuf, "__msrc_{d}", .{id}) catch return g_api.nilObj;
    _ = g_api.send(g_store, sel_assert, &[_]Oop{ g_methods_entity, g_api.intern(key.ptr), args[3] }, 3);
    return installMethod(args[0], args[1], args[2]);
}

/// The vtable an `on:` target names. Uniform, no family/instance fork
/// (docs/design/41 §A): a well-known name (`Integer`, `String`, `Vector`) is a
/// PROTOTYPE that owns its family, so `ensurePrivateVtable` returns that family
/// and the method extends every instance; any other object gets a copy-on-write
/// private vtable so the method specialises just it (e.g. a `[Object clone]`
/// singleton like `Toml`).
fn targetVtable(on: Oop) Oop {
    return g_api.ensurePrivateVtable(on);
}

/// Install `blk` as method `sel` on `on` (a type or an object — see targetVtable):
/// stash the block in a uniquely-named global, compile a forwarding trampoline,
/// and register it through the store's dispatch mirror. Shared by both
/// `defineMethod:` forms.
fn installMethod(sel: Oop, on: Oop, blk: Oop) Oop {
    const vt = targetVtable(on);
    const k = selectorArity(sel);

    // Stash the block in a uniquely-named global the trampoline reads at dispatch.
    const id = method_blk_counter;
    method_blk_counter += 1;
    var keybuf: [32]u8 = undefined;
    const key = std.fmt.bufPrintZ(&keybuf, "__mblk_{d}", .{id}) catch return g_api.nilObj;
    _ = g_api.send(g_store, sel_assert, &[_]Oop{ g_globals_entity, g_api.intern(key.ptr), blk }, 3);

    // The trampoline: forward [self, args…] to the block via value:…(k+1 parts).
    var buf: std.ArrayList(u8) = .{};
    defer buf.deinit(ca);
    put(&buf,
        \\extern void* birnam_send(void* rcv, void* sel, void** args, long n);
        \\extern void* birnam_intern(const char* name);
        \\extern void* birnam_global(const char* name);
        \\
    );
    putFmt(&buf, "void* __tramp(void* self, void** args, long n){{ (void)n; void* a[{d}]; a[0]=self; long i; for(i=0;i<{d};i++) a[i+1]=args[i]; return birnam_send(birnam_global(\"{s}\"), birnam_intern(\"", .{ k + 1, k, key });
    var vi: usize = 0;
    while (vi < k + 1) : (vi += 1) put(&buf, "value:");
    putFmt(&buf, "\"), a, {d}); }}", .{k + 1});

    const tramp = compileFn(buf.items, "__tramp");
    if (tramp == g_api.nilObj) return g_api.nilObj;

    // Install through the store mirror (one write, two reps: vtable dict + fact).
    _ = g_api.send(g_store, sel_addMethodOn, &[_]Oop{ sel, vt, tramp }, 3);
    return vt;
}

/// Emit `bytes` as the body of a C string literal (no surrounding quotes),
/// escaping everything that could break the literal or be misread. Non-printable
/// bytes become 3-digit octal (`\ooo`, unambiguous — unlike greedy `\x`). A C
/// string literal's runtime byte count equals `bytes.len` (escapes are one byte
/// each), so callers can pass `bytes.len` as the length unchanged.
fn putCStr(buf: *std.ArrayList(u8), bytes: []const u8) void {
    for (bytes) |ch| {
        switch (ch) {
            '"' => put(buf, "\\\""),
            '\\' => put(buf, "\\\\"),
            '\n' => put(buf, "\\n"),
            '\r' => put(buf, "\\r"),
            '\t' => put(buf, "\\t"),
            else => {
                if (ch >= 0x20 and ch < 0x7f) {
                    buf.append(ca, ch) catch @panic("codegen: oom");
                } else {
                    buf.append(ca, '\\') catch @panic("codegen: oom");
                    buf.append(ca, '0' + ((ch >> 6) & 7)) catch @panic("codegen: oom");
                    buf.append(ca, '0' + ((ch >> 3) & 7)) catch @panic("codegen: oom");
                    buf.append(ca, '0' + (ch & 7)) catch @panic("codegen: oom");
                }
            },
        }
    }
}

/// Emit `birnam_send(<recv>, birnam_intern("<sel>"), <args>, <n>L)` — a real
/// dispatch through libid. `argNodes` are emitted into a C99 compound-literal
/// `void*[]` (whose lifetime spans the call); n=0 passes a null buffer.
fn emitGeneralSend(buf: *std.ArrayList(u8), recvNode: Oop, sel: []const u8, argNodes: []const Oop) bool {
    put(buf, "birnam_send(");
    if (!emit(buf, recvNode)) return false;
    put(buf, ", birnam_intern(\"");
    putCStr(buf, sel);
    put(buf, "\"), ");
    if (argNodes.len == 0) {
        put(buf, "0, 0L)");
        return true;
    }
    put(buf, "(void*[]){");
    for (argNodes, 0..) |an, i| {
        if (i != 0) put(buf, ", ");
        if (!emit(buf, an)) return false;
    }
    var nb: [24]u8 = undefined;
    const ns = std.fmt.bufPrint(&nb, "}}, {d}L)", .{argNodes.len}) catch return false;
    put(buf, ns);
    return true;
}

/// Emit `[<recv> defineMethod: … on: … as: …]` as the 4-arg
/// `defineMethod:on:as:source:` send, appending the definition's original source
/// text (the whole send node's span) as a String 4th arg. The runtime records it
/// as a store fact — the portable truth image save/replay is built on (docs/40 §4).
fn emitDefineMethodSourced(buf: *std.ArrayList(u8), recvNode: Oop, argNodes: []const Oop, sendNode: Oop) bool {
    put(buf, "birnam_send(");
    if (!emit(buf, recvNode)) return false;
    put(buf, ", birnam_intern(\"defineMethod:on:as:source:\"), (void*[]){");
    for (argNodes) |an| {
        if (!emit(buf, an)) return false;
        put(buf, ", ");
    }
    put(buf, "birnam_mkString(\"");
    const s = text(sendNode);
    putCStr(buf, s);
    var nb: [24]u8 = undefined;
    put(buf, std.fmt.bufPrint(&nb, "\", {d}L)}}, 4L)", .{s.len}) catch return false);
    return true;
}

/// The number of `:` in a selector string (its keyword arity; 0 for unary/binary).
fn colonCount(sel: []const u8) usize {
    var k: usize = 0;
    for (sel) |ch| {
        if (ch == ':') k += 1;
    }
    return k;
}

/// Artifact-mode `[Reflect defineMethod: #sel on: T as: blk]` (docs/40 slice 4):
/// emit the method so that LOADING the artifact installs PRE-COMPILED code — no
/// runtime compile. Mirrors `installMethod` (the JIT path) but statically: the
/// block becomes an artifact function+instance stashed in a per-artifact global
/// (`__amblk_N`), a forwarding trampoline (`__amtramp_N`) is emitted into
/// `fn_defs`, and a statement-expression stashes the block then `birnam_method`-
/// installs the trampoline pointer directly. The `__amblk`/`__amtramp` namespace
/// is distinct from the live session's `__mblk` so a boot's stdlib re-eval can't
/// collide. `sel`/`T` resolve relocatably (birnam_intern / birnam_global).
fn emitDefineMethodInstallable(buf: *std.ArrayList(u8), argNodes: []const Oop) bool {
    const selRaw = text(argNodes[0]); // "#sel"
    const sel = if (selRaw.len >= 1 and selRaw[0] == '#') selRaw[1..] else selRaw;
    const k = colonCount(sel);
    const n = artifact_method_counter;
    artifact_method_counter += 1;

    // The trampoline: forward [self, args…] to the stashed block via value:…(k+1).
    putFmt(&fn_defs, "void* __amtramp{d}(void* self, void** args, long n){{ (void)n; void* a[{d}]; a[0]=self; long i; for(i=0;i<{d};i++) a[i+1]=args[i]; return birnam_send(birnam_global(\"__amblk{d}\"), birnam_intern(\"", .{ n, k + 1, k, n });
    var vi: usize = 0;
    while (vi < k + 1) : (vi += 1) put(&fn_defs, "value:");
    putFmt(&fn_defs, "\"), a, {d}); }}\n", .{k + 1});

    // The install site: stash the block, then install the trampoline on the type.
    // `on:` names a prototype now (e.g. Integer = the integer prototype), so resolve
    // its owned family via ensurePrivateVtable — same as the JIT path's targetVtable.
    putFmt(buf, "({{ void* _vt = birnam_ensurePrivateVtable(", .{});
    if (!emit(buf, argNodes[1])) return false; // the type expression (e.g. Integer)
    putFmt(buf, "); birnam_global_set(\"__amblk{d}\", ", .{n});
    if (!emit(buf, argNodes[2])) return false; // the block literal → instance
    put(buf, "); birnam_method(_vt, birnam_intern(\"");
    putCStr(buf, sel);
    putFmt(buf, "\"), (void*)&__amtramp{d}); _vt; }})", .{n});
    return true;
}

/// Emit a left-folded chain of same-category messages: for receiver R and message
/// nodes [m0 … m_{k-1}], `birnam_send(… birnam_send(R, m0) …, m_{k-1})`. `upto`
/// peels from the right so the innermost (receiver) send is emitted first. Only
/// binary or unary chains reach here (keyword parts fold inside one keywordMsg).
fn emitSendChain(buf: *std.ArrayList(u8), recvNode: Oop, msgParent: Oop, upto: usize) bool {
    if (upto == 0) return emit(buf, recvNode);
    const msg = skip(child(msgParent, upto - 1));
    const mr = ruleOf(msg);
    put(buf, "birnam_send(");
    if (!emitSendChain(buf, recvNode, msgParent, upto - 1)) return false; // the receiver
    put(buf, ", birnam_intern(\"");
    if (mr == r_binaryMsg) {
        putCStr(buf, text(child(msg, 0)));
        put(buf, "\"), (void*[]){");
        if (!emit(buf, child(msg, 1))) return false;
        put(buf, "}, 1L)");
        return true;
    }
    if (mr == r_unaryMsg) {
        putCStr(buf, text(msg));
        put(buf, "\"), 0, 0L)");
        return true;
    }
    if (mr == r_keywordMsg) {
        const nc = nchild(msg);
        var i: usize = 0;
        while (i + 1 < nc) : (i += 2) putCStr(buf, text(child(msg, i)));
        put(buf, "\"), (void*[]){");
        i = 1;
        var first = true;
        while (i < nc) : (i += 2) {
            if (!first) put(buf, ", ");
            first = false;
            if (!emit(buf, child(msg, i))) return false;
        }
        var nb: [24]u8 = undefined;
        put(buf, std.fmt.bufPrint(&nb, "}}, {d}L)", .{nc / 2}) catch return false);
        return true;
    }
    return false;
}

/// Emit a C expression of type `void*` (a tagged SmallInt / Oop) for `node0`.
/// Returns false on anything not yet supported.
fn emit(buf: *std.ArrayList(u8), node0: Oop) bool {
    const node = skip(node0);
    const r = ruleOf(node);
    if (r == r_number) {
        put(buf, "MKV(");
        put(buf, text(node)); // the integer literal text (incl. any leading '-')
        put(buf, "L)");
        return true;
    }
    if (r == r_float) {
        // A boxed f64: the span text (`3.14`, `-0.5`) is already a valid C double
        // literal — hand it to the runtime constructor.
        put(buf, "birnam_mkFloat(");
        put(buf, text(node));
        put(buf, ")");
        return true;
    }
    if (r == r_ident) {
        const name = text(node);
        if (literalSingleton(name)) |shim| {
            put(buf, shim);
            return true;
        }
        if (isBound(name)) {
            put(buf, "b_");
            put(buf, name);
            return true;
        }
        // Unbound → a global reference, resolved at runtime through the binding
        // (late-bound, relocatable; undeclared/forward refs read nil — docs/36).
        // The selector is baked at compile time (no runtime intern).
        putGlobalRef(buf, name);
        return true;
    }
    if (r == r_string) {
        const raw = text(node); // includes the surrounding quotes
        const inner = if (raw.len >= 2) raw[1 .. raw.len - 1] else raw;
        // Unescape the KNOWN birnam escapes into a temp buffer: `\"`→`"`, `\\`→`\`,
        // `\n`/`\t`/`\r`/`\0` → the control byte. An UNKNOWN escape keeps its
        // backslash verbatim (`\c` stays `\c`) — so no pre-existing string with a
        // literal backslash changes meaning. putCStr then re-escapes the RESULT for
        // the C literal; the length is the unescaped byte count.
        var un: std.ArrayList(u8) = .{};
        defer un.deinit(ca);
        var i: usize = 0;
        while (i < inner.len) : (i += 1) {
            if (inner[i] == '\\' and i + 1 < inner.len) {
                const decoded: ?u8 = switch (inner[i + 1]) {
                    '"' => '"',
                    '\\' => '\\',
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '0' => 0,
                    else => null,
                };
                if (decoded) |d| {
                    un.append(ca, d) catch @panic("codegen: oom");
                    i += 1; // consume the escaped char too
                    continue;
                }
                // Unknown escape: keep the backslash; the next char is emitted normally.
            }
            un.append(ca, inner[i]) catch @panic("codegen: oom");
        }
        put(buf, "birnam_mkString(\"");
        putCStr(buf, un.items);
        put(buf, "\", ");
        var nb: [24]u8 = undefined;
        const ns = std.fmt.bufPrint(&nb, "{d}L)", .{un.items.len}) catch return false;
        put(buf, ns);
        return true;
    }
    if (r == r_symbol) {
        const raw = text(node); // "#sel" — strip the leading '#'
        const sel = if (raw.len >= 1) raw[1..] else raw;
        put(buf, "birnam_intern(\"");
        putCStr(buf, sel);
        put(buf, "\")");
        return true;
    }
    if (r == r_send) {
        const recv = child(node, 0);
        const msg = skip(child(node, 1));
        const mr = ruleOf(msg);

        // A chain of same-category messages (`message` kept >1 child): fold left,
        // each message applied to the accumulated receiver (docs: chained sends).
        if (mr == r_message) return emitSendChain(buf, recv, msg, nchild(msg));

        if (mr == r_binaryMsg) {
            const op = text(child(msg, 0)); // the binop char(s)
            const arg = child(msg, 1);
            // EVERY binary op — including `+ - * /` — is a real send, so it
            // dispatches to the SmallInt protocol (the prelude) and honors its
            // error flow: `[10 / 0]` yields a #zeroDivide value, and an error
            // receiver propagates (doesNotUnderstand: returns self) instead of
            // being untagged into garbage. (Arithmetic was once inlined as raw C
            // math, which bypassed both — that was the F7 codegen gap.)
            return emitGeneralSend(buf, recv, op, &[_]Oop{arg});
        }

        if (mr == r_unaryMsg) {
            return emitGeneralSend(buf, recv, text(msg), &[_]Oop{});
        }

        if (mr == r_keywordMsg) {
            // children alternate keyword / expr; selector = concat of keyword
            // texts (each "name:"), args = the expr nodes between them.
            const nc = nchild(msg);
            var sel: std.ArrayList(u8) = .{};
            defer sel.deinit(ca);
            var args: std.ArrayList(Oop) = .{};
            defer args.deinit(ca);
            var i: usize = 0;
            while (i + 1 < nc) : (i += 2) {
                sel.appendSlice(ca, text(child(msg, i))) catch @panic("codegen: oom");
                args.append(ca, child(msg, i + 1)) catch @panic("codegen: oom");
            }
            // `defineMethod:on:as:` is lowered to the 4-arg `…:source:` form,
            // threading the ORIGINAL source text so the method's portable source
            // lands in the store (docs/40 §4). `node` is the whole send — its span
            // is the re-evaluable definition.
            if (std.mem.eql(u8, sel.items, "defineMethod:on:as:") and args.items.len == 3) {
                // Artifact mode (docs/40 slice 4): emit the method PRE-COMPILED —
                // the trampoline as a static fn + a direct `birnam_method` install,
                // so loading the artifact installs mapped code with no recompile.
                // The JIT/source-recording path is used only for LIVE definition.
                if (artifact_mode) return emitDefineMethodInstallable(buf, args.items);
                return emitDefineMethodSourced(buf, recv, args.items, node);
            }
            return emitGeneralSend(buf, recv, sel.items, args.items);
        }

        return false;
    }
    if (r == r_block) {
        return emitBlock(buf, node);
    }
    if (r == r_vectorLit) {
        return emitVectorLit(buf, node, nchild(node));
    }
    if (r == r_mapLit) {
        return emitMapLit(buf, node, nchild(node));
    }
    if (r == r_logicVar) {
        // `?p` → `[LogicVar named: #p]` — the canonical query variable for name p.
        const name = text(child(node, 0)); // the ident after '?'
        put(buf, "birnam_send(");
        putGlobalRef(buf, "LogicVar");
        put(buf, ", birnam_intern(\"named:\"), (void*[]){birnam_intern(\"");
        putCStr(buf, name);
        put(buf, "\")}, 1L)");
        return true;
    }
    noteError("cannot compile this expression (not in the compilable subset)", spanStart(node));
    return false;
}

/// Emit a `#[e0 e1 …]` literal as a persistent-`add:` chain over a fresh empty
/// vector: `[[[Vector new] add: e0] add: e1] …`. `upto` is the prefix length —
/// recursion peels the last element so the receiver (all earlier adds) is emitted
/// first. `add:` is persistent, so each link yields a new vector; the global
/// `Vector` resolves to the collection factory.
fn emitVectorLit(buf: *std.ArrayList(u8), node: Oop, upto: usize) bool {
    if (upto == 0) {
        put(buf, "birnam_send(");
        putGlobalRef(buf, "Vector");
        put(buf, ", birnam_intern(\"new\"), 0, 0L)");
        return true;
    }
    put(buf, "birnam_send(");
    if (!emitVectorLit(buf, node, upto - 1)) return false; // receiver: earlier adds
    put(buf, ", birnam_intern(\"add:\"), (void*[]){");
    if (!emit(buf, child(node, upto - 1))) return false; // the last element
    put(buf, "}, 1L)");
    return true;
}

/// Emit a `{k0: v0, k1: v1, …}` literal as a persistent-`at:put:` chain over a
/// fresh empty map: `[[[Map new] at: k0 put: v0] at: k1 put: v1] …`. Mirrors
/// `emitVectorLit`; each child is a `mapEntry` node (key child 0, value child 1).
fn emitMapLit(buf: *std.ArrayList(u8), node: Oop, upto: usize) bool {
    if (upto == 0) {
        put(buf, "birnam_send(");
        putGlobalRef(buf, "Map");
        put(buf, ", birnam_intern(\"new\"), 0, 0L)");
        return true;
    }
    put(buf, "birnam_send(");
    if (!emitMapLit(buf, node, upto - 1)) return false; // receiver: earlier puts
    put(buf, ", birnam_intern(\"at:put:\"), (void*[]){");
    const entry = child(node, upto - 1);
    if (!emit(buf, child(entry, 0))) return false; // key
    put(buf, ", ");
    if (!emit(buf, child(entry, 1))) return false; // value
    put(buf, "}, 2L)");
    return true;
}

// -- blocks (closures) -------------------------------------------------------
//
// A block `[:a :b | body]` compiles to a file-scope C function with the Method
// ABI; its params bind to args[i] and its captured free variables to slots in
// the block object's payload (read via `self`). The block LITERAL evaluates to a
// statement-expression that lazily builds the block's vtable (installing the fn
// under the arity-matched value/value:/value:value: selector) and allocates an
// instance holding the current values of the captured variables. Invoking a block
// (`[blk value: x]`) is just an ordinary keyword send — no special handling.

/// Collect the value-position refs of a single message node (binary/unary/keyword);
/// selector positions are not refs. Shared by the single-send and chained-send
/// (`r_message`) paths so capture analysis scans exactly what `emit` references.
fn collectRefsMsg(msg: Oop, set: *std.ArrayList([]const u8)) void {
    const mr = ruleOf(msg);
    if (mr == r_binaryMsg) {
        collectRefs(child(msg, 1), set); // arg only (op is not a ref)
    } else if (mr == r_keywordMsg) {
        const nc = nchild(msg);
        var i: usize = 1;
        while (i < nc) : (i += 2) collectRefs(child(msg, i), set); // args
    } // unaryMsg: selector ident is not a ref
}

/// Collect identifiers referenced as VALUES (receivers / args / primaries) under
/// `node`, skipping selector positions. Descends into nested blocks so a block's
/// transitive free variables (needed by inner blocks) are surfaced too.
fn collectRefs(node0: Oop, set: *std.ArrayList([]const u8)) void {
    const node = skip(node0);
    const r = ruleOf(node);
    if (r == r_ident) {
        // nil/true/false are literals, not free variables — never capture them.
        if (literalSingleton(text(node)) == null) setAdd(set, text(node));
        return;
    }
    if (r == r_number or r == r_float or r == r_string) return;
    if (r == r_send) {
        collectRefs(child(node, 0), set); // receiver
        const msg = skip(child(node, 1));
        const mr = ruleOf(msg);
        if (mr == r_message) {
            // A chain of same-category messages (>1 child, `emitSendChain`): each
            // child is a binary/unary/keyword message applied left-to-right. The
            // single-message branch below sees only one message; without this the
            // chain's args are never scanned and their free variables go
            // uncaptured — the generated C then references an undeclared name.
            const nc = nchild(msg);
            var i: usize = 0;
            while (i < nc) : (i += 1) collectRefsMsg(skip(child(msg, i)), set);
        } else {
            collectRefsMsg(msg, set);
        }
        return;
    }
    if (r == r_block) {
        var i: usize = 0;
        const nc = nchild(node);
        while (i < nc) : (i += 1) {
            if (ruleOf(child(node, i)) == r_statements) collectRefsStmts(child(node, i), set);
        }
        return;
    }
    if (r == r_vectorLit) {
        const nc = nchild(node);
        var i: usize = 0;
        while (i < nc) : (i += 1) collectRefs(child(node, i), set); // element exprs
        return;
    }
    if (r == r_mapLit) {
        const nc = nchild(node);
        var i: usize = 0;
        while (i < nc) : (i += 1) {
            const entry = child(node, i); // mapEntry: key + value exprs
            collectRefs(child(entry, 0), set);
            collectRefs(child(entry, 1), set);
        }
        return;
    }
}

fn collectRefsStmts(statements: Oop, set: *std.ArrayList([]const u8)) void {
    const ns = nchild(statements);
    var i: usize = 0;
    while (i < ns) : (i += 1) {
        const stmt = child(statements, i);
        if (nchild(stmt) < 1) continue;
        const inner = child(stmt, 0);
        if (ruleOf(inner) == r_letBinding) {
            collectRefs(child(inner, 1), set); // RHS only (the let name is a binder)
        } else {
            collectRefs(inner, set);
        }
    }
}

/// True if `statements` has a top-level `let` binding `name` (which would shadow
/// an enclosing variable of the same name, so it must not be captured).
fn hasTopLevelLet(statements: Oop, name: []const u8) bool {
    const ns = nchild(statements);
    var i: usize = 0;
    while (i < ns) : (i += 1) {
        const stmt = child(statements, i);
        if (nchild(stmt) < 1) continue;
        const inner = child(stmt, 0);
        if (ruleOf(inner) == r_letBinding and std.mem.eql(u8, text(child(inner, 0)), name)) return true;
    }
    return false;
}

/// Emit a block literal: append its function definition to `fn_defs` and emit a
/// statement-expression (building vtable + capturing instance) into `buf`.
fn emitBlock(buf: *std.ArrayList(u8), blockNode: Oop) bool {
    const nc = nchild(blockNode);

    // Classify children: leading idents are params; a `statements` child is the
    // body (optional).
    var params: std.ArrayList([]const u8) = .{};
    defer params.deinit(ca);
    var body: Oop = null;
    {
        var i: usize = 0;
        while (i < nc) : (i += 1) {
            const cnode = child(blockNode, i);
            const cr = ruleOf(cnode);
            if (cr == r_ident) {
                params.append(ca, text(cnode)) catch @panic("codegen: oom");
            } else if (cr == r_statements) {
                body = cnode;
            }
        }
    }

    // Capture set: enclosing variables referenced in the body, minus this block's
    // own params and top-level lets (which provide those names locally). Computed
    // against the CURRENT (enclosing) lexical scope, in scope order for stable
    // slot indices.
    var refs: std.ArrayList([]const u8) = .{};
    defer refs.deinit(ca);
    if (body != null) collectRefsStmts(body, &refs);
    var caps: std.ArrayList([]const u8) = .{};
    defer caps.deinit(ca);
    for (bound_names.items) |name| {
        if (!listContains(refs.items, name)) continue;
        if (listContains(params.items, name)) continue;
        if (body != null and hasTopLevelLet(body, name)) continue;
        caps.append(ca, name) catch @panic("codegen: oom");
    }

    const n = blk_counter;
    blk_counter += 1;

    // Emit the block's function body into a temp buffer; nested blocks append
    // their defs to fn_defs DURING this, so they land before ours (post-order).
    // The block body sees only params + captures (then its own lets), so swap the
    // lexical scope.
    const saved = ca.dupe([]const u8, bound_names.items) catch @panic("codegen: oom");
    defer ca.free(saved);
    bound_names.clearRetainingCapacity();
    for (params.items) |p| bound_names.append(ca, p) catch @panic("codegen: oom");
    for (caps.items) |cName| bound_names.append(ca, cName) catch @panic("codegen: oom");

    var body_buf: std.ArrayList(u8) = .{};
    defer body_buf.deinit(ca);
    for (params.items, 0..) |p, i| {
        putFmt(&body_buf, "void* b_{s} = args[{d}];\n", .{ p, i });
    }
    for (caps.items, 0..) |cName, i| {
        putFmt(&body_buf, "void* b_{s} = ((void**)self)[{d}];\n", .{ cName, i });
    }
    const body_ok = if (body != null) emitBody(&body_buf, body, false) else blk: {
        put(&body_buf, "return birnam_nil();\n");
        break :blk true;
    };

    // Restore the enclosing scope regardless of outcome.
    bound_names.clearRetainingCapacity();
    bound_names.appendSlice(ca, saved) catch @panic("codegen: oom");
    if (!body_ok) return false;

    // Assemble the file-scope function definition and append it to fn_defs.
    putFmt(&fn_defs, "void* __blk{d}(void* self, void** args, long n){{ (void)self;(void)args;(void)n;\n", .{n});
    put(&fn_defs, body_buf.items);
    put(&fn_defs, "}\n");

    // The arity-matched value selector: "value", "value:", "value:value:", …
    var sel: std.ArrayList(u8) = .{};
    defer sel.deinit(ca);
    if (params.items.len == 0) {
        put(&sel, "value");
    } else {
        for (params.items) |_| put(&sel, "value:");
    }

    // Emit the creating statement-expression into the current function body.
    putFmt(buf, "({{ static void* __vt{d} = 0; if(!__vt{d}){{ __vt{d} = birnam_proto(birnam_objectVt()); birnam_method(__vt{d}, birnam_intern(\"", .{ n, n, n, n });
    putCStr(buf, sel.items);
    putFmt(buf, "\"), (void*)&__blk{d}); birnam_layoutAllOops(__vt{d}); }} void* __o{d} = birnam_alloc(__vt{d}, {d}*sizeof(void*));\n", .{ n, n, n, n, caps.items.len });
    for (caps.items, 0..) |cName, i| {
        putFmt(buf, "((void**)__o{d})[{d}] = b_{s};\n", .{ n, i, cName });
    }
    putFmt(buf, "__o{d}; }})", .{n});
    return true;
}

/// Emit a `statements` node as the body of the enclosing C function: each `let`
/// becomes a C local `void* b_<name> = …;` (and binds the name for later
/// statements), each bare expression is evaluated, and the LAST statement's value
/// is `return`ed. A `let` as the final statement returns its bound value; an
/// empty body returns nil. Returns false if any statement is out of subset.
///
/// With `persist` true (REPL mode — only ever the OUTERMOST statements, never a
/// block body), each top-level `let` ALSO writes its value to the Globals namespace
/// so it survives to the next evaluation; block-local lets stay local.
fn emitBody(buf: *std.ArrayList(u8), statements: Oop, persist: bool) bool {
    const ns = nchild(statements);
    var i: usize = 0;
    while (i < ns) : (i += 1) {
        const last = (i == ns - 1);
        const stmt = child(statements, i);
        if (nchild(stmt) < 1) return false;
        const inner = child(stmt, 0); // letBinding | expr

        if (ruleOf(inner) == r_letBinding) {
            const name = text(child(inner, 0));
            const rhs = child(inner, 1);
            put(buf, "void* b_");
            put(buf, name);
            put(buf, " = ");
            if (!emit(buf, rhs)) return false; // RHS sees only earlier bindings
            put(buf, ";\n");
            bound_names.append(ca, name) catch @panic("codegen: oom");
            if (persist) {
                put(buf, "birnam_global_set(\"");
                putCStr(buf, name);
                put(buf, "\", b_");
                put(buf, name);
                put(buf, ");\n");
                // Record the binding's source too (docs/40 §6): an image re-derives
                // a block-valued global from this when its value can't be serialized.
                const rhs_src = text(rhs);
                put(buf, "birnam_record_global_src(\"");
                putCStr(buf, name);
                put(buf, "\", \"");
                putCStr(buf, rhs_src);
                var nb: [24]u8 = undefined;
                put(buf, std.fmt.bufPrint(&nb, "\", {d}L);\n", .{rhs_src.len}) catch return false);
            }
            if (last) {
                put(buf, "return b_");
                put(buf, name);
                put(buf, ";\n");
            }
        } else if (last) {
            put(buf, "return ");
            if (!emit(buf, inner)) return false;
            put(buf, ";\n");
        } else {
            put(buf, "(void)(");
            if (!emit(buf, inner)) return false;
            put(buf, ");\n");
        }
    }
    return true;
}

// -- [Codegen eval: src] -----------------------------------------------------

fn mEval(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return runProgram(a[0], false);
}

/// `[Codegen evalPersisting: src]` — like `eval:`, but top-level `let`s persist to
/// the Globals namespace (so they survive to the next call). The REPL's evaluator.
fn mEvalPersisting(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return runProgram(a[0], true);
}

/// `[Codegen compile: src output: path]` — compile a birnam program to a shared
/// library on disk (the relocatable artifact form, docs/40 §3a). The artifact
/// leaves the `birnam_*` shims as undefined externals — the dynamic loader
/// resolves them against the running process at dlopen (tcc.zig/codegen.zig
/// export them as dynamic symbols) — and re-derives its global handles by name
/// in `__birnam_unit_init`, so nothing in it depends on this process's
/// addresses. Load with `[Codegen load: path]`. Returns the path, or nil.
fn mCompileOutput(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    var buf = emitProgram(a[0], false, true) orelse return g_api.nilObj;
    defer buf.deinit(ca);
    const c_src = g_api.mkString(buf.items.ptr, buf.items.len);
    return g_api.send(tcc, g_api.intern("compile:output:as:"), &[_]Oop{ c_src, a[1], g_api.intern("dll") }, 3);
}

/// `[Codegen compileImageCode: path]` — the image's `.code` section (docs/40
/// slice 4). Gather EVERY recorded method source (the `__methods__` facts — the
/// durable source of truth, reliable across image boots, not an in-memory list)
/// into one program and compile it to a relocatable artifact at `path` in the
/// INSTALLABLE form: loading it installs every method with no recompile. Returns
/// the path, or nil if there are no methods or the compile fails.
fn mCompileImageCode(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const nAttrs = g_api.smallIntValue(g_api.send(g_store, g_api.intern("attrCount:"), &[_]Oop{g_methods_entity}, 1));
    if (nAttrs <= 0) return g_api.nilObj;
    var prog: std.ArrayList(u8) = .{};
    defer prog.deinit(ca);
    var i: i64 = 0;
    while (i < nAttrs) : (i += 1) {
        const attr = g_api.send(g_store, g_api.intern("attrAt:index:"), &[_]Oop{ g_methods_entity, g_api.mkSmallInt(i) }, 2);
        const src = g_api.send(g_store, g_api.intern("currentValue:attr:"), &[_]Oop{ g_methods_entity, attr }, 2);
        if (src == g_api.nilObj) continue;
        var len: usize = 0;
        const p = g_api.stringBytes(src, &len);
        prog.appendSlice(ca, p[0..len]) catch @panic("codegen: oom");
        prog.appendSlice(ca, ";\n") catch @panic("codegen: oom");
    }
    if (prog.items.len == 0) return g_api.nilObj;
    const progStr = g_api.mkString(prog.items.ptr, prog.items.len);
    var buf = emitProgram(progStr, false, true) orelse return g_api.nilObj;
    defer buf.deinit(ca);
    const c_src = g_api.mkString(buf.items.ptr, buf.items.len);
    return g_api.send(tcc, g_api.intern("compile:output:as:"), &[_]Oop{ c_src, a[0], g_api.intern("dll") }, 3);
}

/// `[Codegen replayMethods]` — the image's REPLAY path (docs/40 §4): rebuild the
/// whole library from source by re-evaluating every recorded method (the
/// `__methods__` facts), recompiling as it goes. Used when the compiled `.code`
/// section can't be trusted (arch/ABI mismatch) or the user asked to rebuild —
/// and it subsumes the stdlib, since the stdlib is itself all defineMethod. All
/// sources are gathered FIRST (re-eval re-records into `__methods__`, so we must
/// not iterate it while mutating). Answers the count replayed.
fn mReplayMethods(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const nAttrs = g_api.smallIntValue(g_api.send(g_store, g_api.intern("attrCount:"), &[_]Oop{g_methods_entity}, 1));
    var srcs: std.ArrayList(Oop) = .{};
    defer srcs.deinit(ca);
    var i: i64 = 0;
    while (i < nAttrs) : (i += 1) {
        const attr = g_api.send(g_store, g_api.intern("attrAt:index:"), &[_]Oop{ g_methods_entity, g_api.mkSmallInt(i) }, 2);
        const src = g_api.send(g_store, g_api.intern("currentValue:attr:"), &[_]Oop{ g_methods_entity, attr }, 2);
        if (src != g_api.nilObj) srcs.append(ca, src) catch @panic("codegen: oom");
    }
    for (srcs.items) |src| _ = g_api.send(self, g_api.intern("eval:"), &[_]Oop{src}, 1);
    return g_api.mkSmallInt(@intCast(srcs.items.len));
}

/// `[Codegen deriveImageGlobals]` — the image-load path for block-valued (and any
/// deferred) globals (docs/40 §6). For every recorded top-level global source (the
/// `__globalsrc__` facts): if the global is currently UNBOUND — its value fact was
/// deferred at save because a closure can't be serialized — re-eval `let <name> :=
/// <src>` to rebuild it in this process (a fresh closure over the now-mapped code).
/// Leaf globals that mapped via the fact stream stay bound and are left untouched.
/// Sources are gathered FIRST (re-eval re-records into `__globalsrc__`). Called by
/// the boot path after code is resolved (mapped or replayed), so a re-derived block
/// body can send freely. Answers the count re-derived.
fn mDeriveImageGlobals(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const nAttrs = g_api.smallIntValue(g_api.send(g_store, g_api.intern("attrCount:"), &[_]Oop{g_globalsrc_entity}, 1));
    var names: std.ArrayList(Oop) = .{};
    defer names.deinit(ca);
    var srcs: std.ArrayList(Oop) = .{};
    defer srcs.deinit(ca);
    var i: i64 = 0;
    while (i < nAttrs) : (i += 1) {
        const attr = g_api.send(g_store, g_api.intern("attrAt:index:"), &[_]Oop{ g_globalsrc_entity, g_api.mkSmallInt(i) }, 2);
        const src = g_api.send(g_store, sel_currentValue, &[_]Oop{ g_globalsrc_entity, attr }, 2);
        if (src == null or src == g_api.nilObj) continue;
        names.append(ca, attr) catch @panic("codegen: oom");
        srcs.append(ca, src) catch @panic("codegen: oom");
    }
    var count: i64 = 0;
    for (names.items, srcs.items) |nameSym, src| {
        // Already bound (a leaf global mapped via the fact stream)? Leave it — only
        // the deferred (un-serializable, absent) globals need re-derivation.
        const cur = g_api.send(g_store, sel_currentValue, &[_]Oop{ g_globals_entity, nameSym }, 2);
        if (cur != null) continue;
        var prog: std.ArrayList(u8) = .{};
        defer prog.deinit(ca);
        prog.appendSlice(ca, "let ") catch @panic("codegen: oom");
        prog.appendSlice(ca, std.mem.span(g_api.symbolName(nameSym))) catch @panic("codegen: oom");
        prog.appendSlice(ca, " := ") catch @panic("codegen: oom");
        var slen: usize = 0;
        const sp = g_api.stringBytes(src, &slen);
        prog.appendSlice(ca, sp[0..slen]) catch @panic("codegen: oom");
        const progStr = g_api.mkString(prog.items.ptr, prog.items.len);
        _ = g_api.send(self, g_api.intern("evalPersisting:"), &[_]Oop{progStr}, 1);
        count += 1;
    }
    return g_api.mkSmallInt(count);
}

const RTLD_LAZY: c_int = 1;

/// `[Codegen load: path]` — load a compiled-birnam artifact: dlopen it (the
/// loader resolves its `birnam_*` externals against this process), run its
/// `__birnam_unit_init` fix-up hook (re-derives the global handles here), then
/// evaluate its `__ev` entry point. Returns the program's value, or nil if the
/// library or either symbol is missing.
fn mLoadUnit(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    var len: usize = 0;
    const p = g_api.stringBytes(a[0], &len);
    const path = ca.allocSentinel(u8, len, 0) catch @panic("codegen: oom");
    defer ca.free(path);
    @memcpy(path[0..len], p[0..len]);

    const h = g_api.dlopen(path.ptr, RTLD_LAZY) orelse return g_api.nilObj;
    const init_p = g_api.dlsym(h, "__birnam_unit_init") orelse return g_api.nilObj;
    const init_fn: *const fn () callconv(.c) void = @ptrCast(@alignCast(init_p));
    init_fn();
    const ev_p = g_api.dlsym(h, "__ev") orelse return g_api.nilObj;
    const ev_fn: idapi.Method = @ptrCast(@alignCast(ev_p));
    return ev_fn(g_api.nilObj, &[_]Oop{}, 0);
}

/// Parse `src` and emit the complete C translation unit into a fresh buffer
/// (caller deinits). With `artifact` true, emit the RELOCATABLE spelling
/// (docs/40 §3a): the baked `__g<i>` linker handles become static slots that a
/// per-unit `__birnam_unit_init` hook re-derives by interning their names at
/// load time, so the output carries no address from this process and can be
/// written to disk, dlopen'd elsewhere, and fixed up there. The JIT keeps the
/// baked handles (`artifact` false) — nothing changes for a live session.
/// Returns null on a parse or emit failure.
fn emitProgram(src: Oop, persist: bool, artifact: bool) ?std.ArrayList(u8) {
    var len: usize = 0;
    const p = g_api.stringBytes(src, &len);
    src_bytes = p[0..len];
    clearError();

    const cst = s1(lang, "parse:", src);
    if (cst == g_api.nilObj) {
        // The parser tracks the furthest offset it reached — a good "the syntax
        // error is around here" pointer.
        const off: usize = @intCast(g_api.smallIntValue(s0(lang, "furthestFail")));
        noteError("syntax error", off);
        return null;
    }

    // program → statements → statement+ ; an empty program has no value.
    if (nchild(cst) < 1) return null;
    const statements = child(cst, 0);
    if (nchild(statements) < 1) return null;

    // From here a failure is an emit (out-of-subset) failure; the deepest emitter
    // notes the precise node. A final fallback guarantees a message either way.

    bound_names.clearRetainingCapacity();
    fn_defs.clearRetainingCapacity();
    blk_counter = 0;
    artifact_method_counter = 0;
    baked_globals.clearRetainingCapacity();
    global_memo.clearRetainingCapacity();
    artifact_mode = artifact;
    defer artifact_mode = false;

    // The `__ev` function body is emitted first (which populates fn_defs with any
    // block functions, in post-order); the final source is prelude + fn_defs +
    // __ev so every block fn is defined before it's referenced.
    var ev: std.ArrayList(u8) = .{};
    defer ev.deinit(ca);
    put(&ev, "void* __ev(void* self, void** args, long n){ (void)self;(void)args;(void)n;\n");
    if (!emitBody(&ev, statements, persist)) {
        noteError("cannot compile this program (not in the compilable subset)", spanStart(statements));
        return null;
    }
    put(&ev, "}");

    var buf: std.ArrayList(u8) = .{};
    put(&buf,
        \\extern void* birnam_mkSmallInt(long v);
        \\#define MKV(x) (birnam_mkSmallInt((long)(x)))
        \\extern void* birnam_send(void* rcv, void* sel, void** args, long n);
        \\extern void* birnam_intern(const char* name);
        \\extern void* birnam_mkString(const char* p, long n);
        \\extern void* birnam_mkFloat(double v);
        \\extern void* birnam_nil(void);
        \\extern void* birnam_true(void);
        \\extern void* birnam_false(void);
        \\extern void* birnam_proto(void* parent);
        \\extern void* birnam_ensurePrivateVtable(void* obj);
        \\extern void* birnam_objectVt(void);
        \\extern void birnam_method(void* vt, void* sel, void* fn);
        \\extern void birnam_layoutAllOops(void* vt);
        \\extern void* birnam_alloc(void* vt, long nbytes);
        \\extern void* birnam_global(const char* name);
        \\extern void* birnam_global_b(void* sym);
        \\extern void birnam_global_set(const char* name, void* value);
        \\extern void birnam_record_global_src(const char* name, const char* src, long n);
        \\
    );
    if (artifact) {
        // Each baked handle is a static slot `__birnam_unit_init` fills at load.
        for (0..baked_globals.items.len) |i| putFmt(&buf, "static void* __g{d};\n", .{i});
    } else {
        // Declare each baked global handle: `extern char __g<i>[];` — its ADDRESS
        // (the array decays) is the interned symbol the linker patches in, so a
        // read is `birnam_global_b((void*)__g<i>)` with no runtime intern.
        for (0..baked_globals.items.len) |i| putFmt(&buf, "extern char __g{d}[];\n", .{i});
    }
    put(&buf, fn_defs.items);
    put(&buf, ev.items);
    if (artifact) {
        // The load-time fix-up hook: re-derive every handle in the loading
        // process. Emitted even when empty so a loader can call it uniformly.
        put(&buf, "\nvoid __birnam_unit_init(void){\n");
        for (baked_globals.items, 0..) |sym, i| {
            putFmt(&buf, "  __g{d} = birnam_intern(\"", .{i});
            putCStr(&buf, std.mem.span(g_api.symbolName(sym)));
            put(&buf, "\");\n");
        }
        put(&buf, "}\n");
    }

    if (std.posix.getenv("CODEGEN_DUMP") != null) std.debug.print("---emitted C---\n{s}\n---------------\n", .{buf.items});
    return buf;
}

fn runProgram(src: Oop, persist: bool) Oop {
    var buf = emitProgram(src, persist, false) orelse return g_api.nilObj;
    defer buf.deinit(ca);

    // Staged compile: bind the runtime shims into the symbol window between
    // compile and relocate, so the emitted `birnam_*` calls resolve.
    const c_src = g_api.mkString(buf.items.ptr, buf.items.len);
    const unit = s0(tcc, "new");
    if (unit == g_api.nilObj) return g_api.nilObj;
    if (s1(unit, "compile:", c_src) == g_api.nilObj) {
        // The subset parsed and emitted but the generated C didn't compile — a
        // codegen bug or an emit gap, not user syntax. Surface it rather than a
        // silent nil (tcc already printed the C diagnostic to stderr).
        noteError("internal error: generated code failed to compile (please report)", 0);
        return g_api.nilObj;
    }
    _ = s0(unit, "bindRuntime");
    addShim(unit, "birnam_global", @ptrCast(&rtGlobal));
    addShim(unit, "birnam_global_b", @ptrCast(&rtGlobalB));
    addShim(unit, "birnam_global_set", @ptrCast(&rtGlobalSet));
    addShim(unit, "birnam_record_global_src", @ptrCast(&rtRecordGlobalSrc));
    // Patch each baked handle `__g<i>` to its interned symbol's address.
    for (baked_globals.items, 0..) |sym, i| {
        var nb: [16]u8 = undefined;
        const nm = std.fmt.bufPrint(&nb, "__g{d}", .{i}) catch return g_api.nilObj;
        addShim(unit, nm, @ptrCast(sym.?));
    }
    if (s0(unit, "relocate") == g_api.nilObj) return g_api.nilObj;

    if (ev_harness_vt == null) ev_harness_vt = g_api.proto(g_api.object_vt);
    const vt = ev_harness_vt;
    const sel = g_api.intern("__ev");
    const fname = "__ev";
    _ = g_api.send(unit, g_api.intern("install:as:on:"), &[_]Oop{ g_api.mkString(fname, fname.len), sel, vt }, 3);
    const inst = g_api.alloc(vt, 0);
    const result = g_api.send(inst, sel, &[_]Oop{}, 0);

    // Reclaim this eval's native code if it can't be referenced after the call.
    // A block literal installs a `value` method pointing into THIS unit's code
    // (emitBlock → birnam_method), so a returned/stored closure would keep that
    // pointer live — keep the unit then. With no blocks (blk_counter == 0) the
    // result is pure data built through the runtime shims, so the code is dead:
    // mark the unit disposable and let the collector `tcc_delete` it. Bounds the
    // per-eval TccState leak for the common (no-closure) REPL line.
    if (blk_counter == 0) _ = s0(unit, "disposable");

    // Reaching here means THIS top-level program compiled, relocated, and ran. The
    // program may itself have called `[Codegen compile: …]` on bad source (e.g.
    // aot.birnam probing error handling), which left the shared error flag set — but
    // that is a nested compile the program handled, not a failure of this eval. Clear
    // it so `[Codegen lastError]` reflects only the top-level outcome (audit §B9): a
    // failed top-level compile returns nil BEFORE this point, with its note intact.
    clearError();
    return result;
}

// -- bringup / module entrypoint --------------------------------------------

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    lang = api.import("Lang") orelse @panic("codegen: Lang not loaded");
    tcc = api.import("Tcc") orelse @panic("codegen: Tcc not loaded");
    g_store = api.import("Store") orelse @panic("codegen: Store not loaded");
    g_globals_entity = api.intern("__globals__");
    g_methods_entity = api.intern("__methods__");
    g_globalsrc_entity = api.intern("__globalsrc__");
    sel_currentValue = api.intern("currentValue:attr:");
    sel_addSymbol = api.intern("addSymbol:value:");
    sel_assert = api.intern("assert:attr:value:");
    sel_addMethodOn = api.intern("addMethod:on:with:");

    r_expr = api.intern("expr");
    r_primary = api.intern("primary");
    r_message = api.intern("message");
    r_number = api.intern("number");
    r_float = api.intern("float");
    r_send = api.intern("send");
    r_binaryMsg = api.intern("binaryMsg");
    r_unaryMsg = api.intern("unaryMsg");
    r_keywordMsg = api.intern("keywordMsg");
    r_ident = api.intern("ident");
    r_string = api.intern("string");
    r_symbol = api.intern("symbol");
    r_letBinding = api.intern("letBinding");
    r_block = api.intern("block");
    r_statements = api.intern("statements");
    r_vectorLit = api.intern("vectorLit");
    r_mapLit = api.intern("mapLit");
    r_mapEntry = api.intern("mapEntry");
    r_logicVar = api.intern("logicVar");

    const reloading = api.isExported("Codegen") != 0;
    if (reloading) {
        codegen_vt = api.import("CodegenVt");
        codegen_obj = api.import("Codegen");
        reflect_vt = api.import("ReflectVt");
        reflect_obj = api.import("Reflect");
    } else {
        codegen_vt = api.proto(api.object_vt);
        codegen_obj = api.alloc(codegen_vt, 0);
        reflect_vt = api.proto(api.object_vt);
        reflect_obj = api.alloc(reflect_vt, 0);
    }

    api.method(codegen_vt, api.intern("eval:"), &mEval);
    api.method(codegen_vt, api.intern("evalPersisting:"), &mEvalPersisting);
    api.method(codegen_vt, api.intern("compile:output:"), &mCompileOutput);
    api.method(codegen_vt, api.intern("compileImageCode:"), &mCompileImageCode);
    api.method(codegen_vt, api.intern("replayMethods"), &mReplayMethods);
    api.method(codegen_vt, api.intern("deriveImageGlobals"), &mDeriveImageGlobals);
    api.method(codegen_vt, api.intern("lastError"), &mLastError);
    api.method(codegen_vt, api.intern("load:"), &mLoadUnit);
    api.method(reflect_vt, api.intern("defineMethod:on:as:"), &mDefineMethod);
    api.method(reflect_vt, api.intern("defineMethod:on:as:source:"), &mDefineMethodSourced);

    if (!reloading) {
        _ = api.exportObj("CodegenVt", codegen_vt);
        _ = api.exportObj("Codegen", codegen_obj);
        _ = api.exportObj("ReflectVt", reflect_vt);
        _ = api.exportObj("Reflect", reflect_obj);
        // Object/Integer/String and the other floor names are exported by libid
        // now, bound to their PROTOTYPES (docs/design/41 §A) — not re-exported here.
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
