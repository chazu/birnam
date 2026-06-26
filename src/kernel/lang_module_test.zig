//! Phase 10.1.b — Lang dylib integration smoke test.
//!
//! Validates that the lang dylib's grammar construction round-trips:
//! load PEG dylib first (installs PEG_VT / GRAMMAR_VT methods), then
//! load lang dylib (asserts birnam grammar via PEG send dispatch),
//! then send `parse:from:` against the asserted grammar and verify CST.

const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const id_api_mod = @import("id_api.zig");
const module = @import("module.zig");
const kernel_eids = @import("kernel_eids.zig");
const bootstrap = @import("bootstrap.zig");
const send_mod = @import("send.zig");
const strings = @import("strings.zig");
const image = @import("image.zig");
const boxed = @import("boxed.zig");
const freeze = @import("freeze.zig");
const ffi_module = @import("ffi_module.zig");
const prune = @import("prune.zig");
const mcache = @import("mcache.zig");

/// Assert a Ref is exactly SmallInt(want). Checks the tag first so a regression
/// that drops/garbles the value fails cleanly instead of panicking in untagSmallInt.
fn expectInt(want: i64, r: Ref) !void {
    try testing.expect(ref_mod.isSmallInt(r));
    try testing.expectEqual(want, ref_mod.untagSmallInt(r));
}

const Ref = ref_mod.Ref;

fn shellLog(_: [*:0]const u8) callconv(.c) void {}
fn shellSetReplHandler(_: ?*const fn ([*:0]const u8) callconv(.c) void) callconv(.c) void {}

var shell_ctx = id_api_mod.ShellContext{
    .log = &shellLog,
    .set_repl_handler = &shellSetReplHandler,
};

fn fullSetup(arena: *std.heap.ArenaAllocator) !struct {
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
    manager: *module.Manager,
    api: *const id_api_mod.IdApi,
} {
    const allocator = arena.allocator();
    const store = try allocator.create(fact_store.FactStore);
    store.* = fact_store.FactStore.init(allocator);
    const table = try allocator.create(object_table.ObjectTable);
    table.* = object_table.ObjectTable.init(allocator);
    const interner = try allocator.create(interner_mod.Interner);
    interner.* = interner_mod.Interner.init(allocator);
    _ = try bootstrap.boot(allocator, store, table, interner);
    id_api_mod.setInterner(interner);

    const api_struct = try allocator.create(id_api_mod.IdApi);
    api_struct.* = id_api_mod.populate();

    const manager = try allocator.create(module.Manager);
    manager.* = module.Manager.init(allocator, &shell_ctx, api_struct.*);
    return .{ .store = store, .table = table, .interner = interner, .manager = manager, .api = api_struct };
}

test "lang dylib: loads after PEG, exports grammar on module entity" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);

    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const sel_grammar = s.interner.str_to_eid.get("grammar").?;
    const g_ref = s.store.currentValue(lang_mod_eid, sel_grammar) orelse return error.GrammarNotExported;
    try testing.expect(ref_mod.isEid(g_ref));
}

fn parseVia(s: anytype, src: []const u8, rule: []const u8, lang_mod_eid: u64, a: std.mem.Allocator) Ref {
    const sel_grammar = s.interner.str_to_eid.get("grammar").?;
    const sel_parse = s.interner.str_to_eid.get("parse:from:").?;
    const grammar = s.store.currentValue(lang_mod_eid, sel_grammar).?;
    return send_mod.send2(
        grammar,
        sel_parse,
        strings.mkString(a, src) catch unreachable,
        strings.mkString(a, rule) catch unreachable,
    );
}

test "lang grammar: identifier matches \"hello\"" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);

    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const ast_ref = parseVia(s, "hello", "identifier", lang_mod_eid, a);
    const ast = ref_mod.untagEid(ast_ref);
    // A successful parse yields a CST node tagged CST_NODE_VT (since 1ab5d21);
    // an error entity would be ERROR_VT. Asserting CST_NODE_VT confirms success.
    try testing.expectEqual(kernel_eids.CST_NODE_VT, ref_mod.untagEid(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE).?));
    const end_ref = s.store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 5), ref_mod.untagSmallInt(end_ref));
}

test "lang grammar: integer matches \"42\"" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const ast = ref_mod.untagEid(parseVia(s, "42", "integer", lang_mod_eid, a));
    const end_ref = s.store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(end_ref));
}

test "lang grammar: start matches \"  42  \" (whitespace-wrapped atom)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const ast = ref_mod.untagEid(parseVia(s, "  42  ", "start", lang_mod_eid, a));
    // A successful parse yields a CST node tagged CST_NODE_VT (since 1ab5d21);
    // an error entity would be ERROR_VT. Asserting CST_NODE_VT confirms success.
    try testing.expectEqual(kernel_eids.CST_NODE_VT, ref_mod.untagEid(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE).?));
    const end_ref = s.store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 6), ref_mod.untagSmallInt(end_ref));
}

test "lang grammar: start matches \"true\" via atom choice" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const ast = ref_mod.untagEid(parseVia(s, "true", "start", lang_mod_eid, a));
    // A successful parse yields a CST node tagged CST_NODE_VT (since 1ab5d21);
    // an error entity would be ERROR_VT. Asserting CST_NODE_VT confirms success.
    try testing.expectEqual(kernel_eids.CST_NODE_VT, ref_mod.untagEid(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE).?));
    const end_ref = s.store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 4), ref_mod.untagSmallInt(end_ref));
}

test "lang grammar: string literal \"\\\"hello\\\"\"" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const ast = ref_mod.untagEid(parseVia(s, "\"hello\"", "string", lang_mod_eid, a));
    // A successful parse yields a CST node tagged CST_NODE_VT (since 1ab5d21);
    // an error entity would be ERROR_VT. Asserting CST_NODE_VT confirms success.
    try testing.expectEqual(kernel_eids.CST_NODE_VT, ref_mod.untagEid(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE).?));
    const end_ref = s.store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 7), ref_mod.untagSmallInt(end_ref));
}

test "lang grammar: reserved-word boundary — \"self2\" parses as identifier, not kw-self" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // kw-self alone on "self2" must fail (because the trailing "2" is alphanumeric).
    const r = parseVia(s, "self2", "kw-self", lang_mod_eid, a);
    const err = ref_mod.untagEid(r);
    try testing.expectEqual(kernel_eids.ERROR_VT, ref_mod.untagEid(s.store.currentValue(err, kernel_eids.ATTR_VTABLE).?));

    // identifier should match the whole thing.
    const ok = ref_mod.untagEid(parseVia(s, "self2", "identifier", lang_mod_eid, a));
    const end_ref = s.store.currentValue(ok, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 5), ref_mod.untagSmallInt(end_ref));
}

test "lang grammar: bracket-unary \"[3 squared]\"" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const ast = ref_mod.untagEid(parseVia(s, "[3 squared]", "start", lang_mod_eid, a));
    // A successful parse yields a CST node tagged CST_NODE_VT (since 1ab5d21);
    // an error entity would be ERROR_VT. Asserting CST_NODE_VT confirms success.
    try testing.expectEqual(kernel_eids.CST_NODE_VT, ref_mod.untagEid(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE).?));
    const end_ref = s.store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 11), ref_mod.untagSmallInt(end_ref));
}

test "lang grammar: bracket-keyword \"[obj at: 0 put: nil]\" (compound keyword)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const ast = ref_mod.untagEid(parseVia(s, "[obj at: 0 put: nil]", "start", lang_mod_eid, a));
    // A successful parse yields a CST node tagged CST_NODE_VT (since 1ab5d21);
    // an error entity would be ERROR_VT. Asserting CST_NODE_VT confirms success.
    try testing.expectEqual(kernel_eids.CST_NODE_VT, ref_mod.untagEid(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE).?));
    const end_ref = s.store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 20), ref_mod.untagSmallInt(end_ref));
}

test "lang grammar: nested send \"[[1 succ] squared]\"" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const ast = ref_mod.untagEid(parseVia(s, "[[1 succ] squared]", "start", lang_mod_eid, a));
    // A successful parse yields a CST node tagged CST_NODE_VT (since 1ab5d21);
    // an error entity would be ERROR_VT. Asserting CST_NODE_VT confirms success.
    try testing.expectEqual(kernel_eids.CST_NODE_VT, ref_mod.untagEid(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE).?));
    const end_ref = s.store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 18), ref_mod.untagSmallInt(end_ref));
}

test "DEMO: parse + eval [3 squared] → 9" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    const src_ref = try strings.mkString(a, "[3 squared]");
    const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
    try testing.expect(ref_mod.isEid(ast_ref));

    const val = send_mod.send0(ast_ref, sel_eval);
    try testing.expect(ref_mod.isSmallInt(val));
    try testing.expectEqual(@as(i64, 9), ref_mod.untagSmallInt(val));
}

test "DEMO 16 deferred: let destructuring — #[a b] := seq" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    // Destructure into the top scope, then read the bindings back. Each form is
    // parsed + evaluated against the persistent REPL scope in sequence.
    const prog = [_][]const u8{
        "let #[a b c] := #[10 20 30]",
        "[a + [b + c]]", // 10+20+30 = 60
    };
    var result: ref_mod.Ref = ref_mod.NIL;
    for (prog) |line| {
        const src = try strings.mkString(a, line);
        const ast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, src);
        result = send_mod.send0(ast, sel_eval);
    }
    try testing.expectEqual(@as(i64, 60), ref_mod.untagSmallInt(result));

    // Destructure a range too.
    {
        const l1 = try strings.mkString(a, "let #[x y] := [1 to: 9]");
        _ = send_mod.send0(send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, l1), sel_eval);
        const l2 = try strings.mkString(a, "[x + y]"); // 1 + 2
        const r = send_mod.send0(send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, l2), sel_eval);
        try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(r));
    }
}

/// Cluster ① — parse `src` and run it through the COMPILED top-level path
/// (api.eval_top_level), exactly as replHandler/file-in do in `.bytecode`
/// mode. `ok` is set false when the statement couldn't be compiled.
fn evalTopLevel(
    api: *const id_api_mod.IdApi,
    interner: *interner_mod.Interner,
    mod: u64,
    src: []const u8,
    a: std.mem.Allocator,
    ok: *bool,
) Ref {
    const sel_parse = interner.str_to_eid.get("parseExpr:").?;
    const sel_lookup = interner.str_to_eid.get("lookup:").?;
    const sel_bind = interner.str_to_eid.get("bind:value:").?;
    const sel_set = interner.str_to_eid.get("set:value:").?;
    const src_ref = strings.mkString(a, src) catch return ref_mod.NIL;
    const ast = send_mod.send1(ref_mod.mkEid(mod), sel_parse, src_ref);
    return api.eval_top_level(ast >> 3, kernel_eids.TOP_SCOPE_EID, sel_lookup, sel_bind, sel_set, ok);
}

test "Cluster ①: compiled top-level — cross-line let / global / := / undefined" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // `let n := 41` compiles (outer let lowers to bind:value:) + persists.
    var ok: bool = false;
    const r1 = evalTopLevel(s.api, s.interner, lang, "let n := 41", a, &ok);
    try testing.expect(ok);
    try testing.expectEqual(@as(i64, 41), ref_mod.untagSmallInt(r1));

    // A LATER line sees it: free var `n` lowers to lookup: → 41 + 1 = 42.
    ok = false;
    const r2 = evalTopLevel(s.api, s.interner, lang, "[n + 1]", a, &ok);
    try testing.expect(ok);
    try testing.expectEqual(@as(i64, 42), ref_mod.untagSmallInt(r2));

    // `n := 100` lowers to set: → mutates the existing binding; later read sees it.
    ok = false;
    _ = evalTopLevel(s.api, s.interner, lang, "n := 100", a, &ok);
    try testing.expect(ok);
    ok = false;
    const r3 = evalTopLevel(s.api, s.interner, lang, "n", a, &ok);
    try testing.expect(ok);
    try testing.expectEqual(@as(i64, 100), ref_mod.untagSmallInt(r3));

    // Global binding `Module` resolves through the scope chain (not an error).
    ok = false;
    const r4 = evalTopLevel(s.api, s.interner, lang, "Module", a, &ok);
    try testing.expect(ok);
    try testing.expect(ref_mod.isEid(r4));
    try testing.expect(s.store.currentValue(r4 >> 3, kernel_eids.ATTR_VTABLE) != ref_mod.mkEid(kernel_eids.ERROR_VT));

    // An undefined free var compiles + runs (ok) but yields an error VALUE,
    // identical to the tree-walker's undefined-variable result.
    ok = false;
    const r5 = evalTopLevel(s.api, s.interner, lang, "nope", a, &ok);
    try testing.expect(ok);
    try testing.expect(ref_mod.isEid(r5));
    try testing.expectEqual(ref_mod.mkEid(kernel_eids.ERROR_VT), s.store.currentValue(r5 >> 3, kernel_eids.ATTR_VTABLE).?);
}

test "15.A-AST: top-level method def + closure-in-var survive :prune; churn sweeps" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    var ok: bool = false;
    // A method DEFINED at the top level (compiled path → the block arrives as a
    // bytecode closure; must install a prune-rootable :bytecode-stream method).
    _ = evalTopLevel(s.api, s.interner, lang, "[[3 vtable] method: #pfoo do: [| 42]]", a, &ok);
    try testing.expect(ok);
    ok = false;
    try expectInt(42, evalTopLevel(s.api, s.interner, lang, "[3 pfoo]", a, &ok));

    // A method with NESTED blocks (its block children live in its constant pool;
    // prune must mark them forward via pool-descent, not the removed over-seed).
    ok = false;
    _ = evalTopLevel(s.api, s.interner, lang, "[[3 vtable] method: #pbig do: [| [[self < 10] ifTrue: [| 100] ifFalse: [| 200]]]]", a, &ok);
    try testing.expect(ok);
    ok = false;
    try expectInt(100, evalTopLevel(s.api, s.interner, lang, "[3 pbig]", a, &ok));

    // A closure BOUND to a top-level var references its block method via an
    // in-memory pointer — prune must keep it alive (closure-aware marking).
    ok = false;
    _ = evalTopLevel(s.api, s.interner, lang, "let pf := [:x | [x + 1]]", a, &ok);
    ok = false;
    try expectInt(6, evalTopLevel(s.api, s.interner, lang, "[pf value: 5]", a, &ok));

    // Generate throwaway top-level churn, then prune.
    var i: usize = 0;
    while (i < 5) : (i += 1) {
        ok = false;
        _ = evalTopLevel(s.api, s.interner, lang, "[1 + 1]", a, &ok);
    }
    const stats = try prune.pruneReachable(s.store, s.table, a);
    try testing.expect(stats.swept_entities > 0); // dead top-level/throwaway trees collected

    // ROBUST (cache-independent): each method's install FACT survived prune AND is
    // a proper :bytecode-stream method — not a raw closure (the Cluster ①
    // regression) and not dropped (the Root-2 / closure-aware bug). Reading facts
    // directly cannot be fooled by a stale dispatch cache (what masked the bug).
    for ([_][]const u8{ "pfoo", "pbig" }) |name| {
        const sel = s.interner.str_to_eid.get(name).?;
        const install = s.store.currentValue(kernel_eids.SMALLINT_VT, sel) orelse
            return error.MethodInstallDroppedByPrune;
        try testing.expect(ref_mod.isEid(install));
        try testing.expect(s.store.currentValue(ref_mod.untagEid(install), kernel_eids.ATTR_BYTECODE_STREAM) != null);
    }

    // Force a COLD rebind (drop any stale cache) so the re-dispatch below reflects
    // the real post-prune store, not a cache hit. All three live things still work.
    mcache.clearAll();
    ok = false;
    try expectInt(42, evalTopLevel(s.api, s.interner, lang, "[3 pfoo]", a, &ok));
    try testing.expect(ok);
    ok = false;
    try expectInt(100, evalTopLevel(s.api, s.interner, lang, "[3 pbig]", a, &ok));
    try testing.expect(ok);
    ok = false;
    try expectInt(6, evalTopLevel(s.api, s.interner, lang, "[pf value: 5]", a, &ok));
    try testing.expect(ok);
}

