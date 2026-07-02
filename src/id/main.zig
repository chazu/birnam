//! The siphon — birnam's bare entry point (the Zig analog of `~/dev/c/birnam`'s
//! `system.c` siphon @ d35caee). It does ONE raw `dlopen` of the id dylib to get
//! the id API (`id_init`), and from then on does *everything through the API* —
//! including re-importing the id layer itself (`api.import("Object")`) and
//! re-loading the id dylib through the API's own `dlopen`. Module loading and the
//! object namespace are part of the id system, so the floor is reflective: the
//! siphon can re-import even id.
//!
//! No eids, no fact store.

const std = @import("std");
const idapi = @import("idapi.zig");

const IdApi = idapi.IdApi;
const Oop = idapi.Oop;

const RTLD_LAZY: c_int = 1;

// A primitive installed on SmallInt for the 3c.3 demo, so emitted birnam sends
// have a real target to dispatch to. `g_api` lets it reach the id api.
var g_api: *IdApi = undefined;
fn mDouble(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    return g_api.mkSmallInt(g_api.smallIntValue(self) * 2);
}
fn mPlus(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return g_api.mkSmallInt(g_api.smallIntValue(self) + g_api.smallIntValue(args[0]));
}

// -- exe bake: a self-booting executable = this runtime + an appended image ---
//
// The standalone-exe bake (docs/40 §6): rather than an OS-level dual-format file
// (given up — dyld/glibc fight it), a "runnable app" is a COPY of the birnam-id
// runtime with an image appended after a trailer. Trailing bytes are ignored by
// both the Mach-O and ELF loaders, so the copy still executes as birnam-id; on
// startup it notices the trailer and boots the embedded image (auto-running
// `main`). The image format and boot path are unchanged — this only packages.
//
//   file layout:  [ birnam-id binary ][ image bytes ][ u64 img_len ][ magic ]
//
// The magic MUST stay in sync with image.zig's `bake_magic` (its `bakeExe:`).
const BAKE_MAGIC = "BIRNMEXE";
const BAKE_TRAILER = 16; // u64 length + 8-byte magic

/// Read this executable's own bytes (its path from `selfExePath`).
fn selfExeBytes(a: std.mem.Allocator) ![]u8 {
    const self_path = try std.fs.selfExePathAlloc(a);
    defer a.free(self_path);
    return std.fs.cwd().readFileAlloc(a, self_path, 1 << 30);
}

/// The image sub-slice a bake trailer points to inside `bytes` (a whole exe file),
/// or null if there is no valid trailer — a plain birnam-id, or bytes too short /
/// a bad length. Pure: no I/O, so it is unit-testable.
fn bakeTrailerImage(bytes: []const u8) ?[]const u8 {
    if (bytes.len < BAKE_TRAILER) return null;
    if (!std.mem.eql(u8, bytes[bytes.len - 8 ..], BAKE_MAGIC)) return null;
    const img_len = std.mem.readInt(u64, bytes[bytes.len - BAKE_TRAILER ..][0..8], .little);
    if (img_len == 0 or img_len + BAKE_TRAILER > bytes.len) return null;
    const start = bytes.len - BAKE_TRAILER - @as(usize, @intCast(img_len));
    return bytes[start .. bytes.len - BAKE_TRAILER];
}

/// The embedded image appended to this exe (the bake trailer), or null for a plain
/// birnam-id (no trailer). Caller frees the returned slice.
fn embeddedImageBytes(a: std.mem.Allocator) !?[]u8 {
    const bytes = try selfExeBytes(a);
    defer a.free(bytes);
    const img = bakeTrailerImage(bytes) orelse return null;
    return try a.dupe(u8, img);
}

/// `birnam-id --bake <image> <out-exe>` — append an existing `.image` to a copy of
/// this runtime, producing a self-booting executable. (The in-session form is
/// `[System bakeExe: path]`, which bakes the running system directly.)
fn bakeExeCli(a: std.mem.Allocator, image_path: []const u8, out_path: []const u8) !void {
    const self_bytes = try selfExeBytes(a);
    defer a.free(self_bytes);
    const img_bytes = try std.fs.cwd().readFileAlloc(a, image_path, 1 << 30);
    defer a.free(img_bytes);
    const f = try std.fs.cwd().createFile(out_path, .{ .mode = 0o755 });
    defer f.close();
    try f.writeAll(self_bytes);
    try f.writeAll(img_bytes);
    var lb: [8]u8 = undefined;
    std.mem.writeInt(u64, &lb, img_bytes.len, .little);
    try f.writeAll(&lb);
    try f.writeAll(BAKE_MAGIC);
    out("[bake] wrote ");
    out(out_path);
    out("\n");
}

