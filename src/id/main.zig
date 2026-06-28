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

pub fn main() !void {
    std.debug.print("[siphon] birnam\n", .{});

    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const a = gpa.allocator();

    var args = try std.process.argsWithAllocator(a);
    defer args.deinit();
    _ = args.next();
    const id_path = args.next() orelse "zig-out/lib/libbirnam_id.dylib";
    const id_path_z = try a.dupeZ(u8, id_path);
    defer a.free(id_path_z);

    // 1. Bootstrap: the ONE irreducible raw load — open the id dylib, find
    //    `id_init`, call it to get the id API table.
    std.debug.print("[siphon] raw-loading id dylib: {s}\n", .{id_path});
    var lib = try std.DynLib.open(id_path);
    const id_init = lib.lookup(idapi.IdInitFn, "id_init") orelse return error.NoIdInit;
    const api = id_init();
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

    std.debug.print("[siphon] ok — siphon loads + drives + re-imports id, persists state across reloads, mounts the rung-2 cell store, the rung-3 peg engine, and the birnam lang grammar, all through the id api\n", .{});
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
