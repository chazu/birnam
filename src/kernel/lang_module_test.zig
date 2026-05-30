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