/// Boot an image carried as bytes (the exe-bake path): write it to a per-process
/// temp `.image` and run it through the ordinary image boot (so mapped code,
/// replay, global re-derivation, and the `main`-entry convention all apply).
fn runImageBytes(a: std.mem.Allocator, api: *IdApi, bytes: []const u8, rebuild: bool) !void {
    var pbuf: [64]u8 = undefined;
    const tmp = try std.fmt.bufPrint(&pbuf, "/tmp/birnam-baked-{d}.image", .{std.c.getpid()});
    {
        const f = try std.fs.cwd().createFile(tmp, .{});
        defer f.close();
        try f.writeAll(bytes);
    }
    defer std.fs.cwd().deleteFile(tmp) catch {};
    return runImage(a, api, tmp, rebuild);
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const a = gpa.allocator();

    var args = try std.process.argsWithAllocator(a);
    defer args.deinit();
    _ = args.next();
    var script_path = args.next(); // optional: a .birnam file to compile + run
    // `--rebuild foo.image` forces the replay path (rebuild code from the image's
    // own source) instead of mapping the compiled `.code` section.
    var rebuild = false;
    if (script_path) |sp| {
        if (std.mem.eql(u8, sp, "--rebuild")) {
            rebuild = true;
            script_path = args.next();
        }
    }
    // The id dylib: an explicit `$BIRNAM_ID_DYLIB`, else libbirnam_id.dylib in the
    // resolved lib dir (exe-relative, so a baked exe boots from any cwd).
    const id_path = if (std.posix.getenv("BIRNAM_ID_DYLIB")) |d|
        try a.dupe(u8, d)
    else blk: {
        const dir = try resolveLibDir(a);
        defer a.free(dir);
        break :blk try std.fs.path.join(a, &.{ dir, "libbirnam_id.dylib" });
    };
    defer a.free(id_path);
    const id_path_z = try a.dupeZ(u8, id_path);
    defer a.free(id_path_z);

    // `--bake <image> <out-exe>` — tooling op, no runtime needed yet.
    if (script_path) |sp| {
        if (std.mem.eql(u8, sp, "--bake")) {
            const img = args.next() orelse {
                out("usage: birnam-id --bake <image> <out-exe>\n");
                return error.BakeArgs;
            };
            const outp = args.next() orelse {
                out("usage: birnam-id --bake <image> <out-exe>\n");
                return error.BakeArgs;
            };
            return bakeExeCli(a, img, outp);
        }
    }

    // The ONE irreducible raw load — open the id dylib, find `id_init`, call it to
    // get the id API table. Both the script runner and the demo build on this.
    var lib = try std.DynLib.open(id_path);
    const id_init = lib.lookup(idapi.IdInitFn, "id_init") orelse return error.NoIdInit;
    const api = id_init();
    g_api = api;

    // Baked exe: if this binary carries an appended image, boot it — this IS the
    // app. A plain birnam-id has no trailer and falls through to normal handling.
    if (try embeddedImageBytes(a)) |img| {
        defer a.free(img);
        return runImageBytes(a, api, img, rebuild);
    }

    // Script mode: `birnam-id path.birnam` compiles + runs the file, quietly.
    // REPL mode: `birnam-id --repl` (or `-i` / `repl`) starts an interactive loop.
    if (script_path) |sp| {
        if (std.mem.eql(u8, sp, "--repl") or std.mem.eql(u8, sp, "-i") or std.mem.eql(u8, sp, "repl"))
            return runRepl(a, api, false);
        if (std.mem.endsWith(u8, sp, ".image"))
            return runImage(a, api, sp, rebuild);
        return runScript(a, api, sp);
    }

    std.debug.print("[siphon] birnam\n", .{});
    std.debug.print("[siphon] raw-loading id dylib: {s}\n", .{id_path});
    std.debug.print("[siphon] id api up: object_vt={*}\n", .{api.object_vt});

    // 2. RE-IMPORT THE ID LAYER through the id api itself. The siphon never
    //    touches libid internals again — it asks id for id.
    const object_vt = api.import("Object");
    const vtable_vt = api.import("Vtable");
    const symbol_vt = api.import("Symbol");
    std.debug.print("[siphon] re-imported the id layer: Object={*} Vtable={*} Symbol={*}\n", .{ object_vt, vtable_vt, symbol_vt });
    if (object_vt != api.object_vt or vtable_vt != api.vtable_vt or symbol_vt != api.symbol_vt) {
        return error.ReimportMismatch;
    }
    if (api.isExported("Object") == 0) return error.NotExported;

    // 3. Build through the api: a System object delegating to the re-imported
    //    Object, with a method installed and dispatched.
    const sys_proto = api.proto(object_vt);
    const s_ping = api.intern("ping");
    api.method(sys_proto, s_ping, &pingImpl);
    const sys = api.alloc(sys_proto, 0);
    const r = api.send(sys, s_ping, &[_]Oop{}, 0);
    std.debug.print("[siphon] [system ping] => {d}\n", .{@intFromPtr(r)});
    if (@intFromPtr(r) != 7) return error.UnexpectedResult;

    // 4. RE-LOAD the id dylib THROUGH the api's own dlopen — proof that even
    //    loading the id layer is an id-api operation. The reloaded handle yields
    //    the same live object system (process-global), so the re-import is stable.
    const h = api.dlopen(id_path_z, RTLD_LAZY) orelse return error.ReopenFailed;
    const id_init2_raw = api.dlsym(h, "id_init") orelse return error.NoIdInit2;
    const id_init2: idapi.IdInitFn = @ptrCast(@alignCast(id_init2_raw));
    const api2 = id_init2();
    std.debug.print("[siphon] re-loaded id THROUGH the id api: object_vt={*}\n", .{api2.object_vt});
    if (api2.object_vt != object_vt) return error.ReloadMismatch;

    // 5. Hot-reload state persistence. Load a reloadable module THROUGH the id
    //    api, then reload it twice — the siphon threads its opaque state, so the
    //    module's private data survives the reloads (count 1 -> 2 -> 3).
    const counter_path = try a.dupeZ(u8, "zig-out/lib/libbirnam_counter.dylib");
    defer a.free(counter_path);
    var mstate: ?*anyopaque = null;
    mstate = try loadModule(api, counter_path, mstate); // fresh   -> 1
    mstate = try loadModule(api, counter_path, mstate); // reload  -> 2
    mstate = try loadModule(api, counter_path, mstate); // reload  -> 3
    std.debug.print("[siphon] hot-reload: module state persisted across 3 loads\n", .{});

    // 6. RUNG 2 — load the cell/relational store module THROUGH the id api. Its
    //    module_init mints + seeds the ONE well-known store (raw-append → seed →
    //    flip), then we drive it: a post-flip assert consults the store's own
    //    seeded schema (the cardinality knot). The relational model models itself.
    const store_path = try a.dupeZ(u8, "zig-out/lib/libbirnam_store.dylib");
    defer a.free(store_path);
    _ = try loadModule(api, store_path, null);
    const store = api.import("Store") orelse return error.NoStore;

    const s_assert = api.intern("assert:attr:value:");
    const s_currentValue = api.intern("currentValue:attr:");
    const s_cardinality = api.intern("cardinality");
    const s_many = api.intern("many");
    const s_tags = api.intern("tags");

    // Register `tags` as :many — placing this fact consults cardinalityOf(
    // :cardinality) == one (the seeded knot) — then read it back through dispatch.
    _ = api.send(store, s_assert, &[_]Oop{ s_tags, s_cardinality, s_many }, 3);
    const back = api.send(store, s_currentValue, &[_]Oop{ s_tags, s_cardinality }, 2);
    if (back != s_many) return error.RungTwoMismatch;
    std.debug.print("[siphon] rung 2: [tags cardinality] read back as :many — the store models its own schema\n", .{});

    // 7. RUNG 3 — load the peg module THROUGH the id api and drive it ENTIRELY
    //    through messages: build a tiny grammar, parse, walk the CST. lang will
    //    use this same message api to construct birnam's grammar + parser.
    const peg_path = try a.dupeZ(u8, "zig-out/lib/libbirnam_id_peg.dylib");
    defer a.free(peg_path);
    _ = try loadModule(api, peg_path, null);
    const peg = api.import("Peg") orelse return error.NoPeg;
    const empty = &[_]Oop{};

    const g = api.send(peg, api.intern("grammar"), empty, 0);
    const digit = api.intern("digit");
    const number = api.intern("number");
    const cs = api.send(peg, api.intern("charset"), empty, 0);
    _ = api.send(cs, api.intern("from:to:"), &[_]Oop{ api.mkSmallInt('0'), api.mkSmallInt('9') }, 2);
    _ = api.send(g, api.intern("rule:is:"), &[_]Oop{ digit, cs }, 2);
    const dref = api.send(peg, api.intern("ruleRef:"), &[_]Oop{digit}, 1);
    _ = api.send(g, api.intern("rule:is:"), &[_]Oop{ number, api.send(dref, api.intern("plus"), empty, 0) }, 2);
    const sample = "12345";
    const root = api.send(g, api.intern("parse:from:"), &[_]Oop{ api.mkString(sample, sample.len), number }, 2);
    if (root == api.nilObj) return error.PegParseFailed;
    const cc = api.smallIntValue(api.send(root, api.intern("childCount"), empty, 0));
    if (cc != 5) return error.PegWrongCount;
    std.debug.print("[siphon] rung 3: peg parsed \"12345\" => {d} digit nodes, built + walked entirely through the id api\n", .{cc});

    // 8. RUNG 3b — load the lang module. It builds birnam's grammar through peg's
    //    message api; we parse a real birnam line and confirm a `program` CST.
    const lang_path = try a.dupeZ(u8, "zig-out/lib/libbirnam_id_lang.dylib");
    defer a.free(lang_path);
    _ = try loadModule(api, lang_path, null);
    const lang = api.import("Lang") orelse return error.NoLang;
    const prog = "let fizzy := [:n | [[n mod: 3] = 0]]";
    const cst = api.send(lang, api.intern("parse:"), &[_]Oop{api.mkString(prog, prog.len)}, 1);
    if (cst == api.nilObj) return error.BirnamParseFailed;
    if (api.send(cst, api.intern("rule"), empty, 0) != api.intern("program")) return error.NotAProgram;
    std.debug.print("[siphon] rung 3b: lang parsed a birnam let-binding => program CST, grammar built via the peg message api\n", .{});

    // 9. RUNG 3c — load the libtcc compiler module: compile C to native, install
    //    it as a method, and dispatch — the proximate goal's backend, end to end
    //    through the id api.
    const tcc_path = try a.dupeZ(u8, "zig-out/lib/libbirnam_id_tcc.dylib");
    defer a.free(tcc_path);
    _ = try loadModule(api, tcc_path, null);
    const tcc = api.import("Tcc") orelse return error.NoTcc;
    const csrc = "void* answer(void* self, void** args, long n){ (void)self;(void)args;(void)n; return (void*)((42L<<1)|1L); }";
    const unit = api.send(tcc, api.intern("compile:"), &[_]Oop{api.mkString(csrc, csrc.len)}, 1);
    if (unit == api.nilObj) return error.TccCompileFailed;
    const avt = api.proto(object_vt);
    const asel = api.intern("answer");
    const fname = "answer";
    _ = api.send(unit, api.intern("install:as:on:"), &[_]Oop{ api.mkString(fname, fname.len), asel, avt }, 3);
    const inst = api.alloc(avt, 0);
    const av = api.smallIntValue(api.send(inst, asel, empty, 0));
    if (av != 42) return error.TccDispatchWrong;
    std.debug.print("[siphon] rung 3c: libtcc compiled C → native, installed + dispatched => {d}, through the id api\n", .{av});

    // 10. RUNG 3c.2 — load the codegen module and run the full pipeline: birnam
    //     source → CST → emitted C → libtcc → native → value, all through the api.
    const codegen_path = try a.dupeZ(u8, "zig-out/lib/libbirnam_id_codegen.dylib");
    defer a.free(codegen_path);
    _ = try loadModule(api, codegen_path, null);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;
    const expr = "[[3 + 4] * 2]";
    const ev = api.smallIntValue(api.send(codegen, api.intern("eval:"), &[_]Oop{api.mkString(expr, expr.len)}, 1));
    if (ev != 14) return error.CodegenWrong;
    std.debug.print("[siphon] rung 3c.2: codegen compiled birnam {s} → C → native => {d}, through the id api\n", .{ expr, ev });

    // 11. RUNG 3c.3 — a real send: install #double on SmallInt, then compile
    //     birnam that DISPATCHES it. The emitted native code calls back into
    //     libid via the bound birnam_send / birnam_intern runtime shims.
    g_api = api;
    api.method(api.smallint_vt, api.intern("double"), &mDouble);
    const sendExpr = "[[20 plus: 1] double]";
    // (no `plus:` on SmallInt — use a keyword the demo installs too)
    api.method(api.smallint_vt, api.intern("plus:"), &mPlus);
    const sv = api.smallIntValue(api.send(codegen, api.intern("eval:"), &[_]Oop{api.mkString(sendExpr, sendExpr.len)}, 1));
    if (sv != 42) return error.CodegenSendWrong;
    std.debug.print("[siphon] rung 3c.3: codegen compiled birnam {s} → C → native, dispatched through libid => {d}\n", .{ sendExpr, sv });

    // 12. RUNG 3c.3 — the whole grammar: variables, a closure that captures an
    //     enclosing variable, compiled to native and invoked back through libid.
    const blkExpr = "let x := 20; let add := [:y | [x plus: y]]; [add value: 22]";
    const bv = api.smallIntValue(api.send(codegen, api.intern("eval:"), &[_]Oop{api.mkString(blkExpr, blkExpr.len)}, 1));
    if (bv != 42) return error.CodegenBlockWrong;
    std.debug.print("[siphon] rung 3c.3: codegen compiled a birnam closure (let + capture + block) → native => {d}\n", .{bv});

    // 13. docs/36 — the running system: define a global and a method that
    //     references it, then dispatch the method. The method is compiled to
    //     native and INSTALLED into the live id system; `base` is late-bound.
    const sysExpr =
        "[Globals at: #base put: 40]; " ++
        "[Reflect defineMethod: #boosted on: Integer as: [:self | [self + base]]]; " ++
        "[2 boosted]";
    const yv = api.smallIntValue(api.send(codegen, api.intern("eval:"), &[_]Oop{api.mkString(sysExpr, sysExpr.len)}, 1));
    if (yv != 42) return error.CodegenSystemWrong;
    std.debug.print("[siphon] docs/36: birnam defined a global + a method referencing it, dispatched => {d}\n", .{yv});

    std.debug.print("[siphon] ok — the whole birnam grammar compiles to native through the full stack (libid → store → peg → lang → tcc → codegen), all through the id api\n", .{});
}

