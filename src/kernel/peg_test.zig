//! PEG engine tests (Phase 10.0 step 3).
//!
//! Drives the engine via its public Zig API (Engine, mkLiteral, parse, ...)
//! rather than through the dylib boundary, so combinator correctness can
//! be unit-tested cheaply. Send-dispatched surface lands in step 4.

const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const fact_store_mod = @import("fact_store.zig");
const object_table_mod = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const bootstrap = @import("bootstrap.zig");
const kernel_eids = @import("kernel_eids.zig");
const peg = @import("peg.zig");

const M_TEST: u64 = kernel_eids.M_PEG;

fn bootAndEngine(
    a: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    interner: *interner_mod.Interner,
) !peg.Engine {
    store.* = fact_store_mod.FactStore.init(a);
    table.* = object_table_mod.ObjectTable.init(a);
    interner.* = interner_mod.Interner.init(a);
    _ = try bootstrap.boot(a, store, table, interner);
    return try peg.Engine.init(a, store, table, interner, M_TEST);
}

test "literal matches its byte string at position 0" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    const pat = try peg.mkLiteral(&engine, "hello");
    _ = try peg.defineRule(&engine, g, "start", pat);

    const out = try peg.parse(&engine, g, "hello", "start");
    switch (out) {
        .success => |ast| {
            const end_ref = store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
            try testing.expectEqual(@as(i64, 5), ref_mod.untagSmallInt(end_ref));
        },
        .failure => return error.UnexpectedFailure,
    }
}

test "literal mismatch produces ERROR_VT with furthest position" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    _ = try peg.defineRule(&engine, g, "start", try peg.mkLiteral(&engine, "foo"));

    const out = try peg.parse(&engine, g, "bar", "start");
    switch (out) {
        .success => return error.UnexpectedSuccess,
        .failure => |err| {
            const vt_ref = store.currentValue(err, kernel_eids.ATTR_VTABLE).?;
            try testing.expectEqual(kernel_eids.ERROR_VT, ref_mod.untagEid(vt_ref));
            const pos_ref = store.currentValue(err, kernel_eids.ATTR_PEG_POSITION).?;
            try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(pos_ref));
        },
    }
}

test "seq matches concatenated literals" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    const children = [_]u64{
        try peg.mkLiteral(&engine, "foo"),
        try peg.mkLiteral(&engine, "bar"),
    };
    _ = try peg.defineRule(&engine, g, "start", try peg.mkSeq(&engine, &children));

    const out = try peg.parse(&engine, g, "foobar", "start");
    const ast = out.success;
    const end_ref = store.currentValue(ast, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 6), ref_mod.untagSmallInt(end_ref));
}

test "choice picks first matching alternative" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    const alts = [_]u64{
        try peg.mkLiteral(&engine, "foo"),
        try peg.mkLiteral(&engine, "bar"),
    };
    _ = try peg.defineRule(&engine, g, "start", try peg.mkChoice(&engine, &alts));

    const out_b = try peg.parse(&engine, g, "bar", "start");
    const end = store.currentValue(out_b.success, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(end));
}

test "star matches zero or more" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    const a_lit = try peg.mkLiteral(&engine, "a");
    _ = try peg.defineRule(&engine, g, "start", try peg.mkStar(&engine, a_lit));

    // Empty input → zero matches still succeeds.
    {
        const out = try peg.parse(&engine, g, "", "start");
        try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(
            store.currentValue(out.success, kernel_eids.ATTR_AST_BYTE_END).?,
        ));
    }
    // Triple match.
    {
        const out = try peg.parse(&engine, g, "aaa", "start");
        try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(
            store.currentValue(out.success, kernel_eids.ATTR_AST_BYTE_END).?,
        ));
    }
}

test "plus requires at least one match" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    const a_lit = try peg.mkLiteral(&engine, "a");
    _ = try peg.defineRule(&engine, g, "start", try peg.mkPlus(&engine, a_lit));

    const out_empty = try peg.parse(&engine, g, "", "start");
    try testing.expect(out_empty == .failure);
    const out_one = try peg.parse(&engine, g, "a", "start");
    try testing.expect(out_one == .success);
}

test "opt: success when child matches, success-with-zero when not" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    _ = try peg.defineRule(&engine, g, "start", try peg.mkOpt(&engine, try peg.mkLiteral(&engine, "x")));

    const out0 = try peg.parse(&engine, g, "", "start");
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(
        store.currentValue(out0.success, kernel_eids.ATTR_AST_BYTE_END).?,
    ));
    const out1 = try peg.parse(&engine, g, "x", "start");
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(
        store.currentValue(out1.success, kernel_eids.ATTR_AST_BYTE_END).?,
    ));
}

test "and: positive lookahead doesn't consume" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    const children = [_]u64{
        try peg.mkAnd(&engine, try peg.mkLiteral(&engine, "foo")),
        try peg.mkLiteral(&engine, "foo"),
    };
    _ = try peg.defineRule(&engine, g, "start", try peg.mkSeq(&engine, &children));

    const out = try peg.parse(&engine, g, "foo", "start");
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(
        store.currentValue(out.success, kernel_eids.ATTR_AST_BYTE_END).?,
    ));
}

test "not: negative lookahead succeeds when child fails" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    const children = [_]u64{
        try peg.mkNot(&engine, try peg.mkLiteral(&engine, "foo")),
        try peg.mkLiteral(&engine, "bar"),
    };
    _ = try peg.defineRule(&engine, g, "start", try peg.mkSeq(&engine, &children));

    const out = try peg.parse(&engine, g, "bar", "start");
    try testing.expect(out == .success);
    const out2 = try peg.parse(&engine, g, "foo", "start");
    try testing.expect(out2 == .failure);
}