// NOTE on the prune mcache flush: it is NOT independently unit-tested here on
// purpose. `compact` re-asserts every kept fact, which fires the per-attribute
// mcache flush hook — so a *kept* install's cache entry is invalidated whether or
// not prune calls `mcache.clearAll()`. The explicit `clearAll()` in prune only
// matters for a *dropped* install whose attr has no surviving fact (a stale entry
// → freed Closure → use-after-free); the regression-causing case (a method install
// being dropped at all) is what the cache-independent fact assertions in the test
// above actually guard. A test of the dropped-install path would have to sweep a
// live-cached method, which the install-rooting fix now prevents for real methods.

test "15.A-AST: prelude methods are CODE-provenance (not M_USER)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    _ = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // `min:` is installed by the prelude (installSourceMethod). Its method ENTITY
    // must be materialized under the module eid (CODE) — reconstructed each boot,
    // not imaged, never swept as user data — NOT M_USER. (If this regresses to
    // M_USER, prune can't root it by its install and images bloat with it.)
    const min_sel = s.interner.str_to_eid.get("min:").?;
    const m_ref = s.store.currentValue(kernel_eids.SMALLINT_VT, min_sel) orelse return error.NoMinMethod;
    try testing.expect(ref_mod.isEid(m_ref));
    const m_eid = ref_mod.untagEid(m_ref);

    var it = s.store.meatIter();
    var saw: bool = false;
    while (it.next()) |f| {
        if (f.e != m_eid) continue;
        saw = true;
        try testing.expect(f.m != kernel_eids.M_USER); // CODE provenance, not user data
    }
    try testing.expect(saw); // the method entity has facts (and they're all CODE)
}

test "15.A-AST: auto-prune fires on churn, preserves live state, gated on retention" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    var ok: bool = false;
    _ = evalTopLevel(s.api, s.interner, lang, "let keep := 99", a, &ok);
    _ = evalTopLevel(s.api, s.interner, lang, "[[3 vtable] method: #amk do: [| 7]]", a, &ok);

    // Generate enough throwaway churn to cross the auto-prune growth threshold.
    prune.resetAutoPrune(s.table);
    const base = s.table.highWaterEid();
    while (s.table.highWaterEid() < base + prune.PRUNE_GROWTH_THRESHOLD + 1000) {
        ok = false;
        _ = evalTopLevel(s.api, s.interner, lang, "[1 + 1]", a, &ok);
    }

    // GATED: under a history-retaining policy, auto-prune must NOT fire (it would
    // discard wanted history).
    s.store.retention_policy = .keep_all;
    try testing.expect((try prune.maybePrune(s.store, s.table, a)) == null);

    // Under current-only retention, it fires and reclaims the accumulated churn.
    s.store.retention_policy = .current_only;
    const pst = (try prune.maybePrune(s.store, s.table, a)) orelse return error.AutoPruneDidNotFire;
    try testing.expect(pst.swept_entities > 0);

    // Live state survived the auto-prune.
    ok = false;
    try expectInt(99, evalTopLevel(s.api, s.interner, lang, "keep", a, &ok));
    ok = false;
    try expectInt(7, evalTopLevel(s.api, s.interner, lang, "[3 amk]", a, &ok));
}

test "DEMO 16 deferred: strings — size/at:/,/=/asSymbol + seq protocol" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    const Case = struct { src: []const u8, want: i64 };
    const cases = [_]Case{
        .{ .src = "[ \"hello\" size]", .want = 5 },
        .{ .src = "[ \"hello\" at: 0]", .want = 104 }, // 'h'
        .{ .src = "[ [\"foo\" , \"bar\"] size]", .want = 6 },
        // inject over bytes: 'a'+'b'+'c' = 97+98+99
        .{ .src = "[ \"abc\" inject: 0 into: [:acc :c | [acc + c]]]", .want = 294 },
        // collect: via OBJECT_VT (strings provide inject:into:) → vector of bytes
        .{ .src = "[ [\"abc\" collect: [:c | [c + 1]]] size]", .want = 3 },
    };
    for (cases) |c| {
        const src = try strings.mkString(a, c.src);
        const ast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, src);
        try testing.expectEqual(c.want, ref_mod.untagSmallInt(send_mod.send0(ast, sel_eval)));
    }
    // equality returns booleans
    const eqsrc = try strings.mkString(a, "[ \"x\" = \"x\"]");
    const eqast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, eqsrc);
    try testing.expectEqual(kernel_eids.TRUE_EID, ref_mod.untagEid(send_mod.send0(eqast, sel_eval)));
}

test "DEMO 16 deferred: ranges + seq-unification (collect:/select: across types)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    const Case = struct { src: []const u8, want: i64 };
    const cases = [_]Case{
        .{ .src = "[ [1 to: 5] size]", .want = 5 },
        .{ .src = "[ [1 to: 5] at: 2]", .want = 3 },
        .{ .src = "[ [1 to: 10 by: 2] size]", .want = 5 },
        .{ .src = "[ [1 to: 5] inject: 0 into: [:acc :x | [acc + x]]]", .want = 15 },
        .{ .src = "[ [[1 to: 4] collect: [:x | [x * x]]] inject: 0 into: [:a :x | [a + x]]]", .want = 30 },
        .{ .src = "[ [[1 to: 100] select: [:x | [x < 4]]] size]", .want = 3 },
        .{ .src = "[ [1 to: 100] detect: [:x | [x > 50]]]", .want = 51 },
        .{ .src = "[ {a: 10 b: 20 c: 30} inject: 0 into: [:acc :v | [acc + v]]]", .want = 60 },
        .{ .src = "[ [{a: 1 b: 2} collect: [:v | [v * 100]]] inject: 0 into: [:a :v | [a + v]]]", .want = 300 },
    };
    for (cases) |c| {
        const src = try strings.mkString(a, c.src);
        const ast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, src);
        const val = send_mod.send0(ast, sel_eval);
        try testing.expect(ref_mod.isSmallInt(val));
        try testing.expectEqual(c.want, ref_mod.untagSmallInt(val));
    }
}

test "DEMO 16 Tier D: doesNotUnderstand: — unknown selector → ERROR_VT, known unaffected" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    // Unknown selector → an ERROR_VT diagnostic (not a crash, not NIL).
    {
        const src = try strings.mkString(a, "[5 frobnicate]");
        const ast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, src);
        const val = send_mod.send0(ast, sel_eval);
        try testing.expect(ref_mod.isEid(val));
        try testing.expectEqual(kernel_eids.ERROR_VT, ref_mod.untagEid(s.store.currentValue(ref_mod.untagEid(val), kernel_eids.ATTR_VTABLE).?));
    }
    // Known selector still dispatches normally.
    {
        const src = try strings.mkString(a, "[5 squared]");
        const ast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, src);
        try testing.expectEqual(@as(i64, 25), ref_mod.untagSmallInt(send_mod.send0(ast, sel_eval)));
    }
}

test "DEMO 16 Tier D: bool/nil ergonomics — isNil/ifNil:/ifNotNil:/not/and:/or:" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    const IntCase = struct { src: []const u8, want: i64 };
    const int_cases = [_]IntCase{
        .{ .src = "[nil ifNil: [| 7]]", .want = 7 },
        .{ .src = "[5 ifNil: [| 7]]", .want = 5 },
        .{ .src = "[5 ifNotNil: [:v | [v + 1]]]", .want = 6 },
    };
    for (int_cases) |c| {
        const src = try strings.mkString(a, c.src);
        const ast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, src);
        try testing.expectEqual(c.want, ref_mod.untagSmallInt(send_mod.send0(ast, sel_eval)));
    }
    const BoolCase = struct { src: []const u8, want: u64 };
    const bool_cases = [_]BoolCase{
        .{ .src = "[nil isNil]", .want = kernel_eids.TRUE_EID },
        .{ .src = "[5 isNil]", .want = kernel_eids.FALSE_EID },
        .{ .src = "[#[1 2] isNil]", .want = kernel_eids.FALSE_EID }, // delegation to OBJECT_VT
        .{ .src = "[true not]", .want = kernel_eids.FALSE_EID },
        .{ .src = "[false and: [| true]]", .want = kernel_eids.FALSE_EID }, // short-circuit
        .{ .src = "[true and: [| true]]", .want = kernel_eids.TRUE_EID },
        .{ .src = "[false or: [| true]]", .want = kernel_eids.TRUE_EID },
    };
    for (bool_cases) |c| {
        const src = try strings.mkString(a, c.src);
        const ast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, src);
        try testing.expectEqual(c.want, ref_mod.untagEid(send_mod.send0(ast, sel_eval)));
    }
    {
        const src = try strings.mkString(a, "[nil ifNotNil: [:v | [v + 1]]]");
        const ast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, src);
        try testing.expectEqual(ref_mod.NIL, send_mod.send0(ast, sel_eval));
    }
}

test "DEMO 16 Tier C: maps — {k: v} literal, at:, at:ifAbsent:, includesKey:, merge:, keys/values" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    const Case = struct { src: []const u8, want: i64 };
    const cases = [_]Case{
        .{ .src = "[ {a: 1 b: 2} at: #a]", .want = 1 },
        .{ .src = "[ {a: 1 b: 2} at: #b]", .want = 2 },
        .{ .src = "[ {a: 1} at: #z ifAbsent: [| 99]]", .want = 99 },
        .{ .src = "[ {a: 1 b: 2 c: 3} size]", .want = 3 },
        .{ .src = "[ [{a: 1} merge: {b: 2}] size]", .want = 2 },
        .{ .src = "[ [{x: 10 y: 20} values] inject: 0 into: [:s :v | [s + v]]]", .want = 30 },
        // at:put: returns a new map; original unchanged (immutability)
        .{ .src = "[ [{a: 1} at: #b put: 2] at: #b]", .want = 2 },
        // |> pipe on a map
        .{ .src = "{a: 5} |> at: #a", .want = 5 },
    };
    for (cases) |c| {
        const src = try strings.mkString(a, c.src);
        const ast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, src);
        const val = send_mod.send0(ast, sel_eval);
        try testing.expect(ref_mod.isSmallInt(val));
        try testing.expectEqual(c.want, ref_mod.untagSmallInt(val));
    }
    // includesKey: returns a boolean
    {
        const src = try strings.mkString(a, "[ {a: 1} includesKey: #a]");
        const ast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, src);
        const val = send_mod.send0(ast, sel_eval);
        try testing.expectEqual(kernel_eids.TRUE_EID, ref_mod.untagEid(val));
    }
}

test "DEMO 16 Tier B: self-hosted collect/select/count/concat + |> pipe" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    const Case = struct { src: []const u8, want: i64 };
    const cases = [_]Case{
        // collect squares then sum: #[1 4 9] inject 0 → 14
        .{ .src = "[ [#[1 2 3] collect: [:x | [x * x]]] inject: 0 into: [:a :x | [a + x]]]", .want = 14 },
        // select evens-ish (>2): #[3 4] size = 2
        .{ .src = "[ [#[1 2 3 4] select: [:x | [x > 2]]] size]", .want = 2 },
        .{ .src = "[ #[1 2 3] count]", .want = 3 },
        .{ .src = "[ [#[1 2] , #[3 4 5]] size]", .want = 5 },
        // |> pipe: left-assoc chain, then size (bare — pipe is an expression)
        .{ .src = "#[1 2 3] |> collect: [:x | [x * x]] |> select: [:x | [x > 3]] |> size", .want = 2 },
        // |> unary
        .{ .src = "#[5 6 7] |> size", .want = 3 },
    };
    for (cases) |c| {
        const src = try strings.mkString(a, c.src);
        const ast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, src);
        const val = send_mod.send0(ast, sel_eval);
        try testing.expect(ref_mod.isSmallInt(val));
        try testing.expectEqual(c.want, ref_mod.untagSmallInt(val));
    }
}

test "DEMO 16: vectors — #[..] literal, size, at:, with:, inject:into:" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    const Case = struct { src: []const u8, want: i64 };
    const cases = [_]Case{
        .{ .src = "[ #[1 2 3] size]", .want = 3 },
        .{ .src = "[ #[] size]", .want = 0 },
        .{ .src = "[ #[10 20 30] at: 1]", .want = 20 },
        .{ .src = "[ #[10 20 30] at: 9 ifAbsent: [| 99]]", .want = 99 },
        .{ .src = "[ [#[1 2] with: 3] size]", .want = 3 },
        .{ .src = "[ #[1 2 3 4] inject: 0 into: [:acc :x | [acc + x]]]", .want = 10 },
    };
    for (cases) |c| {
        const src = try strings.mkString(a, c.src);
        const ast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, src);
        const val = send_mod.send0(ast, sel_eval);
        try testing.expect(ref_mod.isSmallInt(val));
        try testing.expectEqual(c.want, ref_mod.untagSmallInt(val));
    }
}

test "DEMO 15.R: db surface — [[db now] basisT] is a SmallInt, [db retainAll] is self" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    // [db now] yields a SNAPSHOT_VT object; basisT reads its basis-T.
    {
        const src = try strings.mkString(a, "[[db now] basisT]");
        const ast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, src);
        const val = send_mod.send0(ast, sel_eval);
        try testing.expect(ref_mod.isSmallInt(val));
        try testing.expect(ref_mod.untagSmallInt(val) >= 1);
    }
    // [db retainAll] returns the db prototype itself (self).
    {
        const src = try strings.mkString(a, "[db retainAll]");
        const ast = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse, src);
        const val = send_mod.send0(ast, sel_eval);
        try testing.expectEqual(kernel_eids.DB_PROTO, ref_mod.untagEid(val));
    }
}