/// The language stack a script needs, in dependency order. Loaded quietly
/// through the id api; codegen resolves globals through the store, so the store
/// comes first.
const script_modules = [_][]const u8{
    "libbirnam_store",
    "libbirnam_id_peg",
    "libbirnam_id_lang",
    "libbirnam_id_tcc",
    "libbirnam_id_codegen",
    "libbirnam_id_prelude",
    "libbirnam_id_collection",
    "libbirnam_id_error",
    "libbirnam_id_relational",
    "libbirnam_id_alien",
    "libbirnam_id_image",
};

// The self-hosted part of the standard library — birnam defining more of its own
// library on top of the Zig primitives. Embedded so it is always available; loaded
// by default, skipped with BIRNAM_NO_STDLIB=1 (the bare Zig floor as an option).
const STDLIB_SRC = @embedFile("stdlib.birnam");

/// Evaluate the self-hosted stdlib through codegen, unless opted out.
fn loadStdlib(api: *IdApi, codegen: Oop) void {
    if (std.posix.getenv("BIRNAM_NO_STDLIB") != null) return;
    _ = api.send(codegen, api.intern("eval:"), &[_]Oop{api.mkString(STDLIB_SRC, STDLIB_SRC.len)}, 1);
}

/// Resolve the directory holding the runtime module dylibs. Tries, in order:
/// `$BIRNAM_LIB_DIR`, then next to this executable (`<exedir>` and `<exedir>/../lib`
/// — so a baked exe placed beside its dylibs just works), then `zig-out/lib` (the
/// dev tree). The first directory that actually holds `libbirnam_store.dylib` wins;
/// falls back to `zig-out/lib`. Caller frees the returned slice.
fn resolveLibDir(a: std.mem.Allocator) ![]u8 {
    if (std.posix.getenv("BIRNAM_LIB_DIR")) |d| return a.dupe(u8, d);
    const probe = "libbirnam_store.dylib";
    if (std.fs.selfExeDirPathAlloc(a)) |exedir| {
        defer a.free(exedir);
        const parent_lib = try std.fs.path.join(a, &.{ exedir, "..", "lib" });
        defer a.free(parent_lib);
        for ([_][]const u8{ exedir, parent_lib }) |cand| {
            const p = try std.fs.path.join(a, &.{ cand, probe });
            defer a.free(p);
            if (std.fs.cwd().access(p, .{})) |_| {
                return a.dupe(u8, cand); // cand is exedir/parent_lib, both freed after we dupe
            } else |_| {}
        }
    } else |_| {}
    return a.dupe(u8, "zig-out/lib");
}

