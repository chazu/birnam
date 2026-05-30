//! Phase 10.0 step 6 — PEG dylib integration smoke test.
//!
//! Loads `libbirnam_peg.dylib` through the Phase 5 module manager,
//! verifies birnam_module_init runs end-to-end, then exercises the
//! installed `parse:from:` method via send-dispatch against a small
//! grammar. This validates the full path: kernel boot → dylib dlopen
//! → IdApi.peg_install_send_api → method assert → send dispatch →
//! engine parse → CST root EID.

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
const peg = @import("peg.zig");

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

test "load peg dylib: parse:from: is installed on GRAMMAR_VT" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);

    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.LoadFailed;

    // Verify parse:from: now lives on GRAMMAR_VT — selector was interned at
    // install time, look it up by name.
    const sel = s.interner.str_to_eid.get("parse:from:") orelse return error.SelectorMissing;
    const method_ref = s.store.currentValue(kernel_eids.GRAMMAR_VT, sel) orelse return error.MethodNotInstalled;
    try testing.expect(ref_mod.isEid(method_ref));
}

test "send-dispatched: build grammar entirely through sends and parse" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);

    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.LoadFailed;

    const peg_ref = ref_mod.mkEid(kernel_eids.PEG_EID);
    const sel_new_grammar = s.interner.str_to_eid.get("newGrammar").?;
    const sel_literal = s.interner.str_to_eid.get("literal:").?;
    const sel_plus = s.interner.str_to_eid.get("plus:").?;
    const sel_define = s.interner.str_to_eid.get("defineRule:as:").?;
    const sel_parse = s.interner.str_to_eid.get("parse:from:").?;

    const grammar = send_mod.send0(peg_ref, sel_new_grammar);
    try testing.expect(ref_mod.isEid(grammar));

    const lit = send_mod.send1(peg_ref, sel_literal, try strings.mkString(a, "a"));
    const plus = send_mod.send1(peg_ref, sel_plus, lit);
    const start_name = try strings.mkString(a, "start");
    _ = send_mod.send2(grammar, sel_define, start_name, plus);

    const src = try strings.mkString(a, "aaa");
    const result = send_mod.send2(grammar, sel_parse, src, start_name);
    try testing.expect(ref_mod.isEid(result));
    const ast = ref_mod.untagEid(result);
    const end_ref = s.store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(end_ref));
}

test "send-dispatched: eof + anyByte + opt round-trip" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);

    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.LoadFailed;

    const peg_ref = ref_mod.mkEid(kernel_eids.PEG_EID);
    const sel_new_grammar = s.interner.str_to_eid.get("newGrammar").?;
    const sel_any = s.interner.str_to_eid.get("anyByte").?;
    const sel_eof = s.interner.str_to_eid.get("eof").?;
    const sel_plus = s.interner.str_to_eid.get("plus:").?;
    const sel_define = s.interner.str_to_eid.get("defineRule:as:").?;
    const sel_parse = s.interner.str_to_eid.get("parse:from:").?;

    // We can't build seq via send yet (deferred — needs vector arg), so the
    // grammar is just `start = anyByte+`. Verify it consumes everything.
    const grammar = send_mod.send0(peg_ref, sel_new_grammar);
    const any_pat = send_mod.send0(peg_ref, sel_any);
    const plus = send_mod.send1(peg_ref, sel_plus, any_pat);
    _ = send_mod.send2(grammar, sel_define, try strings.mkString(a, "start"), plus);

    const result = send_mod.send2(grammar, sel_parse, try strings.mkString(a, "xyz"), try strings.mkString(a, "start"));
    const ast = ref_mod.untagEid(result);
    const end_ref = s.store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(end_ref));
    // eof alone matches empty input.
    const grammar2 = send_mod.send0(peg_ref, sel_new_grammar);
    const eof_pat = send_mod.send0(peg_ref, sel_eof);
    _ = send_mod.send2(grammar2, sel_define, try strings.mkString(a, "start"), eof_pat);
    const empty_result = send_mod.send2(grammar2, sel_parse, try strings.mkString(a, ""), try strings.mkString(a, "start"));
    const empty_ast = ref_mod.untagEid(empty_result);
    try testing.expect(s.store.currentValue(empty_ast, kernel_eids.ATTR_VTABLE) == null);
}

test "load peg dylib + Zig-built grammar + send-dispatched parse" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);

    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.LoadFailed;

    // Build a grammar using the in-process engine API (mirrors what the
    // language dylib will do at its own init in Phase 10.1).
    var engine = try peg.Engine.init(arena.allocator(), s.store, s.table, s.interner, kernel_eids.M_PEG);
    const g = try peg.mkGrammar(&engine);
    const ranges = [_]peg.Range{ .{ .lo = 'a', .hi = 'z' } };
    _ = try peg.defineRule(&engine, g, "word", try peg.mkPlus(&engine, try peg.mkCharset(&engine, &ranges)));

    // Send-dispatch parse:from: through the engine that the dylib installed.
    const sel = s.interner.str_to_eid.get("parse:from:").?;
    const src = try strings.mkString(arena.allocator(), "hello");
    const rule = try strings.mkString(arena.allocator(), "word");
    const result = send_mod.send2(ref_mod.mkEid(g), sel, src, rule);

    try testing.expect(ref_mod.isEid(result));
    const ast = ref_mod.untagEid(result);
    // Success path → no :vtable on the CST node.
    try testing.expect(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE) == null);
    const end_ref = s.store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 5), ref_mod.untagSmallInt(end_ref));
}
