//! Phase 14 M1 — AOT static entry point (no-strip slice).
//!
//! A standalone executable with kernel + modules + a `.brnimg` snapshot all
//! linked in. No `dlopen`: modules are statically `@import`ed and their init
//! called namespace-qualified (M4 — the shared `birnam_module_init` export
//! symbol doesn't collide because these are intra-Zig calls). The image is
//! `@embedFile`d; at startup we boot, run module init, load the embedded
//! snapshot, rebuild closures, then drive the normal REPL over the baked
//! state.
//!
//! This slice = `--aot=static` with no strip (full malleable tower). The
//! strip-flag matrix (drop frontend / collapse history) and the
//! `birnam --aot=…` driver that emits + compiles this per-app are follow-ups.

const std = @import("std");
const fact_store = @import("kernel/fact_store.zig");
const object_table = @import("kernel/object_table.zig");
const interner_mod = @import("kernel/interner.zig");
const bootstrap = @import("kernel/bootstrap.zig");
const id_api_mod = @import("kernel/id_api.zig");
const image = @import("kernel/image.zig");
const prune = @import("kernel/prune.zig");
const ffi_module = @import("kernel/ffi_module.zig");
const send_mod = @import("kernel/send.zig");
const ref_mod = @import("kernel/ref.zig");
const kernel_eids = @import("kernel/kernel_eids.zig");
const module = @import("kernel/module.zig");
const dylib_search = @import("dylib_search.zig");
const bopts = @import("build_opts");

// Modules linked statically (self-contained — import only std, talk via the
// C-ABI IdApi). Init is called namespace-qualified; the kernel's ShellContext
// / IdApi are ptr-cast to each module's ABI-identical mirror types.
const peg = @import("modules/peg/peg.zig");
const lang = @import("modules/lang/lang.zig");

// Embed path is a build option so the `birnam --aot` driver can point it at a
// per-app image (default = the in-repo demo fixture).
const BakedImage = @embedFile(bopts.embed_path);

fn stderrPrint(comptime fmt: []const u8, args: anytype) void {
    var buf: [512]u8 = undefined;
    const s = std.fmt.bufPrint(&buf, fmt, args) catch return;
    _ = std.fs.File.stderr().write(s) catch {};
}
fn stdoutPrint(comptime fmt: []const u8, args: anytype) void {
    var buf: [512]u8 = undefined;
    const s = std.fmt.bufPrint(&buf, fmt, args) catch return;
    _ = std.fs.File.stdout().write(s) catch {};
}

var repl_handler: ?*const fn ([*:0]const u8) callconv(.c) void = null;
fn shellLog(msg: [*:0]const u8) callconv(.c) void {
    stderrPrint("{s}\n", .{std.mem.span(msg)});
}
fn shellSetReplHandler(h: ?*const fn ([*:0]const u8) callconv(.c) void) callconv(.c) void {
    repl_handler = h;
}
var shell_context = id_api_mod.ShellContext{ .log = &shellLog, .set_repl_handler = &shellSetReplHandler };

/// Call a statically-linked module's `birnam_module_init`, casting the
/// kernel ShellContext/IdApi to the module's ABI-identical mirror types.
fn initModule(comptime M: type, ctx: *id_api_mod.ShellContext, api: *const id_api_mod.IdApi, eid: u64) void {
    const InitFn = *const fn (*anyopaque, *const anyopaque, u64) callconv(.c) void;
    const f: InitFn = @ptrCast(&M.moduleInitImpl);
    f(@ptrCast(ctx), @ptrCast(api), eid);
}

/// Walk a scope's binding chain (and parents) for `name_eid`, returning the
/// bound value Ref. Mirrors lang's `lookupInScope` but uses the kernel IdApi
/// directly (no lang dependency under --strip=malleable).
fn lookupBinding(api: *const id_api_mod.IdApi, scope_eid0: u64, name_eid: u64) ?ref_mod.Ref {
    var cur = scope_eid0;
    while (cur != 0) {
        var b_ref = api.query_eavt(cur, kernel_eids.ATTR_SCOPE_FIRST_BINDING);
        while (ref_mod.isEid(b_ref) and b_ref != ref_mod.NIL) {
            const b_eid = b_ref >> 3;
            const name_ref = api.query_eavt(b_eid, kernel_eids.ATTR_BIND_NAME);
            if (ref_mod.isEid(name_ref) and (name_ref >> 3) == name_eid) {
                return api.query_eavt(b_eid, kernel_eids.ATTR_BIND_VALUE);
            }
            b_ref = api.query_eavt(b_eid, kernel_eids.ATTR_BIND_NEXT);
        }
        const parent = api.query_eavt(cur, kernel_eids.ATTR_SCOPE_PARENT);
        if (!ref_mod.isEid(parent) or parent == ref_mod.NIL) return null;
        cur = parent >> 3;
    }
    return null;
}