/// Load the full language stack quietly through the id api (store first, since
/// codegen resolves globals through it).
fn loadStack(a: std.mem.Allocator, api: *IdApi) !void {
    const lib_dir = try resolveLibDir(a);
    defer a.free(lib_dir);
    for (script_modules) |m| {
        const ps = try std.fmt.allocPrint(a, "{s}/{s}.dylib", .{ lib_dir, m });
        defer a.free(ps);
        const p = try a.dupeZ(u8, ps);
        defer a.free(p);
        _ = try loadModule(api, p.ptr, null);
    }
}

/// Compile + run a birnam script file. Loads the full stack quietly, then
/// `[Codegen evalPersisting: source]`. The script does its own I/O via `System`.
/// Top-level `let`s PERSIST to the Globals store (as in the REPL) — so a script
/// that binds `main` (or any global) leaves it in the store for `[System
/// saveImage:]` to capture, and records its source for image re-derivation
/// (docs/40 §1 entry-point convention, §6 source-backed block globals).
fn runScript(a: std.mem.Allocator, api: *IdApi, path: []const u8) !void {
    try loadStack(a, api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    // Anchor the conservative GC's stack scan here: every frame the eval (and the
    // native code it runs) pushes lives BELOW this local, so `[System gc]` from
    // within the script can find all stack-held roots. Without this, gcCollect is
    // a no-op (it refuses to sweep when it cannot find stack roots).
    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);

    loadStdlib(api, codegen);

    const src = try std.fs.cwd().readFileAlloc(a, path, 16 << 20);
    defer a.free(src);
    _ = api.send(codegen, api.intern("evalPersisting:"), &[_]Oop{api.mkString(src.ptr, src.len)}, 1);

    // Surface a compile failure instead of exiting quietly on a program that
    // silently became nil (a syntax error or an out-of-subset construct).
    const err = api.send(codegen, api.intern("lastError"), &[_]Oop{}, 0);
    if (err != api.nilObj) {
        out("birnam: compile error in ");
        out(path);
        out(":\n");
        outErrorString(api, err);
        std.process.exit(1);
    }
}

