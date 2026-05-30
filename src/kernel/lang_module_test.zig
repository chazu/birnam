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

test "lang dylib: parse \"xxx\" via the asserted identifier-x rule" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);

    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const sel_grammar = s.interner.str_to_eid.get("grammar").?;
    const sel_parse = s.interner.str_to_eid.get("parse:from:").?;
    const grammar = s.store.currentValue(lang_mod_eid, sel_grammar).?;

    const result = send_mod.send2(
        grammar,
        sel_parse,
        try strings.mkString(a, "xxx"),
        try strings.mkString(a, "start"),
    );
    try testing.expect(ref_mod.isEid(result));
    const ast = ref_mod.untagEid(result);
    // Success → no :vtable on CST node.
    try testing.expect(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE) == null);
    const end_ref = s.store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(end_ref));
}

test "lang dylib: parse \"true\" via kw-true literal rule" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try fullSetup(&arena);

    _ = (try s.manager.load("./zig-out/lib/libbirnam_peg.dylib")) orelse return error.PegLoadFailed;
    const lang_mod_eid = (try s.manager.load("./zig-out/lib/libbirnam_lang.dylib")) orelse return error.LangLoadFailed;

    const sel_grammar = s.interner.str_to_eid.get("grammar").?;
    const sel_parse = s.interner.str_to_eid.get("parse:from:").?;
    const grammar = s.store.currentValue(lang_mod_eid, sel_grammar).?;

    const result = send_mod.send2(
        grammar,
        sel_parse,
        try strings.mkString(a, "true"),
        try strings.mkString(a, "kw-true"),
    );
    const ast = ref_mod.untagEid(result);
    try testing.expect(s.store.currentValue(ast, kernel_eids.ATTR_VTABLE) == null);
    const end_ref = s.store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 4), ref_mod.untagSmallInt(end_ref));
}