test "DEMO: parse + eval [3 succ] → 4" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;
    const src_ref = try strings.mkString(a, "[3 succ]");
    const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
    const val = send_mod.send0(ast_ref, sel_eval);
    try testing.expectEqual(@as(i64, 4), ref_mod.untagSmallInt(val));
}

test "DEMO: parse + eval [[3 succ] squared] → 16" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;
    const src_ref = try strings.mkString(a, "[[3 succ] squared]");
    const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
    const val = send_mod.send0(ast_ref, sel_eval);
    try testing.expectEqual(@as(i64, 16), ref_mod.untagSmallInt(val));
}

test "DEMO: parse + eval bare integer literal → 42" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;
    const src_ref = try strings.mkString(a, "42");
    const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
    const val = send_mod.send0(ast_ref, sel_eval);
    try testing.expectEqual(@as(i64, 42), ref_mod.untagSmallInt(val));
}

test "DEMO 10.4b: block params — [3 doubled] via method that takes param" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    // Install a 1-param block as method `+`: [SmallInt method:#plusOne do:[:x| [x succ]]]
    // Then [3 plusOne: 0] would need keyword send dispatch — but plusOne: is
    // installed. Just test [3 plusOne: 0] returns 4 via x = 0... wait, the
    // block ignores x and does [x succ]. So [3 plusOne: 7] returns 8.
    {
        const src = "[[3 vtable] method: #plusOne: do: [:x | [x succ]]]";
        const src_ref = try strings.mkString(a, src);
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        try testing.expect(ref_mod.isEid(ast_ref));
        _ = send_mod.send0(ast_ref, sel_eval);
    }

    {
        const src = "[3 plusOne: 7]";
        const src_ref = try strings.mkString(a, src);
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(@as(i64, 8), ref_mod.untagSmallInt(val));
    }
}

test "DEMO 10.4c: 2-param block via method install + invoke" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    // Method `pick:or:` on SmallInt; body returns second param.
    // Validates 2-param binding (a bound but unused, b returned).
    {
        const src = "[[3 vtable] method: #pick:or: do: [:a :b | b]]";
        const src_ref = try strings.mkString(a, src);
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        try testing.expect(ref_mod.isEid(ast_ref));
        _ = send_mod.send0(ast_ref, sel_eval);
    }

    {
        const src = "[3 pick: 10 or: 20]";
        const src_ref = try strings.mkString(a, src);
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(@as(i64, 20), ref_mod.untagSmallInt(val));
    }
}

test "DEMO 10.4: install method via [vt method: #sel do: [| body]], then dispatch" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    // Install `neg` on SmallInt's vt: [[3 vtable] method: #neg do: [| [self negate]]]
    {
        const src_ref = try strings.mkString(a, "[[3 vtable] method: #neg do: [| [self negate]]]");
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        try testing.expect(ref_mod.isEid(ast_ref));
        const result = send_mod.send0(ast_ref, sel_eval);
        // result is the vt-Ref (since [vt method:do:] returns receiver)
        try testing.expect(ref_mod.isEid(result));
    }

    // Now `[3 neg]` should dispatch the block and return -3.
    {
        const src_ref = try strings.mkString(a, "[3 neg]");
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expect(ref_mod.isSmallInt(val));
        try testing.expectEqual(@as(i64, -3), ref_mod.untagSmallInt(val));
    }

    // `[5 neg]` reuses the same installed method → -5.
    {
        const src_ref = try strings.mkString(a, "[5 neg]");
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(@as(i64, -5), ref_mod.untagSmallInt(val));
    }
}

test "DEMO 10.x: binary ops [3 + 4] → 7, [10 - 7] → 3, [2 * 5] → 10" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    inline for (.{ .{ "[3 + 4]", 7 }, .{ "[10 - 7]", 3 }, .{ "[2 * 5]", 10 }, .{ "[20 / 4]", 5 } }) |pair| {
        const src_ref = try strings.mkString(a, pair[0]);
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(@as(i64, pair[1]), ref_mod.untagSmallInt(val));
    }
}

test "DEMO 10.x: comparisons return TRUE_EID / FALSE_EID" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    inline for (.{ .{ "[3 < 5]", 72 }, .{ "[3 = 3]", 72 }, .{ "[3 > 5]", 73 } }) |pair| {
        const src_ref = try strings.mkString(a, pair[0]);
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(pair[1], ref_mod.untagEid(val));
    }
}

test "DEMO 10.x: ifTrue:ifFalse: branches correctly" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    {
        const src_ref = try strings.mkString(a, "[true ifTrue: [| 100] ifFalse: [| 200]]");
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(@as(i64, 100), ref_mod.untagSmallInt(val));
    }
    {
        const src_ref = try strings.mkString(a, "[false ifTrue: [| 100] ifFalse: [| 200]]");
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(@as(i64, 200), ref_mod.untagSmallInt(val));
    }
    {
        const src_ref = try strings.mkString(a, "[[3 < 5] ifTrue: [| 999] ifFalse: [| 0]]");
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(@as(i64, 999), ref_mod.untagSmallInt(val));
    }
}

test "DEMO 10.4.5 S3: let binding + REPL-style persistence + closure capture" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    // Top-level scope EID is module-static in the lang dylib; the test must
    // resolve a scope-Ref by sending parseExpr+eval against the lang module.
    // For persistence across calls, each eval threads through the same
    // top-level scope automatically since send1(ast, eval) with no scope-arg
    // defaults to the singleton.
    inline for (.{
        .{ "let x := 42", 42 },
        .{ "x", 42 },
        .{ "[x squared]", 1764 },
        .{ "let y := [x + 8]", 50 },
        .{ "y", 50 },
    }) |pair| {
        const src_ref = try strings.mkString(a, pair[0]);
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(@as(i64, pair[1]), ref_mod.untagSmallInt(val));
    }
}

test "DEMO 10.4.5 S3: closure-over-enclosing-scope — makeAdder" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    inline for (.{
        "let makeAdder := [:a | [:b | [a + b]]]",
        "let add10 := [makeAdder value: 10]",
    }) |line| {
        const src_ref = try strings.mkString(a, line);
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        _ = send_mod.send0(ast_ref, sel_eval);
    }

    // [add10 value: 5] → 15 (a=10 captured, b=5 supplied).
    const src_ref = try strings.mkString(a, "[add10 value: 5]");
    const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
    const val = send_mod.send0(ast_ref, sel_eval);
    try testing.expectEqual(@as(i64, 15), ref_mod.untagSmallInt(val));
}

test "DEMO 10.x: whileTrue: counts to 5" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    inline for (.{
        "let n := 0",
        "[[| [n < 5]] whileTrue: [| n := [n + 1]]]",
    }) |line| {
        const src_ref = try strings.mkString(a, line);
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        _ = send_mod.send0(ast_ref, sel_eval);
    }
    const src_ref = try strings.mkString(a, "n");
    const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
    const val = send_mod.send0(ast_ref, sel_eval);
    try testing.expectEqual(@as(i64, 5), ref_mod.untagSmallInt(val));
}

test "DEMO 10.x: sum-to-10 via whileTrue: + multi-stmt block body" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    inline for (.{
        "let sum := 0",
        "let i := 1",
        "[[| [i < 11]] whileTrue: [| sum := [sum + i]; i := [i + 1]]]",
    }) |line| {
        const src_ref = try strings.mkString(a, line);
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        _ = send_mod.send0(ast_ref, sel_eval);
    }
    const src_ref = try strings.mkString(a, "sum");
    const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
    const val = send_mod.send0(ast_ref, sel_eval);
    try testing.expectEqual(@as(i64, 55), ref_mod.untagSmallInt(val));
}

test "DEMO 10.x: 1 to: 10 do: sums to 55 via to:do:" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    inline for (.{
        "let sum := 0",
        "[1 to: 10 do: [:i | sum := [sum + i]]]",
    }) |line| {
        const src_ref = try strings.mkString(a, line);
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        _ = send_mod.send0(ast_ref, sel_eval);
    }
    const src_ref = try strings.mkString(a, "sum");
    const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
    const val = send_mod.send0(ast_ref, sel_eval);
    try testing.expectEqual(@as(i64, 55), ref_mod.untagSmallInt(val));
}

test "DEMO 10.x: 3 timesRepeat: adds 7 three times" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;
    inline for (.{
        "let n := 0",
        "[3 timesRepeat: [| n := [n + 7]]]",
    }) |line| {
        const src_ref = try strings.mkString(a, line);
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        _ = send_mod.send0(ast_ref, sel_eval);
    }
    const src_ref = try strings.mkString(a, "n");
    const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
    const val = send_mod.send0(ast_ref, sel_eval);
    try testing.expectEqual(@as(i64, 21), ref_mod.untagSmallInt(val));
}

test "DEMO 10.x: line + block comments skipped" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;
    // line comment after expression
    {
        const src_ref = try strings.mkString(a, "42 // the answer");
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(@as(i64, 42), ref_mod.untagSmallInt(val));
    }
    // block comment in the middle
    {
        const src_ref = try strings.mkString(a, "[3 /* doubled */ + 4]");
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(@as(i64, 7), ref_mod.untagSmallInt(val));
    }
}

test "DEMO 10.x: float literal parses + prints" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    const src_ref = try strings.mkString(a, "3.14");
    const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
    const val = send_mod.send0(ast_ref, sel_eval);
    // val is a pointer Ref (tag 0) — not SmallInt, not EID.
    try testing.expect((val & 0b111) == 0);
    try testing.expect(val != 0);
}

test "DEMO 10.x: ^ short-circuits stmt-list; method returns 99" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    // Install: [SmallInt method:#early do:[| ^99; 100]]
    inline for (.{
        "[[3 vtable] method: #early do: [| ^99; 100]]",
    }) |line| {
        const src_ref = try strings.mkString(a, line);
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        _ = send_mod.send0(ast_ref, sel_eval);
    }
    const src_ref = try strings.mkString(a, "[7 early]");
    const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
    const val = send_mod.send0(ast_ref, sel_eval);
    try testing.expectEqual(@as(i64, 99), ref_mod.untagSmallInt(val));
}

test "DEMO 10.x: ^ escapes through inner value-frame to method frame" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    inline for (.{
        "[[3 vtable] method: #viablock do: [| [[| ^88] value]]]",
    }) |line| {
        const src_ref = try strings.mkString(a, line);
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        _ = send_mod.send0(ast_ref, sel_eval);
    }
    const src_ref = try strings.mkString(a, "[5 viablock]");
    const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
    const val = send_mod.send0(ast_ref, sel_eval);
    try testing.expectEqual(@as(i64, 88), ref_mod.untagSmallInt(val));
}

test "DEMO 10.x: float arithmetic + comparisons" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;

    // [3.14 asInt] returns SmallInt 3
    {
        const src_ref = try strings.mkString(a, "[3.14 asInt]");
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(val));
    }
    // [[1.5 exp: 10] asInt] returns SmallInt 15000000000
    {
        const src_ref = try strings.mkString(a, "[[1.5 exp: 10] asInt]");
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(@as(i64, 15_000_000_000), ref_mod.untagSmallInt(val));
    }
    // [3.0 < 5.0] returns TRUE_EID
    {
        const src_ref = try strings.mkString(a, "[3.0 < 5.0]");
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(@as(u64, 72), ref_mod.untagEid(val));
    }
    // SmallInt + Float coerces: [3 asFloat] + 0.14 = 3.14 (then asInt = 3)
    {
        const src_ref = try strings.mkString(a, "[[[3 asFloat] + 0.14] asInt]");
        const ast_ref = send_mod.send1(ref_mod.mkEid(lang_mod_eid), sel_parse_expr, src_ref);
        const val = send_mod.send0(ast_ref, sel_eval);
        try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(val));
    }
}

test "lang grammar: start fails on \"!!\" (no atom matches)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const result = parseVia(s, "!!", "start", lang_mod_eid, a);
    const err = ref_mod.untagEid(result);
    const vt = s.store.currentValue(err, kernel_eids.ATTR_VTABLE).?;
    try testing.expectEqual(kernel_eids.ERROR_VT, ref_mod.untagEid(vt));
}

fn evalLine(s: anytype, lang_eid: u64, src: []const u8, a: std.mem.Allocator) Ref {
    const sel_parse_expr = s.interner.str_to_eid.get("parseExpr:").?;
    const sel_eval = s.interner.str_to_eid.get("eval").?;
    const src_ref = strings.mkString(a, src) catch unreachable;
    const ast_ref = send_mod.send1(ref_mod.mkEid(lang_eid), sel_parse_expr, src_ref);
    return send_mod.send1(ast_ref, sel_eval, ref_mod.mkEid(kernel_eids.TOP_SCOPE_EID));
}

// Result error model — Stage 1: ERROR_VT propagation + inspectors.
fn isErrTrue(v: Ref) bool {
    return ref_mod.isEid(v) and (v >> 3) == kernel_eids.TRUE_EID;
}
test "errors: propagation through sends + inspectors" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // A primitive failure produces an ERROR_VT value.
    const err = evalLine(s, lang, "[#[1 2 3] at: 99]", a);
    try testing.expect(ref_mod.isEid(err));
    try testing.expectEqual(@as(u64, kernel_eids.ERROR_VT), s.store.currentValue(err >> 3, kernel_eids.ATTR_VTABLE).? >> 3);

    // isError discriminates errors from ordinary values.
    try testing.expect(isErrTrue(evalLine(s, lang, "[[#[1 2 3] at: 99] isError]", a)));
    try testing.expect(!isErrTrue(evalLine(s, lang, "[5 isError]", a)));
    try testing.expect(!isErrTrue(evalLine(s, lang, "[#[1 2 3] isError]", a)));

    // [err message] returns a (non-nil) string.
    const msg = evalLine(s, lang, "[[#[1 2 3] at: 99] message]", a);
    try testing.expect(msg != ref_mod.NIL);

    // Propagation: sending ANY unhandled message to an error returns the error
    // itself (still isError), not a new does-not-understand error.
    try testing.expect(isErrTrue(evalLine(s, lang, "[[[#[1 2 3] at: 99] frobnicate] isError]", a)));
    // Chained sends propagate the original error.
    try testing.expect(isErrTrue(evalLine(s, lang, "[[[[#[1 2 3] at: 99] foo] bar] isError]", a)));
}

