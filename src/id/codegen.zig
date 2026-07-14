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
var sel_rawCurrentValue: Oop = null; // the raw tier: absence = Zig null (rtGlobal/rtGlobalB)
var sel_addSymbol: Oop = null;
var sel_assert: Oop = null;
var sel_addMethodOn: Oop = null;
var method_blk_counter: usize = 0; // unique keys for stashed method blocks
var g_methods_entity: Oop = null; // the reserved entity method-source facts hang off of
var g_methodops_entity: Oop = null; // post-cutover sequenced install/remove sources
var g_globalsrc_entity: Oop = null; // the reserved entity top-level global-source facts hang off of
var artifact_method_counter: usize = 0; // per-emit ids for installable artifact methods
var method_op_counter: usize = 0; // tie-breaker within a Store basis
var suppress_methodop_recording = false; // image replay/load must not log its own recovery
var artifact_record_ops = true; // false only for compacted image-code emission
var candidate_preparing = false;
var candidate_commit_in_progress = false;
var image_dispatch_recovery = false;
var initial_generation_sealed = false;
const RuntimePhase = enum { bootstrapping, running, quiescent, image_recovery };
var runtime_phase: RuntimePhase = .bootstrapping;

const ARTIFACT_ABI = "birnam-artifact-generation-v1";
const CANDIDATE_ABI: u32 = 3;
const CANDIDATE_PLATFORM = "aarch64-macos";
const CANDIDATE_MAGIC = "BIRNAM-CANDIDATE-2";

const CandidateOpKind = enum { install, remove };
const CandidateOp = struct {
    target_name: Oop,
    selector: Oop,
    kind: CandidateOpKind,
    source: Oop,
    opened_captured: bool = false,
    opened_target: Oop = null,
    opened_vt: Oop = null,
    opened_present: bool = false,
    opened_method: Oop = null,
    target: Oop = null,
    vt: Oop = null,
    prior_present: bool = false,
    prior_method: Oop = null,
    prior_source: Oop = null,
    post_present: bool = false,
    post_method: Oop = null,
    post_log_source: Oop = null,
};

const CandidateState = struct {
    unit: Oop,
    name: Oop,
    source: Oop,
    author_path: []u8,
    content_path: ?[]u8 = null,
    checksum: [64]u8,
    source_digest: [64]u8 = [_]u8{'0'} ** 64,
    non_method_digest: [64]u8 = [_]u8{'0'} ** 64,
    method_digest: [64]u8 = [_]u8{'0'} ** 64,
    revision_digest: [64]u8 = [_]u8{'0'} ** 64,
    exported_manifest: []u8,
    requires: std.ArrayList(Oop) = .{},
    ops: std.ArrayList(CandidateOp) = .{},
    handle: ?*anyopaque = null,
    validate_fn: ?idapi.Method = null,
    activate_fn: ?idapi.Method = null,
    prepare_fn: ?idapi.Method = null,
    prepared_token: Oop = null,
    prepared_rollback: bool = false,
    prepared_publication: ?CandidatePublication = null,
    active: bool = false,
    predecessor: ?*CandidateState = null,
    prepared_predecessor: ?*CandidateState = null,
};

const PublicationAction = enum { activate, rollback };

/// Oops copied out of the pure-Birnam publication plan before candidate
/// preparation.  Keeping this positional and native makes the eventual commit a
/// mechanical Store projection: no replaceable CompiledUnit method runs after
/// the first dispatch mutation.
const CandidatePublication = struct {
    action: PublicationAction,
    enforce_lifecycle: bool,
    unit: Oop,
    name: Oop,
    prior: Oop,
    event: Oop,
    basis: Oop,
    expected_generation: Oop,
    successor_generation: Oop,
};

const TokenPayload = extern struct {
    unit: Oop,
    consumed: usize,
};

var candidate_states: std.AutoHashMapUnmanaged(Oop, *CandidateState) = .{};
// Complete authored module documents collected during boot. This is staging,
// not durable authority: `initialGenerationProjection` seals these values into
// G0, after which image/save/evolution read the generation's source map.
var boot_module_sources: std.AutoHashMapUnmanaged(Oop, Oop) = .{};
var candidate_token_vt: Oop = null;
var candidate_roots_registered = false;

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
var candidate_artifact_mode: bool = false;
// Per-unit salt for the artifact method-block GLOBAL keys (`__amblk`). Those keys
// live in the process-wide `birnam_global` string namespace, so without a salt two
// separately-compiled dylibs (stdlib, toml, config) would both write `__amblk0…`
// and the later-loaded one would clobber the earlier's blocks — a method would then
// fetch the wrong-arity block and DNU on `value:value:`. A hash of the unit's source
// makes the keys unique per module (and stable across recompiles of the same source).
var artifact_salt: u64 = 0;

// The module name for the current artifact compile, or null for a non-module
// artifact (the image `.code` section). Set by `mCompileOutput` (the module-dylib
// path) and read by `emitProgram` to self-record the whole-module source as a
// `__modsrc__` fact at load. Points into the caller's out-path bytes, valid for the
// synchronous emitProgram call; cleared right after.
var g_module_name: ?[]const u8 = null;

const CandidateRootKey = struct { salt: u64, index: usize };
var candidate_roots: std.AutoHashMapUnmanaged(CandidateRootKey, Oop) = .{};

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
    // The RAW read (`rawCurrentValue:attr:`): absence is Zig null (≠ the `nilObj`
    // singleton), so a present binding — even one bound to nil — is non-null and
    // wins over the exported env. The nil-normalised `currentValue:attr:` surface
    // can't serve here (an unbound name would read as bound-to-nil and shadow
    // every export), and a `hasValue:` pre-check would double this hot path.
    const cur = g_api.send(g_store, sel_rawCurrentValue, &[_]Oop{ g_globals_entity, sym }, 2);
    if (cur != null) return cur;
    if (g_api.isExported(name) != 0) return g_api.import(name);
    return g_api.nilObj;
}