/// Print a `[Codegen lastError]` String (a `line:col: message` block) to stderr.
fn outErrorString(api: *IdApi, s: Oop) void {
    var len: usize = 0;
    const p = api.stringBytes(s, &len);
    std.fs.File.stderr().writeAll(p[0..len]) catch {};
    std.fs.File.stderr().writeAll("\n") catch {};
}

/// Boot an image: `birnam-id foo.image` (docs/40). Bring up the language stack
/// (the native module vtables the image relinks against), map the image back in,
/// then honor the entry-point convention — if the global `main` is bound, send it
/// `value`; otherwise open the REPL with every restored binding live (docs/40 §1).
///
/// By default the image's COMPILED code is MAPPED (its methods installed with no
/// recompile). `[System loadImage:]` answers `true` when it did so; `false` for a
/// data-only image or an arch/ABI mismatch (or `--rebuild`), where we fall back to
/// re-evaluating the library from source (here: the stdlib; user-method replay is
/// slice 4's next step).
fn runImage(a: std.mem.Allocator, api: *IdApi, path: []const u8, rebuild: bool) !void {
    try loadStack(a, api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);

    // Map the image. NOTE: stdlib is NOT loaded up front — a mapped `.code` section
    // installs the whole library (stdlib + user) with no recompile, which is the
    // point. We only re-eval the stdlib if the image had no mappable code.
    const system = api.import("System") orelse return error.NoSystem;
    const path_str = api.mkString(path.ptr, path.len);
    const rebuild_arg: Oop = if (rebuild) api.trueObj else api.falseObj;
    const status = api.send(system, api.intern("loadImage:rebuild:"), &[_]Oop{ path_str, rebuild_arg }, 2);
    if (status == api.nilObj) {
        out("[image] failed to load ");
        out(path);
        out("\n");
        return error.ImageLoadFailed;
    }
    // Code not mapped (data-only image, arch/ABI mismatch, or --rebuild): rebuild
    // the whole library by REPLAYING the image's own recorded method source
    // (docs/40 §4). This subsumes the stdlib; if the image predates source capture
    // and replays nothing, fall back to the embedded stdlib so the REPL is usable.
    if (status == api.falseObj) {
        const replayed = api.send(codegen, api.intern("replayMethods"), &[_]Oop{}, 0);
        if (api.smallIntValue(replayed) == 0) loadStdlib(api, codegen);
    }

    // Re-derive block-valued (and any deferred) globals from their recorded source
    // (docs/40 §6): a closure can't be serialized, so its value fact was skipped at
    // save; here we rebuild it from `__globalsrc__` over the now-resolved code. The
    // entry-point `main` closure lands through this path.
    _ = api.send(codegen, api.intern("deriveImageGlobals"), &[_]Oop{}, 0);

    // Checkpoint the append baseline: boot reconstruction (mapped/replayed code,
    // re-derived globals) re-asserts facts we must NOT re-journal — a later
    // `appendImage:` should carry only changes made AFTER boot (docs/40 §7.4).
    _ = api.send(system, api.intern("imageCheckpoint"), &[_]Oop{}, 0);

    // Entry-point convention: `main` bound → run it; else drop into the REPL.
    if (api.import("Globals")) |globals| {
        const m = api.send(globals, api.intern("at:"), &[_]Oop{api.intern("main")}, 1);
        if (m != api.nilObj) {
            _ = api.send(m, api.intern("value"), &[_]Oop{}, 0);
            return;
        }
    }
    return runRepl(a, api, true); // booted: stack + stdlib already up (mapped/replayed)
}