// Result error model — Stage 2: statement sequences halt on error.
test "errors: statement sequence halts on error" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // A multi-statement block whose first statement errors yields the error,
    // not the value of the later statement (the later statement never runs).
    const r = evalLine(s, lang, "[[| [#[1 2 3] at: 99]; 42] value]", a);
    try testing.expect(isErrTrue(evalLine(s, lang, "[[[| [#[1 2 3] at: 99]; 42] value] isError]", a)));
    try testing.expect(ref_mod.isEid(r));
    // Sanity: a non-erroring sequence still returns its last value.
    try testing.expectEqual(@as(i64, 42), ref_mod.untagSmallInt(evalLine(s, lang, "[[| 1; 42] value]", a)));
}

// Phase 17.7 finish — a VM-COMPILED method body (installed via method:do:, run
// through the bytecode VM per 11.12) now halts mid-sequence on an error, matching
// the tree-walker. Before BRANCH_IF_ERROR the method ran the second statement and
// returned 42.
test "errors: VM-compiled method body halts mid-sequence on error" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // [3 halts] body: `let z := [10 / 0]; 42` — stmt 1 is a zeroDivide error.
    _ = evalLine(s, lang, "[[3 vtable] method: #halts do: [| let z := [10 / 0]; 42]]", a);
    const r = evalLine(s, lang, "[3 halts]", a);
    try testing.expect(evalIsError(s, lang, "[3 halts]", a)); // halted → error, not 42
    try testing.expect(ref_mod.isEid(r));
    // The propagated error is the zeroDivide (read the kind fact directly).
    const kind = s.store.currentValue(r >> 3, kernel_eids.ATTR_ERROR_KIND);
    try testing.expectEqual(ref_mod.mkEid(s.interner.str_to_eid.get("zeroDivide").?), kind.?);

    // A non-erroring compiled method body still returns its last statement.
    _ = evalLine(s, lang, "[[3 vtable] method: #fine do: [| let z := 1; 42]]", a);
    try testing.expectEqual(@as(i64, 42), ref_mod.untagSmallInt(evalLine(s, lang, "[3 fine]", a)));
}

// True if evaluating `src` yields an ERROR_VT value (checks the vtable directly,
// so it works for bare statements like `x := 5` that can't be wrapped in `[]`).
fn evalIsError(s: anytype, lang: u64, src: []const u8, a: std.mem.Allocator) bool {
    const v = evalLine(s, lang, src, a);
    if (!ref_mod.isEid(v)) return false;
    const vt = s.store.currentValue(v >> 3, kernel_eids.ATTR_VTABLE) orelse return false;
    return vt == ref_mod.mkEid(kernel_eids.ERROR_VT);
}

// Result error model — Stage 3: silent-nil failure sites now report errors.
test "errors: undefined var / undefined assign / divide by zero report" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // Undefined variable read → error (not silent nil).
    try testing.expect(evalIsError(s, lang, "nonesuch", a));
    // A bound var still resolves.
    _ = evalLine(s, lang, "let defined := 7", a);
    try testing.expectEqual(@as(i64, 7), ref_mod.untagSmallInt(evalLine(s, lang, "defined", a)));

    // Assignment to an undeclared variable → error.
    try testing.expect(evalIsError(s, lang, "neverDeclared := 5", a));
    // Assignment to a declared variable works.
    try testing.expectEqual(@as(i64, 9), ref_mod.untagSmallInt(evalLine(s, lang, "defined := 9", a)));

    // Divide by zero → error (SmallInt and Float).
    try testing.expect(evalIsError(s, lang, "[10 / 0]", a));
    try testing.expect(evalIsError(s, lang, "[10.0 / 0.0]", a));
    // Normal division still works.
    try testing.expectEqual(@as(i64, 5), ref_mod.untagSmallInt(evalLine(s, lang, "[10 / 2]", a)));
}

// Result error model — Phase 17.7 type-mismatch + overflow hardening.
test "errors: arithmetic type-mismatch + integer overflow are kinded errors (17.7)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const TRUE = ref_mod.mkEid(kernel_eids.TRUE_EID);
    const FALSE = ref_mod.mkEid(kernel_eids.FALSE_EID);

    // Arithmetic on a non-number → #typeMismatch (was: silent bit-reinterpret).
    try testing.expect(evalIsError(s, lang, "[1 + #foo]", a));
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(
        evalLine(s, lang, "[[| [1 + #foo]] on: #typeMismatch do: [:e | 1]]", a)));
    try testing.expect(evalIsError(s, lang, "[2.5 * nil]", a)); // float side too
    try testing.expect(evalIsError(s, lang, "[1 - \"x\"]", a));

    // Integer overflow → #overflow (was: safe-build panic / silent high-bit drop).
    try testing.expect(evalIsError(s, lang, "[2000000000 * 2000000000]", a));
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(
        evalLine(s, lang, "[[| [2000000000 * 2000000000]] on: #overflow do: [:e | 1]]", a)));

    // Mixed int/float arithmetic promotes (no garble): 1 + 2.5 = 3.5.
    try testing.expectEqual(TRUE, evalLine(s, lang, "[[1 + 2.5] = 3.5]", a));
    try testing.expectEqual(TRUE, evalLine(s, lang, "[1 < 2.5]", a));

    // `=` with a non-number is false (incomparable), NOT an error; ordering errors.
    try testing.expectEqual(FALSE, evalLine(s, lang, "[1 = #foo]", a));
    try testing.expect(evalIsError(s, lang, "[1 < #foo]", a));

    // mod by zero is now a kinded error (was: silent nil); normal mod still works.
    try testing.expect(evalIsError(s, lang, "[7 mod: 0]", a));
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(evalLine(s, lang, "[7 mod: 3]", a)));

    // Plain arithmetic in range is unaffected.
    try testing.expectEqual(@as(i64, 5), ref_mod.untagSmallInt(evalLine(s, lang, "[2 + 3]", a)));
    try testing.expectEqual(@as(i64, 12), ref_mod.untagSmallInt(evalLine(s, lang, "[3 * 4]", a)));
    try testing.expectEqual(@as(i64, -7), ref_mod.untagSmallInt(evalLine(s, lang, "[0 - 7]", a)));
}

// Phase 16 deferred nicety — block-param destructuring `[:#[a b]| …]`.
test "block-param destructuring binds vector elements" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // Single destructured param: elements bind to a, b.
    try testing.expectEqual(@as(i64, 30), ref_mod.untagSmallInt(
        evalLine(s, lang, "[[:#[a b]| [a + b]] value: #[10 20]]", a)));
    // Three-element pattern.
    try testing.expectEqual(@as(i64, 6), ref_mod.untagSmallInt(
        evalLine(s, lang, "[[:#[x y z]| [[x + y] + z]] value: #[1 2 3]]", a)));
    // Destructured param + a normal param mixed (arity 2 via value:value:).
    try testing.expectEqual(@as(i64, 35), ref_mod.untagSmallInt(
        evalLine(s, lang, "[[:#[a b] :c | [[a + b] + c]] value: #[10 20] value: 5]", a)));
    // A normal single-param block is unaffected (regression guard).
    try testing.expectEqual(@as(i64, 42), ref_mod.untagSmallInt(
        evalLine(s, lang, "[[:n| [n * 2]] value: 21]", a)));
}

// Result error model — Stage 5: on:do: catch combinator.
test "errors: on:do: catches by kind, wildcard, passes through" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // The protected computation is a zero-arg block `[| ... ]` (deferred), not
    // an eager `[10 / 0]` send.
    // Catch by exact kind → handler result.
    try testing.expectEqual(@as(i64, 99), ref_mod.untagSmallInt(
        evalLine(s, lang, "[[| [10 / 0]] on: #zeroDivide do: [:e | 99]]", a)));
    // Catch by #Error wildcard.
    try testing.expectEqual(@as(i64, 42), ref_mod.untagSmallInt(
        evalLine(s, lang, "[[| [10 / 0]] on: #Error do: [:e | 42]]", a)));
    // No error → block result passes through, handler not run.
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(
        evalLine(s, lang, "[[| [10 / 5]] on: #zeroDivide do: [:e | 99]]", a)));
    // Unmatched kind → error propagates (not caught).
    try testing.expect(evalIsError(s, lang, "[[| [10 / 0]] on: #someOtherKind do: [:e | 99]]", a));
    // Handler receives the error: read its message back (non-nil string).
    try testing.expect(ref_mod.NIL != evalLine(s, lang, "[[| [10 / 0]] on: #zeroDivide do: [:e | [e message]]]", a));
}

// Result error model — ensure: cleanup + user-raised errors.
test "errors: ensure: runs cleanup, [msg error] raises + catches" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // ensure: runs the cleanup block (side effect on a top-level var) and returns
    // the protected block's value.
    _ = evalLine(s, lang, "let cleaned := 0", a);
    try testing.expectEqual(@as(i64, 7), ref_mod.untagSmallInt(
        evalLine(s, lang, "[[| 7] ensure: [| cleaned := 1]]", a)));
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(evalLine(s, lang, "cleaned", a)));

    // ensure: cleanup runs even when the protected block errors; the error
    // still propagates out.
    _ = evalLine(s, lang, "let c2 := 0", a);
    try testing.expect(evalIsError(s, lang, "[[| [10 / 0]] ensure: [| c2 := 1]]", a));
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(evalLine(s, lang, "c2", a)));

    // User-raised error: [msg error] is an error value, catchable by #Error.
    try testing.expect(evalIsError(s, lang, "[\"boom\" error]", a));
    try testing.expectEqual(@as(i64, 5), ref_mod.untagSmallInt(
        evalLine(s, lang, "[[| [\"boom\" error]] on: #Error do: [:e | 5]]", a)));
    // Raised with an explicit kind, caught by that kind.
    try testing.expectEqual(@as(i64, 8), ref_mod.untagSmallInt(
        evalLine(s, lang, "[[| [\"nope\" error: #myKind]] on: #myKind do: [:e | 8]]", a)));
}

fn refIsTrue(v: Ref) bool {
    return ref_mod.isEid(v) and (v >> 3) == kernel_eids.TRUE_EID;
}

// Phase 17.8 I/O — File read/write/append/exists/delete round-trip.
test "io: File read/write/append/exists/delete" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const path = "test_io_tmp.txt";
    defer std.fs.cwd().deleteFile(path) catch {};

    // write → success (nil), read back → exact contents.
    try testing.expect(ref_mod.NIL == evalLine(s, lang, "[File write: \"hello\" to: \"test_io_tmp.txt\"]", a));
    try testing.expect(refIsTrue(evalLine(s, lang, "[[File read: \"test_io_tmp.txt\"] = \"hello\"]", a)));
    try testing.expect(refIsTrue(evalLine(s, lang, "[File exists: \"test_io_tmp.txt\"]", a)));

    // append → contents grow.
    try testing.expect(ref_mod.NIL == evalLine(s, lang, "[File append: \" world\" to: \"test_io_tmp.txt\"]", a));
    try testing.expect(refIsTrue(evalLine(s, lang, "[[File read: \"test_io_tmp.txt\"] = \"hello world\"]", a)));

    // delete → gone.
    try testing.expect(ref_mod.NIL == evalLine(s, lang, "[File delete: \"test_io_tmp.txt\"]", a));
    try testing.expect(!refIsTrue(evalLine(s, lang, "[File exists: \"test_io_tmp.txt\"]", a)));

    // read of a missing file → kinded error (fileNotFound), catchable.
    try testing.expect(evalIsError(s, lang, "[File read: \"does_not_exist_xyz.txt\"]", a));
    try testing.expectEqual(@as(i64, 7), ref_mod.untagSmallInt(
        evalLine(s, lang, "[[| [File read: \"does_not_exist_xyz.txt\"]] on: #fileNotFound do: [:e | 7]]", a)));

    // System print:/println:/error: are clean fd writes; just confirm they run
    // and return nil (stdout capture isn't available in-process here).
    try testing.expect(ref_mod.NIL == evalLine(s, lang, "[System print: \"\"]", a));
    try testing.expect(ref_mod.NIL == evalLine(s, lang, "[System println: \"io-smoke\"]", a));

    // System arguments → a vector. In this harness no args are registered, so
    // it's empty; real argv passing (`birnam … -- a b`) is shell-smoke tested.
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(evalLine(s, lang, "[[System arguments] size]", a)));
}