/// `birnam_global_b(sym)` — the baked-handle read: `sym` is an already-interned
/// selector (resolved at compile time, address patched in by the linker), so this
/// skips the runtime intern that `rtGlobal` does. Same late-bound resolution
/// otherwise: the Globals store wins, else a module facade / seeded type, else nil.
fn rtGlobalB(sym: Oop) callconv(.c) Oop {
    const cur = g_api.send(g_store, sel_rawCurrentValue, &[_]Oop{ g_globals_entity, sym }, 2);
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

/// `birnam_is_error(v)` — 1 iff `v` is an error VALUE (its vtable IS the error
/// module's ErrorVt; a bare-floor boot with no error module answers 0). A raw
/// vt compare, not an `isError` send: top-level statement values include raw
/// vtables (the artifact defineMethod lowering answers `_vt`), which must not
/// be dispatched into. Emitted programs check every DISCARDED top-level
/// statement result through this and abort with the error instead: discarding
/// an error silently is the one illegal fate for one — the same contract the
/// loop primitives keep ("a body error stops the loop and propagates").
fn rtIsError(v: Oop) callconv(.c) c_long {
    if (v == null or v == g_api.nilObj) return 0;
    const evt = g_api.import("ErrorVt");
    if (evt == null or evt == g_api.nilObj) return 0;
    return if (g_api.vtableOf(v) == evt) 1 else 0;
}

/// Artifact user-method install: mutate through Store's tracked MOP and append the
/// same sequenced portable operation as live source evaluation. `n == 0` is the
/// compacted image path: its source is already present in the loaded facts.
fn rtInstallMethod(vt: Oop, sel: Oop, method: Oop, src: [*]const u8, n: c_long) callconv(.c) Oop {
    if (candidate_preparing) return g_store;
    const result = g_api.send(g_store, g_api.intern("addMethod:on:with:"), &[_]Oop{ sel, vt, method }, 3);
    if (n > 0) recordMethodOp(g_api.mkString(src, @intCast(n)));
    return result;
}

/// Artifact exact removal with optional post-cutover operation recording.
fn rtRemoveMethod(target: Oop, sel: Oop, src: [*]const u8, n: c_long) callconv(.c) Oop {
    if (candidate_preparing) return g_store;
    const vt = g_api.vtableOf(target);
    if (vt == null or g_api.vtableOwner(vt) != target) return g_api.nilObj;
    const result = g_api.send(g_store, g_api.intern("removeMethod:on:"), &[_]Oop{ sel, vt }, 2);
    if (result == g_store and n > 0) recordMethodOp(g_api.mkString(src, @intCast(n)));
    return result;
}

fn rtCandidateRootSet(salt: u64, index: c_long, value: Oop) callconv(.c) Oop {
    candidate_roots.put(ca, .{ .salt = salt, .index = @intCast(index) }, value) catch @panic("codegen: oom");
    return value;
}

fn rtCandidateRootGet(salt: u64, index: c_long) callconv(.c) Oop {
    return candidate_roots.get(.{ .salt = salt, .index = @intCast(index) }) orelse g_api.nilObj;
}

fn rtCandidatePrepareMode(on: c_long) callconv(.c) void {
    candidate_preparing = on != 0;
}

fn authorizeManagedMethodMutation() callconv(.c) Oop {
    if (candidate_commit_in_progress or image_dispatch_recovery or runtime_phase == .bootstrapping or
        runtime_phase == .quiescent or runtime_phase == .image_recovery) return g_api.trueObj;
    return candidateError("dispatchMutationRequiresQuiescence");
}

fn mAuthorizeManagedMethodMutation(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return authorizeManagedMethodMutation();
}

pub export fn birnam_codegen_enter_running() c_int {
    if (runtime_phase != .bootstrapping and runtime_phase != .quiescent) return 0;
    runtime_phase = .running;
    return 1;
}

pub export fn birnam_codegen_begin_quiescence() c_int {
    if (runtime_phase != .running) return 0;
    runtime_phase = .quiescent;
    return 1;
}

pub export fn birnam_codegen_end_quiescence() c_int {
    if (runtime_phase != .quiescent) return 0;
    runtime_phase = .running;
    return 1;
}

/// Candidate-owned validation calls back once per manifest operation. This is
/// deliberately independent of CandidateState: the artifact enumerates what it
/// believes it will mutate, while Codegen separately validates the inspected
/// envelope and captured base. Activation proceeds only when both agree.
fn rtCandidateValidateBinding(target_name: [*:0]const u8, selector_name: [*:0]const u8, arity: c_long, removal: c_long) callconv(.c) Oop {
    const target = globalBySymbol(g_api.intern(target_name));
    if (target == g_api.nilObj) return g_api.falseObj;
    const vt = g_api.vtableOf(target);
    if (vt == null or g_api.vtableOwner(vt) != target) return g_api.falseObj;
    const selector = g_api.intern(selector_name);
    if (selectorArity(selector) != @as(usize, @intCast(arity))) return g_api.falseObj;
    if (removal != 0 and directMethod(vt, selector) == null) return g_api.falseObj;
    return g_api.trueObj;
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
    @export(&rtRecordModuleSource, .{ .name = "birnam_record_module_source" });
    @export(&rtIsError, .{ .name = "birnam_is_error" });
    @export(&rtInstallMethod, .{ .name = "birnam_install_method" });
    @export(&rtRemoveMethod, .{ .name = "birnam_remove_method" });
    @export(&rtCandidateRootSet, .{ .name = "birnam_candidate_root_set" });
    @export(&rtCandidateRootGet, .{ .name = "birnam_candidate_root_get" });
    @export(&rtCandidatePrepareMode, .{ .name = "birnam_candidate_prepare_mode" });
    @export(&rtCandidateValidateBinding, .{ .name = "birnam_candidate_validate_binding" });
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
    return selectorArityStr(std.mem.span(g_api.symbolName(sel)));
}

/// The argument arity of a selector spelled as bytes: one arg per `:` for a
/// keyword selector, one for a binary-operator selector (leading non-letter),
/// none for a unary selector (leading letter, no `:`). This is the method block's
/// parameter count MINUS the receiver, so both method trampolines forward `self`
/// plus these args to the block as `value:`×(arity+1). The Oop-based
/// `selectorArity` and the AOT `emitDefineMethodInstallable` share this ONE rule —
/// a binary op like `|>` or `=` has zero colons but still takes one argument, so a
/// plain colon count (the old bug) dropped it and ran the block a param short.
fn selectorArityStr(sel: []const u8) usize {
    var colons: usize = 0;
    for (sel) |ch| {
        if (ch == ':') colons += 1;
    }
    if (colons > 0) return colons;
    if (sel.len == 0) return 0;
    const c0 = sel[0];
    const is_letter = (c0 >= 'a' and c0 <= 'z') or (c0 >= 'A' and c0 <= 'Z') or c0 == '_';
    return if (is_letter) 0 else 1;
}

const CandidateScan = struct {
    name: Oop = null,
    requires: std.ArrayList(Oop) = .{},
    ops: std.ArrayList(CandidateOp) = .{},
    source_digest: [64]u8 = undefined,
    non_method_digest: [64]u8 = undefined,
    method_digest: [64]u8 = undefined,
    revision_digest: [64]u8 = undefined,

    fn deinit(self: *CandidateScan) void {
        self.requires.deinit(ca);
        self.ops.deinit(ca);
    }
};

fn candidateError(kind: [*:0]const u8) Oop {
    const ef = g_api.import("Error") orelse return g_api.nilObj;
    return g_api.send(ef, g_api.intern("kind:"), &[_]Oop{g_api.intern(kind)}, 1);
}

fn keywordParts(msg: Oop, selector: *std.ArrayList(u8), args: *std.ArrayList(Oop)) bool {
    if (ruleOf(msg) != r_keywordMsg) return false;
    const nc = nchild(msg);
    var i: usize = 0;
    while (i + 1 < nc) : (i += 2) {
        selector.appendSlice(ca, text(child(msg, i))) catch @panic("codegen: oom");
        args.append(ca, child(msg, i + 1)) catch @panic("codegen: oom");
    }
    return true;
}

fn symbolNode(node0: Oop) ?Oop {
    const node = skip(node0);
    if (ruleOf(node) != r_symbol) return null;
    const raw = text(node);
    if (raw.len < 2 or raw[0] != '#') return null;
    const z = ca.allocSentinel(u8, raw.len - 1, 0) catch @panic("codegen: oom");
    defer ca.free(z);
    @memcpy(z[0 .. raw.len - 1], raw[1..]);
    return g_api.intern(z.ptr);
}

fn identNode(node0: Oop) ?Oop {
    const node = skip(node0);
    if (ruleOf(node) != r_ident) return null;
    const raw = text(node);
    const z = ca.allocSentinel(u8, raw.len, 0) catch @panic("codegen: oom");
    defer ca.free(z);
    @memcpy(z[0..raw.len], raw);
    return g_api.intern(z.ptr);
}

fn blockParamCount(node0: Oop) ?usize {
    const node = skip(node0);
    if (ruleOf(node) != r_block) return null;
    var count: usize = 0;
    var i: usize = 0;
    while (i < nchild(node)) : (i += 1) if (ruleOf(child(node, i)) == r_ident) {
        count += 1;
    };
    return count;
}

const ScanMode = enum { operation, module };

fn hashLength(hasher: *std.crypto.hash.sha2.Sha256, len: usize) void {
    var encoded: [8]u8 = undefined;
    std.mem.writeInt(u64, &encoded, @intCast(len), .big);
    hasher.update(&encoded);
}

fn hashField(hasher: *std.crypto.hash.sha2.Sha256, bytes: []const u8) void {
    hashLength(hasher, bytes.len);
    hasher.update(bytes);
}

fn finishHex(hasher: *std.crypto.hash.sha2.Sha256) [64]u8 {
    var digest: [32]u8 = undefined;
    hasher.final(&digest);
    return std.fmt.bytesToHex(digest, .lower);
}

/// Parse a complete module document without evaluating it. In `.module` mode,
/// arbitrary top-level forms are retained as the non-method projection while
/// direct declarative Reflect method operations are projected into an inert
/// manifest. `.operation` mode is the deliberately narrow parser used only to
/// recognize one historical sourced operation while that implementation is
/// being retired.
fn scanSource(src: Oop, mode: ScanMode) ?CandidateScan {
    var len: usize = 0;
    const p = g_api.stringBytes(src, &len);
    src_bytes = p[0..len];
    const cst = s1(lang, "parse:", src);
    const parse_failed = cst == g_api.nilObj or
        (g_api.isExported("Error") != 0 and g_api.send(cst, g_api.intern("isError"), &.{}, 0) == g_api.trueObj);
    if (parse_failed or nchild(cst) < 1) return null;
    const statements = child(cst, 0);
    var scan: CandidateScan = .{};
    errdefer scan.deinit();
    var non_method_spans: std.ArrayList([2]usize) = .{};
    defer non_method_spans.deinit(ca);

    var si: usize = 0;
    while (si < nchild(statements)) : (si += 1) {
        const stmt = child(statements, si);
        if (nchild(stmt) < 1) {
            if (mode == .operation) return null;
            continue;
        }
        const whole = skip(child(stmt, 0));
        if (ruleOf(whole) != r_send) {
            if (mode == .operation) return null;
            non_method_spans.append(ca, .{ spanStart(whole), spanEnd(whole) }) catch @panic("codegen: oom");
            continue;
        }
        const recv = skip(child(whole, 0));
        const msg = skip(child(whole, 1));
        if (ruleOf(recv) != r_ident) {
            if (mode == .operation) return null;
            non_method_spans.append(ca, .{ spanStart(whole), spanEnd(whole) }) catch @panic("codegen: oom");
            continue;
        }
        var selector: std.ArrayList(u8) = .{};
        defer selector.deinit(ca);
        var args: std.ArrayList(Oop) = .{};
        defer args.deinit(ca);
        if (!keywordParts(msg, &selector, &args)) {
            if (mode == .operation) return null;
            non_method_spans.append(ca, .{ spanStart(whole), spanEnd(whole) }) catch @panic("codegen: oom");
            continue;
        }
        const recv_name = text(recv);

        if (std.mem.eql(u8, recv_name, "Module") and std.mem.eql(u8, selector.items, "named:requires:") and args.items.len == 2) {
            if (scan.name != null) return null;
            scan.name = symbolNode(args.items[0]) orelse return null;
            const req = skip(args.items[1]);
            if (ruleOf(req) != r_vectorLit) return null;
            var ri: usize = 0;
            while (ri < nchild(req)) : (ri += 1)
                scan.requires.append(ca, symbolNode(child(req, ri)) orelse return null) catch @panic("codegen: oom");
            non_method_spans.append(ca, .{ spanStart(whole), spanEnd(whole) }) catch @panic("codegen: oom");
            continue;
        }
        if (!std.mem.eql(u8, recv_name, "Reflect")) {
            if (mode == .operation) return null;
            non_method_spans.append(ca, .{ spanStart(whole), spanEnd(whole) }) catch @panic("codegen: oom");
            continue;
        }
        const is_define = (std.mem.eql(u8, selector.items, "defineMethod:on:as:") and args.items.len == 3) or
            (std.mem.eql(u8, selector.items, "defineMethod:on:as:doc:") and args.items.len == 4);
        const is_remove = std.mem.eql(u8, selector.items, "removeMethod:on:") and args.items.len == 2;
        if (!is_define and !is_remove) {
            if (mode == .operation) return null;
            non_method_spans.append(ca, .{ spanStart(whole), spanEnd(whole) }) catch @panic("codegen: oom");
            continue;
        }
        const sel = symbolNode(args.items[0]) orelse return null;
        const target_name = identNode(args.items[1]) orelse return null;
        if (is_define) {
            const params = blockParamCount(args.items[2]) orelse return null;
            if (params != selectorArity(sel) + 1) return null;
        }
        for (scan.ops.items) |old| if (old.target_name == target_name and old.selector == sel) return null;
        const op_text = text(whole);
        scan.ops.append(ca, .{
            .target_name = target_name,
            .selector = sel,
            .kind = if (is_define) .install else .remove,
            .source = g_api.mkString(op_text.ptr, op_text.len),
        }) catch @panic("codegen: oom");
    }

    if (mode == .module and scan.name == null) return null;
    if (mode == .operation and scan.ops.items.len == 0) return null;

    scan.source_digest = sha256Hex(src_bytes);

    var non_method = std.crypto.hash.sha2.Sha256.init(.{});
    non_method.update("birnam.non-method.v1\x00");
    for (non_method_spans.items) |span| {
        if (span[1] < span[0] or span[1] > src_bytes.len) return null;
        hashField(&non_method, src_bytes[span[0]..span[1]]);
    }
    scan.non_method_digest = finishHex(&non_method);

    var methods = std.crypto.hash.sha2.Sha256.init(.{});
    methods.update("birnam.methods.v1\x00");
    for (scan.ops.items) |op| {
        methods.update(&.{if (op.kind == .install) @as(u8, 'I') else @as(u8, 'R')});
        hashField(&methods, std.mem.span(g_api.symbolName(op.target_name)));
        hashField(&methods, std.mem.span(g_api.symbolName(op.selector)));
        hashField(&methods, oopBytes(op.source));
    }
    scan.method_digest = finishHex(&methods);

    var revision = std.crypto.hash.sha2.Sha256.init(.{});
    revision.update("birnam.source-revision.v1\x00");
    if (scan.name) |name| hashField(&revision, std.mem.span(g_api.symbolName(name)));
    hashLength(&revision, scan.requires.items.len);
    for (scan.requires.items) |dep| hashField(&revision, std.mem.span(g_api.symbolName(dep)));
    hashField(&revision, src_bytes);
    scan.revision_digest = finishHex(&revision);
    return scan;
}

fn scanCandidateSource(src: Oop) ?CandidateScan {
    return scanSource(src, .module);
}

fn scanOperationSource(src: Oop) ?CandidateScan {
    return scanSource(src, .operation);
}

fn directMethod(vt: Oop, sel: Oop) ?Oop {
    var i: usize = 0;
    while (g_api.vtableSelectorAt(vt, i)) |bound| : (i += 1) {
        if (bound == sel) return g_api.send(vt, g_api.intern("lookup"), &[_]Oop{sel}, 1);
    }
    return null;
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

/// `[Reflect removeMethod: #sel on: target]` — exact direct removal through
/// Store's supported MOP. Removal never calls ensurePrivateVtable: the target must
/// already own its stable family/private vtable, otherwise changing its structural
/// identity would make an exact inverse impossible.
fn mRemoveMethod(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const target = args[1];
    const vt = g_api.vtableOf(target);
    if (vt == null or g_api.vtableOwner(vt) != target) {
        const ef = g_api.import("Error") orelse return g_api.nilObj;
        return g_api.send(ef, g_api.intern("kind:"), &[_]Oop{g_api.intern("unstableMethodTarget")}, 1);
    }
    return g_api.send(g_store, g_api.intern("removeMethod:on:"), &[_]Oop{ args[0], vt }, 2);
}
/// `[Reflect vtableOf: x]` — the mirror's structural read: surface the floor's
/// `vtableOf` as a meta-capability (Bracha stratification — type identity is a
/// question asked THROUGH the mirror, not a method on the base object). Type
/// predicates (`isSymbol:`/`isString:`/…) are then pure birnam on `Reflect`,
/// comparing `[Reflect vtableOf: x]` against an exemplar's vtable.
fn mVtableOf(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return g_api.vtableOf(args[0]);
}

/// Give a pure-Birnam prototype family a process-stable loader name. Durable
/// instances sharing that private vtable can then serialize by name and relink
/// during image heap load, before the module source reconstructs its facade.
fn mExportVtableOfAs(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    if (g_api.vtableOf(args[1]) != g_api.symbol_vt) return candidateError("notASymbol");
    const vt = g_api.vtableOf(args[0]);
    if (vt == null or g_api.vtableOwner(vt) != args[0]) return candidateError("unstableMethodTarget");
    const name = g_api.symbolName(args[1]);
    if (g_api.isExported(name) != 0) return if (g_api.import(name) == vt) args[0] else candidateError("vtableNameConflict");
    if (g_api.exportObj(name, vt) == null) return candidateError("vtableExportFailed");
    return args[0];
}

fn mExportObjectAs(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    if (g_api.vtableOf(args[1]) != g_api.symbol_vt) return candidateError("notASymbol");
    const name = g_api.symbolName(args[1]);
    if (g_api.isExported(name) != 0) return if (g_api.import(name) == args[0]) args[0] else candidateError("objectNameConflict");
    if (g_api.exportObj(name, args[0]) == null) return candidateError("objectExportFailed");
    return args[0];
}

/// `[Reflect nameOf: #sym]` — a symbol's bare name as a String ("stdlib" for
/// #stdlib, no sigil): the mirror's inverse of interning, so base code can build
/// strings (dylib paths, display names) from a symbol without the printString-
/// then-strip-"#" detour. A non-symbol answers a kinded #notASymbol error (or
/// nil before the error module is up — the honest signal, never a wrong name).
fn mNameOf(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    if (g_api.vtableOf(args[0]) != g_api.symbol_vt) {
        const ef = g_api.import("Error") orelse return g_api.nilObj;
        const msg = "nameOf: expects a symbol";
        return g_api.send(ef, g_api.intern("kind:message:"), &[_]Oop{
            g_api.intern("notASymbol"), g_api.mkString(msg.ptr, msg.len),
        }, 2);
    }
    const nm = std.mem.span(g_api.symbolName(args[0]));
    return g_api.mkString(nm.ptr, nm.len);
}

/// Append one supported post-cutover method operation. The Store basis is captured
/// immediately before the unique assertion; that assertion advances it, so sorting
/// these numeric keys reproduces live mutation order independently of attrAt order.
fn recordMethodOp(src: Oop) void {
    _ = src;
}

/// `[Reflect defineMethod: #sel on: T as: blk source: srcString]` — the source-
/// carrying form codegen lowers `defineMethod:on:as:` to. Install first, then append
/// the portable operation; after the dictionary commit only fatal allocation can
/// interrupt recording.
fn mDefineMethodSourced(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const result = installMethod(args[0], args[1], args[2]);
    if (result != g_api.nilObj) recordMethodOp(args[3]);
    return result;
}

/// `[Reflect defineMethod: #sel on: T as: blk doc: d source: srcString]` — the
/// source-carrying form codegen lowers `defineMethod:on:as:doc:` to (the doc'd
/// twin of mDefineMethodSourced). Records the definition's source — the recorded
/// span INCLUDES the doc string, so image replay re-carries it — then re-sends the
/// 4-arg `defineMethod:on:as:doc:` LATE-BOUND: modules/doc.birnam overrides that
/// selector to capture docstrings when docgen re-loads modules, and the override
/// must keep seeing every doc'd definition.
fn mDefineMethodDocSourced(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const result = g_api.send(self, g_api.intern("defineMethod:on:as:doc:"), args, 4);
    if (result != g_api.nilObj) recordMethodOp(args[4]);
    return result;
}

/// Source-carrying exact removal used by live codegen. The plain operation remains
/// the public API; this internal spelling is what makes removal image-recoverable.
fn mRemoveMethodSourced(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const result = mRemoveMethod(self, args, 2);
    if (result != g_api.nilObj and result == g_store) recordMethodOp(args[2]);
    return result;
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

/// Is `recv` syntactically the `Reflect` global? The source-recording lowering of
/// `defineMethod:on:as:` (below) is a Reflect-SPECIFIC compile-time transform — it
/// records the definition's span and, in artifact mode, precompiles the install. It
/// must key on the RECEIVER, not just the selector, so a user object that happens to
/// define its own `defineMethod:on:as:` compiles as an ordinary overridable send
/// (audit §E2 — the intercept was receiver-blind). Matches the literal
/// `[Reflect defineMethod: …]` shape all real code uses; an aliased `Reflect` simply
/// falls back to a plain 3-arg send (correct, just not source-recorded).
fn isReflectRecv(recv: Oop) bool {
    const nd = skip(recv);
    return ruleOf(nd) == r_ident and std.mem.eql(u8, text(nd), "Reflect");
}

/// Emit `[<recv> defineMethod: … on: … as: …]` (3 args) or `[… defineMethod: …
/// on: … as: … doc: …]` (4 args) as the corresponding `…source:` send, appending
/// the definition's original source text (the whole send node's span) as a String
/// last arg. The runtime records it as a store fact — the portable truth image
/// save/replay is built on (docs/40 §4).
fn emitDefineMethodSourced(buf: *std.ArrayList(u8), recvNode: Oop, argNodes: []const Oop, sendNode: Oop) bool {
    put(buf, "birnam_send(");
    if (!emit(buf, recvNode)) return false;
    put(buf, if (argNodes.len == 4)
        ", birnam_intern(\"defineMethod:on:as:doc:source:\"), (void*[]){"
    else
        ", birnam_intern(\"defineMethod:on:as:source:\"), (void*[]){");
    for (argNodes) |an| {
        if (!emit(buf, an)) return false;
        put(buf, ", ");
    }
    put(buf, "birnam_mkString(\"");
    const s = text(sendNode);
    putCStr(buf, s);
    var nb: [24]u8 = undefined;
    put(buf, std.fmt.bufPrint(&nb, "\", {d}L)}}, {d}L)", .{ s.len, argNodes.len + 1 }) catch return false);
    return true;
}

/// Live exact removal with its portable source operation appended only after the
/// direct dictionary mutation succeeds.
fn emitRemoveMethodLiveSourced(buf: *std.ArrayList(u8), recvNode: Oop, argNodes: []const Oop, sendNode: Oop) bool {
    put(buf, "birnam_send(");
    if (!emit(buf, recvNode)) return false;
    put(buf, ", birnam_intern(\"removeMethod:on:source:\"), (void*[]){");
    for (argNodes) |an| {
        if (!emit(buf, an)) return false;
        put(buf, ", ");
    }
    put(buf, "birnam_mkString(\"");
    const s = text(sendNode);
    putCStr(buf, s);
    var nb: [24]u8 = undefined;
    put(buf, std.fmt.bufPrint(&nb, "\", {d}L)}}, 3L)", .{s.len}) catch return false);
    return true;
}

/// Artifact-mode `[Reflect defineMethod: #sel on: T as: blk]` (docs/40 slice 4):
/// emit the method so that LOADING the artifact installs PRE-COMPILED code — no
/// runtime compile. Mirrors `installMethod` (the JIT path) but statically: the
/// block becomes an artifact function+instance stashed in a per-artifact global
/// (`__amblk_N`), a forwarding trampoline (`__amtramp_N`) is emitted into
/// `fn_defs`, and a statement-expression stashes the block then installs the
/// trampoline through Store's tracked MOP shim. The `__amblk`/`__amtramp` namespace
/// is distinct from the live session's `__mblk` so a boot's stdlib re-eval can't
/// collide. `sel`/`T` resolve relocatably (birnam_intern / birnam_global).
fn emitDefineMethodInstallable(buf: *std.ArrayList(u8), argNodes: []const Oop, sendNode: Oop) bool {
    const selRaw = text(argNodes[0]); // "#sel"
    const sel = if (selRaw.len >= 1 and selRaw[0] == '#') selRaw[1..] else selRaw;
    const k = selectorArityStr(sel);
    const n = artifact_method_counter;
    artifact_method_counter += 1;
    const src = text(sendNode);

    // The trampoline: forward [self, args…] to the stashed block via value:…(k+1).
    putFmt(&fn_defs, "void* __amtramp{d}(void* self, void** args, long n){{ (void)n; void* a[{d}]; a[0]=self; long i; for(i=0;i<{d};i++) a[i+1]=args[i]; return birnam_send(", .{ n, k + 1, k });
    if (candidate_artifact_mode)
        putFmt(&fn_defs, "birnam_candidate_root_get({d}ULL, {d}L)", .{ artifact_salt, n })
    else
        putFmt(&fn_defs, "birnam_global(\"__amblk_{x}_{d}\")", .{ artifact_salt, n });
    put(&fn_defs, ", birnam_intern(\"");
    var vi: usize = 0;
    while (vi < k + 1) : (vi += 1) put(&fn_defs, "value:");
    putFmt(&fn_defs, "\"), a, {d}); }}\n", .{k + 1});

    // The install site: stash the block, then install the trampoline on the type.
    // `on:` names a prototype now (e.g. Integer = the integer prototype), so resolve
    // its owned family via ensurePrivateVtable — same as the JIT path's targetVtable.
    putFmt(buf, "({{ void* _vt = birnam_ensurePrivateVtable(", .{});
    if (!emit(buf, argNodes[1])) return false; // the type expression (e.g. Integer)
    if (candidate_artifact_mode) {
        putFmt(buf, "); void* _blk = birnam_candidate_root_get({d}ULL, {d}L); if(_blk == birnam_nil()){{ _blk = birnam_candidate_root_set({d}ULL, {d}L, ", .{ artifact_salt, n, artifact_salt, n });
    } else {
        putFmt(buf, "); birnam_global_set(\"__amblk_{x}_{d}\", ", .{ artifact_salt, n });
    }
    if (!emit(buf, argNodes[2])) return false; // the block literal → instance
    put(buf, if (candidate_artifact_mode) "); } birnam_install_method(_vt, birnam_intern(\"" else "); birnam_install_method(_vt, birnam_intern(\"");
    putCStr(buf, sel);
    putFmt(buf, "\"), (void*)&__amtramp{d}, \"", .{n});
    putCStr(buf, src);
    putFmt(buf, "\", {d}L); _vt; }})", .{if (artifact_record_ops) src.len else 0});
    return true;
}

fn emitRemoveMethodSourced(buf: *std.ArrayList(u8), argNodes: []const Oop, sendNode: Oop) bool {
    put(buf, "birnam_remove_method(");
    if (!emit(buf, argNodes[1])) return false;
    put(buf, ", ");
    if (!emit(buf, argNodes[0])) return false;
    put(buf, ", \"");
    const src = text(sendNode);
    putCStr(buf, src);
    putFmt(buf, "\", {d}L)", .{if (artifact_record_ops) src.len else 0});
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
            // `defineMethod:on:as:` — and its doc'd twin `defineMethod:on:as:doc:`
            // — are lowered to the `…source:` forms, threading the ORIGINAL source
            // text so the method's portable source lands in the store (docs/40 §4).
            // `node` is the whole send — its span is the re-evaluable definition
            // (for the doc'd form, the span carries the doc string with it). The
            // doc'd form MUST be intercepted too: as a plain send it installs live
            // but records nothing, so the method silently vanishes from image
            // save/replay (the pre-slice-4 stdlib-string gap).
            const dm3 = std.mem.eql(u8, sel.items, "defineMethod:on:as:") and args.items.len == 3;
            const dm4 = std.mem.eql(u8, sel.items, "defineMethod:on:as:doc:") and args.items.len == 4;
            if ((dm3 or dm4) and isReflectRecv(recv)) {
                // Artifact mode (docs/40 slice 4): emit the method PRE-COMPILED —
                // the trampoline as a static fn + a direct `birnam_method` install,
                // so loading the artifact installs mapped code with no recompile.
                // The doc string is not consumed at artifact-load time (the base
                // `doc:` handler drops it; docgen harvests docs by re-loading
                // modules from SOURCE), so the doc'd form installs identically.
                // The JIT/source-recording path is used only for LIVE definition.
                if (artifact_mode) return emitDefineMethodInstallable(buf, args.items[0..3], node);
                return emitDefineMethodSourced(buf, recv, args.items, node);
            }
            const rm = std.mem.eql(u8, sel.items, "removeMethod:on:") and args.items.len == 2;
            if (rm and isReflectRecv(recv)) {
                if (artifact_mode) return emitRemoveMethodSourced(buf, args.items, node);
                return emitRemoveMethodLiveSourced(buf, recv, args.items, node);
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
    const body_ok = if (body != null) emitBody(&body_buf, body, false, false) else blk: {
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
    putFmt(buf, "({{ static void* __vt{d} = 0; if(!__vt{d}){{ __vt{d} = birnam_proto(birnam_blockVt()); birnam_method(__vt{d}, birnam_intern(\"", .{ n, n, n, n });
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
fn emitBody(buf: *std.ArrayList(u8), statements: Oop, persist: bool, top: bool) bool {
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
        } else if (top) {
            // A non-last TOP-LEVEL statement's result is DISCARDED — which would
            // silently swallow an error value (the one illegal fate for one; a
            // `let`-bound error is intentional capture and stays). Check it and
            // abort the program with the error instead, so a fileIn of a broken
            // form propagates rather than limping on. Block/method bodies keep
            // discard semantics (`top` false): there, ignoring a returned error
            // is an authoring choice with on:do:/ifError: as the handled path.
            putFmt(buf, "{{ void* __s{d} = ", .{i});
            if (!emit(buf, inner)) return false;
            putFmt(buf, "; if (birnam_is_error(__s{d})) return __s{d}; }}\n", .{ i, i });
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

/// The module name embedded in a dylib path: the basename with any directory, the
/// `lib`/`libbirnam_` prefix, and the extension stripped — `…/libbirnam_socket.dylib`
/// → `socket`. Mirrors the loader's `libbirnam_<name>.dylib` convention so the
/// recorded `__modsrc__` key matches the symbol modules are loaded by.
fn moduleNameFromPath(path: []const u8) []const u8 {
    var base = path;
    if (std.mem.lastIndexOfScalar(u8, base, '/')) |s| base = base[s + 1 ..];
    if (std.mem.lastIndexOfScalar(u8, base, '.')) |d| base = base[0..d];
    if (std.mem.startsWith(u8, base, "libbirnam_")) {
        base = base["libbirnam_".len..];
    } else if (std.mem.startsWith(u8, base, "lib")) {
        base = base["lib".len..];
    }
    return base;
}

fn sha256Hex(bytes: []const u8) [64]u8 {
    var digest: [32]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(bytes, &digest, .{});
    return std.fmt.bytesToHex(digest, .lower);
}

fn oopBytes(o: Oop) []const u8 {
    var len: usize = 0;
    const p = g_api.stringBytes(o, &len);
    return p[0..len];
}

fn writeAtomic(path: []const u8, bytes: []const u8) bool {
    const tmp = std.fmt.allocPrint(ca, "{s}.tmp.{x}", .{ path, std.crypto.random.int(u64) }) catch return false;
    defer ca.free(tmp);
    const f = std.fs.cwd().createFile(tmp, .{ .exclusive = true }) catch return false;
    defer f.close();
    f.writeAll(bytes) catch return false;
    f.sync() catch return false;
    std.fs.cwd().rename(tmp, path) catch return false;
    return true;
}

fn publishExclusive(path: []const u8, bytes: []const u8) bool {
    const tmp = std.fmt.allocPrint(ca, "{s}.tmp.{x}", .{ path, std.crypto.random.int(u64) }) catch return false;
    defer ca.free(tmp);
    const f = std.fs.cwd().createFile(tmp, .{ .exclusive = true }) catch return false;
    f.writeAll(bytes) catch {
        f.close();
        std.fs.cwd().deleteFile(tmp) catch {};
        return false;
    };
    f.sync() catch {
        f.close();
        std.fs.cwd().deleteFile(tmp) catch {};
        return false;
    };
    f.close();
    std.posix.link(tmp, path) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => {
            std.fs.cwd().deleteFile(tmp) catch {};
            return false;
        },
    };
    std.fs.cwd().deleteFile(tmp) catch {};
    return true;
}

fn appendCandidateManifest(buf: *std.ArrayList(u8), name: Oop, scan: *const CandidateScan) void {
    buf.appendSlice(ca, "v2|") catch @panic("codegen: oom");
    buf.appendSlice(ca, ARTIFACT_ABI) catch @panic("codegen: oom");
    buf.append(ca, '|') catch @panic("codegen: oom");
    buf.appendSlice(ca, std.mem.span(g_api.symbolName(name))) catch @panic("codegen: oom");
    buf.append(ca, '|') catch @panic("codegen: oom");
    buf.appendSlice(ca, &scan.source_digest) catch @panic("codegen: oom");
    buf.append(ca, '|') catch @panic("codegen: oom");
    buf.appendSlice(ca, &scan.non_method_digest) catch @panic("codegen: oom");
    buf.append(ca, '|') catch @panic("codegen: oom");
    buf.appendSlice(ca, &scan.method_digest) catch @panic("codegen: oom");
    buf.append(ca, '|') catch @panic("codegen: oom");
    buf.appendSlice(ca, &scan.revision_digest) catch @panic("codegen: oom");
    buf.append(ca, '|') catch @panic("codegen: oom");
    for (scan.requires.items, 0..) |dep, i| {
        if (i != 0) buf.append(ca, ',') catch @panic("codegen: oom");
        buf.appendSlice(ca, std.mem.span(g_api.symbolName(dep))) catch @panic("codegen: oom");
    }
    buf.append(ca, '|') catch @panic("codegen: oom");
    for (scan.ops.items, 0..) |op, i| {
        if (i != 0) buf.append(ca, ';') catch @panic("codegen: oom");
        const target = std.mem.span(g_api.symbolName(op.target_name));
        const selector = std.mem.span(g_api.symbolName(op.selector));
        const row = std.fmt.allocPrint(ca, "{s},{s},{c},{d}", .{ target, selector, if (op.kind == .install) @as(u8, 'I') else @as(u8, 'R'), selectorArity(op.selector) }) catch @panic("codegen: oom");
        defer ca.free(row);
        buf.appendSlice(ca, row) catch @panic("codegen: oom");
    }
}

/// Compile only the declarative operation body into a candidate-capable artifact
/// and publish its pre-map pointer-free sidecar after hashing the finished dylib.
fn mCompileCandidate(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    var scan = scanCandidateSource(a[0]) orelse return candidateError("unsafeCandidateSource");
    defer scan.deinit();
    if (scan.name != a[1]) return candidateError("candidateMetadataMismatch");
    if (scan.ops.items.len == 0) return candidateError("candidateHasNoMethodDelta");

    var program: std.ArrayList(u8) = .{};
    defer program.deinit(ca);
    for (scan.ops.items) |op| {
        program.appendSlice(ca, oopBytes(op.source)) catch @panic("codegen: oom");
        program.appendSlice(ca, ";\n") catch @panic("codegen: oom");
    }
    const program_oop = g_api.mkString(program.items.ptr, program.items.len);
    artifact_record_ops = true;
    candidate_artifact_mode = true;
    defer candidate_artifact_mode = false;
    var emitted = emitProgram(program_oop, false, true, true) orelse return candidateError("candidateCompileFailed");
    defer emitted.deinit(ca);
    var manifest: std.ArrayList(u8) = .{};
    defer manifest.deinit(ca);
    appendCandidateManifest(&manifest, a[1], &scan);

    var out: std.ArrayList(u8) = .{};
    defer out.deinit(ca);
    out.appendSlice(ca, "const char __birnam_artifact_abi[] = \"" ++ ARTIFACT_ABI ++ "\";\n") catch @panic("codegen: oom");
    out.appendSlice(ca, "const char __birnam_module_meta[] = \"") catch @panic("codegen: oom");
    for (scan.requires.items, 0..) |dep, i| {
        if (i != 0) out.append(ca, ',') catch @panic("codegen: oom");
        out.appendSlice(ca, std.mem.span(g_api.symbolName(dep))) catch @panic("codegen: oom");
    }
    out.appendSlice(ca, "\";\nconst char __birnam_candidate_manifest[] = \"") catch @panic("codegen: oom");
    putCStr(&out, manifest.items);
    out.appendSlice(ca, "\";\n") catch @panic("codegen: oom");
    out.appendSlice(ca, emitted.items) catch @panic("codegen: oom");
    out.appendSlice(ca, "void* __birnam_candidate_validate(void* s, void** a, long n){(void)s;(void)a;(void)n;\n") catch @panic("codegen: oom");
    for (scan.ops.items) |op| {
        out.appendSlice(ca, "if(birnam_candidate_validate_binding(\"") catch @panic("codegen: oom");
        out.appendSlice(ca, std.mem.span(g_api.symbolName(op.target_name))) catch @panic("codegen: oom");
        out.appendSlice(ca, "\",\"") catch @panic("codegen: oom");
        out.appendSlice(ca, std.mem.span(g_api.symbolName(op.selector))) catch @panic("codegen: oom");
        const check = std.fmt.allocPrint(ca, "\",{d}L,{d}L)!=birnam_true())return birnam_false();\n", .{
            selectorArity(op.selector), @intFromBool(op.kind == .remove),
        }) catch @panic("codegen: oom");
        defer ca.free(check);
        out.appendSlice(ca, check) catch @panic("codegen: oom");
    }
    out.appendSlice(ca, "return birnam_true();}\n") catch @panic("codegen: oom");
    out.appendSlice(ca,
        \\void* __birnam_candidate_prepare(void* s, void** a, long n){birnam_candidate_prepare_mode(1L);(void)__ev(s,a,n);birnam_candidate_prepare_mode(0L);return birnam_true();}
        \\void* __birnam_candidate_activate(void* s, void** a, long n){return __ev(s,a,n);}
        \\void* __birnam_candidate_rollback(void* s, void** a, long n){(void)s;(void)a;(void)n;return birnam_nil();}
        \\
    ) catch @panic("codegen: oom");
    const compiled = g_api.send(tcc, g_api.intern("compile:output:as:"), &[_]Oop{ g_api.mkString(out.items.ptr, out.items.len), a[2], g_api.intern("dll") }, 3);
    if (compiled == g_api.nilObj) return candidateError("candidateCompileFailed");

    const artifact = std.fs.cwd().readFileAlloc(ca, oopBytes(a[2]), 1 << 30) catch return candidateError("candidateArtifactUnreadable");
    defer ca.free(artifact);
    const checksum = sha256Hex(artifact);
    var sidecar: std.ArrayList(u8) = .{};
    defer sidecar.deinit(ca);
    const w = sidecar.writer(ca);
    w.print("{s}\nartifact={s}\nruntime={s}\ncandidate={d}\nplatform={s}\nmanifest={s}\nsource={d}\n", .{
        CANDIDATE_MAGIC, checksum, ARTIFACT_ABI, CANDIDATE_ABI, CANDIDATE_PLATFORM, manifest.items, oopBytes(a[0]).len,
    }) catch @panic("codegen: oom");
    sidecar.appendSlice(ca, oopBytes(a[0])) catch @panic("codegen: oom");
    const side_path = std.fmt.allocPrint(ca, "{s}.candidate", .{oopBytes(a[2])}) catch return candidateError("candidateManifestWriteFailed");
    defer ca.free(side_path);
    if (!writeAtomic(side_path, sidecar.items)) return candidateError("candidateManifestWriteFailed");
    return a[2];
}

fn nextLine(bytes: []const u8, pos: *usize) ?[]const u8 {
    if (pos.* >= bytes.len) return null;
    const end = std.mem.indexOfScalarPos(u8, bytes, pos.*, '\n') orelse return null;
    const line = bytes[pos.*..end];
    pos.* = end + 1;
    return line;
}

fn fieldValue(line: []const u8, key: []const u8) ?[]const u8 {
    if (!std.mem.startsWith(u8, line, key) or line.len <= key.len or line[key.len] != '=') return null;
    return line[key.len + 1 ..];
}

fn fixedName(bytes: []const u8) []const u8 {
    return bytes[0 .. std.mem.indexOfScalar(u8, bytes, 0) orelse bytes.len];
}

/// Trusted-candidate Mach-O policy: a thin arm64 MH_DYLIB, no initializer
/// sections, and no load command beyond libSystem. This inspection runs on bytes
/// before dlopen, so constructors cannot evade it by lying through an export.
fn candidateMachOSafe(bytes: []const u8) bool {
    if (bytes.len < 32 or std.mem.readInt(u32, bytes[0..4], .little) != 0xfeedfacf) return false;
    if (std.mem.readInt(u32, bytes[4..8], .little) != 0x0100000c) return false; // arm64
    if (std.mem.readInt(u32, bytes[12..16], .little) != 6) return false; // MH_DYLIB
    const ncmds = std.mem.readInt(u32, bytes[16..20], .little);
    const sizeofcmds = std.mem.readInt(u32, bytes[20..24], .little);
    if (32 + @as(usize, sizeofcmds) > bytes.len) return false;
    var off: usize = 32;
    var ci: u32 = 0;
    while (ci < ncmds) : (ci += 1) {
        if (off + 8 > bytes.len) return false;
        const cmd = std.mem.readInt(u32, bytes[off..][0..4], .little);
        const cmdsize: usize = std.mem.readInt(u32, bytes[off + 4 ..][0..4], .little);
        if (cmdsize < 8 or off + cmdsize > bytes.len) return false;
        if (cmd == 0x19) { // LC_SEGMENT_64
            if (cmdsize < 72) return false;
            const nsects = std.mem.readInt(u32, bytes[off + 64 ..][0..4], .little);
            if (72 + @as(usize, nsects) * 80 > cmdsize) return false;
            var so = off + 72;
            var si: u32 = 0;
            while (si < nsects) : (si += 1) {
                const sect = fixedName(bytes[so..][0..16]);
                if (std.mem.eql(u8, sect, "__mod_init_func") or std.mem.eql(u8, sect, "__init_offsets")) return false;
                so += 80;
            }
        } else if (cmd == 0xc or cmd == 0x18 or cmd == 0x80000018 or cmd == 0x1f or cmd == 0x8000001f or cmd == 0x20 or cmd == 0x23 or cmd == 0x80000023) {
            // Every dylib-loading command is denied except the compiler's one
            // ordinary libSystem dependency.
            if (cmdsize < 24) return false;
            const noff: usize = std.mem.readInt(u32, bytes[off + 8 ..][0..4], .little);
            if (noff >= cmdsize) return false;
            const dep = fixedName(bytes[off + noff .. off + cmdsize]);
            if (cmd != 0xc or !std.mem.eql(u8, dep, "/usr/lib/libSystem.B.dylib")) return false;
        }
        off += cmdsize;
    }
    return off == 32 + sizeofcmds;
}

fn assertUnitFact(unit: Oop, attr: [*:0]const u8, value: Oop) void {
    _ = g_api.send(g_store, sel_assert, &[_]Oop{ unit, g_api.intern(attr), value }, 3);
}

fn makeVector(items: []const Oop) Oop {
    const vector = g_api.import("Vector") orelse return g_api.nilObj;
    var result = g_api.send(vector, g_api.intern("new"), &[_]Oop{}, 0);
    for (items) |item| result = g_api.send(result, g_api.intern("add:"), &[_]Oop{item}, 1);
    return result;
}

fn makeSourceRevision(scan: *const CandidateScan, source: Oop) Oop {
    return makeVector(&.{
        g_api.intern("sourceRevision"),
        scan.name,
        makeVector(scan.requires.items),
        source,
        g_api.mkString(&scan.non_method_digest, scan.non_method_digest.len),
        g_api.mkString(&scan.method_digest, scan.method_digest.len),
        g_api.mkString(&scan.revision_digest, scan.revision_digest.len),
    });
}

/// `[Codegen sourceRevisionOf: source]` parses a complete module document into
/// the immutable revision value consumed by Birnam lifecycle policy. It neither
/// evaluates the document nor rewrites authored bytes.
fn mSourceRevisionOf(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    var scan = scanCandidateSource(a[0]) orelse return candidateError("invalidModuleSource");
    defer scan.deinit();
    return makeSourceRevision(&scan, a[0]);
}

fn recordModuleSource(name: Oop, source: Oop) Oop {
    var scan = scanCandidateSource(source) orelse return candidateError("invalidModuleSource");
    defer scan.deinit();
    if (scan.name != name) return candidateError("moduleIdentityMismatch");
    if (boot_module_sources.get(name)) |old| {
        if (std.mem.eql(u8, oopBytes(old), oopBytes(source))) return source;
        return candidateError("duplicateModuleSource");
    }
    boot_module_sources.put(ca, name, source) catch @panic("codegen: oom");
    return source;
}

fn mRecordModuleSource(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    if (initial_generation_sealed) return candidateError("moduleIntroductionNotAvailable");
    if (g_api.vtableOf(a[1]) != g_api.symbol_vt) return candidateError("notASymbol");
    return recordModuleSource(a[1], a[0]);
}

fn mSealInitialGeneration(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    initial_generation_sealed = true;
    return g_api.trueObj;
}

fn rtRecordModuleSource(name: [*:0]const u8, src: [*]const u8, n: c_long) callconv(.c) void {
    _ = recordModuleSource(g_api.intern(name), g_api.mkString(src, @intCast(n)));
}

fn sourceNameLessThan(_: void, lhs: Oop, rhs: Oop) bool {
    return std.mem.order(u8, std.mem.span(g_api.symbolName(lhs)), std.mem.span(g_api.symbolName(rhs))) == .lt;
}

fn sourceMapDigest(source_map: Oop) ?[64]u8 {
    const vector_vt = g_api.import("VectorVt") orelse return null;
    if (g_api.vtableOf(source_map) != vector_vt) return null;
    var hash = std.crypto.hash.sha2.Sha256.init(.{});
    hash.update("birnam.dispatch-generation.v1\x00");
    const count = g_api.smallIntValue(g_api.send(source_map, g_api.intern("size"), &.{}, 0));
    if (count < 0) return null;
    hashLength(&hash, @intCast(count));
    var i: i64 = 0;
    while (i < count) : (i += 1) {
        const entry = vectorItem(source_map, i);
        if (g_api.vtableOf(entry) != vector_vt or g_api.smallIntValue(g_api.send(entry, g_api.intern("size"), &.{}, 0)) != 3) return null;
        const name = vectorItem(entry, 0);
        const revision = vectorItem(entry, 1);
        if (g_api.vtableOf(name) != g_api.symbol_vt or g_api.vtableOf(revision) != vector_vt or
            g_api.smallIntValue(g_api.send(revision, g_api.intern("size"), &.{}, 0)) != 7 or
            vectorItem(revision, 0) != g_api.intern("sourceRevision") or vectorItem(revision, 1) != name)
            return null;
        hashField(&hash, std.mem.span(g_api.symbolName(name)));
        hashField(&hash, oopBytes(vectorItem(revision, 6)));
    }
    return finishHex(&hash);
}

/// Answers `#[#generationProjection sourceMap digest]`. Source entries are
/// lexically ordered so the digest and image input are independent of load order.
fn mInitialGenerationProjection(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    var names: std.ArrayList(Oop) = .{};
    defer names.deinit(ca);
    var it = boot_module_sources.keyIterator();
    while (it.next()) |name| names.append(ca, name.*) catch @panic("codegen: oom");
    std.mem.sort(Oop, names.items, {}, sourceNameLessThan);
    var entries: std.ArrayList(Oop) = .{};
    defer entries.deinit(ca);
    for (names.items) |name| {
        const source = boot_module_sources.get(name) orelse return candidateError("moduleSourceMissing");
        var scan = scanCandidateSource(source) orelse return candidateError("invalidModuleSource");
        defer scan.deinit();
        if (scan.name != name) return candidateError("moduleIdentityMismatch");
        entries.append(ca, makeVector(&.{ name, makeSourceRevision(&scan, source), g_api.nilObj })) catch @panic("codegen: oom");
    }
    const source_map = makeVector(entries.items);
    const digest = sourceMapDigest(source_map) orelse return candidateError("invalidSourceMap");
    return makeVector(&.{ g_api.intern("generationProjection"), source_map, g_api.mkString(&digest, digest.len) });
}

fn revisionsEqual(lhs: Oop, rhs: Oop, include_method: bool) bool {
    const vector_vt = g_api.import("VectorVt") orelse return false;
    if (g_api.vtableOf(lhs) != vector_vt or g_api.vtableOf(rhs) != vector_vt) return false;
    if (g_api.smallIntValue(g_api.send(lhs, g_api.intern("size"), &.{}, 0)) != 7 or
        g_api.smallIntValue(g_api.send(rhs, g_api.intern("size"), &.{}, 0)) != 7) return false;
    if (vectorItem(lhs, 0) != g_api.intern("sourceRevision") or vectorItem(rhs, 0) != g_api.intern("sourceRevision") or
        vectorItem(lhs, 1) != vectorItem(rhs, 1)) return false;
    const ldeps = vectorItem(lhs, 2);
    const rdeps = vectorItem(rhs, 2);
    const ln = g_api.smallIntValue(g_api.send(ldeps, g_api.intern("size"), &.{}, 0));
    if (ln != g_api.smallIntValue(g_api.send(rdeps, g_api.intern("size"), &.{}, 0))) return false;
    var i: i64 = 0;
    while (i < ln) : (i += 1) if (vectorItem(ldeps, i) != vectorItem(rdeps, i)) return false;
    if (!std.mem.eql(u8, oopBytes(vectorItem(lhs, 4)), oopBytes(vectorItem(rhs, 4)))) return false;
    return !include_method or std.mem.eql(u8, oopBytes(vectorItem(lhs, 5)), oopBytes(vectorItem(rhs, 5)));
}

/// Replace one existing module entry. This is intentionally not an insertion
/// API: L1a permits every module's declarative method revision to change, but
/// module introduction and top-level migration arrive in later phases.
fn mSuccessorGenerationProjection(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const source_map = a[0];
    const name = a[1];
    const revision = a[2];
    const unit = a[3];
    var entries: std.ArrayList(Oop) = .{};
    defer entries.deinit(ca);
    const count = g_api.smallIntValue(g_api.send(source_map, g_api.intern("size"), &.{}, 0));
    var found = false;
    var i: i64 = 0;
    while (i < count) : (i += 1) {
        const entry = vectorItem(source_map, i);
        if (vectorItem(entry, 0) == name) {
            if (found) return candidateError("duplicateModuleSource");
            found = true;
            const current = vectorItem(entry, 1);
            if (!revisionsEqual(current, revision, false)) return candidateError("topLevelEvolutionNotAvailable");
            entries.append(ca, makeVector(&.{ name, revision, unit })) catch @panic("codegen: oom");
        } else entries.append(ca, entry) catch @panic("codegen: oom");
    }
    if (!found) return candidateError("moduleIntroductionNotAvailable");
    const successor = makeVector(entries.items);
    const digest = sourceMapDigest(successor) orelse return candidateError("invalidSourceMap");
    return makeVector(&.{ g_api.intern("generationProjection"), successor, g_api.mkString(&digest, digest.len) });
}

fn mModuleSourceFor(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const current = factValue(g_api.intern("dispatch"), "activeGeneration");
    if (current != g_api.nilObj) {
        const entry = sourceMapEntry(factValue(current, "sourceMap"), a[0]);
        return if (entry == g_api.nilObj) g_api.nilObj else vectorItem(vectorItem(entry, 1), 3);
    }
    return boot_module_sources.get(a[0]) orelse g_api.nilObj;
}

fn freeCandidateState(st: *CandidateState) void {
    ca.free(st.author_path);
    if (st.content_path) |p| ca.free(p);
    ca.free(st.exported_manifest);
    st.requires.deinit(ca);
    st.ops.deinit(ca);
    ca.destroy(st);
}

/// Read and verify the detached envelope and dylib bytes without mapping native
/// code. `unit` is minted by pure Birnam policy; this mechanism only reflects the
/// inspected pointer-free metadata and retains its transient native record.
fn mInspectCandidate(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const path = oopBytes(a[0]);
    const side_path = std.fmt.allocPrint(ca, "{s}.candidate", .{path}) catch return candidateError("candidateManifestUnreadable");
    defer ca.free(side_path);
    const side = std.fs.cwd().readFileAlloc(ca, side_path, 1 << 30) catch return candidateError("candidateManifestUnreadable");
    defer ca.free(side);
    var pos: usize = 0;
    if (!std.mem.eql(u8, nextLine(side, &pos) orelse return candidateError("candidateManifestInvalid"), CANDIDATE_MAGIC))
        return candidateError("candidateManifestInvalid");
    const checksum_s = fieldValue(nextLine(side, &pos) orelse return candidateError("candidateManifestInvalid"), "artifact") orelse return candidateError("candidateManifestInvalid");
    const runtime = fieldValue(nextLine(side, &pos) orelse return candidateError("candidateManifestInvalid"), "runtime") orelse return candidateError("candidateManifestInvalid");
    const cand_s = fieldValue(nextLine(side, &pos) orelse return candidateError("candidateManifestInvalid"), "candidate") orelse return candidateError("candidateManifestInvalid");
    const platform = fieldValue(nextLine(side, &pos) orelse return candidateError("candidateManifestInvalid"), "platform") orelse return candidateError("candidateManifestInvalid");
    const manifest = fieldValue(nextLine(side, &pos) orelse return candidateError("candidateManifestInvalid"), "manifest") orelse return candidateError("candidateManifestInvalid");
    const source_len_s = fieldValue(nextLine(side, &pos) orelse return candidateError("candidateManifestInvalid"), "source") orelse return candidateError("candidateManifestInvalid");
    if (!std.mem.eql(u8, runtime, ARTIFACT_ABI) or !std.mem.eql(u8, platform, CANDIDATE_PLATFORM) or
        (std.fmt.parseInt(u32, cand_s, 10) catch 0) != CANDIDATE_ABI or checksum_s.len != 64)
        return candidateError("candidateAbiMismatch");
    const source_len = std.fmt.parseInt(usize, source_len_s, 10) catch return candidateError("candidateManifestInvalid");
    if (source_len != side.len - pos) return candidateError("candidateManifestInvalid");
    const source = g_api.mkString(side[pos..].ptr, source_len);

    const artifact = std.fs.cwd().readFileAlloc(ca, path, 1 << 30) catch return candidateError("candidateArtifactUnreadable");
    defer ca.free(artifact);
    if (!candidateMachOSafe(artifact)) return candidateError("unsafeCandidateArtifact");
    const actual = sha256Hex(artifact);
    if (!std.mem.eql(u8, &actual, checksum_s)) return candidateError("candidateChecksumMismatch");

    var parts = std.mem.splitScalar(u8, manifest, '|');
    if (!std.mem.eql(u8, parts.next() orelse "", "v2") or
        !std.mem.eql(u8, parts.next() orelse "", ARTIFACT_ABI)) return candidateError("candidateManifestInvalid");
    const name_s = parts.next() orelse return candidateError("candidateManifestInvalid");
    const source_hash_s = parts.next() orelse return candidateError("candidateManifestInvalid");
    const non_method_hash_s = parts.next() orelse return candidateError("candidateManifestInvalid");
    const method_hash_s = parts.next() orelse return candidateError("candidateManifestInvalid");
    const revision_hash_s = parts.next() orelse return candidateError("candidateManifestInvalid");
    const deps_s = parts.next() orelse "";
    const ops_s = parts.next() orelse return candidateError("candidateManifestInvalid");
    if (parts.next() != null or source_hash_s.len != 64 or non_method_hash_s.len != 64 or
        method_hash_s.len != 64 or revision_hash_s.len != 64) return candidateError("candidateManifestInvalid");
    const source_hash = sha256Hex(side[pos..]);
    if (!std.mem.eql(u8, &source_hash, source_hash_s)) return candidateError("candidateChecksumMismatch");
    const name_z = ca.dupeZ(u8, name_s) catch @panic("codegen: oom");
    defer ca.free(name_z);
    const name = g_api.intern(name_z.ptr);
    var scan = scanCandidateSource(source) orelse return candidateError("unsafeCandidateSource");
    defer scan.deinit();
    if (scan.name != name or !std.mem.eql(u8, &scan.source_digest, source_hash_s) or
        !std.mem.eql(u8, &scan.non_method_digest, non_method_hash_s) or
        !std.mem.eql(u8, &scan.method_digest, method_hash_s) or
        !std.mem.eql(u8, &scan.revision_digest, revision_hash_s))
        return candidateError("candidateMetadataMismatch");
    var deps_check: std.ArrayList(u8) = .{};
    defer deps_check.deinit(ca);
    for (scan.requires.items, 0..) |dep, i| {
        if (i != 0) deps_check.append(ca, ',') catch @panic("codegen: oom");
        deps_check.appendSlice(ca, std.mem.span(g_api.symbolName(dep))) catch @panic("codegen: oom");
    }
    if (!std.mem.eql(u8, deps_check.items, deps_s)) return candidateError("candidateMetadataMismatch");
    var ops_check: std.ArrayList(u8) = .{};
    defer ops_check.deinit(ca);
    for (scan.ops.items, 0..) |op, i| {
        if (i != 0) ops_check.append(ca, ';') catch @panic("codegen: oom");
        const row = std.fmt.allocPrint(ca, "{s},{s},{c},{d}", .{
            std.mem.span(g_api.symbolName(op.target_name)),          std.mem.span(g_api.symbolName(op.selector)),
            if (op.kind == .install) @as(u8, 'I') else @as(u8, 'R'), selectorArity(op.selector),
        }) catch @panic("codegen: oom");
        defer ca.free(row);
        ops_check.appendSlice(ca, row) catch @panic("codegen: oom");
    }
    if (!std.mem.eql(u8, ops_check.items, ops_s)) return candidateError("candidateMetadataMismatch");

    if (candidate_states.get(a[1])) |existing| {
        if (std.mem.eql(u8, &existing.checksum, checksum_s)) return a[1];
        return candidateError("candidateUnitAlreadyStaged");
    }
    const st = ca.create(CandidateState) catch @panic("codegen: oom");
    st.* = .{
        .unit = a[1],
        .name = name,
        .source = source,
        .author_path = ca.dupe(u8, path) catch @panic("codegen: oom"),
        .checksum = actual,
        .source_digest = scan.source_digest,
        .non_method_digest = scan.non_method_digest,
        .method_digest = scan.method_digest,
        .revision_digest = scan.revision_digest,
        .exported_manifest = ca.dupe(u8, manifest) catch @panic("codegen: oom"),
    };
    st.requires.appendSlice(ca, scan.requires.items) catch @panic("codegen: oom");
    st.ops.appendSlice(ca, scan.ops.items) catch @panic("codegen: oom");
    for (st.ops.items) |*op| {
        const target = globalBySymbol(op.target_name);
        if (target == g_api.nilObj) continue;
        const vt = g_api.vtableOf(target);
        op.opened_captured = true;
        op.opened_target = target;
        op.opened_vt = vt;
        op.opened_method = directMethod(vt, op.selector) orelse null;
        op.opened_present = op.opened_method != null;
    }
    candidate_states.put(ca, a[1], st) catch @panic("codegen: oom");

    assertUnitFact(a[1], "unitName", name);
    assertUnitFact(a[1], "kind", g_api.intern("candidateArtifact"));
    assertUnitFact(a[1], "sourceHash", g_api.mkString(source_hash_s.ptr, source_hash_s.len));
    assertUnitFact(a[1], "authoredSource", source);
    assertUnitFact(a[1], "nonMethodDigest", g_api.mkString(non_method_hash_s.ptr, non_method_hash_s.len));
    assertUnitFact(a[1], "methodDigest", g_api.mkString(method_hash_s.ptr, method_hash_s.len));
    assertUnitFact(a[1], "revisionDigest", g_api.mkString(revision_hash_s.ptr, revision_hash_s.len));
    assertUnitFact(a[1], "artifactChecksum", g_api.mkString(checksum_s.ptr, checksum_s.len));
    assertUnitFact(a[1], "platform", g_api.intern(CANDIDATE_PLATFORM));
    assertUnitFact(a[1], "runtimeAbi", g_api.intern(ARTIFACT_ABI));
    assertUnitFact(a[1], "candidateAbi", g_api.mkSmallInt(CANDIDATE_ABI));
    assertUnitFact(a[1], "requires", makeVector(st.requires.items));
    var provided: std.ArrayList(Oop) = .{};
    defer provided.deinit(ca);
    const object = g_api.import("Object") orelse return candidateError("candidateRuntimeUnavailable");
    for (st.ops.items, 0..) |op, i| {
        const oe = g_api.send(object, g_api.intern("clone"), &[_]Oop{}, 0);
        assertUnitFact(oe, "targetName", op.target_name);
        assertUnitFact(oe, "selector", op.selector);
        assertUnitFact(oe, "operation", g_api.intern(if (op.kind == .install) "install" else "remove"));
        assertUnitFact(oe, "definitionSource", op.source);
        assertUnitFact(oe, "operationSequence", g_api.mkSmallInt(@intCast(i)));
        provided.append(ca, oe) catch @panic("codegen: oom");
    }
    assertUnitFact(a[1], "provides", makeVector(provided.items));
    return a[1];
}

fn gcCandidateRoots(mark: idapi.MarkFn) callconv(.c) void {
    var source_it = boot_module_sources.iterator();
    while (source_it.next()) |entry| {
        mark(entry.key_ptr.*);
        mark(entry.value_ptr.*);
    }
    var root_it = candidate_roots.valueIterator();
    while (root_it.next()) |value| mark(value.*);
    var it = candidate_states.valueIterator();
    while (it.next()) |pst| {
        const st = pst.*;
        mark(st.unit);
        mark(st.name);
        mark(st.source);
        if (st.prepared_publication) |plan| {
            mark(plan.unit);
            mark(plan.name);
            mark(plan.prior);
            mark(plan.event);
            mark(plan.basis);
        }
        for (st.requires.items) |dep| mark(dep);
        for (st.ops.items) |op| {
            mark(op.target_name);
            mark(op.selector);
            mark(op.source);
            mark(op.opened_target);
            mark(op.opened_vt);
            mark(op.target);
            mark(op.vt);
            mark(op.prior_source);
            mark(op.post_log_source);
        }
    }
}

fn globalBySymbol(sym: Oop) Oop {
    return rtGlobalB(sym);
}

fn ensureContentPath(st: *CandidateState) bool {
    if (st.content_path != null) return true;
    const dir = std.fs.path.dirname(st.author_path) orelse ".";
    const object_dir = std.fs.path.join(ca, &.{ dir, ".birnam-candidates" }) catch return false;
    defer ca.free(object_dir);
    std.fs.cwd().makePath(object_dir) catch return false;
    const file_name = std.fmt.allocPrint(ca, "{s}.dylib", .{st.checksum}) catch return false;
    defer ca.free(file_name);
    const dest = std.fs.path.join(ca, &.{ object_dir, file_name }) catch return false;
    errdefer ca.free(dest);
    const source = std.fs.cwd().readFileAlloc(ca, st.author_path, 1 << 30) catch return false;
    defer ca.free(source);
    if (!std.mem.eql(u8, &sha256Hex(source), &st.checksum)) return false;
    if (std.fs.cwd().readFileAlloc(ca, dest, 1 << 30)) |existing| {
        defer ca.free(existing);
        if (!std.mem.eql(u8, &sha256Hex(existing), &st.checksum)) return false;
    } else |_| {
        if (!publishExclusive(dest, source)) return false;
    }
    const published = std.fs.cwd().readFileAlloc(ca, dest, 1 << 30) catch return false;
    defer ca.free(published);
    if (!std.mem.eql(u8, &sha256Hex(published), &st.checksum)) return false;
    st.content_path = dest;
    return true;
}

/// The explicitly named native mapping boundary. Inspection and dependency
/// reporting happen earlier and never call dlopen.
fn mMapCandidate(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const st = candidate_states.get(a[0]) orelse return candidateError("unknownCandidate");
    if (st.handle != null) return a[0];
    if (st.requires.items.len != 0) {
        const module = globalBySymbol(g_api.intern("Module"));
        if (module == g_api.nilObj) return candidateError("dependenciesMissing");
        for (st.requires.items) |dep| {
            if (g_api.send(module, g_api.intern("isLoaded:"), &[_]Oop{dep}, 1) != g_api.trueObj)
                return candidateError("dependenciesMissing");
        }
    }
    if (!ensureContentPath(st)) return candidateError("candidatePublicationFailed");
    const z = ca.dupeZ(u8, st.content_path.?) catch @panic("codegen: oom");
    defer ca.free(z);
    const h = g_api.dlopen(z.ptr, RTLD_LAZY) orelse return candidateError("candidateMapFailed");
    const abi_p = g_api.dlsym(h, "__birnam_artifact_abi") orelse return candidateError("candidateAbiMismatch");
    if (!std.mem.eql(u8, std.mem.span(@as([*:0]const u8, @ptrCast(abi_p))), ARTIFACT_ABI)) return candidateError("candidateAbiMismatch");
    const manifest_p = g_api.dlsym(h, "__birnam_candidate_manifest") orelse return candidateError("candidateManifestMismatch");
    if (!std.mem.eql(u8, std.mem.span(@as([*:0]const u8, @ptrCast(manifest_p))), st.exported_manifest)) return candidateError("candidateManifestMismatch");
    const validate_p = g_api.dlsym(h, "__birnam_candidate_validate") orelse return candidateError("candidateAbiMismatch");
    const prepare_p = g_api.dlsym(h, "__birnam_candidate_prepare") orelse return candidateError("candidateAbiMismatch");
    const activate_p = g_api.dlsym(h, "__birnam_candidate_activate") orelse return candidateError("candidateAbiMismatch");
    _ = g_api.dlsym(h, "__birnam_candidate_rollback") orelse return candidateError("candidateAbiMismatch");
    const init_p = g_api.dlsym(h, "__birnam_unit_init") orelse return candidateError("candidateAbiMismatch");
    const init_fn: *const fn () callconv(.c) void = @ptrCast(@alignCast(init_p));
    init_fn();
    const validate_fn: idapi.Method = @ptrCast(@alignCast(validate_p));
    if (validate_fn(g_api.nilObj, &[_]Oop{}, 0) != g_api.trueObj) return candidateError("candidateValidationFailed");
    st.handle = h;
    st.validate_fn = validate_fn;
    st.prepare_fn = @ptrCast(@alignCast(prepare_p));
    st.activate_fn = @ptrCast(@alignCast(activate_p));
    return a[0];
}

fn sourceOperation(src: Oop, target_name: Oop, selector: Oop) ?CandidateOpKind {
    var scan = scanOperationSource(src) orelse return null;
    defer scan.deinit();
    if (scan.ops.items.len != 1) return null;
    const op = scan.ops.items[0];
    if (op.target_name != target_name or op.selector != selector) return null;
    return op.kind;
}

const PortableOperation = struct { kind: CandidateOpKind, source: Oop };

fn latestPortableOperation(target_name: Oop, selector: Oop) ?PortableOperation {
    var latest: ?PortableOperation = null;
    const legacy_count = g_api.smallIntValue(g_api.send(g_store, g_api.intern("attrCount:"), &[_]Oop{g_methods_entity}, 1));
    var i: i64 = 0;
    while (i < legacy_count) : (i += 1) {
        const attr = g_api.send(g_store, g_api.intern("attrAt:index:"), &[_]Oop{ g_methods_entity, g_api.mkSmallInt(i) }, 2);
        const src = g_api.send(g_store, sel_currentValue, &[_]Oop{ g_methods_entity, attr }, 2);
        if (src != g_api.nilObj) {
            if (sourceOperation(src, target_name, selector)) |kind|
                latest = .{ .kind = kind, .source = src };
        }
    }
    var ops: std.ArrayList(MethodOpRec) = .{};
    defer ops.deinit(ca);
    collectMethodOps(&ops);
    for (ops.items) |rec| {
        if (sourceOperation(rec.src, target_name, selector)) |kind|
            latest = .{ .kind = kind, .source = rec.src };
    }
    return latest;
}

fn latestPortableSource(target_name: Oop, selector: Oop) Oop {
    const latest = latestPortableOperation(target_name, selector) orelse return null;
    return if (latest.kind == .install) latest.source else null;
}

fn clearPrepared(st: *CandidateState) void {
    st.prepared_publication = null;
    st.prepared_predecessor = null;
}

fn vectorItem(vector: Oop, index: i64) Oop {
    return g_api.send(vector, g_api.intern("at:"), &[_]Oop{g_api.mkSmallInt(index)}, 1);
}

fn sourceMapEntry(source_map: Oop, name: Oop) Oop {
    const count = g_api.smallIntValue(g_api.send(source_map, g_api.intern("size"), &.{}, 0));
    var i: i64 = 0;
    while (i < count) : (i += 1) {
        const entry = vectorItem(source_map, i);
        if (vectorItem(entry, 0) == name) return entry;
    }
    return g_api.nilObj;
}

fn candidateMatchesRevision(st: *CandidateState, revision: Oop) bool {
    if (revision == g_api.nilObj or vectorItem(revision, 0) != g_api.intern("sourceRevision") or
        vectorItem(revision, 1) != st.name) return false;
    if (!std.mem.eql(u8, oopBytes(vectorItem(revision, 3)), oopBytes(st.source)) or
        !std.mem.eql(u8, oopBytes(vectorItem(revision, 4)), &st.non_method_digest) or
        !std.mem.eql(u8, oopBytes(vectorItem(revision, 5)), &st.method_digest) or
        !std.mem.eql(u8, oopBytes(vectorItem(revision, 6)), &st.revision_digest)) return false;
    const deps = vectorItem(revision, 2);
    if (g_api.smallIntValue(g_api.send(deps, g_api.intern("size"), &.{}, 0)) != st.requires.items.len) return false;
    for (st.requires.items, 0..) |dep, i| if (vectorItem(deps, @intCast(i)) != dep) return false;
    return true;
}

fn generationIsCanonical(generation: Oop) bool {
    if (generation == g_api.nilObj) return false;
    const source_map = factValue(generation, "sourceMap");
    const recorded = factValue(generation, "baseDigest");
    const actual = sourceMapDigest(source_map) orelse return false;
    return std.mem.eql(u8, oopBytes(recorded), &actual);
}

fn publicationGenerationValid(st: *CandidateState, plan: CandidatePublication) bool {
    if (!plan.enforce_lifecycle) return true;
    if (!generationIsCanonical(plan.expected_generation) or !generationIsCanonical(plan.successor_generation)) return false;
    const expected_entry = sourceMapEntry(factValue(plan.expected_generation, "sourceMap"), st.name);
    const successor_entry = sourceMapEntry(factValue(plan.successor_generation, "sourceMap"), st.name);
    if (expected_entry == g_api.nilObj or successor_entry == g_api.nilObj) return false;
    if (plan.action == .activate) {
        if (vectorItem(expected_entry, 2) != plan.prior or vectorItem(successor_entry, 2) != st.unit or
            factValue(plan.successor_generation, "predecessor") != plan.expected_generation or
            factValue(plan.successor_generation, "deltaUnit") != st.unit) return false;
        const current_revision = vectorItem(expected_entry, 1);
        const successor_revision = vectorItem(successor_entry, 1);
        return revisionsEqual(current_revision, successor_revision, false) and candidateMatchesRevision(st, successor_revision);
    }
    return vectorItem(expected_entry, 2) == st.unit and factValue(plan.expected_generation, "predecessor") == plan.successor_generation and
        factValue(plan.expected_generation, "deltaUnit") == st.unit and vectorItem(successor_entry, 2) == plan.prior;
}

fn publicationStillCurrent(plan: CandidatePublication) bool {
    if (!plan.enforce_lifecycle) return true;
    const state = factValue(plan.unit, "state");
    const active = factValue(g_api.intern("dispatch"), "activeGeneration");
    if (active != plan.expected_generation) return false;
    if (plan.action == .activate) {
        if (state != g_api.intern("candidate")) return false;
        if (plan.prior != g_api.nilObj and factValue(plan.prior, "state") != g_api.intern("active")) return false;
        return true;
    }
    return state == g_api.intern("active") and factValue(plan.unit, "supersedes") == plan.prior;
}

fn copyPublicationPlan(st: *CandidateState, plan: Oop, expected: PublicationAction, strict: bool) Oop {
    const vector_vt = g_api.import("VectorVt") orelse return candidateError("candidateRuntimeUnavailable");
    if (plan == null or g_api.vtableOf(plan) != vector_vt) return candidateError("invalidPublicationPlan");
    const size = g_api.send(plan, g_api.intern("size"), &[_]Oop{}, 0);
    if (g_api.smallIntValue(size) != 8) return candidateError("invalidPublicationPlan");
    const action = vectorItem(plan, 0);
    const unit = vectorItem(plan, 1);
    const name = vectorItem(plan, 2);
    const prior = vectorItem(plan, 3);
    const event = vectorItem(plan, 4);
    const basis = vectorItem(plan, 5);
    const expected_generation = vectorItem(plan, 6);
    const successor_generation = vectorItem(plan, 7);
    const expected_symbol = g_api.intern(if (expected == .activate) "activate" else "rollback");
    if (action != expected_symbol or unit != st.unit or name != st.name or event == null or basis == null or
        (strict and (expected_generation == g_api.nilObj or successor_generation == g_api.nilObj)))
        return candidateError("invalidPublicationPlan");

    const copied: CandidatePublication = .{
        .action = expected,
        .enforce_lifecycle = strict,
        .unit = unit,
        .name = name,
        .prior = prior,
        .event = event,
        .basis = basis,
        .expected_generation = expected_generation,
        .successor_generation = successor_generation,
    };
    if (strict and !publicationGenerationValid(st, copied)) return candidateError("invalidGenerationProjection");
    if (strict and !publicationStillCurrent(copied)) return candidateError("stalePublicationPlan");
    st.prepared_publication = copied;
    return unit;
}

fn syntheticPublicationPlan(st: *CandidateState, action: PublicationAction, prior: Oop) Oop {
    const object = g_api.import("Object") orelse return g_api.nilObj;
    const event = g_api.send(object, g_api.intern("clone"), &[_]Oop{}, 0);
    const basis = g_api.send(g_store, g_api.intern("basis"), &[_]Oop{}, 0);
    return makeVector(&.{
        g_api.intern(if (action == .activate) "activate" else "rollback"),
        st.unit,
        st.name,
        prior,
        event,
        basis,
        g_api.nilObj,
        g_api.nilObj,
    });
}

fn applyPublication(plan: CandidatePublication) void {
    if (plan.action == .activate) {
        if (plan.prior != g_api.nilObj) {
            assertUnitFact(plan.unit, "supersedes", plan.prior);
            assertUnitFact(plan.prior, "state", g_api.intern("superseded"));
        }
        assertUnitFact(plan.unit, "state", g_api.intern("active"));
        assertUnitFact(plan.unit, "rollbackAvailable", g_api.trueObj);
        assertUnitFact(plan.unit, "recoveryPortable", g_api.trueObj);
    } else {
        if (plan.prior != g_api.nilObj)
            assertUnitFact(plan.prior, "state", g_api.intern("active"));
        assertUnitFact(plan.unit, "state", g_api.intern("rolledBack"));
        assertUnitFact(plan.unit, "rollbackAvailable", g_api.falseObj);
    }
    if (plan.enforce_lifecycle) {
        const cell = g_api.send(g_store, g_api.intern("cellFor:attr:"), &.{ g_api.intern("dispatch"), g_api.intern("activeGeneration") }, 2);
        _ = g_api.send(cell, g_api.intern("set:"), &.{plan.successor_generation}, 1);
    }
    assertUnitFact(plan.event, "unit", plan.unit);
    assertUnitFact(plan.event, "action", g_api.intern(if (plan.action == .activate) "activate" else "rollback"));
    assertUnitFact(plan.event, "basis", plan.basis);
    if (plan.prior != g_api.nilObj) assertUnitFact(plan.event, "priorUnit", plan.prior);
    assertUnitFact(plan.unit, "activationEvent", plan.event);
}

fn tokenPayload(token: Oop) *TokenPayload {
    return @ptrCast(@alignCast(token.?));
}

fn tokenFinalizer(token: Oop) callconv(.c) void {
    const p = tokenPayload(token);
    if (p.consumed != 0) return;
    if (candidate_states.get(p.unit)) |st| {
        if (st.prepared_token == token) {
            st.prepared_token = null;
            clearPrepared(st);
        }
    }
    p.consumed = 1;
}

fn makePreparedToken(st: *CandidateState, rollback: bool) Oop {
    if (st.prepared_token != null) return candidateError("candidateAlreadyPrepared");
    const token = g_api.alloc(candidate_token_vt, @sizeOf(TokenPayload));
    const p = tokenPayload(token);
    p.* = .{ .unit = st.unit, .consumed = 0 };
    g_api.setFinalizer(token, &tokenFinalizer);
    st.prepared_token = token;
    st.prepared_rollback = rollback;
    return token;
}

fn sameBinding(a: CandidateOp, b: CandidateOp) bool {
    return a.target_name == b.target_name and a.selector == b.selector;
}

fn bindingShadowed(root: *CandidateState, owner: *CandidateState, needle: CandidateOp) bool {
    var cursor: ?*CandidateState = root;
    while (cursor) |st| : (cursor = st.predecessor) {
        if (st == owner) return false;
        for (st.ops.items) |op| if (sameBinding(op, needle)) return true;
    }
    return false;
}

/// Validate the inherited, overlayed ledger represented by `root`.  A newer
/// generation's entry shadows the same pair in every predecessor; untouched
/// predecessor entries remain live obligations of the active generation.
fn validateEffectiveLedger(root: *CandidateState) Oop {
    var cursor: ?*CandidateState = root;
    while (cursor) |owner| : (cursor = owner.predecessor) {
        for (owner.ops.items) |op| {
            if (bindingShadowed(root, owner, op)) continue;
            if (globalBySymbol(op.target_name) != op.target or g_api.vtableOf(op.target) != op.vt)
                return candidateError("candidateDrifted");
            const cur = directMethod(op.vt, op.selector);
            if ((cur != null) != op.post_present or (cur orelse null) != op.post_method)
                return candidateError("candidateDrifted");
        }
    }
    return root.unit;
}

fn managedMethodMutation(vt: Oop, selector: Oop) callconv(.c) void {
    if (candidate_commit_in_progress or image_dispatch_recovery or suppress_methodop_recording) return;
    var it = candidate_states.valueIterator();
    while (it.next()) |pst| {
        const root = pst.*;
        if (!root.active) continue;
        var cursor: ?*CandidateState = root;
        var changed = false;
        while (cursor) |owner| : (cursor = owner.predecessor) {
            for (owner.ops.items) |op| {
                if (bindingShadowed(root, owner, op) or op.vt != vt or op.selector != selector) continue;
                const cur = directMethod(vt, selector);
                changed = (cur != null) != op.post_present or (cur orelse null) != op.post_method;
                break;
            }
            if (changed) break;
        }
        if (changed) {
            assertUnitFact(root.unit, "state", g_api.intern("drifted"));
            assertUnitFact(root.unit, "rollbackAvailable", g_api.falseObj);
        }
    }
}

fn mManagedMethodChanged(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    managedMethodMutation(args[1], args[0]);
    return self;
}

pub fn beginImageDispatchRecovery() void {
    image_dispatch_recovery = true;
    // Raw candidate handles are process-local; the durable generation remains
    // authoritative and its complete documents rebuild executable dispatch.
    var it = candidate_states.valueIterator();
    while (it.next()) |pst| pst.*.active = false;
}

pub fn endImageDispatchRecovery() void {
    image_dispatch_recovery = false;
    // A successfully restored image already carries G0 (or a successor). Mirror
    // that durable fact in the native bootstrap gate so ordinary AOT loading
    // cannot introduce modules behind the generation model.
    initial_generation_sealed = true;
}

fn mBeginImageDispatchRecovery(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    beginImageDispatchRecovery();
    return self;
}

fn mEndImageDispatchRecovery(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    endImageDispatchRecovery();
    return self;
}

fn validateCandidateBase(st: *CandidateState, rollback: bool) Oop {
    if (st.handle == null) return candidateError("candidateNotMapped");
    if (rollback and !st.active) return candidateError("candidateNotActive");
    if (!rollback and st.active) return candidateError("candidateAlreadyActive");
    if (rollback) return validateEffectiveLedger(st);
    if (st.prepared_predecessor) |prior| {
        if (!prior.active) return candidateError("candidateStaleBase");
        const inherited = validateEffectiveLedger(prior);
        if (inherited != prior.unit) return inherited;
    }
    for (st.ops.items) |*op| {
        if (!rollback) {
            const target = globalBySymbol(op.target_name);
            if (target == g_api.nilObj) return candidateError("candidateTargetMissing");
            const vt = g_api.vtableOf(target);
            if (vt == null or g_api.vtableOwner(vt) != target) return candidateError("unstableMethodTarget");
            if (op.opened_captured) {
                const opened_cur = directMethod(vt, op.selector);
                if (target != op.opened_target or vt != op.opened_vt or
                    (opened_cur != null) != op.opened_present or (opened_cur orelse null) != op.opened_method)
                    return candidateError("candidateStaleBase");
            }
            op.target = target;
            op.vt = vt;
            op.prior_method = directMethod(vt, op.selector) orelse null;
            op.prior_present = op.prior_method != null;
            if (op.kind == .remove and !op.prior_present) return candidateError("missingDirectMethod");
        }
    }
    return st.unit;
}

fn mPrepareCandidate(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    const st = candidate_states.get(a[0]) orelse return candidateError("unknownCandidate");
    if (st.prepared_token != null) return candidateError("candidateAlreadyPrepared");
    const vector_vt = g_api.import("VectorVt");
    const explicit_plan = n > 1 and vector_vt != null and g_api.vtableOf(a[1]) == vector_vt.?;
    const prior = if (explicit_plan) vectorItem(a[1], 3) else if (n > 1) a[1] else g_api.nilObj;
    const plan = if (explicit_plan) a[1] else syntheticPublicationPlan(st, .activate, prior);
    const copied = copyPublicationPlan(st, plan, .activate, explicit_plan);
    if (copied != st.unit) return copied;
    // A durable generation may name a prior CompiledUnit whose process-local
    // candidate handle did not cross image boot. The current generation/source
    // digest and opened binding snapshot are the validation base; a native prior
    // ledger is optional, used only when it still exists in this process.
    st.prepared_predecessor = if (prior != g_api.nilObj) candidate_states.get(prior) else null;
    if (st.validate_fn.?(g_api.nilObj, &[_]Oop{}, 0) != g_api.trueObj) {
        clearPrepared(st);
        return candidateError("candidateValidationFailed");
    }
    const valid = validateCandidateBase(st, false);
    if (valid != st.unit) {
        clearPrepared(st);
        return valid;
    }
    const prepared = st.prepare_fn.?(g_api.nilObj, &[_]Oop{}, 0);
    if (prepared != g_api.trueObj) {
        clearPrepared(st);
        return candidateError("candidatePrepareFailed");
    }
    return makePreparedToken(st, false);
}

fn mPrepareCandidateRollback(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    const st = candidate_states.get(a[0]) orelse return candidateError("unknownCandidate");
    if (st.prepared_token != null) return candidateError("candidateAlreadyPrepared");
    const prior = if (st.predecessor) |p| p.unit else g_api.nilObj;
    const explicit_plan = n > 1;
    const plan = if (explicit_plan) a[1] else syntheticPublicationPlan(st, .rollback, prior);
    const copied = copyPublicationPlan(st, plan, .rollback, explicit_plan);
    if (copied != st.unit) return copied;
    const valid = validateCandidateBase(st, true);
    if (valid != st.unit) {
        clearPrepared(st);
        assertUnitFact(st.unit, "state", g_api.intern("drifted"));
        assertUnitFact(st.unit, "rollbackAvailable", g_api.falseObj);
        return valid;
    }
    return makePreparedToken(st, true);
}

fn consumeToken(token: Oop) ?*CandidateState {
    if (token == null or g_api.vtableOf(token) != candidate_token_vt) return null;
    const p = tokenPayload(token);
    if (p.consumed != 0) return null;
    const st = candidate_states.get(p.unit) orelse return null;
    if (st.prepared_token != token) return null;
    p.consumed = 1;
    st.prepared_token = null;
    return st;
}

fn mCommitPrepared(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const token = a[0];
    if (token == null or g_api.vtableOf(token) != candidate_token_vt) return candidateError("invalidPreparedToken");
    const p = tokenPayload(token);
    const st = candidate_states.get(p.unit) orelse return candidateError("invalidPreparedToken");
    if (p.consumed != 0 or st.prepared_token != token or st.prepared_rollback) return candidateError("invalidPreparedToken");
    const publication = st.prepared_publication orelse return candidateError("invalidPreparedToken");
    if (publication.enforce_lifecycle and runtime_phase != .quiescent and runtime_phase != .bootstrapping) {
        _ = consumeToken(token);
        clearPrepared(st);
        return candidateError("quiescenceRequired");
    }
    if (!publicationStillCurrent(publication)) {
        _ = consumeToken(token);
        clearPrepared(st);
        return candidateError("stalePublicationPlan");
    }
    const valid = validateCandidateBase(st, false);
    if (valid != st.unit) {
        _ = consumeToken(token);
        clearPrepared(st);
        return valid;
    }
    _ = consumeToken(token);
    candidate_commit_in_progress = true;
    defer candidate_commit_in_progress = false;
    const result = st.activate_fn.?(g_api.nilObj, &[_]Oop{}, 0);
    for (st.ops.items) |*op| {
        op.post_method = directMethod(op.vt, op.selector) orelse null;
        op.post_present = op.post_method != null;
        op.post_log_source = null;
    }
    st.active = true;
    st.predecessor = st.prepared_predecessor;
    if (st.predecessor) |prior| prior.active = false;
    applyPublication(publication);
    clearPrepared(st);
    _ = result;
    return st.unit;
}

fn removalSource(op: CandidateOp) Oop {
    var b: std.ArrayList(u8) = .{};
    defer b.deinit(ca);
    b.appendSlice(ca, "[Reflect removeMethod: #") catch @panic("codegen: oom");
    b.appendSlice(ca, std.mem.span(g_api.symbolName(op.selector))) catch @panic("codegen: oom");
    b.appendSlice(ca, " on: ") catch @panic("codegen: oom");
    b.appendSlice(ca, std.mem.span(g_api.symbolName(op.target_name))) catch @panic("codegen: oom");
    b.append(ca, ']') catch @panic("codegen: oom");
    return g_api.mkString(b.items.ptr, b.items.len);
}

fn mCommitPreparedRollback(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const token = a[0];
    if (token == null or g_api.vtableOf(token) != candidate_token_vt) return candidateError("invalidPreparedToken");
    const p = tokenPayload(token);
    const st = candidate_states.get(p.unit) orelse return candidateError("invalidPreparedToken");
    if (p.consumed != 0 or st.prepared_token != token or !st.prepared_rollback) return candidateError("invalidPreparedToken");
    const publication = st.prepared_publication orelse return candidateError("invalidPreparedToken");
    if (publication.enforce_lifecycle and runtime_phase != .quiescent and runtime_phase != .bootstrapping) {
        _ = consumeToken(token);
        clearPrepared(st);
        return candidateError("quiescenceRequired");
    }
    if (!publicationStillCurrent(publication)) {
        _ = consumeToken(token);
        clearPrepared(st);
        return candidateError("stalePublicationPlan");
    }
    const valid = validateCandidateBase(st, true);
    if (valid != st.unit) {
        _ = consumeToken(token);
        clearPrepared(st);
        assertUnitFact(st.unit, "state", g_api.intern("drifted"));
        return valid;
    }
    _ = consumeToken(token);
    candidate_commit_in_progress = true;
    defer candidate_commit_in_progress = false;
    var i = st.ops.items.len;
    while (i > 0) {
        i -= 1;
        const op = st.ops.items[i];
        if (op.prior_present) {
            _ = g_api.send(g_store, sel_addMethodOn, &[_]Oop{ op.selector, op.vt, op.prior_method }, 3);
        } else {
            _ = g_api.send(g_store, g_api.intern("removeMethod:on:"), &[_]Oop{ op.selector, op.vt }, 2);
        }
    }
    st.active = false;
    if (st.predecessor) |prior| prior.active = true;
    applyPublication(publication);
    clearPrepared(st);
    return st.unit;
}

fn mDiscardPrepared(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const st = consumeToken(a[0]) orelse return candidateError("invalidPreparedToken");
    clearPrepared(st);
    return g_api.trueObj;
}

fn mReconcileCandidate(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const st = candidate_states.get(a[0]) orelse return a[0];
    if (!st.active) return st.unit;
    const valid = validateCandidateBase(st, true);
    if (valid == st.unit) return st.unit;
    assertUnitFact(st.unit, "state", g_api.intern("drifted"));
    assertUnitFact(st.unit, "rollbackAvailable", g_api.falseObj);
    return valid;
}

fn mReconcileAllCandidates(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    var portable = true;
    var it = candidate_states.valueIterator();
    while (it.next()) |pst| {
        const st = pst.*;
        if (!st.active) continue;
        var drifted = false;
        var cursor: ?*CandidateState = st;
        while (cursor) |owner| : (cursor = owner.predecessor) {
            for (owner.ops.items) |op| {
                if (bindingShadowed(st, owner, op)) continue;
                if (globalBySymbol(op.target_name) != op.target or g_api.vtableOf(op.target) != op.vt) {
                    drifted = true;
                    portable = false;
                    continue;
                }
                const cur = directMethod(op.vt, op.selector);
                if ((cur != null) == op.post_present and (cur orelse null) == op.post_method) continue;
                drifted = true;
                portable = false;
            }
        }
        if (drifted) {
            assertUnitFact(st.unit, "state", g_api.intern("drifted"));
            assertUnitFact(st.unit, "rollbackAvailable", g_api.falseObj);
        }
    }
    return if (portable) g_api.trueObj else candidateError("nonPortableDrift");
}

fn mAdoptCandidateLive(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const old = candidate_states.get(a[0]) orelse return candidateError("unknownCandidate");
    if (!old.active) return candidateError("candidateNotActive");
    if (candidate_states.contains(a[1])) return candidateError("candidateUnitAlreadyStaged");
    const baseline = ca.create(CandidateState) catch @panic("codegen: oom");
    baseline.* = .{
        .unit = a[1],
        .name = old.name,
        .source = old.source,
        .author_path = ca.dupe(u8, "") catch @panic("codegen: oom"),
        .checksum = [_]u8{'0'} ** 64,
        .exported_manifest = ca.dupe(u8, "") catch @panic("codegen: oom"),
        .handle = old.handle,
        .active = true,
        .predecessor = old,
    };
    errdefer freeCandidateState(baseline);
    baseline.requires.appendSlice(ca, old.requires.items) catch @panic("codegen: oom");
    var cursor: ?*CandidateState = old;
    while (cursor) |owner| : (cursor = owner.predecessor) {
        for (owner.ops.items) |old_op| {
            if (bindingShadowed(old, owner, old_op)) continue;
            var op = old_op;
            if (globalBySymbol(op.target_name) != op.target or g_api.vtableOf(op.target) != op.vt)
                return candidateError("nonPortableDrift");
            const cur = directMethod(op.vt, op.selector);
            if ((cur != null) != op.post_present or (cur orelse null) != op.post_method) {
                const latest = latestPortableOperation(op.target_name, op.selector) orelse return candidateError("nonPortableDrift");
                if (latest.source == op.post_log_source or ((cur != null) and latest.kind != .install) or
                    ((cur == null) and latest.kind != .remove)) return candidateError("nonPortableDrift");
                op.post_log_source = latest.source;
            }
            op.post_present = cur != null;
            op.post_method = cur orelse null;
            op.prior_present = false;
            op.prior_method = null;
            op.prior_source = null;
            baseline.ops.append(ca, op) catch @panic("codegen: oom");
        }
    }
    candidate_states.put(ca, a[1], baseline) catch @panic("codegen: oom");
    old.active = false;
    return a[1];
}

fn factValue(entity: Oop, attr: [*:0]const u8) Oop {
    return g_api.send(g_store, sel_currentValue, &[_]Oop{ entity, g_api.intern(attr) }, 2);
}

fn mRebuildComposedLedger(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    if (candidate_states.contains(a[0])) return a[0];
    const st = ca.create(CandidateState) catch @panic("codegen: oom");
    st.* = .{
        .unit = a[0],
        .name = g_api.intern("imageComposition"),
        .source = g_api.mkString("", 0),
        .author_path = ca.dupe(u8, "") catch @panic("codegen: oom"),
        .checksum = [_]u8{'0'} ** 64,
        .exported_manifest = ca.dupe(u8, "") catch @panic("codegen: oom"),
        .active = true,
    };
    errdefer freeCandidateState(st);
    const origins = a[1];
    const origin_count = g_api.smallIntValue(g_api.send(origins, g_api.intern("size"), &[_]Oop{}, 0));
    var oi: i64 = 0;
    while (oi < origin_count) : (oi += 1) {
        const origin = g_api.send(origins, g_api.intern("at:"), &[_]Oop{g_api.mkSmallInt(oi)}, 1);
        if (candidate_states.get(origin)) |origin_state| origin_state.active = false;
        const provided = factValue(origin, "provides");
        if (provided == g_api.nilObj) continue;
        const count = g_api.smallIntValue(g_api.send(provided, g_api.intern("size"), &[_]Oop{}, 0));
        var pi: i64 = 0;
        while (pi < count) : (pi += 1) {
            const entity = g_api.send(provided, g_api.intern("at:"), &[_]Oop{g_api.mkSmallInt(pi)}, 1);
            const target_name = factValue(entity, "targetName");
            const selector = factValue(entity, "selector");
            const target = globalBySymbol(target_name);
            if (target == g_api.nilObj) continue;
            const vt = g_api.vtableOf(target);
            const cur = directMethod(vt, selector);
            const latest = latestPortableOperation(target_name, selector);
            st.ops.append(ca, .{
                .target_name = target_name,
                .selector = selector,
                .kind = if (cur != null) .install else .remove,
                .source = factValue(entity, "definitionSource"),
                .target = target,
                .vt = vt,
                .post_present = cur != null,
                .post_method = cur orelse null,
                .post_log_source = if (latest) |op| op.source else null,
            }) catch @panic("codegen: oom");
        }
    }
    candidate_states.put(ca, a[0], st) catch @panic("codegen: oom");
    return a[0];
}

/// `[Codegen compile: src output: path]` — compile a birnam program to a shared
/// library on disk (the relocatable artifact form, docs/40 §3a). The artifact
/// leaves the `birnam_*` shims as undefined externals — the dynamic loader
/// resolves them against the running process at dlopen (tcc.zig/codegen.zig
/// export them as dynamic symbols) — and re-derives its global handles by name
/// in `__birnam_unit_init`, so nothing in it depends on this process's
/// addresses. Load with `[Codegen load: path]`. Returns the path, or nil.
///
/// A compiled module also self-records its source at load — the whole-module text as
/// a `__modsrc__` fact keyed by module name, and each method operation on
/// `__methodops__` — so a compiled module is fact-level
/// indistinguishable from a source load: queryable, and carried by image save/replay.
fn mCompileOutput(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    // Name the module from its output path (`…/libbirnam_<name>.dylib`) so `emitProgram`
    // can self-record the whole-module source under `#<name>`. Scoped to THIS compile.
    var olen: usize = 0;
    const op = g_api.stringBytes(a[1], &olen);
    g_module_name = moduleNameFromPath(op[0..olen]);
    defer g_module_name = null;
    var buf = emitProgram(a[0], false, true, true) orelse return g_api.nilObj;
    defer buf.deinit(ca);
    // Embed a `__birnam_module_meta` symbol carrying this unit's DEPENDENCIES (scanned
    // from its `[Module … requires: #[…]]` declaration). A runtime loader reads it via
    // `dlsym` BEFORE running `__ev`, so it can load the dependencies first — the
    // pre-install requires knowledge on-demand `[Module load:]` needs. Empty when the
    // unit declares no requires (or isn't a module).
    var out: std.ArrayList(u8) = .{};
    defer out.deinit(ca);
    var slen: usize = 0;
    const sp = g_api.stringBytes(a[0], &slen);
    out.appendSlice(ca, "const char __birnam_artifact_abi[] = \"" ++ ARTIFACT_ABI ++ "\";\n") catch @panic("codegen: oom");
    out.appendSlice(ca, "const char __birnam_module_meta[] = \"") catch @panic("codegen: oom");
    scanRequiresCsv(sp[0..slen], &out); // only [A-Za-z0-9_,] — no C-string escaping needed
    out.appendSlice(ca, "\";\n") catch @panic("codegen: oom");
    out.appendSlice(ca, buf.items) catch @panic("codegen: oom");
    const c_src = g_api.mkString(out.items.ptr, out.items.len);
    return g_api.send(tcc, g_api.intern("compile:output:as:"), &[_]Oop{ c_src, a[1], g_api.intern("dll") }, 3);
}

/// Scan a module source for its `[Module … requires: #[ #a #b ]]` declaration and
/// append the dependency names as a comma-separated list to `out` (empty if none).
/// The declaration is the module's first statement (`requires:` never precedes it),
/// so this reads the requires without parsing — mirrors main.zig's `scanRequires`.
fn scanRequiresCsv(src: []const u8, out: *std.ArrayList(u8)) void {
    const rk = std.mem.indexOf(u8, src, "requires:") orelse return;
    const br = std.mem.indexOfPos(u8, src, rk, "#[") orelse return;
    var i = br + 2;
    var first = true;
    while (i < src.len and src[i] != ']') {
        if (src[i] == '#') {
            const start = i + 1;
            var j = start;
            while (j < src.len and (std.ascii.isAlphanumeric(src[j]) or src[j] == '_')) j += 1;
            if (j > start) {
                if (!first) out.append(ca, ',') catch @panic("codegen: oom");
                out.appendSlice(ca, src[start..j]) catch @panic("codegen: oom");
                first = false;
            }
            i = j;
        } else i += 1;
    }
}

/// Append current String sources from one reserved store entity to `prog`. Module
/// sources must precede individual method sources: a module's top-level program
/// establishes its named globals and runtime objects before its method installs.
fn appendImageSources(prog: *std.ArrayList(u8), entity: Oop) bool {
    const nAttrs = g_api.smallIntValue(g_api.send(g_store, g_api.intern("attrCount:"), &[_]Oop{entity}, 1));
    var found = false;
    var i: i64 = 0;
    while (i < nAttrs) : (i += 1) {
        const attr = g_api.send(g_store, g_api.intern("attrAt:index:"), &[_]Oop{ entity, g_api.mkSmallInt(i) }, 2);
        const src = g_api.send(g_store, g_api.intern("currentValue:attr:"), &[_]Oop{ entity, attr }, 2);
        if (src == null or src == g_api.nilObj or g_api.vtableOf(src) != g_api.string_vt) continue;
        var len: usize = 0;
        const p = g_api.stringBytes(src, &len);
        prog.appendSlice(ca, p[0..len]) catch @panic("codegen: oom");
        prog.appendSlice(ca, ";\n") catch @panic("codegen: oom");
        found = true;
    }
    return found;
}

const MethodOpRec = struct {
    seq: i64,
    tie: i64,
    src: Oop,
};

fn methodOpLess(_: void, x: MethodOpRec, y: MethodOpRec) bool {
    return x.seq < y.seq or (x.seq == y.seq and x.tie < y.tie);
}

/// Collect post-cutover operations by their numeric Store-basis key. Store
/// attribute enumeration is deliberately not an ordering contract.
fn collectMethodOps(recs: *std.ArrayList(MethodOpRec)) void {
    const nAttrs = g_api.smallIntValue(g_api.send(g_store, g_api.intern("attrCount:"), &[_]Oop{g_methodops_entity}, 1));
    var i: i64 = 0;
    while (i < nAttrs) : (i += 1) {
        const attr = g_api.send(g_store, g_api.intern("attrAt:index:"), &[_]Oop{ g_methodops_entity, g_api.mkSmallInt(i) }, 2);
        const src = g_api.send(g_store, sel_currentValue, &[_]Oop{ g_methodops_entity, attr }, 2);
        if (src == null or src == g_api.nilObj or g_api.vtableOf(src) != g_api.string_vt) continue;
        const nm = std.mem.span(g_api.symbolName(attr));
        if (!std.mem.startsWith(u8, nm, "__mop_")) continue;
        const rest = nm["__mop_".len..];
        const split = std.mem.indexOfScalar(u8, rest, '_') orelse continue;
        const seq = std.fmt.parseInt(i64, rest[0..split], 10) catch continue;
        const tie = std.fmt.parseInt(i64, rest[split + 1 ..], 10) catch continue;
        recs.append(ca, .{ .seq = seq, .tie = tie, .src = src }) catch @panic("codegen: oom");
    }
    std.mem.sort(MethodOpRec, recs.items, {}, methodOpLess);
}

fn appendMethodOps(prog: *std.ArrayList(u8)) bool {
    var recs: std.ArrayList(MethodOpRec) = .{};
    defer recs.deinit(ca);
    collectMethodOps(&recs);
    for (recs.items) |rec| {
        var len: usize = 0;
        const p = g_api.stringBytes(rec.src, &len);
        prog.appendSlice(ca, p[0..len]) catch @panic("codegen: oom");
        prog.appendSlice(ca, ";\n") catch @panic("codegen: oom");
    }
    return recs.items.len != 0;
}

fn sourceMapIndex(source_map: Oop, name: Oop) ?usize {
    const count = g_api.smallIntValue(g_api.send(source_map, g_api.intern("size"), &.{}, 0));
    var i: i64 = 0;
    while (i < count) : (i += 1) if (vectorItem(vectorItem(source_map, i), 0) == name) return @intCast(i);
    return null;
}

fn appendGenerationSourceAt(source_map: Oop, index: usize, state: []u8, ordered: *std.ArrayList(Oop)) bool {
    if (state[index] == 2) return true;
    if (state[index] == 1) return false;
    state[index] = 1;
    const entry = vectorItem(source_map, @intCast(index));
    const revision = vectorItem(entry, 1);
    const dependencies = vectorItem(revision, 2);
    const dep_count = g_api.smallIntValue(g_api.send(dependencies, g_api.intern("size"), &.{}, 0));
    var i: i64 = 0;
    while (i < dep_count) : (i += 1) {
        const dependency = vectorItem(dependencies, i);
        if (sourceMapIndex(source_map, dependency)) |dep_index| {
            if (!appendGenerationSourceAt(source_map, dep_index, state, ordered)) return false;
        } else {
            const module = globalBySymbol(g_api.intern("Module"));
            if (module == g_api.nilObj or g_api.send(module, g_api.intern("kindOf:"), &.{dependency}, 1) != g_api.intern("kernel"))
                return false;
        }
    }
    ordered.append(ca, vectorItem(revision, 3)) catch @panic("codegen: oom");
    state[index] = 2;
    return true;
}

fn orderedGenerationSources() ?std.ArrayList(Oop) {
    const generation = factValue(g_api.intern("dispatch"), "activeGeneration");
    if (!generationIsCanonical(generation)) return null;
    const source_map = factValue(generation, "sourceMap");
    const count_i = g_api.smallIntValue(g_api.send(source_map, g_api.intern("size"), &.{}, 0));
    if (count_i < 0) return null;
    const count: usize = @intCast(count_i);
    const state = ca.alloc(u8, count) catch @panic("codegen: oom");
    defer ca.free(state);
    @memset(state, 0);
    var ordered: std.ArrayList(Oop) = .{};
    errdefer ordered.deinit(ca);
    for (0..count) |index| if (!appendGenerationSourceAt(source_map, index, state, &ordered)) return null;
    return ordered;
}

fn mGenerationSources(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    var sources = orderedGenerationSources() orelse return candidateError("invalidSourceMap");
    defer sources.deinit(ca);
    return makeVector(sources.items);
}

fn mGenerationMethodSources(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    g_api.gcSetAuto(0);
    defer g_api.gcSetAuto(1);
    var documents = orderedGenerationSources() orelse blk: {
        // Tooling may inspect sources during boot before the host seals G0 (CLI
        // dispatch loads its requested documentation modules in that window).
        // The process-local stage is the sole pre-G0 source set; after G0 the
        // branch above always uses durable generation authority.
        var staged: std.ArrayList(Oop) = .{};
        var it = boot_module_sources.valueIterator();
        while (it.next()) |source| staged.append(ca, source.*) catch @panic("codegen: oom");
        break :blk staged;
    };
    defer documents.deinit(ca);
    var methods: std.ArrayList(Oop) = .{};
    defer methods.deinit(ca);
    for (documents.items) |document| {
        var scan = scanCandidateSource(document) orelse return candidateError("invalidModuleSource");
        defer scan.deinit();
        for (scan.ops.items) |op| if (op.kind == .install)
            methods.append(ca, op.source) catch @panic("codegen: oom");
    }
    return makeVector(methods.items);
}

/// The content identity which seals the current dispatch generation. Image
/// authority and the compiled image unit both carry this exact digest, so a
/// loader can reject mixed source/fact/code state before mapping executable
/// bytes.
fn mCurrentGenerationDigest(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    const generation = factValue(g_api.intern("dispatch"), "activeGeneration");
    if (!generationIsCanonical(generation)) return candidateError("invalidGeneration");
    return factValue(generation, "baseDigest");
}

/// `[Codegen compileImageCode: path]` compiles each current generation document
/// whole and exactly once, in dependency order. The sealed source map is the only
/// portable dispatch provenance; legacy module/method fact streams are ignored.
fn mCompileImageCode(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    var prog: std.ArrayList(u8) = .{};
    defer prog.deinit(ca);
    var sources = orderedGenerationSources() orelse return g_api.nilObj;
    defer sources.deinit(ca);
    if (sources.items.len == 0) return g_api.nilObj;
    for (sources.items) |source| {
        prog.appendSlice(ca, oopBytes(source)) catch @panic("codegen: oom");
        prog.appendSlice(ca, ";\n") catch @panic("codegen: oom");
    }
    const progStr = g_api.mkString(prog.items.ptr, prog.items.len);
    artifact_record_ops = false;
    defer artifact_record_ops = true;
    var buf = emitProgram(progStr, false, true, true) orelse return g_api.nilObj;
    defer buf.deinit(ca);
    var out: std.ArrayList(u8) = .{};
    defer out.deinit(ca);
    out.appendSlice(ca, "const char __birnam_artifact_abi[] = \"" ++ ARTIFACT_ABI ++ "\";\n") catch @panic("codegen: oom");
    out.appendSlice(ca, buf.items) catch @panic("codegen: oom");
    const c_src = g_api.mkString(out.items.ptr, out.items.len);
    return g_api.send(tcc, g_api.intern("compile:output:as:"), &[_]Oop{ c_src, a[0], g_api.intern("dll") }, 3);
}

/// Rebuild dispatch from the same dependency-ordered complete documents used by
/// mapped code capture. Each document executes once and preserves its own form
/// order.
fn mReplayMethods(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    var srcs = orderedGenerationSources() orelse return candidateError("invalidSourceMap");
    defer srcs.deinit(ca);
    suppress_methodop_recording = true;
    defer suppress_methodop_recording = false;
    for (srcs.items) |src| _ = g_api.send(self, g_api.intern("eval:"), &[_]Oop{src}, 1);
    return g_api.mkSmallInt(@intCast(srcs.items.len));
}

/// `[Codegen replayAliens]` — re-install the Alien surface at boot. A wrap's
/// native trampoline and a dlopen'd library can't ride the image, so alien.zig
/// records each successful `[Alien dlopen:]` / `[Alien wrap:…]` send as replayable
/// source on `__aliens__` (the `__methods__` spine applied to FFI). Re-evaluated
/// here in key order — every dlopen was recorded before the wraps needing it —
/// after the code section is resolved, so the `on:` targets (exported prototypes
/// and restored module singletons) are bound. Re-evaluation re-enters the alien
/// recorder, whose by-value dedupe makes this a fact-level no-op. A failed replay
/// (library gone, symbol missing) warns and continues — one dead wrap must not
/// kill the boot. Answers the count replayed cleanly.
fn mReplayAliens(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const entity = g_api.intern("__aliens__");
    const nAttrs = g_api.smallIntValue(g_api.send(g_store, g_api.intern("attrCount:"), &[_]Oop{entity}, 1));
    const Rec = struct { idx: i64, src: Oop };
    var recs: std.ArrayList(Rec) = .{};
    defer recs.deinit(ca);
    var i: i64 = 0;
    while (i < nAttrs) : (i += 1) {
        const attr = g_api.send(g_store, g_api.intern("attrAt:index:"), &[_]Oop{ entity, g_api.mkSmallInt(i) }, 2);
        const src = g_api.send(g_store, sel_currentValue, &[_]Oop{ entity, attr }, 2);
        if (src == null or src == g_api.nilObj) continue; // retracted (a dlclose'd library)
        const nm = std.mem.span(g_api.symbolName(attr));
        if (!std.mem.startsWith(u8, nm, "__alien_")) continue;
        const idx = std.fmt.parseInt(i64, nm["__alien_".len..], 10) catch continue;
        recs.append(ca, .{ .idx = idx, .src = src }) catch @panic("codegen: oom");
    }
    std.mem.sort(Rec, recs.items, {}, struct {
        fn lt(_: void, x: Rec, y: Rec) bool {
            return x.idx < y.idx;
        }
    }.lt);
    const error_vt = g_api.import("ErrorVt");
    var count: i64 = 0;
    for (recs.items) |rec| {
        const r = g_api.send(self, g_api.intern("eval:"), &[_]Oop{rec.src}, 1);
        if (r == g_api.nilObj or (error_vt != null and g_api.vtableOf(r) == error_vt)) {
            var len: usize = 0;
            const p = g_api.stringBytes(rec.src, &len);
            std.debug.print("[image] alien replay failed: {s}\n", .{p[0..len]});
        } else count += 1;
    }
    return g_api.mkSmallInt(count);
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
        // the deferred (un-serializable, absent) globals need re-derivation. The
        // presence test rides `hasValue:attr:`: `currentValue:attr:` normalises
        // absence to nil at the surface (the no-NULL lock), so nil can't be read
        // as "unbound" here — a global BOUND to nil must not be re-evaluated.
        const bound = g_api.send(g_store, g_api.intern("hasValue:attr:"), &[_]Oop{ g_globals_entity, nameSym }, 2);
        if (bound == g_api.trueObj) continue;
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
/// library or either symbol is missing. The explicit artifact ABI is checked
/// before either initializer or entry point can mutate live state.
fn mLoadUnit(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    var len: usize = 0;
    const p = g_api.stringBytes(a[0], &len);
    const path = ca.allocSentinel(u8, len, 0) catch @panic("codegen: oom");
    defer ca.free(path);
    @memcpy(path[0..len], p[0..len]);

    const h = g_api.dlopen(path.ptr, RTLD_LAZY) orelse return g_api.nilObj;
    const abi_p = g_api.dlsym(h, "__birnam_artifact_abi") orelse {
        _ = std.c.dlclose(h);
        return g_api.nilObj;
    };
    const abi = std.mem.span(@as([*:0]const u8, @ptrCast(abi_p)));
    if (!std.mem.eql(u8, abi, ARTIFACT_ABI)) {
        _ = std.c.dlclose(h);
        return g_api.nilObj;
    }
    // Ordinary AOT module loading is the G0 bootstrap path. Once the source map
    // is sealed, every module change must go through a checked CompiledUnit
    // replacement and host safe point; L1a deliberately has no module-introduction
    // operation. Image recovery is the sole exception because it reinstalls the
    // already-authorized generation carried by the image.
    if (initial_generation_sealed and !image_dispatch_recovery)
        return candidateError("moduleIntroductionNotAvailable");
    const init_p = g_api.dlsym(h, "__birnam_unit_init") orelse return g_api.nilObj;
    const init_fn: *const fn () callconv(.c) void = @ptrCast(@alignCast(init_p));
    init_fn();
    const ev_p = g_api.dlsym(h, "__ev") orelse return g_api.nilObj;
    const ev_fn: idapi.Method = @ptrCast(@alignCast(ev_p));
    return ev_fn(g_api.nilObj, &[_]Oop{}, 0);
}

/// `[Codegen moduleMetaOf: path]` — read a compiled unit's `__birnam_module_meta`
/// symbol (the comma-separated dependency names embedded at compile time) WITHOUT
/// running its install code: the pre-install knowledge a dependency-first runtime
/// load needs (modules.birnam's `load:`). dlopen is refcounted, so the later
/// `load:` of the same path is cheap. Answers nil when the dylib can't be opened,
/// "" when it carries no meta (no declared deps).
fn mModuleMetaOf(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    var len: usize = 0;
    const p = g_api.stringBytes(a[0], &len);
    const path = ca.allocSentinel(u8, len, 0) catch @panic("codegen: oom");
    defer ca.free(path);
    @memcpy(path[0..len], p[0..len]);
    const h = g_api.dlopen(path.ptr, RTLD_LAZY) orelse return g_api.nilObj;
    const mp = g_api.dlsym(h, "__birnam_module_meta") orelse return g_api.mkString("", 0);
    const meta = std.mem.span(@as([*:0]const u8, @ptrCast(mp)));
    return g_api.mkString(meta.ptr, meta.len);
}

/// Parse `src` and emit the complete C translation unit into a fresh buffer
/// (caller deinits). With `artifact` true, emit the RELOCATABLE spelling
/// (docs/40 §3a): the baked `__g<i>` linker handles become static slots that a
/// per-unit `__birnam_unit_init` hook re-derives by interning their names at
/// load time, so the output carries no address from this process and can be
/// written to disk, dlopen'd elsewhere, and fixed up there. The JIT keeps the
/// baked handles (`artifact` false) — nothing changes for a live session.
/// Returns null on a parse or emit failure.
/// `checked` — abort-on-discarded-error at the top level (the fileIn contract).
/// True for user programs (eval / fileIn / module compiles); FALSE only for the
/// image's composed `.code` program, which concatenates INDEPENDENT records —
/// there one stale record must not keep the rest of the image from installing
/// (the same best-effort stance as replayAliens' "one dead wrap must not kill
/// the boot").
fn emitProgram(src: Oop, persist: bool, artifact: bool, checked: bool) ?std.ArrayList(u8) {
    var len: usize = 0;
    const p = g_api.stringBytes(src, &len);
    src_bytes = p[0..len];
    clearError();

    const cst = s1(lang, "parse:", src);
    // Parse failure now surfaces as a kinded error VALUE when the error module is
    // loaded (docs/42 §C2/§C3), or nil when it isn't — detect both. (Guarded on the
    // protocol existing so the `isError` send can't DNU-panic in an error-free boot.)
    const parse_failed = cst == g_api.nilObj or
        (g_api.isExported("Error") != 0 and g_api.send(cst, g_api.intern("isError"), &[_]Oop{}, 0) == g_api.trueObj);
    if (parse_failed) {
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
    if (artifact) artifact_salt = std.hash.Wyhash.hash(0, src_bytes); // unique __amblk keys per unit

    // The `__ev` function body is emitted first (which populates fn_defs with any
    // block functions, in post-order); the final source is prelude + fn_defs +
    // __ev so every block fn is defined before it's referenced.
    var ev: std.ArrayList(u8) = .{};
    defer ev.deinit(ca);
    put(&ev, "void* __ev(void* self, void** args, long n){ (void)self;(void)args;(void)n;\n");
    // Module dylibs stage their exact whole source for G0. The stage is native and
    // process-local; durable authority begins only when DispatchGeneration is
    // published. Only the module-compile path sets `g_module_name`.
    if (artifact and g_module_name != null) {
        put(&ev, "birnam_record_module_source(\"");
        putCStr(&ev, g_module_name.?);
        put(&ev, "\", \"");
        putCStr(&ev, src_bytes);
        putFmt(&ev, "\", {d}L);\n", .{src_bytes.len});
    }
    if (!emitBody(&ev, statements, persist, checked)) {
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
        \\extern void* birnam_blockVt(void);
        \\extern void birnam_method(void* vt, void* sel, void* fn);
        \\extern void birnam_layoutAllOops(void* vt);
        \\extern void* birnam_alloc(void* vt, long nbytes);
        \\extern void* birnam_global(const char* name);
        \\extern void* birnam_global_b(void* sym);
        \\extern void birnam_global_set(const char* name, void* value);
        \\extern void birnam_record_global_src(const char* name, const char* src, long n);
        \\extern void birnam_record_module_source(const char* name, const char* src, long n);
        \\extern long birnam_is_error(void* v);
        \\extern void* birnam_install_method(void* vt, void* sel, void* method, const char* src, long n);
        \\extern void* birnam_remove_method(void* target, void* sel, const char* src, long n);
        \\extern void* birnam_candidate_root_set(unsigned long long salt, long index, void* value);
        \\extern void* birnam_candidate_root_get(unsigned long long salt, long index);
        \\extern void birnam_candidate_prepare_mode(long on);
        \\extern void* birnam_candidate_validate_binding(const char* target, const char* selector, long arity, long removal);
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
    var buf = emitProgram(src, persist, false, true) orelse return g_api.nilObj;
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
    addShim(unit, "birnam_is_error", @ptrCast(&rtIsError));
    addShim(unit, "birnam_install_method", @ptrCast(&rtInstallMethod));
    addShim(unit, "birnam_remove_method", @ptrCast(&rtRemoveMethod));
    addShim(unit, "birnam_candidate_validate_binding", @ptrCast(&rtCandidateValidateBinding));
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
    g_methodops_entity = api.intern("__methodops__");
    g_globalsrc_entity = api.intern("__globalsrc__");
    sel_currentValue = api.intern("currentValue:attr:");
    sel_rawCurrentValue = api.intern("rawCurrentValue:attr:");
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
    if (!reloading) {
        var stale = candidate_states.valueIterator();
        while (stale.next()) |pst| freeCandidateState(pst.*);
        candidate_states.deinit(ca);
        candidate_states = .{};
        boot_module_sources.deinit(ca);
        boot_module_sources = .{};
        candidate_roots.deinit(ca);
        candidate_roots = .{};
        candidate_token_vt = null;
        candidate_roots_registered = false;
        ev_harness_vt = null;
        candidate_preparing = false;
        candidate_commit_in_progress = false;
        image_dispatch_recovery = false;
        initial_generation_sealed = false;
        runtime_phase = .bootstrapping;
    }
    if (candidate_token_vt == null) {
        candidate_token_vt = api.proto(api.object_vt);
        const token_layout = [_]u8{ idapi.slot_oop, idapi.slot_raw };
        api.setLayout(candidate_token_vt, &token_layout, token_layout.len, idapi.slot_raw);
    }
    if (!candidate_roots_registered) {
        api.gcRegisterRootProvider(&gcCandidateRoots);
        candidate_roots_registered = true;
    }
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
    api.method(codegen_vt, api.intern("sourceRevisionOf:"), &mSourceRevisionOf);
    api.method(codegen_vt, api.intern("recordModuleSource:named:"), &mRecordModuleSource);
    api.method(codegen_vt, api.intern("initialGenerationProjection"), &mInitialGenerationProjection);
    api.method(codegen_vt, api.intern("sealInitialGeneration"), &mSealInitialGeneration);
    api.method(codegen_vt, api.intern("successorGenerationProjection:replacing:with:unit:"), &mSuccessorGenerationProjection);
    api.method(codegen_vt, api.intern("moduleSourceFor:"), &mModuleSourceFor);
    api.method(codegen_vt, api.intern("compileCandidate:named:output:"), &mCompileCandidate);
    api.method(codegen_vt, api.intern("inspectCandidate:into:"), &mInspectCandidate);
    api.method(codegen_vt, api.intern("mapCandidate:"), &mMapCandidate);
    api.method(codegen_vt, api.intern("prepareActivation:"), &mPrepareCandidate);
    api.method(codegen_vt, api.intern("prepareActivation:prior:"), &mPrepareCandidate);
    api.method(codegen_vt, api.intern("prepareActivation:plan:"), &mPrepareCandidate);
    api.method(codegen_vt, api.intern("commitPrepared:"), &mCommitPrepared);
    api.method(codegen_vt, api.intern("prepareRollback:"), &mPrepareCandidateRollback);
    api.method(codegen_vt, api.intern("prepareRollback:plan:"), &mPrepareCandidateRollback);
    api.method(codegen_vt, api.intern("commitPreparedRollback:"), &mCommitPreparedRollback);
    api.method(codegen_vt, api.intern("discardPrepared:"), &mDiscardPrepared);
    api.method(codegen_vt, api.intern("reconcileCandidate:"), &mReconcileCandidate);
    api.method(codegen_vt, api.intern("reconcileAllCandidates"), &mReconcileAllCandidates);
    api.method(codegen_vt, api.intern("managedMethodChanged:on:"), &mManagedMethodChanged);
    api.method(codegen_vt, api.intern("authorizeManagedMethodMutation"), &mAuthorizeManagedMethodMutation);
    api.method(codegen_vt, api.intern("beginImageDispatchRecovery"), &mBeginImageDispatchRecovery);
    api.method(codegen_vt, api.intern("endImageDispatchRecovery"), &mEndImageDispatchRecovery);
    api.method(codegen_vt, api.intern("compileImageCode:"), &mCompileImageCode);
    api.method(codegen_vt, api.intern("generationSources"), &mGenerationSources);
    api.method(codegen_vt, api.intern("generationMethodSources"), &mGenerationMethodSources);
    api.method(codegen_vt, api.intern("currentGenerationDigest"), &mCurrentGenerationDigest);
    api.method(codegen_vt, api.intern("replayMethods"), &mReplayMethods);
    api.method(codegen_vt, api.intern("replayAliens"), &mReplayAliens);
    api.method(codegen_vt, api.intern("deriveImageGlobals"), &mDeriveImageGlobals);
    api.method(codegen_vt, api.intern("lastError"), &mLastError);
    api.method(codegen_vt, api.intern("load:"), &mLoadUnit);
    api.method(codegen_vt, api.intern("moduleMetaOf:"), &mModuleMetaOf);
    api.method(reflect_vt, api.intern("defineMethod:on:as:"), &mDefineMethod);
    api.method(reflect_vt, api.intern("removeMethod:on:"), &mRemoveMethod);
    api.method(reflect_vt, api.intern("removeMethod:on:source:"), &mRemoveMethodSourced);
    api.method(reflect_vt, api.intern("defineMethod:on:as:source:"), &mDefineMethodSourced);
    api.method(reflect_vt, api.intern("defineMethod:on:as:doc:source:"), &mDefineMethodDocSourced);
    api.method(reflect_vt, api.intern("vtableOf:"), &mVtableOf);
    api.method(reflect_vt, api.intern("exportVtableOf:as:"), &mExportVtableOfAs);
    api.method(reflect_vt, api.intern("exportObject:as:"), &mExportObjectAs);
    api.method(reflect_vt, api.intern("nameOf:"), &mNameOf);

    _ = api.send(g_store, api.intern("setMethodMutationObserver:"), &[_]Oop{codegen_obj}, 1);
    _ = api.send(g_store, api.intern("setMethodMutationAuthorizer:"), &[_]Oop{api.mkSmallInt(@intCast(@intFromPtr(&authorizeManagedMethodMutation)))}, 1);

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