/// M6 entry point. `--entry=<selector>[:<recv>]`: send `<selector>` (zero args)
/// to the receiver, then exit with the result's low 8 bits (SmallInt) or 0.
/// `<recv>` resolves as: a bare integer → SmallInt; an identifier → the value
/// bound to that name in the top-level scope (a Module, a user object, …) — the
/// general module-entity entry. Absent → SmallInt 0. Exits the process; returns
/// only when no `--entry` was baked in (so the caller falls through to a REPL).
fn runEntry(interner: *interner_mod.Interner, api: *const id_api_mod.IdApi) void {
    if (bopts.entry_selector.len == 0) return;
    const sel = interner.str_to_eid.get(bopts.entry_selector) orelse {
        stderrPrint("[aot] entry selector '{s}' not in image\n", .{bopts.entry_selector});
        std.process.exit(70);
    };
    const recv = if (bopts.entry_recv_name.len > 0) blk: {
        const name_eid = interner.str_to_eid.get(bopts.entry_recv_name) orelse {
            stderrPrint("[aot] entry receiver '{s}' not bound in image\n", .{bopts.entry_recv_name});
            std.process.exit(70);
        };
        break :blk lookupBinding(api, kernel_eids.TOP_SCOPE_EID, name_eid) orelse {
            stderrPrint("[aot] entry receiver '{s}' not bound in image\n", .{bopts.entry_recv_name});
            std.process.exit(70);
        };
    } else ref_mod.mkSmallInt(bopts.entry_recv).?;

    var no_args: [0]ref_mod.Ref = .{};
    const result = send_mod.sendN(recv, sel, &no_args, 0);
    if (ref_mod.isSmallInt(result)) {
        const n = ref_mod.untagSmallInt(result);
        stdoutPrint("[aot] {s} => {d}\n", .{ bopts.entry_selector, n });
        std.process.exit(@as(u8, @truncate(@as(u64, @bitCast(n)))));
    }
    std.process.exit(0);
}

/// Drive a line-oriented REPL over the baked state via the lang module's
/// registered handler. Shared by the static-full and dynamic (full-tower)
/// paths.
fn replLoop() void {
    const reader = std.fs.File.stdin().deprecatedReader();
    var buf: [4096]u8 = undefined;
    while (true) {
        stdoutPrint("birnam> ", .{});
        const line = reader.readUntilDelimiterOrEof(&buf, '\n') catch continue;
        if (line) |l| {
            if (repl_handler) |h| {
                if (l.len < buf.len) {
                    buf[l.len] = 0;
                    h(@ptrCast(buf[0 .. l.len + 1].ptr));
                }
            }
        } else break;
    }
}

/// `--aot=static --strip=malleable`: pure-VM binary. No peg, no frontend
/// (parser / AST-eval / REPL). Load the image FIRST so the restored interner
/// makes primitive selector interns resolve to the image's eids, THEN install
/// primitives (M3 langPrimitivesTier).
fn runStaticMalleable(allocator: std.mem.Allocator, store: *fact_store.FactStore, table: *object_table.ObjectTable, interner: *interner_mod.Interner, api: *const id_api_mod.IdApi) !void {
    lang.langSetup(@ptrCast(&shell_context), @ptrCast(api), table.alloc());
    const loaded = image.load(store, BakedImage, interner) catch |e| {
        stderrPrint("[aot] image load failed: {}\n", .{e});
        return e;
    };
    if (loaded.next_eid > table.next_eid.load(.monotonic)) table.next_eid.store(loaded.next_eid, .monotonic);
    _ = prune.stripAst(store, table, allocator) catch {}; // M2: drop user AST trees
    lang.langPrimitivesTier();
    _ = image.rematerializeClosures(allocator, store, table) catch {};
    ffi_module.resolveAllPending(allocator, store, table) catch {};
    stderrPrint("[aot] malleable: loaded baked image ({d} facts), AST stripped\n", .{loaded.facts_loaded});

    // M6: send the entry selector to the resolved receiver, exit with the
    // result. No entry baked → nothing to drive (no REPL under malleable).
    runEntry(interner, api);
}