// Phase 12 — automated regression for the REPL-binding round-trip that the
// shell `:save`/`:load` smoke proved manually. Two independent boots (fresh
// store/table/interner each) with peg+lang loaded; a binding defined in the
// first survives image save → reload into the second.
test "image: REPL binding survives save -> fresh boot -> load (lang-driven)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    // ---- Session A: define `let x := 41`, snapshot. ----
    const sa = try fullSetup(&arena);
    _ = (try sa.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_a = (try sa.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    try testing.expectEqual(@as(i64, 41), ref_mod.untagSmallInt(evalLine(sa, lang_a, "let x := 41", a)));
    try testing.expectEqual(@as(i64, 41), ref_mod.untagSmallInt(evalLine(sa, lang_a, "x", a))); // same-session sanity

    const res = try image.write(a, sa.store, sa.table.next_eid.load(.monotonic), .{}, sa.interner);

    // ---- Session B: fresh boot + modules, load image, re-resolve `x`. ----
    const sb = try fullSetup(&arena);
    _ = (try sb.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_b = (try sb.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const loaded = try image.load(sb.store, res.bytes, sb.interner);
    if (loaded.next_eid > sb.table.next_eid.load(.monotonic)) sb.table.next_eid.store(loaded.next_eid, .monotonic);
    _ = try image.rematerializeClosures(a, sb.store, sb.table);

    // The binding (M_USER) re-attaches to the stable top-level scope and the
    // interned name `x` keeps its eid → `x` resolves to 41 in the new session.
    try testing.expectEqual(@as(i64, 41), ref_mod.untagSmallInt(evalLine(sb, lang_b, "x", a)));
}

// W1 Stage 2 (docs/19c) — a module's captureless block-closure GLOBALS survive a
// FULL (v7) image save → fresh boot → load WITHOUT reloading that module. json
// defines `jStr`/`jChild`/`jLower`/… as top-level `let` blocks at file-in (via
// MAKE_BLOCK → CLOSURE_VT wrapper); the wrapper's runtime object is not facts, so
// pre-fix `Json parse:`/`stringify:` returned nil after load. `writeFull` emits a
// `[wrapper :closure-block-method bm]` recipe and `rematerializeClosures` rebuilds
// the captureless wrapper. Session B loads peg+lang only — json comes from the
// image alone (the exact W1 boot-image scenario).
// W1 Stage 3b (docs/19c) — relink-by-name. A native FFI module (net) booted in
// IMAGE-FIRST order (replay the image, THEN load the dylib) rebinds its native
// methods onto the image-restored eids instead of minting fresh ones. net now
// anchors every entity to an interned name and installs methods via
// `bind_native_method`, so: (1) its vtable eid is identical across sessions, and
// (2) after relink the method binding points at a LIVE closure (this process's
// fn-ptr). Proves a Zig FFI module joins a full-image boot in any order.
test "image (W1 Stage 3b): net relinks native methods by name (image-first boot)" {
    if (@import("builtin").os.tag != .macos) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    // ---- Session A: load peg+lang+net, sanity, FULL snapshot. ----
    const sa = try fullSetup(&arena);
    _ = (try sa.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_a = (try sa.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    _ = (try sa.manager.load("./zig-out/lib/libbirnam_net.dylib")) orelse return error.NetLoadFailed;

    const net_vt_a = sa.interner.str_to_eid.get("net:Net").?;
    const listen_sel = sa.interner.str_to_eid.get("listen:").?;
    const a_srv = evalLine(sa, lang_a, "[Net listen: 0]", a); // ephemeral local bind
    try testing.expect(ref_mod.isEid(a_srv)); // sanity: net works in session A

    const res = try image.writeFull(a, sa.store, sa.table.next_eid.load(.monotonic), .{}, sa.interner, sa.table);

    // ---- Session B: peg+lang, replay image FIRST, THEN load net (relink). ----
    const sb = try fullSetup(&arena);
    _ = (try sb.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_b = (try sb.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const loaded = try image.load(sb.store, res.bytes, sb.interner);
    if (loaded.next_eid > sb.table.next_eid.load(.monotonic)) sb.table.next_eid.store(loaded.next_eid, .monotonic);
    _ = try image.rematerializeClosures(a, sb.store, sb.table);

    // The image restored net's facts at their saved eids; the net dylib is NOT
    // loaded yet, so its method closures have no live object-table slot.
    const net_vt_b = sb.interner.str_to_eid.get("net:Net").?;
    try testing.expectEqual(net_vt_a, net_vt_b); // eid stable via interned name
    const pre = sb.store.currentValue(net_vt_b, listen_sel).?;
    try testing.expect(sb.table.get(ref_mod.untagEid(pre)) == null); // closure dead pre-relink

    // Load net AFTER replay — its init resolves intern("net:Net") to the image
    // eid and relinks `listen:` onto the existing binding's closure eid.
    _ = (try sb.manager.load("./zig-out/lib/libbirnam_net.dylib")) orelse return error.NetLoadFailed;
    const post = sb.store.currentValue(net_vt_b, listen_sel).?;
    try testing.expectEqual(pre, post); // SAME binding eid — relinked in place, not re-minted
    try testing.expect(sb.table.get(ref_mod.untagEid(post)) != null); // now LIVE
    try testing.expect(ref_mod.isEid(evalLine(sb, lang_b, "[Net listen: 0]", a))); // and dispatches
}

test "image (W1 Stage 2): json block globals survive full save -> load (no json reload)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    // ---- Session A: load json, sanity parse/stringify, FULL snapshot. ----
    const sa = try fullSetup(&arena);
    _ = (try sa.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_a = (try sa.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    _ = (try sa.manager.load("./zig-out/lib/libbirnam_json.dylib")) orelse return error.JsonLoadFailed;

    // `Json stringify:` exercises the `jStr` block-closure global (no grammar).
    const sa_str = evalLine(sa, lang_a, "[Json stringify: #[1 2 3]]", a);
    try testing.expect(strings.isString(sa_str)); // sanity in session A

    const res = try image.writeFull(a, sa.store, sa.table.next_eid.load(.monotonic), .{}, sa.interner, sa.table);

    // ---- Session B: peg+lang ONLY (no json dylib), load image, rebuild. ----
    const sb = try fullSetup(&arena);
    _ = (try sb.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_b = (try sb.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const loaded = try image.load(sb.store, res.bytes, sb.interner);
    if (loaded.next_eid > sb.table.next_eid.load(.monotonic)) sb.table.next_eid.store(loaded.next_eid, .monotonic);
    _ = try image.rematerializeClosures(a, sb.store, sb.table);

    // json comes entirely from the image; the rebuilt jStr wrapper must run.
    // Pre-fix this returned nil (the wrapper had no object-table entry).
    const sb_str = evalLine(sb, lang_b, "[Json stringify: #[1 2 3]]", a);
    try testing.expect(strings.isString(sb_str));
    try testing.expectEqualStrings("[1,2,3]", strings.bytes(sb_str));
}

// Phase 9.5.6 — the `Module load:` wire from REPL source. Proves the full
// chain: the global `Module` resolves (a binding in the GLOBALS parent scope),
// `load:` dispatches to ffi_module.moduleLoad, and a wrapped FFI fn is callable
// via a keyword send. Doubles as the regression guard for the keyword-send
// arg-arity fix: `[m sqrt: 16.0]` must deliver EXACTLY one arg, or the FFI
// wrap's strict arity check rejects it (NIL). Pre-fix, a recycled-eid stale
// `:next-sibling mkEid(0)` fact made the arg walk overrun to 16.
test "FFI: [Module load:] + wrap + [m sqrt: 16.0] via parse+eval -> 4.0" {
    if (@import("builtin").os.tag != .macos) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // `Module` resolves as a global (GLOBALS scope, parent of TOP_SCOPE).
    const mod_ref = evalLine(s, lang, "let m := [Module load: \"/usr/lib/libSystem.B.dylib\"]", a);
    try testing.expect(ref_mod.isEid(mod_ref));

    // Wrap sqrt — a 3-arg keyword send (selector / name / signature).
    // Declarative: records facts only, no dlsym/install yet.
    _ = evalLine(s, lang, "[m wrap: #sqrt: as: \"sqrt\" signature: \"d)d\"]", a);

    // Commit: dlopen + dlsym + install. evalLine bypasses the REPL handler's
    // auto-resolve, so drive it explicitly (same send the handler does).
    _ = evalLine(s, lang, "[Module resolve]", a);

    // Call — the 1-arg keyword send that the arg-arity fix unblocks.
    const r = evalLine(s, lang, "[m sqrt: 16.0]", a);
    try testing.expect(boxed.isFloat(r));
    try testing.expectApproxEqAbs(@as(f64, 4.0), boxed.untagFloat(r), 1e-9);
}

// Freeze foundation: an FFI wrap survives save → fresh boot → load by
// RELINKING via symbol name. The saving process's :ffi-addr is a raw pointer
// meaningless in the loading process; only the declarative facts (:ffi-path /
// :ffi-wrap-symbol / :ffi-sig-str / :wrap-selector, all M_USER) persist, and
// resolveAllPending re-`dlopen`s + re-`dlsym`s on load. This is exactly what a
// frozen wrapper-dylib's init will do.
test "FFI: wrap survives save -> fresh boot -> load via relink-by-name" {
    if (@import("builtin").os.tag != .macos) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    // ---- Session A: declare + resolve + sanity-call, then snapshot. ----
    const sa = try fullSetup(&arena);
    _ = (try sa.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_a = (try sa.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    _ = evalLine(sa, lang_a, "let m := [Module load: \"/usr/lib/libSystem.B.dylib\"]", a);
    _ = evalLine(sa, lang_a, "[m wrap: #sqrt: as: \"sqrt\" signature: \"d)d\"]", a);
    _ = evalLine(sa, lang_a, "[Module resolve]", a);
    const r_a = evalLine(sa, lang_a, "[m sqrt: 16.0]", a);
    try testing.expect(boxed.isFloat(r_a)); // sanity: works in session A

    const res = try image.write(a, sa.store, sa.table.next_eid.load(.monotonic), .{}, sa.interner);

    // ---- Session B: fresh boot + modules, load image, relink, re-call. ----
    const sb = try fullSetup(&arena);
    _ = (try sb.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_b = (try sb.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const loaded = try image.load(sb.store, res.bytes, sb.interner);
    if (loaded.next_eid > sb.table.next_eid.load(.monotonic)) sb.table.next_eid.store(loaded.next_eid, .monotonic);
    _ = try image.rematerializeClosures(a, sb.store, sb.table);
    // The relink: re-dlopen + re-dlsym by NAME (saved addr is stale here).
    try ffi_module.resolveAllPending(a, sb.store, sb.table);

    const r_b = evalLine(sb, lang_b, "[m sqrt: 81.0]", a);
    try testing.expect(boxed.isFloat(r_b));
    try testing.expectApproxEqAbs(@as(f64, 9.0), boxed.untagFloat(r_b), 1e-9);
}

// file-in (no FFI → portable). `;`-separated forms eval into the top scope.
// Atomic mode stops at the first error (later forms not applied); continue
// mode evaluates every good form. Source spans multiple lines per form.
test "fileIn: language proto evaluates ;-separated forms (atomic + continue)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // -- happy path: all forms apply. --
    const ok_path = "test_filein_ok.birnam";
    try std.fs.cwd().writeFile(.{ .sub_path = ok_path, .data = "let a := 10;\n[a squared];\nlet b := [a + 5]\n" });
    defer std.fs.cwd().deleteFile(ok_path) catch {};
    _ = evalLine(s, lang, "[language fileIn: \"test_filein_ok.birnam\"]", a);
    try testing.expectEqual(@as(i64, 10), ref_mod.untagSmallInt(evalLine(s, lang, "a", a)));
    try testing.expectEqual(@as(i64, 15), ref_mod.untagSmallInt(evalLine(s, lang, "b", a)));

    // -- atomic abort: form 2 is garbage → form 3 never runs (c stays unbound). --
    const bad_path = "test_filein_bad.birnam";
    try std.fs.cwd().writeFile(.{ .sub_path = bad_path, .data = "let g := 1;\n@@nonsense@@;\nlet c := 7\n" });
    defer std.fs.cwd().deleteFile(bad_path) catch {};
    const atomic_res = evalLine(s, lang, "[language fileIn: \"test_filein_bad.birnam\"]", a);
    try testing.expectEqual(ref_mod.NIL, atomic_res); // aborted
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(evalLine(s, lang, "g", a))); // form 1 ran
    // form 3 skipped → c unbound. Result model: reading an unbound name now
    // yields an undefined-variable error (was a silent nil before Stage 3).
    try testing.expect(isErrTrue(evalLine(s, lang, "[c isError]", a)));

    // -- continue mode: form 3 applies despite the form-2 error. --
    _ = evalLine(s, lang, "[language fileIn: \"test_filein_bad.birnam\" onError: #continue]", a);
    try testing.expectEqual(@as(i64, 7), ref_mod.untagSmallInt(evalLine(s, lang, "c", a)));
}

// freeze (1/2): reconstruct canonical source from a live wrapper's facts, then
// re-install it in a FRESH session via fileInSource: — no compiler involved.
test "freeze: reconstructSource round-trips a wrap via fileInSource:" {
    if (@import("builtin").os.tag != .macos) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    // Session A: declare + resolve a sqrt wrap, then reconstruct source.
    const sa = try fullSetup(&arena);
    _ = (try sa.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_a = (try sa.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    _ = evalLine(sa, lang_a, "let lib := [Module load: \"/usr/lib/libSystem.B.dylib\"]", a);
    _ = evalLine(sa, lang_a, "[lib wrap: #sqrt: as: \"sqrt\" signature: \"d)d\"]", a);

    const src = try freeze.reconstructSource(a, sa.store, sa.interner);
    try testing.expect(std.mem.indexOf(u8, src, "Module load:") != null);
    try testing.expect(std.mem.indexOf(u8, src, "#sqrt:") != null);

    // Session B: fresh boot; fileInSource: the reconstructed wrapper, then call.
    const sb = try fullSetup(&arena);
    _ = (try sb.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_b = (try sb.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    const sel = sb.interner.str_to_eid.get("fileInSource:").?;
    _ = send_mod.send1(ref_mod.mkEid(kernel_eids.LANGUAGE_PROTO), sel, try strings.mkString(a, src));

    const r = evalLine(sb, lang_b, "[m0 sqrt: 16.0]", a);
    try testing.expect(boxed.isFloat(r));
    try testing.expectApproxEqAbs(@as(f64, 4.0), boxed.untagFloat(r), 1e-9);
}

// freeze (2/2): the full loop — emit + `zig build-lib` a wrapper dylib, then
// load it into a fresh session; its birnam_module_init re-establishes the wrap.
test "freeze: compiled wrapper dylib re-establishes wraps on load" {
    if (@import("builtin").os.tag != .macos) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const tmp = "test_freeze_tmp";
    std.fs.cwd().makeDir(tmp) catch {};
    defer std.fs.cwd().deleteTree(tmp) catch {};

    // Session A: build a wrap, emit + compile the wrapper dylib.
    const sa = try fullSetup(&arena);
    _ = (try sa.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_a = (try sa.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    _ = evalLine(sa, lang_a, "let lib := [Module load: \"/usr/lib/libSystem.B.dylib\"]", a);
    _ = evalLine(sa, lang_a, "[lib wrap: #sqrt: as: \"sqrt\" signature: \"d)d\"]", a);

    var dir = try std.fs.cwd().openDir(tmp, .{});
    defer dir.close();
    _ = try freeze.writeArtifacts(a, sa.store, sa.interner, dir, "foo");
    const term = freeze.compile(a, tmp, "foo") catch return error.SkipZigTest;
    if (!(term == .Exited and term.Exited == 0)) return error.SkipZigTest; // zig unavailable

    // Session B: fresh boot; load the compiled wrapper dylib. Its init
    // re-files-in the wrap; then the call works against a re-dlopened lib.
    const sb = try fullSetup(&arena);
    _ = (try sb.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_b = (try sb.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;
    _ = (try sb.manager.load(tmp ++ "/libfoo.dylib")) orelse return error.WrapperLoadFailed;

    const r = evalLine(sb, lang_b, "[m0 sqrt: 64.0]", a);
    try testing.expect(boxed.isFloat(r));
    try testing.expectApproxEqAbs(@as(f64, 8.0), boxed.untagFloat(r), 1e-9);
}

// -- Phase 16 Tier E: reference cells -----------------------------------------

test "DEMO 16 Tier E: cell — create / value / set: / history / asOf:" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // Create a cell, read it back.
    _ = evalLine(s, lang, "let c := [db cell: 10]", a);
    try testing.expectEqual(@as(i64, 10), ref_mod.untagSmallInt(evalLine(s, lang, "[c value]", a)));

    // Mutate twice (each set: = a new tx).
    _ = evalLine(s, lang, "[c set: 20]", a);
    _ = evalLine(s, lang, "[c set: 30]", a);
    try testing.expectEqual(@as(i64, 30), ref_mod.untagSmallInt(evalLine(s, lang, "[c value]", a)));

    // History = every value held, oldest→newest: #[10 20 30].
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(evalLine(s, lang, "[[c history] size]", a)));
    try testing.expectEqual(@as(i64, 10), ref_mod.untagSmallInt(evalLine(s, lang, "[[c history] at: 0]", a)));
    try testing.expectEqual(@as(i64, 30), ref_mod.untagSmallInt(evalLine(s, lang, "[[c history] at: 2]", a)));

    // asOf: 0 — before any write — is absent (nil); a high tx sees the latest.
    try testing.expect(ref_mod.isNil(evalLine(s, lang, "[c asOf: 0]", a)));
    try testing.expectEqual(@as(i64, 30), ref_mod.untagSmallInt(evalLine(s, lang, "[c asOf: 100000]", a)));

    // ref:/atom: are synonyms (single-transactor subsumes the split).
    _ = evalLine(s, lang, "let r := [db ref: 7]", a);
    try testing.expectEqual(@as(i64, 7), ref_mod.untagSmallInt(evalLine(s, lang, "[r value]", a)));

    // transact: returns the block value; sets inside are atomic-by-construction.
    const t = evalLine(s, lang, "[db transact: [| [c set: 99] ; [c value]]]", a);
    try testing.expectEqual(@as(i64, 99), ref_mod.untagSmallInt(t));
}

test "DEMO 16 Tier E: metadata — withMeta: / meta on an entity" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    _ = evalLine(s, lang, "let c := [db cell: 1]", a);
    try testing.expect(ref_mod.isNil(evalLine(s, lang, "[c meta]", a))); // none yet
    _ = evalLine(s, lang, "[c withMeta: 42]", a);
    try testing.expectEqual(@as(i64, 42), ref_mod.untagSmallInt(evalLine(s, lang, "[c meta]", a)));
    // Meta is orthogonal to the cell value.
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(evalLine(s, lang, "[c value]", a)));
}

test "DEMO 16 Tier E: applicative — validate: (accumulate) / zip: / whenAll:" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // validate: gathers ALL failures (odd numbers here), not just the first.
    // check answers nil for OK (even), the value for failure (odd).
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(evalLine(s, lang,
        "[[#[1 2 3 4] validate: [:x | [[x even] ifTrue: [| nil] ifFalse: [| x]]]] size]", a)));

    // zip: pairs positions independently; length = min.
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(evalLine(s, lang,
        "[[#[1 2 3] zip: #[10 20 30]] size]", a)));
    try testing.expectEqual(@as(i64, 10), ref_mod.untagSmallInt(evalLine(s, lang,
        "[[[#[1 2 3] zip: #[10 20 30]] at: 0] at: 1]", a)));

    // whenAll: applies the block only when every element is present.
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(evalLine(s, lang,
        "[#[1 2 3] whenAll: [:v | [v size]]]", a)));
    try testing.expect(ref_mod.isNil(evalLine(s, lang,
        "[#[1 nil 3] whenAll: [:v | [v size]]]", a)));
}

test "DEMO 16 Tier E: transducers — map: / filter: / take: / then: / transduce:" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // map transducer, run eagerly in one fold.
    try testing.expectEqual(@as(i64, 10), ref_mod.untagSmallInt(evalLine(s, lang,
        "[[#[1 2 3 4 5] transduce: [Xf map: [:x | [x * 10]]]] at: 0]", a)));
    // filter transducer.
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(evalLine(s, lang,
        "[[#[1 2 3 4 5] transduce: [Xf filter: [:x | [x even]]]] size]", a)));
    // take transducer (stateful counter + reduced early-out).
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(evalLine(s, lang,
        "[[#[1 2 3 4 5] transduce: [Xf take: 2]] size]", a)));
    // then: composes — map *10 then keep > 20 → #[30 40 50].
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(evalLine(s, lang,
        "[[#[1 2 3 4 5] transduce: [[Xf map: [:x | [x * 10]]] then: [Xf filter: [:x | [x > 20]]]]] size]", a)));
}

// -- Phase 17: stdlib — printing, equality/comparison, sort -------------------

fn expectTrue(s: anytype, lang: u64, src: []const u8, a: std.mem.Allocator) !void {
    try testing.expectEqual(kernel_eids.TRUE_EID, ref_mod.untagEid(evalLine(s, lang, src, a)));
}
fn expectFalse(s: anytype, lang: u64, src: []const u8, a: std.mem.Allocator) !void {
    try testing.expectEqual(kernel_eids.FALSE_EID, ref_mod.untagEid(evalLine(s, lang, src, a)));
}

test "DEMO 17.1: printString / displayString / print" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // printString renders any value to a string.
    try expectTrue(s, lang, "[[42 printString] = \"42\"]", a);
    try expectTrue(s, lang, "[[#[1 2 3] printString] = \"#[1 2 3]\"]", a);
    // printString quotes a string (4 chars: "hi"); displayString does not (2).
    try testing.expectEqual(@as(i64, 4), ref_mod.untagSmallInt(evalLine(s, lang, "[[\"hi\" printString] size]", a)));
    try expectTrue(s, lang, "[[\"hi\" displayString] = \"hi\"]", a);
    // asString = displayString.
    try expectTrue(s, lang, "[[99 asString] = \"99\"]", a);
    // print returns self (chainable).
    try testing.expectEqual(@as(i64, 7), ref_mod.untagSmallInt(evalLine(s, lang, "[7 print]", a)));
}

test "DEMO 17.3: equality (default + vector) / string < / sort / sortBy:" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // Default identity `=` on entities (cells).
    _ = evalLine(s, lang, "let c := [db cell: 1]", a);
    try expectTrue(s, lang, "[c = c]", a);
    try expectFalse(s, lang, "[c = [db cell: 1]]", a);

    // Vector structural `=` (incl. nested).
    try expectTrue(s, lang, "[#[1 2 3] = #[1 2 3]]", a);
    try expectFalse(s, lang, "[#[1 2] = #[1 2 3]]", a);
    try expectTrue(s, lang, "[#[#[1] #[2]] = #[#[1] #[2]]]", a);

    // String lexicographic `<`.
    try expectTrue(s, lang, "[\"a\" < \"b\"]", a);
    try expectFalse(s, lang, "[\"b\" < \"a\"]", a);

    // sort ascending via `<`.
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(evalLine(s, lang, "[[#[3 1 2] sort] at: 0]", a)));
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(evalLine(s, lang, "[[#[3 1 2] sort] at: 2]", a)));
    // sortBy: with a key block → descending via negate.
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(evalLine(s, lang, "[[#[3 1 2] sortBy: [:x | [x negate]]] at: 0]", a)));

    // copyFrom:to: substring.
    try expectTrue(s, lang, "[[\"hello\" copyFrom: 1 to: 3] = \"el\"]", a);
}

test "DEMO 17 strings: escapes + split/join/indexOf/trim/case/predicates" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // Lexer escapes: \n \t \" \\ decode to single bytes.
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(evalLine(s, lang, "[\"a\\nb\" size]", a)));
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(evalLine(s, lang, "[\"\\t\" size]", a)));
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(evalLine(s, lang, "[\"a\\\"b\" size]", a))); // a " b
    try testing.expectEqual(@as(i64, 10), ref_mod.untagSmallInt(evalLine(s, lang, "[\"a\\nb\" at: 1]", a))); // newline byte

    // indexOf: (+ ifAbsent: no-NULL).
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(evalLine(s, lang, "[\"hello\" indexOf: \"ll\"]", a)));
    try testing.expectEqual(@as(i64, -1), ref_mod.untagSmallInt(evalLine(s, lang, "[\"hello\" indexOf: \"z\" ifAbsent: [| [1 negate]]]", a)));

    // asInteger / asNumber.
    try testing.expectEqual(@as(i64, 42), ref_mod.untagSmallInt(evalLine(s, lang, "[\"42\" asInteger]", a)));
    try testing.expectEqual(@as(i64, -7), ref_mod.untagSmallInt(evalLine(s, lang, "[\"-7\" asNumber]", a)));
    try testing.expect(ref_mod.isNil(evalLine(s, lang, "[\"xyz\" asInteger]", a))); // unparseable → nil

    // case fold, trim, reverse.
    try expectTrue(s, lang, "[[\"abc\" asUppercase] = \"ABC\"]", a);
    try expectTrue(s, lang, "[[\"ABC\" asLowercase] = \"abc\"]", a);
    try expectTrue(s, lang, "[[\"  hi  \" trim] = \"hi\"]", a);
    try expectTrue(s, lang, "[[\"abc\" reverse] = \"cba\"]", a);

    // split: / join: (native), round-trip.
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(evalLine(s, lang, "[[\"a,b,c\" split: \",\"] size]", a)));
    try expectTrue(s, lang, "[[[\"a,b,c\" split: \",\"] at: 1] = \"b\"]", a);
    try expectTrue(s, lang, "[[#[\"a\" \"b\" \"c\"] join: \"-\"] = \"a-b-c\"]", a);

    // Birnam-composed predicates + ordering + replace + lines/words.
    try expectTrue(s, lang, "[\"hello\" startsWith: \"he\"]", a);
    try expectTrue(s, lang, "[\"hello\" endsWith: \"lo\"]", a);
    try expectTrue(s, lang, "[\"hello\" includes: \"ell\"]", a);
    try expectFalse(s, lang, "[\"hello\" includes: \"z\"]", a);
    try expectTrue(s, lang, "[\"\" isEmpty]", a);
    try expectFalse(s, lang, "[\"x\" isEmpty]", a);
    try expectTrue(s, lang, "[\"a\" <= \"a\"]", a);
    try expectTrue(s, lang, "[\"b\" > \"a\"]", a);
    try expectTrue(s, lang, "[[\"a,b,c\" replace: \",\" with: \"-\"] = \"a-b-c\"]", a);
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(evalLine(s, lang, "[[\"a\\nb\\nc\" lines] size]", a)));
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(evalLine(s, lang, "[[\"  a  b \" words] size]", a)));
}

