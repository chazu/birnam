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

    if (bopts.strip_malleable) {
        // --strip=malleable: pure-VM binary. No peg, no frontend (parser /
        // AST-eval / REPL). Load the image FIRST so the restored interner
        // makes primitive selector interns resolve to the image's eids, THEN
        // install primitives (M3 langPrimitivesTier).
        lang.langSetup(@ptrCast(&shell_context), @ptrCast(&api), table.alloc());
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

        // M6 (minimal): send the entry selector to a SmallInt receiver,
        // exit with the result. (General module-entity entry: pending.)
        if (bopts.entry_selector.len > 0) {
            const sel = interner.str_to_eid.get(bopts.entry_selector) orelse {
                stderrPrint("[aot] entry selector '{s}' not in image\n", .{bopts.entry_selector});
                return;
            };
            const recv = ref_mod.mkSmallInt(bopts.entry_recv).?;
            var no_args: [0]ref_mod.Ref = .{};
            const result = send_mod.sendN(recv, sel, &no_args, 0);
            if (ref_mod.isSmallInt(result)) {
                const n = ref_mod.untagSmallInt(result);
                stdoutPrint("[aot] {s} {d} => {d}\n", .{ bopts.entry_selector, bopts.entry_recv, n });
                std.process.exit(@intCast(@as(u8, @truncate(@as(u64, @bitCast(n))))));
            }
        }
        return;
    }

    // No-strip: full tower. Static module init (peg before lang), then load.
    initModule(peg, &shell_context, &api, table.alloc());
    initModule(lang, &shell_context, &api, table.alloc());

    const loaded = image.load(store, BakedImage, interner) catch |e| {
        stderrPrint("[aot] image load failed: {}\n", .{e});
        return e;
    };
    if (loaded.next_eid > table.next_eid.load(.monotonic)) table.next_eid.store(loaded.next_eid, .monotonic);
    _ = image.rematerializeClosures(allocator, store, table) catch {};
    ffi_module.resolveAllPending(allocator, store, table) catch {};
    stderrPrint("[aot] loaded baked image ({d} facts), entering REPL\n", .{loaded.facts_loaded});

    // REPL over the baked state.
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