// -- the REPL: an interactive tool for exploring the running system ----------

fn out(s: []const u8) void {
    std.fs.File.stdout().writeAll(s) catch {};
}

/// Read one raw line from stdin (newline stripped), or null at EOF. Byte-at-a-time
/// so it needs no buffered-reader plumbing and never blocks past one line.
fn readRawLine(a: std.mem.Allocator) !?[]u8 {
    var b: std.ArrayList(u8) = .{};
    errdefer b.deinit(a);
    const stdin = std.fs.File.stdin();
    var got = false;
    while (true) {
        var ch: [1]u8 = undefined;
        const nr = stdin.read(&ch) catch return null;
        if (nr == 0) {
            if (!got) {
                b.deinit(a);
                return null;
            }
            break;
        }
        got = true;
        if (ch[0] == '\n') break;
        try b.append(a, ch[0]);
    }
    return try b.toOwnedSlice(a);
}

/// Bracket-balanced: are all `[`/`{` closed? Ignores brackets inside `"…"` strings
/// and `//` line comments, so multi-line forms read correctly. Over-closed (< 0)
/// counts as "done" — let the evaluator report the syntax error.
fn isBalanced(s: []const u8) bool {
    var depth: i32 = 0;
    var in_str = false;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        const c = s[i];
        if (in_str) {
            if (c == '"') in_str = false;
            continue;
        }
        if (c == '/' and i + 1 < s.len and s[i + 1] == '/') {
            while (i < s.len and s[i] != '\n') i += 1;
            continue;
        }
        switch (c) {
            '"' => in_str = true,
            '[', '{' => depth += 1,
            ']', '}' => depth -= 1,
            else => {},
        }
    }
    return depth <= 0;
}

