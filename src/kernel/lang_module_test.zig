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
    try testing.expect(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE) == null);
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
    try testing.expect(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE) == null);
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
    try testing.expect(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE) == null);
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
    try testing.expect(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE) == null);
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
    try testing.expect(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE) == null);
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
    try testing.expect(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE) == null);
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
    try testing.expect(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE) == null);
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
    _ = evalLine(s, lang, "[m wrap: #sqrt: as: \"sqrt\" signature: \"d)d\"]", a);

    // Call — the 1-arg keyword send that the arg-arity fix unblocks.
    const r = evalLine(s, lang, "[m sqrt: 16.0]", a);
    try testing.expect(boxed.isFloat(r));
    try testing.expectApproxEqAbs(@as(f64, 4.0), boxed.untagFloat(r), 1e-9);
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
    try testing.expectEqual(ref_mod.NIL, evalLine(s, lang, "c", a)); // form 3 skipped → unbound

    // -- continue mode: form 3 applies despite the form-2 error. --
    _ = evalLine(s, lang, "[language fileIn: \"test_filein_bad.birnam\" onError: #continue]", a);
    try testing.expectEqual(@as(i64, 7), ref_mod.untagSmallInt(evalLine(s, lang, "c", a)));
}
