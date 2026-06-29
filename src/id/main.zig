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

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const a = gpa.allocator();

    var args = try std.process.argsWithAllocator(a);
    defer args.deinit();
    _ = args.next();
    const script_path = args.next(); // optional: a .birnam file to compile + run
    const id_path = std.posix.getenv("BIRNAM_ID_DYLIB") orelse "zig-out/lib/libbirnam_id.dylib";
    const id_path_z = try a.dupeZ(u8, id_path);
    defer a.free(id_path_z);

    // The ONE irreducible raw load — open the id dylib, find `id_init`, call it to
    // get the id API table. Both the script runner and the demo build on this.
    var lib = try std.DynLib.open(id_path);
    const id_init = lib.lookup(idapi.IdInitFn, "id_init") orelse return error.NoIdInit;
    const api = id_init();
    g_api = api;

    // Script mode: `birnam-id path.birnam` compiles + runs the file, quietly.
    if (script_path) |sp| return runScript(a, api, sp);

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
};

/// Compile + run a birnam script file. Loads the full stack quietly, then
/// `[Codegen eval: source]`. The script does its own I/O via `System`.
fn runScript(a: std.mem.Allocator, api: *IdApi, path: []const u8) !void {
    const lib_dir = std.posix.getenv("BIRNAM_LIB_DIR") orelse "zig-out/lib";
    for (script_modules) |m| {
        const ps = try std.fmt.allocPrint(a, "{s}/{s}.dylib", .{ lib_dir, m });
        defer a.free(ps);
        const p = try a.dupeZ(u8, ps);
        defer a.free(p);
        _ = try loadModule(api, p.ptr, null);
    }
    const codegen = api.import("Codegen") orelse return error.NoCodegen;

    const src = try std.fs.cwd().readFileAlloc(a, path, 16 << 20);
    defer a.free(src);
    _ = api.send(codegen, api.intern("eval:"), &[_]Oop{api.mkString(src.ptr, src.len)}, 1);
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