/// Accumulate input lines until the form is bracket-balanced (a continuation
/// prompt is shown between them). Returns the joined source, or null at EOF.
fn readForm(a: std.mem.Allocator) !?[]u8 {
    var buf: std.ArrayList(u8) = .{};
    errdefer buf.deinit(a);
    while (true) {
        const ln = (try readRawLine(a)) orelse {
            if (buf.items.len == 0) {
                buf.deinit(a);
                return null;
            }
            break;
        };
        defer a.free(ln);
        if (buf.items.len > 0) try buf.append(a, '\n');
        try buf.appendSlice(a, ln);
        if (isBalanced(buf.items)) break;
        out("   ... ");
    }
    return try buf.toOwnedSlice(a);
}

/// Print an evaluation result: nil directly (it has no printString of its own),
/// everything else via `printString` (errors render as `error(#kind: msg)`).
fn printResult(api: *IdApi, r: Oop) void {
    if (r == api.nilObj) {
        out("nil\n");
        return;
    }
    const ps = api.send(r, api.intern("printString"), &[_]Oop{}, 0);
    if (ps != null and api.vtableOf(ps) == api.string_vt) {
        var len: usize = 0;
        const p = api.stringBytes(ps, &len);
        out(p[0..len]);
        out("\n");
    } else {
        out("(unprintable result)\n");
    }
}

/// List the user-visible globals (the persisted bindings), skipping internal
/// `__`-prefixed entries (block stashes, etc).
fn listGlobals(api: *IdApi) void {
    const store = api.import("Store") orelse return;
    const ge = api.intern("__globals__");
    const cnt: usize = @intCast(api.smallIntValue(api.send(store, api.intern("attrCount:"), &[_]Oop{ge}, 1)));
    var shown: usize = 0;
    var i: usize = 0;
    while (i < cnt) : (i += 1) {
        const attr = api.send(store, api.intern("attrAt:index:"), &[_]Oop{ ge, api.mkSmallInt(@intCast(i)) }, 2);
        const name = std.mem.span(api.symbolName(attr));
        if (name.len >= 2 and name[0] == '_' and name[1] == '_') continue; // internal
        const val = api.send(store, api.intern("currentValue:attr:"), &[_]Oop{ ge, attr }, 2);
        out("  ");
        out(name);
        out(" = ");
        printResult(api, val orelse api.nilObj);
        shown += 1;
    }
    if (shown == 0) out("  (no globals defined yet)\n");
}

fn replHelp() void {
    out(
        \\Commands:
        \\  :help        this message
        \\  :globals     list the bindings you've defined
        \\  :quit        leave the REPL  (or Ctrl-D)
        \\Everything else is birnam: an expression prints its value; `let x := …`
        \\binds a name that persists across lines; `[Reflect defineMethod:…]` extends
        \\the live system. Multi-line forms continue until the brackets balance.
        \\
    );
}

/// Handle a `:command`. Returns true to quit the REPL.
fn replCommand(api: *IdApi, cmd: []const u8) bool {
    if (std.mem.eql(u8, cmd, ":quit") or std.mem.eql(u8, cmd, ":q") or std.mem.eql(u8, cmd, ":exit")) return true;
    if (std.mem.eql(u8, cmd, ":help") or std.mem.eql(u8, cmd, ":h")) {
        replHelp();
        return false;
    }
    if (std.mem.eql(u8, cmd, ":globals") or std.mem.eql(u8, cmd, ":g")) {
        listGlobals(api);
        return false;
    }
    out("unknown command (try :help)\n");
    return false;
}