// Cluster-1 slivers (Phase 17 finish): Float three-way compare, string format:,
// File directory ops (makeDir:/isDir:/list:/removeDir:).
test "17 cluster-1: Float <=> / compare: + string format: + File dir ops" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // Float three-way compare — both `<=>` infix and `compare:`; SmallInt arg promotes.
    try testing.expectEqual(@as(i64, -1), ref_mod.untagSmallInt(evalLine(s, lang, "[1.5 <=> 2.5]", a)));
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(evalLine(s, lang, "[2.5 <=> 2.5]", a)));
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(evalLine(s, lang, "[9.0 compare: 2]", a)));
    try testing.expectEqual(@as(i64, -1), ref_mod.untagSmallInt(evalLine(s, lang, "[2.0 <=> 3]", a))); // float vs int

    // string format: — positional {} substitution (strings unquoted), {{ }} escapes.
    try expectTrue(s, lang, "[[\"{} + {} = {}\" format: #[2 3 5]] = \"2 + 3 = 5\"]", a);
    try expectTrue(s, lang, "[[\"hi {}\" format: #[\"bob\"]] = \"hi bob\"]", a); // string arg, no quotes
    try expectTrue(s, lang, "[[\"{{literal}}\" format: #[]] = \"{literal}\"]", a); // brace escapes
    // too few args → an error value (isError true).
    try expectTrue(s, lang, "[[\"{} {}\" format: #[1]] isError]", a);

    // File directory ops over a unique temp dir.
    const dir = "/tmp/birnam_cluster1_test";
    std.fs.cwd().deleteTree(dir) catch {}; // clean slate
    try expectTrue(s, lang, "[[File makeDir: \"/tmp/birnam_cluster1_test/sub\"] isNil]", a); // nil on success
    try expectTrue(s, lang, "[File isDir: \"/tmp/birnam_cluster1_test\"]", a);
    try expectFalse(s, lang, "[File isDir: \"/tmp/birnam_cluster1_test/nope\"]", a);
    _ = evalLine(s, lang, "[File write: \"hi\" to: \"/tmp/birnam_cluster1_test/f.txt\"]", a);
    // list: → a vector containing both "sub" and "f.txt" (order unspecified) → size 2.
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(evalLine(s, lang, "[[File list: \"/tmp/birnam_cluster1_test\"] size]", a)));
    // removeDir: on a non-empty dir errors; on an empty one succeeds (nil).
    try expectTrue(s, lang, "[[File removeDir: \"/tmp/birnam_cluster1_test\"] isError]", a);
    try expectTrue(s, lang, "[[File removeDir: \"/tmp/birnam_cluster1_test/sub\"] isNil]", a);
    std.fs.cwd().deleteTree(dir) catch {};
}

// Phase 19.2 — callbacks. A birnam block becomes a C function pointer (libffi
// closure) that libc `qsort` calls back on every comparison. Exercises the full
// trampoline both ways (unmarshal the two element pointers → Refs, marshal the
// closure's int result back) AND re-entrancy: the birnam evaluator runs mid-C
// call. The sort array is 4 longs (8 bytes each, matching pokeInt/peekInt).
test "FFI callback: birnam comparator sorts an array via libc qsort (19.2)" {
    if (@import("builtin").os.tag != .macos) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    _ = evalLine(s, lang, "let m := [Module load: \"/usr/lib/libSystem.B.dylib\"]", a);
    // void qsort(void *base, size_t nmemb, size_t size, int (*compar)(const void*, const void*))
    _ = evalLine(s, lang, "[m wrap: #sort:n:sz:cmp: as: \"qsort\" signature: \"pllp)v\"]", a);
    _ = evalLine(s, lang, "[Module resolve]", a);

    // 4 longs, unsorted: 30 10 20 5.
    _ = evalLine(s, lang, "let buf := [Module alloc: 32]", a);
    _ = evalLine(s, lang, "[buf pokeInt: 30 at: 0]", a);
    _ = evalLine(s, lang, "[buf pokeInt: 10 at: 8]", a);
    _ = evalLine(s, lang, "[buf pokeInt: 20 at: 16]", a);
    _ = evalLine(s, lang, "[buf pokeInt: 5 at: 24]", a);

    // Comparator: ascending by the long each pointer addresses.
    _ = evalLine(s, lang, "let cmp := [:x :y | [[x peekInt: 0] - [y peekInt: 0]]]", a);
    _ = evalLine(s, lang, "let cb := [Module callback: cmp sig: \"pp)i\"]", a);
    _ = evalLine(s, lang, "[m sort: buf n: 4 sz: 8 cmp: cb]", a);

    // Sorted ascending: 5 10 20 30.
    try testing.expectEqual(@as(i64, 5), ref_mod.untagSmallInt(evalLine(s, lang, "[buf peekInt: 0]", a)));
    try testing.expectEqual(@as(i64, 10), ref_mod.untagSmallInt(evalLine(s, lang, "[buf peekInt: 8]", a)));
    try testing.expectEqual(@as(i64, 20), ref_mod.untagSmallInt(evalLine(s, lang, "[buf peekInt: 16]", a)));
    try testing.expectEqual(@as(i64, 30), ref_mod.untagSmallInt(evalLine(s, lang, "[buf peekInt: 24]", a)));

    _ = evalLine(s, lang, "[cb free]", a);
    _ = evalLine(s, lang, "[buf free]", a);
}