test "charset: ascii lowercase letter" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    const ranges = [_]peg.Range{ .{ .lo = 'a', .hi = 'z' } };
    _ = try peg.defineRule(&engine, g, "start", try peg.mkPlus(&engine, try peg.mkCharset(&engine, &ranges)));

    const out = try peg.parse(&engine, g, "hello", "start");
    try testing.expectEqual(@as(i64, 5), ref_mod.untagSmallInt(
        store.currentValue(out.success, kernel_eids.ATTR_AST_BYTE_END).?,
    ));
    const fail = try peg.parse(&engine, g, "HELLO", "start");
    try testing.expect(fail == .failure);
}

test "any consumes one byte" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    _ = try peg.defineRule(&engine, g, "start", try peg.mkPlus(&engine, try peg.mkAny(&engine)));

    const out = try peg.parse(&engine, g, "abc", "start");
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(
        store.currentValue(out.success, kernel_eids.ATTR_AST_BYTE_END).?,
    ));
}

test "eof: succeeds only at end of input" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    const children = [_]u64{
        try peg.mkLiteral(&engine, "ok"),
        try peg.mkEof(&engine),
    };
    _ = try peg.defineRule(&engine, g, "start", try peg.mkSeq(&engine, &children));

    const out = try peg.parse(&engine, g, "ok", "start");
    try testing.expect(out == .success);
    const fail = try peg.parse(&engine, g, "okay", "start");
    try testing.expect(fail == .failure);
}

test "rule-ref: nested rules produce nested CST" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    // word = "x"+ ; start = word "!" ;
    _ = try peg.defineRule(&engine, g, "word", try peg.mkPlus(&engine, try peg.mkLiteral(&engine, "x")));
    const start_children = [_]u64{
        try peg.mkRuleRef(&engine, "word"),
        try peg.mkLiteral(&engine, "!"),
    };
    _ = try peg.defineRule(&engine, g, "start", try peg.mkSeq(&engine, &start_children));

    const out = try peg.parse(&engine, g, "xxx!", "start");
    const root = out.success;
    // Root start node has a first child (the word CST node).
    const fc_ref = store.currentValue(root, kernel_eids.ATTR_AST_FIRST_CHILD).?;
    const word_ast = ref_mod.untagEid(fc_ref);
    const word_name_ref = store.currentValue(word_ast, kernel_eids.ATTR_AST_RULE_NAME).?;
    const word_name_eid = ref_mod.untagEid(word_name_ref);
    try testing.expectEqualStrings("word", interner.nameOf(word_name_eid).?);
}

test "parse:from: dispatches through send and returns CST EID" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    // Build grammar: start = "hi"
    const g = try peg.mkGrammar(&engine);
    _ = try peg.defineRule(&engine, g, "start", try peg.mkLiteral(&engine, "hi"));

    // Install send-dispatched parse:from: on GRAMMAR_VT.
    try peg.installSendApi(a, &store, &table, &interner, M_TEST);

    const send_mod = @import("send.zig");
    const strings_mod = @import("strings.zig");
    const sel_parse = interner.str_to_eid.get("parse:from:").?;
    const src_ref = try strings_mod.mkString(a, "hi");
    const rule_ref = try strings_mod.mkString(a, "start");

    const result = send_mod.send2(ref_mod.mkEid(g), sel_parse, src_ref, rule_ref);
    try testing.expect(ref_mod.isEid(result));
    const ast_eid = ref_mod.untagEid(result);
    const vt_ref = store.currentValue(ast_eid, kernel_eids.ATTR_VTABLE);
    // Success path: AST entity has no :vtable fact (CST nodes aren't typed
    // entities). Failure would have :vtable = ERROR_VT.
    try testing.expect(vt_ref == null);
    const end_ref = store.currentValue(ast_eid, kernel_eids.ATTR_AST_BYTE_END).?;
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(end_ref));
}

test "parse:from: returns ERROR_VT EID on failure" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    _ = try peg.defineRule(&engine, g, "start", try peg.mkLiteral(&engine, "yes"));
    try peg.installSendApi(a, &store, &table, &interner, M_TEST);

    const send_mod = @import("send.zig");
    const strings_mod = @import("strings.zig");
    const sel_parse = interner.str_to_eid.get("parse:from:").?;
    const src_ref = try strings_mod.mkString(a, "no");
    const rule_ref = try strings_mod.mkString(a, "start");

    const result = send_mod.send2(ref_mod.mkEid(g), sel_parse, src_ref, rule_ref);
    try testing.expect(ref_mod.isEid(result));
    const err_eid = ref_mod.untagEid(result);
    const vt_ref = store.currentValue(err_eid, kernel_eids.ATTR_VTABLE).?;
    try testing.expectEqual(kernel_eids.ERROR_VT, ref_mod.untagEid(vt_ref));
}

test "ERROR_VT carries expected-set at furthest position" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store: fact_store_mod.FactStore = undefined;
    var table: object_table_mod.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    var engine = try bootAndEngine(a, &store, &table, &interner);

    const g = try peg.mkGrammar(&engine);
    const alts = [_]u64{
        try peg.mkLiteral(&engine, "foo"),
        try peg.mkLiteral(&engine, "bar"),
    };
    _ = try peg.defineRule(&engine, g, "start", try peg.mkChoice(&engine, &alts));

    const out = try peg.parse(&engine, g, "baz", "start");
    const err = out.failure;
    // :peg-expected is :many — currentValuesIter walks each value.
    var iter = store.currentValuesIter(err, kernel_eids.ATTR_PEG_EXPECTED);
    var count: usize = 0;
    while (iter.next()) |v_ref| {
        try testing.expect(ref_mod.isEid(v_ref));
        count += 1;
    }
    try testing.expect(count >= 2);
}