/// `--aot=static` (no strip): full tower linked in. Static module init (peg
/// before lang), then load. `--entry` runs-and-exits; otherwise REPL.
fn runStaticFull(allocator: std.mem.Allocator, store: *fact_store.FactStore, table: *object_table.ObjectTable, interner: *interner_mod.Interner, api: *const id_api_mod.IdApi) !void {
    initModule(peg, &shell_context, api, table.alloc());
    initModule(lang, &shell_context, api, table.alloc());

    const loaded = image.load(store, BakedImage, interner) catch |e| {
        stderrPrint("[aot] image load failed: {}\n", .{e});
        return e;
    };
    if (loaded.next_eid > table.next_eid.load(.monotonic)) table.next_eid.store(loaded.next_eid, .monotonic);
    _ = image.rematerializeClosures(allocator, store, table) catch {};
    ffi_module.resolveAllPending(allocator, store, table) catch {};

    runEntry(interner, api);
    stderrPrint("[aot] loaded baked image ({d} facts), entering REPL\n", .{loaded.facts_loaded});
    replLoop();
}

/// `--aot=dynamic`: kernel + baked image linked in; modules `dlopen`'d at
/// startup from the image's embedded manifest, resolved by basename against
/// the dylib search path (`$BIRNAM_DYLIB_PATH`, cwd + below, exe dir). This is
/// the portable "ship a plugin host" artifact — peg/lang are NOT statically
/// linked (the static branches are comptime-pruned), so the binary stays the
/// kernel only and finds its modules at runtime. `--entry` runs-and-exits;
/// otherwise REPL (the dlopen'd lang registered the handler).
fn runDynamic(allocator: std.mem.Allocator, store: *fact_store.FactStore, table: *object_table.ObjectTable, interner: *interner_mod.Interner, api: *const id_api_mod.IdApi) !void {
    var arena = std.heap.ArenaAllocator.init(allocator);
    defer arena.deinit();
    const aa = arena.allocator();

    const manifest = image.peekManifest(aa, BakedImage) catch |e| {
        stderrPrint("[aot] dynamic: manifest parse failed: {}\n", .{e});
        return e;
    };

    var libs = std.ArrayList(std.DynLib){};
    defer libs.deinit(allocator);
    for (manifest.modules) |dep| {
        const base = std.fs.path.basename(dep.name);
        const path = dylib_search.resolve(aa, dep.name) orelse {
            stderrPrint("[aot] dynamic: module dylib '{s}' not found (set BIRNAM_DYLIB_PATH or stage it beside the binary)\n", .{base});
            std.process.exit(70);
        };
        var lib = std.DynLib.open(path) catch {
            stderrPrint("[aot] dynamic: dlopen failed for {s}\n", .{path});
            std.process.exit(70);
        };
        const init_fn = lib.lookup(module.InitFn, "birnam_module_init") orelse {
            stderrPrint("[aot] dynamic: {s} has no birnam_module_init\n", .{base});
            std.process.exit(70);
        };
        // ABI-identical mirror types — call directly with kernel structs
        // (same as module.Manager.load).
        init_fn(&shell_context, api, table.alloc());
        libs.append(allocator, lib) catch {};
        stderrPrint("[aot] dynamic: loaded {s}\n", .{base});
    }

    const loaded = image.load(store, BakedImage, interner) catch |e| {
        stderrPrint("[aot] image load failed: {}\n", .{e});
        return e;
    };
    if (loaded.next_eid > table.next_eid.load(.monotonic)) table.next_eid.store(loaded.next_eid, .monotonic);
    _ = image.rematerializeClosures(allocator, store, table) catch {};
    ffi_module.resolveAllPending(allocator, store, table) catch {};

    runEntry(interner, api);
    stderrPrint("[aot] dynamic: loaded baked image ({d} facts), entering REPL\n", .{loaded.facts_loaded});
    replLoop();
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const store = try allocator.create(fact_store.FactStore);
    store.* = fact_store.FactStore.init(allocator);
    const table = try allocator.create(object_table.ObjectTable);
    table.* = object_table.ObjectTable.init(allocator);
    const interner = try allocator.create(interner_mod.Interner);
    interner.* = interner_mod.Interner.init(allocator);

    _ = try bootstrap.boot(allocator, store, table, interner);
    id_api_mod.setInterner(interner);
    const api = id_api_mod.populate();

    // Comptime branch: under --aot=dynamic the static-tower helpers (which
    // reference peg/lang) are never analyzed, so those modules are not linked.
    if (comptime bopts.dynamic) {
        try runDynamic(allocator, store, table, interner, &api);
    } else if (bopts.strip_malleable) {
        try runStaticMalleable(allocator, store, table, interner, &api);
    } else {
        try runStaticFull(allocator, store, table, interner, &api);
    }
}