// Phase 19.2 finish — callback arity > 2. A 3-param birnam block becomes a C
// function pointer with three int args; call the code location directly (a real
// C-ABI 3-arg call) and confirm the trampoline marshals all three + dispatches
// `value:value:value:`.
test "FFI callback arity 3: birnam 3-param block called as a C function (19.2)" {
    if (@import("builtin").os.tag != .macos) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // cb(x,y,z) = x + (y * z).
    _ = evalLine(s, lang, "let f := [:x :y :z | [x + [y * z]]]", a);
    const cb = evalLine(s, lang, "let cb := [Module callback: f sig: \"iii)i\"]", a);
    try testing.expect(boxed.isCallback(cb));

    const code = boxed.callbackCode(cb);
    try testing.expect(code != 0);
    const Fn = *const fn (c_int, c_int, c_int) callconv(.c) c_int;
    const fp: Fn = @ptrFromInt(code);
    try testing.expectEqual(@as(c_int, 2 + 3 * 4), fp(2, 3, 4));
    try testing.expectEqual(@as(c_int, 10 + 5 * 5), fp(10, 5, 5));

    // Arity 5 is rejected → nil (cap is 4).
    try testing.expect(ref_mod.isNil(evalLine(s, lang, "[Module callback: f sig: \"iiiii)i\"]", a)));

    _ = evalLine(s, lang, "[cb free]", a);
}

// Phase 19.5 — struct-as-entity, the user surface: define a struct from birnam
// source, then read/write fields as ordinary message sends.
test "FFI struct-as-entity: [Module struct:fields:] + field sends (19.5)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    _ = evalLine(s, lang, "let Point := [Module struct: #Point fields: #[#x #i  #y #i]]", a);
    _ = evalLine(s, lang, "let p := [Point new]", a);
    _ = evalLine(s, lang, "[p x: 30]", a);
    _ = evalLine(s, lang, "[p y: 12]", a);
    try testing.expectEqual(@as(i64, 42), ref_mod.untagSmallInt(evalLine(s, lang, "[[p x] + [p y]]", a)));
    try testing.expectEqual(@as(i64, 30), ref_mod.untagSmallInt(evalLine(s, lang, "[p x]", a)));
    // `[T size]` — two i32 fields, natural-aligned → 8 bytes.
    try testing.expectEqual(@as(i64, 8), ref_mod.untagSmallInt(evalLine(s, lang, "[Point size]", a)));
    _ = evalLine(s, lang, "[p free]", a);
}

test "17.3 three-way compare: <=> + compare: on SmallInt + String" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // SmallInt — both the `<=>` infix (3-char binary-op) and `compare:` keyword.
    try testing.expectEqual(@as(i64, -1), ref_mod.untagSmallInt(evalLine(s, lang, "[3 <=> 5]", a)));
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(evalLine(s, lang, "[5 <=> 5]", a)));
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(evalLine(s, lang, "[9 <=> 2]", a)));
    try testing.expectEqual(@as(i64, -1), ref_mod.untagSmallInt(evalLine(s, lang, "[3 compare: 5]", a)));
    // String — lexicographic.
    try testing.expectEqual(@as(i64, -1), ref_mod.untagSmallInt(evalLine(s, lang, "[\"abc\" <=> \"abd\"]", a)));
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(evalLine(s, lang, "[\"abc\" compare: \"abc\"]", a)));
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(evalLine(s, lang, "[\"b\" <=> \"a\"]", a)));
    // 3-char bump must not regress the 2-char `<=` / `>=` ops.
    try testing.expectEqual(kernel_eids.TRUE_EID, ref_mod.untagEid(evalLine(s, lang, "[5 <= 5]", a)));
    try testing.expectEqual(kernel_eids.FALSE_EID, ref_mod.untagEid(evalLine(s, lang, "[9 <= 2]", a)));
}

test "SmallInt bitwise + shift (2026-06-17)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    try testing.expectEqual(@as(i64, 8), ref_mod.untagSmallInt(evalLine(s, lang, "[12 bitAnd: 10]", a)));
    try testing.expectEqual(@as(i64, 15), ref_mod.untagSmallInt(evalLine(s, lang, "[12 bitOr: 3]", a)));
    try testing.expectEqual(@as(i64, 6), ref_mod.untagSmallInt(evalLine(s, lang, "[12 bitXor: 10]", a)));
    try testing.expectEqual(@as(i64, -6), ref_mod.untagSmallInt(evalLine(s, lang, "[5 bitNot]", a)));
    try testing.expectEqual(@as(i64, 16), ref_mod.untagSmallInt(evalLine(s, lang, "[1 shiftLeft: 4]", a)));
    try testing.expectEqual(@as(i64, 64), ref_mod.untagSmallInt(evalLine(s, lang, "[256 shiftRight: 2]", a)));
    // `<<` / `>>` infix sugar.
    try testing.expectEqual(@as(i64, 256), ref_mod.untagSmallInt(evalLine(s, lang, "[1 << 8]", a)));
    try testing.expectEqual(@as(i64, 256), ref_mod.untagSmallInt(evalLine(s, lang, "[1024 >> 2]", a)));
    // htons(8080) = byteswap → 0x901F = 36895.
    try testing.expectEqual(@as(i64, 36895), ref_mod.untagSmallInt(evalLine(s, lang, "[[[8080 bitAnd: 255] shiftLeft: 8] bitOr: [8080 shiftRight: 8]]", a)));
    // Integer-only: a Float arg is a typeMismatch error (not a number coercion).
    const ferr = evalLine(s, lang, "[12 bitAnd: 2.5]", a);
    try testing.expectEqual(kernel_eids.ERROR_VT, ref_mod.untagEid(s.store.currentValue(ref_mod.untagEid(ferr), kernel_eids.ATTR_VTABLE).?));
    // Shift amount out of range.
    const serr = evalLine(s, lang, "[1 shiftLeft: 99]", a);
    try testing.expectEqual(kernel_eids.ERROR_VT, ref_mod.untagEid(s.store.currentValue(ref_mod.untagEid(serr), kernel_eids.ATTR_VTABLE).?));
    // `&` / `|` stay boolean — bitwise did not shadow them on bools.
    try testing.expectEqual(kernel_eids.TRUE_EID, ref_mod.untagEid(evalLine(s, lang, "[true & true]", a)));
    try testing.expectEqual(kernel_eids.FALSE_EID, ref_mod.untagEid(evalLine(s, lang, "[true & false]", a)));
}

test "negative numeric literals at primary position (2026-06-17)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // Negative literal as a unary receiver / binary operand.
    try testing.expectEqual(@as(i64, 25), ref_mod.untagSmallInt(evalLine(s, lang, "[-5 squared]", a)));
    try testing.expectEqual(@as(i64, 8), ref_mod.untagSmallInt(evalLine(s, lang, "[-2 + 10]", a)));
    // Binary minus must NOT regress: `[3 - 1]` is still subtraction, not `3 (-1)`.
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(evalLine(s, lang, "[3 - 1]", a)));
    // Binary minus AND a negative literal operand together: 10 - (-5) = 15.
    try testing.expectEqual(@as(i64, 15), ref_mod.untagSmallInt(evalLine(s, lang, "[10 - -5]", a)));
    // Negative float literal evaluates to a Float.
    try testing.expect(boxed.isFloat(evalLine(s, lang, "[-2.5 + 1.0]", a)));
    // Negative literal as a keyword argument (the original parse-error case):
    // `max:` returns the larger, so -7 max: -1 = -1.
    try testing.expectEqual(@as(i64, -1), ref_mod.untagSmallInt(evalLine(s, lang, "[-7 max: -1]", a)));
}

test "Phase 7 datalog: [db findAll:where:] — vars, join, set results" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // Facts: install user methods → [SMALLINT_VT #qfoo m1], [SMALLINT_VT #qbar m2].
    _ = evalLine(s, lang, "[[3 vtable] method: #qfoo do: [| 1]]", a);
    _ = evalLine(s, lang, "[[3 vtable] method: #qbar do: [| 2]]", a);
    const sz = s.interner.str_to_eid.get("size").?;

    // Single clause: which entity has a #qfoo method? → exactly the SmallInt vt.
    const r1 = evalLine(s, lang, "[db findAll: ?vt where: #[ #[?vt #qfoo ?m] ]]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(r1, sz)));
    const svt = evalLine(s, lang, "[3 vtable]", a);
    try testing.expectEqual(svt, send_mod.send1(r1, s.interner.str_to_eid.get("at:").?, ref_mod.mkSmallInt(0).?));

    // Conjunction / join on a shared ?vt — needs BOTH #qfoo and #qbar.
    const r2 = evalLine(s, lang, "[db findAll: ?vt where: #[ #[?vt #qfoo ?a] #[?vt #qbar ?b] ]]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(r2, sz)));

    // A clause whose attr is absent → empty result.
    const r3 = evalLine(s, lang, "[db findAll: ?vt where: #[ #[?vt #qnope ?m] ]]", a);
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(send_mod.send0(r3, sz)));

    // Projecting the value var ?m binds the installed method (non-empty).
    const r4 = evalLine(s, lang, "[db findAll: ?m where: #[ #[?vt #qfoo ?m] ]]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(r4, sz)));

    const at_sel = s.interner.str_to_eid.get("at:").?;

    // Multi-var find → vector of tuple-vectors (one column per find var, in order).
    const rm = evalLine(s, lang, "[db findAll: #[?vt ?m] where: #[ #[?vt #qfoo ?m] ]]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(rm, sz)));
    const row0 = send_mod.send1(rm, at_sel, ref_mod.mkSmallInt(0).?);
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(send_mod.send0(row0, sz)));
    try testing.expectEqual(svt, send_mod.send1(row0, at_sel, ref_mod.mkSmallInt(0).?));

    // asOf: — capture a basis BEFORE a later fact, query as of it → not visible.
    _ = evalLine(s, lang, "let bb := [[db now] basisT]", a);
    _ = evalLine(s, lang, "[[3 vtable] method: #qlate do: [| 9]]", a);
    const cur = evalLine(s, lang, "[db findAll: ?vt where: #[ #[?vt #qlate ?z] ]]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(cur, sz)));
    const past = evalLine(s, lang, "[db findAll: ?vt where: #[ #[?vt #qlate ?z] ] asOf: bb]", a);
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(send_mod.send0(past, sz)));

    // since: — qlate was asserted AFTER bb; qfoo before. (between:and: same path.)
    const since_new = evalLine(s, lang, "[db findAll: ?vt where: #[ #[?vt #qlate ?z] ] since: bb]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(since_new, sz)));
    const since_old = evalLine(s, lang, "[db findAll: ?vt where: #[ #[?vt #qfoo ?z] ] since: bb]", a);
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(send_mod.send0(since_old, sz)));

    // Predicate clause `#[lhs op rhs]` — eq/ne on the two distinct method eids.
    const ne = evalLine(s, lang, "[db findAll: #[?a ?b] where: #[ #[?vt #qfoo ?a] #[?vt #qbar ?b] #[?a #ne ?b] ]]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(ne, sz)));
    const eq = evalLine(s, lang, "[db findAll: #[?a ?b] where: #[ #[?vt #qfoo ?a] #[?vt #qbar ?b] #[?a #eq ?b] ]]", a);
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(send_mod.send0(eq, sz)));

    // [snap find:where:] — a snapshot pins the query to its basis (time-travel).
    _ = evalLine(s, lang, "let snap := [db now]", a);
    _ = evalLine(s, lang, "[[3 vtable] method: #qpost do: [| 7]]", a);
    const snap_blind = evalLine(s, lang, "[snap find: ?vt where: #[ #[?vt #qpost ?z] ]]", a);
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(send_mod.send0(snap_blind, sz)));
    const live_sees = evalLine(s, lang, "[db findAll: ?vt where: #[ #[?vt #qpost ?z] ]]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(live_sees, sz)));

    // Negation `#[#not <clause>]` — has #qfoo but NOT a (nonexistent) #qgone → kept.
    const neg_keep = evalLine(s, lang, "[db findAll: ?vt where: #[ #[?vt #qfoo ?m] #[#not #[?vt #qgone ?x]] ]]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(neg_keep, sz)));
    // …but it DOES have #qbar, so negating that excludes it.
    const neg_drop = evalLine(s, lang, "[db findAll: ?vt where: #[ #[?vt #qfoo ?m] #[#not #[?vt #qbar ?x]] ]]", a);
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(send_mod.send0(neg_drop, sz)));

    // Aggregation `#[#count ?m]` — exactly one #qfoo binding. Result = #[ #[1] ].
    const agg = evalLine(s, lang, "[db findAll: #[ #[#count ?m] ] where: #[ #[?vt #qfoo ?m] ]]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(agg, sz)));
    const agg_row = send_mod.send1(agg, at_sel, ref_mod.mkSmallInt(0).?);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send1(agg_row, at_sel, ref_mod.mkSmallInt(0).?)));

    // Disjunction `#[#or <branch> …]` — has #qfoo (1st branch) even though #qgone absent.
    const dis_hit = evalLine(s, lang, "[db findAll: ?vt where: #[ #[#or #[?vt #qfoo ?m] #[?vt #qgone ?m]] ]]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(dis_hit, sz)));
    // Neither branch attr exists → empty.
    const dis_miss = evalLine(s, lang, "[db findAll: ?vt where: #[ #[#or #[?vt #qx ?m] #[?vt #qy ?m]] ]]", a);
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(send_mod.send0(dis_miss, sz)));

    // Quote sugar `#( [?e attr: ?v] )` lowers to the clause-vector form — same result.
    const qs = evalLine(s, lang, "[db findAll: ?vt where: #( [?vt qfoo: ?m] )]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(qs, sz)));
    try testing.expectEqual(svt, send_mod.send1(qs, at_sel, ref_mod.mkSmallInt(0).?));
    // Multi-clause quote join + multi-var find.
    const qj = evalLine(s, lang, "[db findAll: #[?vt ?m] where: #( [?vt qfoo: ?m] [?vt qbar: ?n] )]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(qj, sz)));
}

test "Phase 7.1 datalog rules: defineRule + recursive ancestor (transitive closure)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const sz = s.interner.str_to_eid.get("size").?;
    const at_sel = s.interner.str_to_eid.get("at:").?;

    // Parent edges asserted kernel-side: alice→bob→carol→dave. The attr eid is
    // intern("parent") so birnam's `#parent` resolves to the same eid; entities
    // are interned symbols so `#alice` etc. match too.
    const parent = s.api.intern("parent".ptr, 6);
    const alice = s.api.intern("alice".ptr, 5);
    const bob = s.api.intern("bob".ptr, 3);
    const carol = s.api.intern("carol".ptr, 5);
    const dave = s.api.intern("dave".ptr, 4);
    _ = try s.store.assert(alice, parent, ref_mod.mkEid(bob), kernel_eids.M_USER);
    _ = try s.store.assert(bob, parent, ref_mod.mkEid(carol), kernel_eids.M_USER);
    _ = try s.store.assert(carol, parent, ref_mod.mkEid(dave), kernel_eids.M_USER);

    // Define `ancestor` as two definitional-disjunction rules: base (a parent of
    // b) + recursive (a parent of x, x ancestor of b).
    _ = evalLine(s, lang, "[db defineRule: #ancestor params: #[?a ?b] body: #[ #[?a #parent ?b] ]]", a);
    _ = evalLine(s, lang, "[db defineRule: #ancestor params: #[?a ?b] body: #[ #[?a #parent ?x] #[#rule #ancestor ?x ?b] ]]", a);

    // Ancestors of alice → {bob, carol, dave} = 3.
    const anc = evalLine(s, lang, "[db findAll: ?x where: #[ #[#rule #ancestor #alice ?x] ]]", a);
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(send_mod.send0(anc, sz)));

    // Full transitive closure (all ?a,?b pairs): 3 direct + 3 transitive = 6.
    const pairs = evalLine(s, lang, "[db findAll: #[?a ?b] where: #[ #[#rule #ancestor ?a ?b] ]]", a);
    try testing.expectEqual(@as(i64, 6), ref_mod.untagSmallInt(send_mod.send0(pairs, sz)));

    // A constant-constant invocation acts as a membership check: dave is an
    // ancestor-descendant of alice (non-empty), but dave has no ancestors.
    const reachable = evalLine(s, lang, "[db findAll: ?b where: #[ #[#rule #ancestor #alice ?b] #[?b #parent ?c] ]]", a);
    // alice's ancestors that themselves have a parent: bob (→carol), carol (→dave) = 2.
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(send_mod.send0(reachable, sz)));
    const dave_anc = evalLine(s, lang, "[db findAll: ?x where: #[ #[#rule #ancestor #dave ?x] ]]", a);
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(send_mod.send0(dave_anc, sz)));

    // The recursive query returns real eids (alice's first ancestor is bob).
    const anc_eids = evalLine(s, lang, "[db findAll: ?x where: #[ #[#rule #ancestor #bob ?x] ]]", a);
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(send_mod.send0(anc_eids, sz))); // carol, dave
    _ = at_sel;

    // Malformed: #rule with no args is an error.
    try testing.expect(evalIsError(s, lang, "[db findAll: ?x where: #[ #[#rule #ancestor] ]]", a));
}