/// The interactive read-eval-print loop. Bindings persist via `evalPersisting:`;
/// input is parsed first so syntax errors report cleanly; faults are catchable
/// values, so the loop never dies on a bad line.
/// The interactive REPL. `booted` = entered from a mapped image (`runImage`), where
/// the stack and stdlib are ALREADY up — re-loading them would re-JIT the whole
/// library and pollute the store with fresh reconstruction facts (which a later
/// `appendImage:` would then journal). A standalone `--repl` passes false to do the
/// one-time init itself.
fn runRepl(a: std.mem.Allocator, api: *IdApi, booted: bool) !void {
    if (!booted) try loadStack(a, api);
    const codegen = api.import("Codegen") orelse return error.NoCodegen;
    const lang = api.import("Lang") orelse return error.NoLang;

    var stack_anchor: usize = 0;
    api.gcSetStackBottom(@intFromPtr(&stack_anchor));
    _ = &stack_anchor;
    if (std.posix.getenv("BIRNAM_GC_PRECISE") != null) api.gcSetPrecise(1);

    if (!booted) loadStdlib(api, codegen);

    out("birnam — an interactive image. :help for commands, :quit (or Ctrl-D) to leave.\n");
    while (true) {
        out("birnam> ");
        const form = (try readForm(a)) orelse break; // EOF
        defer a.free(form);
        const src = std.mem.trim(u8, form, " \t\r\n");
        if (src.len == 0) continue;
        if (src[0] == ':') {
            if (replCommand(api, src)) break;
            continue;
        }
        const result = api.send(codegen, api.intern("evalPersisting:"), &[_]Oop{api.mkString(src.ptr, src.len)}, 1);
        // A compile failure (syntax or out-of-subset) reports with location instead
        // of a bare nil; otherwise print the value.
        const err = api.send(codegen, api.intern("lastError"), &[_]Oop{}, 0);
        if (err != api.nilObj) outErrorString(api, err) else printResult(api, result);
    }
    out("\n");
}

/// Load (or reload) a module THROUGH the id api's own `dlopen`/`dlsym`, threading
/// the opaque per-module `state` so arbitrary state survives a hot reload.
fn loadModule(api: *IdApi, path: [*:0]const u8, state: ?*anyopaque) !?*anyopaque {
    const h = api.dlopen(path, RTLD_LAZY) orelse return error.LoadFailed;
    const raw = api.dlsym(h, "module_init") orelse return error.NoModuleInit;
    const init: idapi.ModuleInitFn = @ptrCast(@alignCast(raw));
    return init(api, state);
}

fn pingImpl(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = args;
    _ = n;
    return @ptrFromInt(7);
}

test "repl: isBalanced tracks brackets, ignoring strings and comments" {
    const expect = std.testing.expect;
    // Balanced / complete forms.
    try expect(isBalanced("[3 + 4]"));
    try expect(isBalanced("42"));
    try expect(isBalanced("#[1 2 3]"));
    try expect(isBalanced("{\"a\" -> 1}"));
    try expect(isBalanced("")); // nothing open
    // Incomplete — needs a continuation line.
    try expect(!isBalanced("let x := [#[1 2 3]"));
    try expect(!isBalanced("[foo"));
    try expect(!isBalanced("{ #a -> [b"));
    // Brackets inside a string don't count.
    try expect(isBalanced("[\"a ] b\"]"));
    try expect(!isBalanced("[\"unterminated"));
    // Brackets inside a // comment don't count.
    try expect(isBalanced("[1] // a ] comment ["));
    // Over-closed reads as done (the evaluator reports the real error).
    try expect(isBalanced("a]"));
}

test "bake: the exe trailer round-trips an image slice; plain bytes have none" {
    const expect = std.testing.expect;
    const a = std.testing.allocator;

    // Assemble a synthetic baked exe: [runtime bytes][image][u64 len][magic].
    const runtime = "MZ...a pretend runtime binary...";
    const image = "BIRNAMIM\x01\x00\x00\x00...pretend image payload...";
    var buf: std.ArrayList(u8) = .{};
    defer buf.deinit(a);
    try buf.appendSlice(a, runtime);
    try buf.appendSlice(a, image);
    var lb: [8]u8 = undefined;
    std.mem.writeInt(u64, &lb, image.len, .little);
    try buf.appendSlice(a, &lb);
    try buf.appendSlice(a, BAKE_MAGIC);

    // The trailer recovers exactly the image slice.
    const got = bakeTrailerImage(buf.items) orelse return error.NoTrailer;
    try std.testing.expectEqualStrings(image, got);

    // A plain binary (no trailer), an empty file, and a corrupt length → null.
    try expect(bakeTrailerImage(runtime) == null);
    try expect(bakeTrailerImage("") == null);
    var bad = try a.dupe(u8, buf.items);
    defer a.free(bad);
    std.mem.writeInt(u64, bad[bad.len - BAKE_TRAILER ..][0..8], 1 << 30, .little); // len past EOF
    try expect(bakeTrailerImage(bad) == null);
}