test "Phase 7.3 datalog pull: flat + nested-ref entity materialization" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const TRUE = ref_mod.mkEid(kernel_eids.TRUE_EID);
    const sz = s.interner.str_to_eid.get("size").?;

    // point :name "Point" :delegate object ; object :name "Object".
    const name_attr = s.api.intern("name".ptr, 4);
    const deleg = s.api.intern("delegate".ptr, 8);
    const point = s.api.intern("point".ptr, 5);
    const object = s.api.intern("object".ptr, 6);
    _ = try s.store.assert(point, name_attr, s.api.make_string("Point".ptr, 5), kernel_eids.M_USER);
    _ = try s.store.assert(point, deleg, ref_mod.mkEid(object), kernel_eids.M_USER);
    _ = try s.store.assert(object, name_attr, s.api.make_string("Object".ptr, 6), kernel_eids.M_USER);

    // Flat scalar pull → map with the attr value.
    try testing.expectEqual(TRUE, evalLine(s, lang, "[[[db pull: #point spec: #[ #name ]] at: #name] = \"Point\"]", a));

    // Nested-ref pull → follow #delegate, pull #name from it.
    try testing.expectEqual(TRUE, evalLine(s, lang,
        "[[[[db pull: #point spec: #[ #[#delegate #[ #name ]] ]] at: #delegate] at: #name] = \"Object\"]", a));

    // Scalars + nested ref together in one spec.
    try testing.expectEqual(TRUE, evalLine(s, lang,
        "[[[db pull: #point spec: #[ #name #[#delegate #[ #name ]] ]] at: #name] = \"Point\"]", a));

    // A missing attribute → nil in the map.
    try testing.expect(ref_mod.isNil(evalLine(s, lang, "[[db pull: #point spec: #[ #nope ]] at: #nope]", a)));

    // Composes with a query: pull every entity that has a #name (point + object).
    const pulled = evalLine(s, lang,
        "[[db findAll: ?e where: #[ #[?e #name ?n] ]] collect: [:e | [db pull: e spec: #[ #name ]]]]", a);
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(send_mod.send0(pulled, sz)));

    // Malformed nested spec → error.
    try testing.expect(evalIsError(s, lang, "[db pull: #point spec: #[ #[#delegate] ]]", a));
}

test "Phase 7.3 pull finish: reverse refs + :many ref nesting + saved queries" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const TRUE = ref_mod.mkEid(kernel_eids.TRUE_EID);
    const sz = s.interner.str_to_eid.get("size").?;
    const at_sel = s.interner.str_to_eid.get("at:").?;

    // Declare `:friend` as a `:many` attribute.
    const many_eid = s.interner.str_to_eid.get("many").?;
    const name_attr = s.api.intern("name".ptr, 4);
    const friend = s.api.intern("friend".ptr, 6);
    _ = try s.store.assert(friend, kernel_eids.ATTR_CARDINALITY, ref_mod.mkEid(many_eid), kernel_eids.M_USER);

    const alice = s.api.intern("alice".ptr, 5);
    const bob = s.api.intern("bob".ptr, 3);
    const carol = s.api.intern("carol".ptr, 5);
    _ = try s.store.assert(alice, name_attr, s.api.make_string("Alice".ptr, 5), kernel_eids.M_USER);
    _ = try s.store.assert(bob, name_attr, s.api.make_string("Bob".ptr, 3), kernel_eids.M_USER);
    _ = try s.store.assert(carol, name_attr, s.api.make_string("Carol".ptr, 5), kernel_eids.M_USER);
    // alice -> {bob, carol} via the :many friend attr.
    _ = try s.store.assert(alice, friend, ref_mod.mkEid(bob), kernel_eids.M_USER);
    _ = try s.store.assert(alice, friend, ref_mod.mkEid(carol), kernel_eids.M_USER);

    // Flat :many pull → at: #friend is a 2-element vector.
    const flat = evalLine(s, lang, "[[db pull: #alice spec: #[ #friend ]] at: #friend]", a);
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(send_mod.send0(flat, sz)));

    // Nested :many ref pull → vector of 2 child maps, each with a #name.
    const nested = evalLine(s, lang, "[[db pull: #alice spec: #[ #[#friend #[ #name ]] ]] at: #friend]", a);
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(send_mod.send0(nested, sz)));
    const first_child = send_mod.send1(nested, at_sel, ref_mod.mkSmallInt(0).?);
    try testing.expect(!ref_mod.isNil(send_mod.send1(first_child, at_sel, ref_mod.mkEid(s.api.intern("name".ptr, 4)))));

    // Reverse ref pull → who points AT bob via friend? Just alice. Bare `#_friend`.
    const rev = evalLine(s, lang, "[[db pull: #bob spec: #[ #_friend ]] at: #_friend]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(rev, sz)));

    // Reverse ref pull WITH sub-spec → vector of referrer maps; alice's name.
    const rev_sub = evalLine(s, lang, "[[db pull: #bob spec: #[ #[#_friend #[ #name ]] ]] at: #_friend]", a);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(send_mod.send0(rev_sub, sz)));
    try testing.expectEqual(TRUE, evalLine(s, lang,
        "[[[[[db pull: #bob spec: #[ #[#_friend #[ #name ]] ]] at: #_friend] at: 0] at: #name] = \"Alice\"]", a));

    // Saved query: define by name, run by name → same as the inline findAll.
    _ = evalLine(s, lang, "[db defineQuery: #namedThings find: ?n where: #[ #[?e #name ?n] ]]", a);
    const ran = evalLine(s, lang, "[db run: #namedThings]", a);
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(send_mod.send0(ran, sz))); // alice/bob/carol
    // Unknown saved query → an error value.
    try testing.expect(evalIsError(s, lang, "[db run: #nopeNope]", a));
}

test "17.5 eager & / | and the | block-bar disambiguation" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);
    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    // Eager boolean and/or.
    try testing.expectEqual(kernel_eids.FALSE_EID, ref_mod.untagEid(evalLine(s, lang, "[true & false]", a)));
    try testing.expectEqual(kernel_eids.TRUE_EID, ref_mod.untagEid(evalLine(s, lang, "[true & true]", a)));
    try testing.expectEqual(kernel_eids.TRUE_EID, ref_mod.untagEid(evalLine(s, lang, "[false | true]", a)));
    try testing.expectEqual(kernel_eids.FALSE_EID, ref_mod.untagEid(evalLine(s, lang, "[false | false]", a)));
    try testing.expectEqual(kernel_eids.TRUE_EID, ref_mod.untagEid(evalLine(s, lang, "[[3 < 5] & [2 < 9]]", a)));
    // `|` inside a block body must still mean eager-or, and the block-param bar
    // / no-param bar must still parse (choice order: block forms before binary).
    try testing.expectEqual(@as(i64, 6), ref_mod.untagSmallInt(evalLine(s, lang, "[[:x | [x + 1]] value: 5]", a)));
    const filtered = evalLine(s, lang, "[#[1 2 3 4] select: [:n | [[n > 1] & [n < 4]]]]", a);
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(send_mod.send1(filtered, s.interner.str_to_eid.get("at:").?, ref_mod.mkSmallInt(0).?)));
}

// W1 Stage 2b part 2 (docs/19c): a durable REPL block global — `let f := [block]`
// typed at the top level — survives a FULL (v7) image save → fresh boot → load.
// A REPL block is a *tree-walker* closure (`CLOSURE_VT { method = blockEvalShim,
// data = OBJECT_VT{ast, scope} }`), a different shape from a file-in bytecode
// block. lang registers `blockEvalShim` with the kernel; the writer emits the
// (AST, scope) recipe; `rematerializeClosures` rebuilds the wrapper at its eid.
// Both capturing-via-scope (`base`) and a no-free-var block are exercised.
test "image (W1 Stage 2b): REPL tree-walker block globals round-trip" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    // ---- Session A: define REPL block globals, sanity, FULL snapshot. ----
    const sa = try fullSetup(&arena);
    _ = (try sa.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_a = (try sa.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    _ = evalLine(sa, lang_a, "let f := [:x | [x + 1]]", a);
    _ = evalLine(sa, lang_a, "let base := 100", a);
    _ = evalLine(sa, lang_a, "let g := [:x | [x + base]]", a); // resolves `base` via the captured scope
    try testing.expectEqual(@as(i64, 11), ref_mod.untagSmallInt(evalLine(sa, lang_a, "[f value: 10]", a)));
    try testing.expectEqual(@as(i64, 105), ref_mod.untagSmallInt(evalLine(sa, lang_a, "[g value: 5]", a)));

    const res = try image.writeFull(a, sa.store, sa.table.next_eid.load(.monotonic), .{}, sa.interner, sa.table);

    // ---- Session B: fresh boot + peg/lang, load image, rebuild, re-invoke. ----
    const sb = try fullSetup(&arena);
    _ = (try sb.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_b = (try sb.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const loaded = try image.load(sb.store, res.bytes, sb.interner);
    if (loaded.next_eid > sb.table.next_eid.load(.monotonic)) sb.table.next_eid.store(loaded.next_eid, .monotonic);
    _ = try image.rematerializeClosures(a, sb.store, sb.table);

    // Pre-fix both returned nil (the wrapper had no object-table entry).
    try testing.expectEqual(@as(i64, 11), ref_mod.untagSmallInt(evalLine(sb, lang_b, "[f value: 10]", a)));
    // `g` resolving 105 proves the captured scope (and its `base` binding) rebound.
    try testing.expectEqual(@as(i64, 105), ref_mod.untagSmallInt(evalLine(sb, lang_b, "[g value: 5]", a)));
}

// W1 Stage 2b part 1 (docs/19c): a durable block global whose block CAPTURED an
// enclosing-method local survives a FULL (v7) image save → fresh boot → load,
// reconstructing its inline captures. `add5 := [makeAdder value: 5]` closes over
// the outer block's param `n = 5` (a true BytecodeClosure inline capture, unlike
// module/REPL free vars which resolve through persistent scope facts). The writer
// emits `[wrapper :closure-block-method bm]` + `[wrapper :closure-block-captures
// #[5]]`; `rematerializeClosures` rebuilds the wrapper with n=5 re-installed.
test "image (W1 Stage 2b): capturing block global round-trips its inline captures" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    // ---- Session A: define add5 (captures n=5), sanity, FULL snapshot. ----
    const sa = try fullSetup(&arena);
    _ = (try sa.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_a = (try sa.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const path = "test_stage2b_cap.birnam";
    try std.fs.cwd().writeFile(.{ .sub_path = path, .data = "let add5 := [[:n | [:x | [x + n]]] value: 5]\n" });
    defer std.fs.cwd().deleteFile(path) catch {};
    _ = evalLine(sa, lang_a, "[language fileIn: \"test_stage2b_cap.birnam\"]", a);
    try testing.expectEqual(@as(i64, 105), ref_mod.untagSmallInt(evalLine(sa, lang_a, "[add5 value: 100]", a))); // sanity

    const res = try image.writeFull(a, sa.store, sa.table.next_eid.load(.monotonic), .{}, sa.interner, sa.table);

    // ---- Session B: fresh boot + peg/lang, load image, rebuild, re-invoke. ----
    const sb = try fullSetup(&arena);
    _ = (try sb.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_b = (try sb.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const loaded = try image.load(sb.store, res.bytes, sb.interner);
    if (loaded.next_eid > sb.table.next_eid.load(.monotonic)) sb.table.next_eid.store(loaded.next_eid, .monotonic);
    _ = try image.rematerializeClosures(a, sb.store, sb.table);

    // The captured n=5 must survive: a captureless rebuild would yield x+0 = 100.
    try testing.expectEqual(@as(i64, 105), ref_mod.untagSmallInt(evalLine(sb, lang_b, "[add5 value: 100]", a)));
}
