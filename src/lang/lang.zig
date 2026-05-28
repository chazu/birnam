const std = @import("std");
const parser_mod = @import("parser.zig");
const ast = @import("ast.zig");
const eval_mod = @import("eval.zig");
const env_mod = @import("env.zig");
const builtins = @import("builtins.zig");
const IdApi = @import("api.zig").IdApi;

/// Shell context (must match shell.zig's layout).
const ShellContext = extern struct {
    load_module: *const fn ([*:0]const u8) callconv(.c) bool,
    set_repl_handler: *const fn (?*const fn ([*:0]const u8) callconv(.c) void) callconv(.c) void,
    log: *const fn ([*:0]const u8) callconv(.c) void,
    export_value: *const fn ([*:0]const u8, ?*anyopaque) callconv(.c) void,
    import_value: *const fn ([*:0]const u8) callconv(.c) ?*anyopaque,
};

var id_api: *const IdApi = undefined;
var shell_log: *const fn ([*:0]const u8) callconv(.c) void = undefined;
var evaluator: eval_mod.Evaluator = undefined;
var global_env: env_mod.Env = undefined;
var arena_instance: std.heap.ArenaAllocator = undefined;

fn log(msg: [*:0]const u8) void {
    shell_log(msg);
}

comptime {
    if (!@import("builtin").is_test) {
        @export(&birnam_module_init, .{ .name = "birnam_module_init" });
    }
}

fn birnam_module_init(ctx: *ShellContext) callconv(.c) void {
    shell_log = ctx.log;
    log("lang: initializing...");

    const api_ptr = ctx.import_value("id_api");
    if (api_ptr == null) {
        log("lang: ERROR - id_api not found (is id module loaded first?)");
        return;
    }

    id_api = @ptrCast(@alignCast(api_ptr));

    // Init arena allocator
    arena_instance = std.heap.ArenaAllocator.init(std.heap.page_allocator);

    // Init builtins (SmallInt vtable etc.)
    builtins.initBuiltins(id_api);
    log("lang: builtins initialized");

    // Init global environment
    global_env = env_mod.Env.init(arena_instance.allocator(), null);

    // Bind global constants
    global_env.define("nil", builtins.getNil());
    global_env.define("true", builtins.getTrue());
    global_env.define("false", builtins.getFalse());

    // Init evaluator
    evaluator = eval_mod.Evaluator.init(id_api, &global_env, arena_instance.allocator());

    // Set global evaluator for trampolines
    eval_mod.setGlobalEvaluator(&evaluator);

    // Register REPL handler
    ctx.set_repl_handler(&replHandler);
    log("lang: REPL handler registered");
    log("lang: ready");
}

fn replHandler(input: [*:0]const u8) callconv(.c) void {
    const source = std.mem.span(input);
    if (source.len == 0) return;

    // Parse
    var p = parser_mod.Parser.init(source, arena_instance.allocator());
    const node = p.parseTopLevel() catch |err| {
        const pos = p.currentPos();
        const w = std.fs.File.stderr().deprecatedWriter();
        w.print("parse error: {s} at line {d}, col {d}\n", .{ @errorName(err), pos.line, pos.col }) catch {};
        return;
    };
    if (node == null) return;

    // Eval
    const result = evaluator.eval(node.?, &global_env) catch |err| {
        const w = std.fs.File.stderr().deprecatedWriter();
        w.print("eval error: {s}\n", .{@errorName(err)}) catch {};
        return;
    };

    // Print result
    const formatted = evaluator.formatResult(result);
    const w = std.fs.File.stdout().deprecatedWriter();
    w.print("{s}\n", .{formatted}) catch {};
}

// -- Tests ------------------------------------------------------------------

fn testInit() *const IdApi {
    const id = @import("id");
    _ = id.bootstrap.init();
    id.api.populate();
    const ptr: *const IdApi = @ptrCast(&id.api.id_api);
    builtins.initBuiltins(ptr);
    return ptr;
}

fn testEval(source: []const u8) !?*anyopaque {
    const api = testInit();
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var global = env_mod.Env.init(alloc, null);
    defer global.deinit();
    global.define("nil", builtins.getNil());
    global.define("true", builtins.getTrue());
    global.define("false", builtins.getFalse());

    var e = eval_mod.Evaluator.init(api, &global, alloc);
    eval_mod.setGlobalEvaluator(&e);
    var p = parser_mod.Parser.init(source, alloc);
    const node = try p.parseTopLevel() orelse return null;
    return e.eval(node, &global);
}

test "parse integer literal" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("42", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .integer_lit);
    try std.testing.expectEqual(@as(i64, 42), node.?.data.integer_lit.value);
}

test "parse negative integer" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("-7", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expectEqual(@as(i64, -7), node.?.data.integer_lit.value);
}

test "parse radix integer 16rFF" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("16rFF", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expectEqual(@as(i64, 255), node.?.data.integer_lit.value);
}

test "parse binary send 3 + 4" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("3 + 4", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .send);
    try std.testing.expect(std.mem.eql(u8, node.?.data.send.selector, "+"));
    try std.testing.expectEqual(@as(i64, 3), node.?.data.send.receiver.data.integer_lit.value);
}

test "parse chained binary sends 1 + 2 * 3" {
    // Binary sends are left-associative, no precedence among binary ops
    // So 1 + 2 * 3 is (1 + 2) * 3
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("1 + 2 * 3", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .send);
    try std.testing.expect(std.mem.eql(u8, node.?.data.send.selector, "*"));
    // receiver should be (1 + 2)
    try std.testing.expect(node.?.data.send.receiver.tag == .send);
}

test "parse parenthesized expression" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("(3 + 4)", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .send);
}

test "parse comment skipping" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("\"a comment\" 42", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expectEqual(@as(i64, 42), node.?.data.integer_lit.value);
}

test "eval integer literal" {
    const result = try testEval("42");
    try std.testing.expect(result != null);
    try std.testing.expect(builtins.isSmallInt(result));
    try std.testing.expectEqual(@as(i64, 42), builtins.oopToInt(result));
}

test "eval 3 + 4 = 7" {
    const result = try testEval("3 + 4");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 7), builtins.oopToInt(result));
}

test "eval 10 - 3 = 7" {
    const result = try testEval("10 - 3");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 7), builtins.oopToInt(result));
}

test "eval 6 * 7 = 42" {
    const result = try testEval("6 * 7");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 42), builtins.oopToInt(result));
}

test "eval 10 // 3 = 3" {
    const result = try testEval("10 // 3");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 3), builtins.oopToInt(result));
}

test "eval chained: (3 + 4) * 2 = 14" {
    const result = try testEval("(3 + 4) * 2");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 14), builtins.oopToInt(result));
}

test "eval comparison: 3 < 5 = 1" {
    const result = try testEval("3 < 5");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 1), builtins.oopToInt(result));
}

test "eval comparison: 5 < 3 = 0" {
    const result = try testEval("5 < 3");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 0), builtins.oopToInt(result));
}

test "eval variable assignment and lookup" {
    const api = testInit();
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var global = env_mod.Env.init(alloc, null);
    defer global.deinit();
    var e = eval_mod.Evaluator.init(api, &global, alloc);

    // x := 10
    var p1 = parser_mod.Parser.init("x := 10", alloc);
    const n1 = try p1.parseTopLevel() orelse unreachable;
    _ = try e.eval(n1, &global);

    // x + 5
    var p2 = parser_mod.Parser.init("x + 5", alloc);
    const n2 = try p2.parseTopLevel() orelse unreachable;
    const result = try e.eval(n2, &global);
    try std.testing.expectEqual(@as(i64, 15), builtins.oopToInt(result));
}

test "eval sequence with dot separator" {
    const api = testInit();
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var global = env_mod.Env.init(alloc, null);
    defer global.deinit();
    var e = eval_mod.Evaluator.init(api, &global, alloc);

    var p = parser_mod.Parser.init("x := 3. x + 4", alloc);
    const node = try p.parseTopLevel() orelse unreachable;
    const result = try e.eval(node, &global);
    try std.testing.expectEqual(@as(i64, 7), builtins.oopToInt(result));
}

test "tagged integer encoding roundtrip" {
    const values = [_]i64{ 0, 1, -1, 42, -42, 1000000, -1000000 };
    for (values) |v| {
        const oop = builtins.intToOop(v);
        try std.testing.expect(builtins.isSmallInt(oop));
        try std.testing.expectEqual(v, builtins.oopToInt(oop));
    }
}

test "format result" {
    const api = testInit();
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var global = env_mod.Env.init(alloc, null);
    defer global.deinit();
    var e = eval_mod.Evaluator.init(api, &global, alloc);

    const formatted = e.formatResult(builtins.intToOop(42));
    try std.testing.expect(std.mem.eql(u8, formatted, "42"));

    const nil_fmt = e.formatResult(null);
    try std.testing.expect(std.mem.eql(u8, nil_fmt, "nil"));
}

test "parse keyword send" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("x at: 1 put: 2", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .send);
    try std.testing.expect(std.mem.eql(u8, node.?.data.send.selector, "at:put:"));
    try std.testing.expectEqual(@as(usize, 2), node.?.data.send.args.len);
}

// -- Phase 2 tests: Strings, Symbols, Variables -----------------------------

test "eval string literal" {
    const result = try testEval("'hello'");
    try std.testing.expect(result != null);
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "hello"));
}

test "eval string concat" {
    const result = try testEval("'Hello, ' , 'world'");
    try std.testing.expect(result != null);
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "Hello, world"));
}

test "eval string size" {
    const result = try testEval("'hello' size");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 5), builtins.oopToInt(result));
}

test "eval string at:" {
    const result = try testEval("'hello' at: 0");
    try std.testing.expect(result != null);
    // 'h' = 104
    try std.testing.expectEqual(@as(i64, 104), builtins.oopToInt(result));
}

test "eval string equality" {
    const result1 = try testEval("'abc' = 'abc'");
    try std.testing.expectEqual(@as(i64, 1), builtins.oopToInt(result1));

    const result2 = try testEval("'abc' = 'def'");
    try std.testing.expectEqual(@as(i64, 0), builtins.oopToInt(result2));

    const result3 = try testEval("'abc' ~= 'def'");
    try std.testing.expectEqual(@as(i64, 1), builtins.oopToInt(result3));
}

test "eval string variable assignment" {
    const api = testInit();
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var global = env_mod.Env.init(alloc, null);
    defer global.deinit();
    global.define("nil", builtins.getNil());
    var e = eval_mod.Evaluator.init(api, &global, alloc);

    // s := 'Hello'. s , ' world'
    var p = parser_mod.Parser.init("s := 'Hello'. s , ' world'", alloc);
    const node = try p.parseTopLevel() orelse unreachable;
    const result = try e.eval(node, &global);
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "Hello world"));
}

test "parse symbol literal #foo" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("#foo", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .symbol_lit);
    try std.testing.expect(std.mem.eql(u8, node.?.data.symbol_lit.value, "foo"));
}

test "parse symbol literal #at:put:" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("#at:put:", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .symbol_lit);
    try std.testing.expect(std.mem.eql(u8, node.?.data.symbol_lit.value, "at:put:"));
}

test "parse quoted symbol #'hello world'" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("#'hello world'", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .symbol_lit);
    try std.testing.expect(std.mem.eql(u8, node.?.data.symbol_lit.value, "hello world"));
}

test "parse binary symbol #+" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("#+", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .symbol_lit);
    try std.testing.expect(std.mem.eql(u8, node.?.data.symbol_lit.value, "+"));
}

test "eval symbol literal produces string" {
    const result = try testEval("#foo");
    try std.testing.expect(result != null);
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "foo"));
}

test "eval nil variable" {
    const result = try testEval("nil");
    try std.testing.expect(result == null);
}

test "eval true and false variables" {
    const t = try testEval("true");
    try std.testing.expect(t != null);
    try std.testing.expect(t == builtins.getTrue());

    const f = try testEval("false");
    try std.testing.expect(f != null);
    try std.testing.expect(f == builtins.getFalse());
}

test "eval escaped string quote" {
    const result = try testEval("'it''s'");
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "it's"));
}

test "format string result" {
    const api = testInit();
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var global = env_mod.Env.init(alloc, null);
    defer global.deinit();
    var e = eval_mod.Evaluator.init(api, &global, alloc);

    const str = builtins.makeString("hello");
    const formatted = e.formatResult(str);
    try std.testing.expect(std.mem.eql(u8, formatted, "'hello'"));
}

test "format true/false result" {
    const api = testInit();
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var global = env_mod.Env.init(alloc, null);
    defer global.deinit();
    var e = eval_mod.Evaluator.init(api, &global, alloc);

    const t = e.formatResult(builtins.getTrue());
    try std.testing.expect(std.mem.eql(u8, t, "true"));

    const f = e.formatResult(builtins.getFalse());
    try std.testing.expect(std.mem.eql(u8, f, "false"));
}

test "eval empty string" {
    const result = try testEval("''");
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expectEqual(@as(usize, 0), s.len);
}

test "eval empty string size" {
    const result = try testEval("'' size");
    try std.testing.expectEqual(@as(i64, 0), builtins.oopToInt(result));
}

test "eval multi-concat" {
    const result = try testEval("'a' , 'b' , 'c'");
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "abc"));
}

// -- Phase 3 tests: Blocks and Control Flow ---------------------------------

test "parse simple block" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("[42]", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .block);
    try std.testing.expectEqual(@as(usize, 0), node.?.data.block.params.len);
    try std.testing.expectEqual(@as(usize, 1), node.?.data.block.body.len);
}

test "parse block with params" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("[:x :y | x + y]", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .block);
    try std.testing.expectEqual(@as(usize, 2), node.?.data.block.params.len);
    try std.testing.expect(std.mem.eql(u8, node.?.data.block.params[0], "x"));
    try std.testing.expect(std.mem.eql(u8, node.?.data.block.params[1], "y"));
}

test "parse block with temps" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("[:x | | tmp | tmp := x + 1. tmp]", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .block);
    try std.testing.expectEqual(@as(usize, 1), node.?.data.block.params.len);
    try std.testing.expectEqual(@as(usize, 1), node.?.data.block.temps.len);
    try std.testing.expect(std.mem.eql(u8, node.?.data.block.temps[0], "tmp"));
}

test "eval block value" {
    const result = try testEval("[42] value");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 42), builtins.oopToInt(result));
}

test "eval block value: with param" {
    const result = try testEval("[:x | x + 1] value: 5");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 6), builtins.oopToInt(result));
}

test "eval block value:value: with two params" {
    const result = try testEval("[:x :y | x + y] value: 3 value: 4");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 7), builtins.oopToInt(result));
}

test "eval block captures environment" {
    const api = testInit();
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var global = env_mod.Env.init(alloc, null);
    defer global.deinit();
    global.define("nil", builtins.getNil());
    global.define("true", builtins.getTrue());
    global.define("false", builtins.getFalse());
    var e = eval_mod.Evaluator.init(api, &global, alloc);

    // x := 10. [:y | x + y] value: 5
    var p = parser_mod.Parser.init("x := 10. [:y | x + y] value: 5", alloc);
    const node = try p.parseTopLevel() orelse unreachable;
    const result = try e.eval(node, &global);
    try std.testing.expectEqual(@as(i64, 15), builtins.oopToInt(result));
}

test "eval ifTrue: with true" {
    const result = try testEval("true ifTrue: [42]");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 42), builtins.oopToInt(result));
}

test "eval ifTrue: with false" {
    const result = try testEval("false ifTrue: [42]");
    try std.testing.expect(result == null); // nil
}

test "eval ifFalse: with false" {
    const result = try testEval("false ifFalse: [42]");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 42), builtins.oopToInt(result));
}

test "eval ifTrue:ifFalse: true branch" {
    const result = try testEval("true ifTrue: ['yes'] ifFalse: ['no']");
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "yes"));
}

test "eval ifTrue:ifFalse: false branch" {
    const result = try testEval("false ifTrue: ['yes'] ifFalse: ['no']");
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "no"));
}

test "eval boolean not" {
    const result1 = try testEval("true not");
    try std.testing.expect(result1 == builtins.getFalse());

    const result2 = try testEval("false not");
    try std.testing.expect(result2 == builtins.getTrue());
}

test "eval boolean and:" {
    const r1 = try testEval("true and: [true]");
    try std.testing.expect(r1 == builtins.getTrue());

    const r2 = try testEval("true and: [false]");
    try std.testing.expect(r2 == builtins.getFalse());

    const r3 = try testEval("false and: [true]");
    try std.testing.expect(r3 == builtins.getFalse());
}

test "eval boolean or:" {
    const r1 = try testEval("false or: [true]");
    try std.testing.expect(r1 == builtins.getTrue());

    const r2 = try testEval("false or: [false]");
    try std.testing.expect(r2 == builtins.getFalse());

    const r3 = try testEval("true or: [false]");
    try std.testing.expect(r3 == builtins.getTrue());
}

test "eval whileTrue: loop" {
    const api = testInit();
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var global = env_mod.Env.init(alloc, null);
    defer global.deinit();
    global.define("nil", builtins.getNil());
    global.define("true", builtins.getTrue());
    global.define("false", builtins.getFalse());
    var e = eval_mod.Evaluator.init(api, &global, alloc);

    // i := 0. [i < 5] whileTrue: [i := i + 1]. i
    var p = parser_mod.Parser.init("i := 0. [i < 5] whileTrue: [i := i + 1]. i", alloc);
    const node = try p.parseTopLevel() orelse unreachable;
    const result = try e.eval(node, &global);
    try std.testing.expectEqual(@as(i64, 5), builtins.oopToInt(result));
}

test "eval block with multiple statements" {
    const result = try testEval("[:x | x + 1. x + 2] value: 10");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 12), builtins.oopToInt(result));
}

test "eval nested blocks" {
    const result = try testEval("[:x | [:y | x + y] value: 3] value: 4");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 7), builtins.oopToInt(result));
}

test "eval block as object" {
    const api = testInit();
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var global = env_mod.Env.init(alloc, null);
    defer global.deinit();
    global.define("nil", builtins.getNil());
    global.define("true", builtins.getTrue());
    global.define("false", builtins.getFalse());
    var e = eval_mod.Evaluator.init(api, &global, alloc);

    // b := [:x | x + 1]. b value: 10
    var p = parser_mod.Parser.init("b := [:x | x + 1]. b value: 10", alloc);
    const node = try p.parseTopLevel() orelse unreachable;
    const result = try e.eval(node, &global);
    try std.testing.expectEqual(@as(i64, 11), builtins.oopToInt(result));
}

test "format block result" {
    const api = testInit();
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var global = env_mod.Env.init(alloc, null);
    defer global.deinit();
    global.define("nil", builtins.getNil());
    global.define("true", builtins.getTrue());
    global.define("false", builtins.getFalse());
    var e = eval_mod.Evaluator.init(api, &global, alloc);

    var p = parser_mod.Parser.init("[42]", alloc);
    const node = try p.parseTopLevel() orelse unreachable;
    const result = try e.eval(node, &global);
    const formatted = e.formatResult(result);
    try std.testing.expect(std.mem.eql(u8, formatted, "a Block"));
}

test "eval comparison returns boolean for ifTrue:" {
    // This tests the integration: SmallInt comparison returns tagged int,
    // but we need booleans for ifTrue:. Let's use the pattern:
    // (3 < 5) ifTrue: [...] won't work because 3 < 5 returns SmallInt 1, not true.
    // Instead, test the boolean path directly.
    const api = testInit();
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var global = env_mod.Env.init(alloc, null);
    defer global.deinit();
    global.define("nil", builtins.getNil());
    global.define("true", builtins.getTrue());
    global.define("false", builtins.getFalse());
    var e = eval_mod.Evaluator.init(api, &global, alloc);

    // (3 < 5) = 1 which is a SmallInt, not boolean true
    // So true ifTrue: [...] is the canonical pattern
    var p = parser_mod.Parser.init("true ifTrue: [1] ifFalse: [0]", alloc);
    const node = try p.parseTopLevel() orelse unreachable;
    const result = try e.eval(node, &global);
    try std.testing.expectEqual(@as(i64, 1), builtins.oopToInt(result));
}

// -- Phase 4 tests: Type Definitions and Methods ----------------------------

fn testEvalMulti(source: []const u8) !struct { result: ?*anyopaque, evaluator: *eval_mod.Evaluator, global: *env_mod.Env } {
    const api = testInit();
    const alloc = std.heap.page_allocator;

    const global = alloc.create(env_mod.Env) catch unreachable;
    global.* = env_mod.Env.init(alloc, null);
    global.define("nil", builtins.getNil());
    global.define("true", builtins.getTrue());
    global.define("false", builtins.getFalse());

    const e = alloc.create(eval_mod.Evaluator) catch unreachable;
    e.* = eval_mod.Evaluator.init(api, global, alloc);
    eval_mod.setGlobalEvaluator(e);

    var p = parser_mod.Parser.init(source, alloc);
    const node = try p.parseTopLevel() orelse return .{ .result = null, .evaluator = e, .global = global };
    const result = try e.eval(node, global);
    return .{ .result = result, .evaluator = e, .global = global };
}

test "parse type definition" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("Point : Object ( x y )", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .type_def);
    try std.testing.expect(std.mem.eql(u8, node.?.data.type_def.name, "Point"));
    try std.testing.expect(std.mem.eql(u8, node.?.data.type_def.parent, "Object"));
    try std.testing.expectEqual(@as(usize, 2), node.?.data.type_def.slots.len);
    try std.testing.expect(std.mem.eql(u8, node.?.data.type_def.slots[0], "x"));
    try std.testing.expect(std.mem.eql(u8, node.?.data.type_def.slots[1], "y"));
}

test "parse unary method definition" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("Point x [^x]", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .method_def);
    try std.testing.expect(std.mem.eql(u8, node.?.data.method_def.type_name, "Point"));
    try std.testing.expect(std.mem.eql(u8, node.?.data.method_def.selector, "x"));
    try std.testing.expectEqual(@as(usize, 0), node.?.data.method_def.params.len);
}

test "parse keyword method definition" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("Point x: v [x := v]", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .method_def);
    try std.testing.expect(std.mem.eql(u8, node.?.data.method_def.type_name, "Point"));
    try std.testing.expect(std.mem.eql(u8, node.?.data.method_def.selector, "x:"));
    try std.testing.expectEqual(@as(usize, 1), node.?.data.method_def.params.len);
    try std.testing.expect(std.mem.eql(u8, node.?.data.method_def.params[0], "v"));
}

test "parse binary method definition" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("Point + other [^x + other x]", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .method_def);
    try std.testing.expect(std.mem.eql(u8, node.?.data.method_def.selector, "+"));
    try std.testing.expectEqual(@as(usize, 1), node.?.data.method_def.params.len);
    try std.testing.expect(std.mem.eql(u8, node.?.data.method_def.params[0], "other"));
}

test "eval type definition creates vtable" {
    const s = try testEvalMulti("Point : Object ( x y )");
    try std.testing.expect(s.result != null);
    // Result is the vtable
    try std.testing.expect(builtins.lookupType("Point") != null);
}

test "eval type new creates instance" {
    const s = try testEvalMulti("Point : Object ( x y ). Point new");
    try std.testing.expect(s.result != null);
    // Should be an instance of Point
    const formatted = s.evaluator.formatResult(s.result);
    try std.testing.expect(std.mem.eql(u8, formatted, "a Point"));
}

test "eval method definition and call" {
    const source =
        \\Point : Object ( x y ).
        \\Point x [^x].
        \\Point x: v [x := v].
        \\Point y [^y].
        \\Point y: v [y := v].
        \\p := Point new.
        \\p x: 10.
        \\p x
    ;
    const s = try testEvalMulti(source);
    try std.testing.expect(s.result != null);
    try std.testing.expectEqual(@as(i64, 10), builtins.oopToInt(s.result));
}

test "eval method with multiple slots" {
    const source =
        \\Point : Object ( x y ).
        \\Point x [^x].
        \\Point x: v [x := v].
        \\Point y [^y].
        \\Point y: v [y := v].
        \\p := Point new.
        \\p x: 3.
        \\p y: 4.
        \\(p x) + (p y)
    ;
    const s = try testEvalMulti(source);
    try std.testing.expect(s.result != null);
    try std.testing.expectEqual(@as(i64, 7), builtins.oopToInt(s.result));
}

test "eval binary method on type" {
    const source =
        \\Counter : Object ( count ).
        \\Counter count [^count].
        \\Counter count: v [count := v].
        \\Counter + other [^(count + (other count))].
        \\a := Counter new. a count: 3.
        \\b := Counter new. b count: 4.
        \\a + b
    ;
    const s = try testEvalMulti(source);
    try std.testing.expect(s.result != null);
    try std.testing.expectEqual(@as(i64, 7), builtins.oopToInt(s.result));
}

test "eval type no slots" {
    const source =
        \\Marker : Object ().
        \\Marker tag [^42].
        \\Marker new tag
    ;
    const s = try testEvalMulti(source);
    try std.testing.expect(s.result != null);
    try std.testing.expectEqual(@as(i64, 42), builtins.oopToInt(s.result));
}

test "eval non-local return from method" {
    const source =
        \\Box : Object ( value ).
        \\Box value [^value].
        \\Box value: v [value := v].
        \\Box earlyReturn [^42. ^99].
        \\b := Box new. b earlyReturn
    ;
    const s = try testEvalMulti(source);
    try std.testing.expect(s.result != null);
    try std.testing.expectEqual(@as(i64, 42), builtins.oopToInt(s.result));
}

test "eval self in method" {
    const source =
        \\Box : Object ( value ).
        \\Box value [^value].
        \\Box value: v [value := v].
        \\Box getSelf [^self].
        \\b := Box new. b value: 5.
        \\(b getSelf) value
    ;
    const s = try testEvalMulti(source);
    try std.testing.expect(s.result != null);
    try std.testing.expectEqual(@as(i64, 5), builtins.oopToInt(s.result));
}

test "eval keyword method on type" {
    const source =
        \\Pair : Object ( first second ).
        \\Pair first [^first].
        \\Pair second [^second].
        \\Pair first: a second: b [first := a. second := b].
        \\p := Pair new. p first: 10 second: 20.
        \\(p first) + (p second)
    ;
    const s = try testEvalMulti(source);
    try std.testing.expect(s.result != null);
    try std.testing.expectEqual(@as(i64, 30), builtins.oopToInt(s.result));
}

// -- Phase 5 tests: Advanced Features ---------------------------------------

test "parse character literal $a" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("$a", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .char_lit);
    try std.testing.expectEqual(@as(u8, 'a'), node.?.data.char_lit);
}

test "parse character literal $\\n" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("$\\n", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .char_lit);
    try std.testing.expectEqual(@as(u8, '\n'), node.?.data.char_lit);
}

test "eval character literal as integer" {
    const result = try testEval("$a");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 97), builtins.oopToInt(result));
}

test "eval character arithmetic" {
    const result = try testEval("$a + 1");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 98), builtins.oopToInt(result)); // 'b'
}

test "parse array literal #(1 2 3)" {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var p = parser_mod.Parser.init("#(1 2 3)", alloc);
    const node = try p.parseTopLevel();
    try std.testing.expect(node != null);
    try std.testing.expect(node.?.tag == .array_lit);
    try std.testing.expectEqual(@as(usize, 3), node.?.data.array_lit.elements.len);
}

test "eval array literal" {
    const result = try testEval("#(1 2 3)");
    try std.testing.expect(result != null);
    try std.testing.expect(builtins.isArray(result));
    try std.testing.expectEqual(@as(usize, 3), builtins.arraySize(result));
}

test "eval array size" {
    const result = try testEval("#(10 20 30) size");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 3), builtins.oopToInt(result));
}

test "eval array at:" {
    const result = try testEval("#(10 20 30) at: 1");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 20), builtins.oopToInt(result));
}

test "eval array at:put:" {
    const api = testInit();
    const alloc = std.heap.page_allocator;
    var global = env_mod.Env.init(alloc, null);
    global.define("nil", builtins.getNil());
    global.define("true", builtins.getTrue());
    global.define("false", builtins.getFalse());
    var e = eval_mod.Evaluator.init(api, &global, alloc);
    eval_mod.setGlobalEvaluator(&e);

    var p = parser_mod.Parser.init("a := #(1 2 3). a at: 0 put: 99. a at: 0", alloc);
    const node = try p.parseTopLevel() orelse unreachable;
    const result = try e.eval(node, &global);
    try std.testing.expectEqual(@as(i64, 99), builtins.oopToInt(result));
}

test "eval empty array" {
    const result = try testEval("#()");
    try std.testing.expect(result != null);
    try std.testing.expect(builtins.isArray(result));
    try std.testing.expectEqual(@as(usize, 0), builtins.arraySize(result));
}

test "eval array do:" {
    const s = try testEvalMulti("sum := 0. #(1 2 3) do: [:x | sum := sum + x]. sum");
    try std.testing.expect(s.result != null);
    try std.testing.expectEqual(@as(i64, 6), builtins.oopToInt(s.result));
}

test "eval array collect:" {
    const s = try testEvalMulti("#(1 2 3) collect: [:x | x * 2]");
    try std.testing.expect(s.result != null);
    try std.testing.expect(builtins.isArray(s.result));
    try std.testing.expectEqual(@as(usize, 3), builtins.arraySize(s.result));
    try std.testing.expectEqual(@as(i64, 2), builtins.oopToInt(builtins.arrayAt(s.result, 0)));
    try std.testing.expectEqual(@as(i64, 4), builtins.oopToInt(builtins.arrayAt(s.result, 1)));
    try std.testing.expectEqual(@as(i64, 6), builtins.oopToInt(builtins.arrayAt(s.result, 2)));
}

test "eval array inject:into:" {
    const s = try testEvalMulti("#(1 2 3 4) inject: 0 into: [:acc :x | acc + x]");
    try std.testing.expect(s.result != null);
    try std.testing.expectEqual(@as(i64, 10), builtins.oopToInt(s.result));
}

test "eval timesRepeat:" {
    const s = try testEvalMulti("sum := 0. 5 timesRepeat: [:i | sum := sum + i]. sum");
    try std.testing.expect(s.result != null);
    try std.testing.expectEqual(@as(i64, 15), builtins.oopToInt(s.result)); // 1+2+3+4+5
}

test "eval cascade" {
    const s = try testEvalMulti(
        \\Box : Object ( value ).
        \\Box value [^value].
        \\Box value: v [value := v].
        \\Box inc [value := value + 1].
        \\b := Box new. b value: 0.
        \\b inc; inc; inc.
        \\b value
    );
    try std.testing.expect(s.result != null);
    try std.testing.expectEqual(@as(i64, 3), builtins.oopToInt(s.result));
}

test "eval global assignment" {
    const s = try testEvalMulti("Foo := 42. Foo + 1");
    try std.testing.expect(s.result != null);
    try std.testing.expectEqual(@as(i64, 43), builtins.oopToInt(s.result));
}

test "format array result" {
    const api = testInit();
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    var global = env_mod.Env.init(alloc, null);
    global.define("nil", builtins.getNil());
    global.define("true", builtins.getTrue());
    global.define("false", builtins.getFalse());
    var e = eval_mod.Evaluator.init(api, &global, alloc);

    const arr = builtins.makeArray(&.{ builtins.intToOop(1), builtins.intToOop(2), builtins.intToOop(3) });
    const formatted = e.formatResult(arr);
    try std.testing.expect(std.mem.eql(u8, formatted, "#(1 2 3)"));
}

test "eval array with strings" {
    const s = try testEvalMulti("#('hello' 'world') size");
    try std.testing.expect(s.result != null);
    try std.testing.expectEqual(@as(i64, 2), builtins.oopToInt(s.result));
}

// -- Phase 6 tests: Polish --------------------------------------------------

test "eval yourself" {
    const result = try testEval("42 yourself");
    try std.testing.expect(result != null);
    try std.testing.expectEqual(@as(i64, 42), builtins.oopToInt(result));
}

test "eval yourself on string" {
    const result = try testEval("'hello' yourself");
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "hello"));
}

test "eval isNil on nil" {
    const result = try testEval("nil isNil");
    try std.testing.expect(result == builtins.getTrue());
}

test "eval isNil on non-nil" {
    const result = try testEval("42 isNil");
    try std.testing.expect(result == builtins.getFalse());
}

test "eval identity == true" {
    const s = try testEvalMulti("x := 'hello'. x == x");
    try std.testing.expect(s.result == builtins.getTrue());
}

test "eval identity == false" {
    const result = try testEval("'hello' == 'hello'");
    // Two different string objects — identity should be false
    try std.testing.expect(result == builtins.getFalse());
}

test "eval identity ~~ true" {
    const result = try testEval("'hello' ~~ 'hello'");
    try std.testing.expect(result == builtins.getTrue());
}

test "eval identity ~~ false" {
    const s = try testEvalMulti("x := 42. x ~~ x");
    try std.testing.expect(s.result == builtins.getFalse());
}

test "eval class on SmallInt" {
    const result = try testEval("42 class");
    try std.testing.expect(result != null);
    try std.testing.expect(result == builtins.getSmallIntVt());
}

test "eval class on string" {
    const result = try testEval("'hello' class");
    try std.testing.expect(result != null);
    try std.testing.expect(result == builtins.getStringVt());
}

test "eval class on nil" {
    const result = try testEval("nil class");
    try std.testing.expect(result != null);
    try std.testing.expect(result == builtins.getNilVt());
}

test "eval printString on integer" {
    const result = try testEval("42 printString");
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "42"));
}

test "eval printString on string" {
    const result = try testEval("'hello' printString");
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "hello"));
}

test "eval printString on nil" {
    const result = try testEval("nil printString");
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "nil"));
}

test "eval printString on boolean" {
    const result = try testEval("true printString");
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "true"));
}

test "eval printString on block" {
    const result = try testEval("[42] printString");
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "a Block"));
}

test "eval printString on array" {
    const result = try testEval("#(1 2 3) printString");
    try std.testing.expect(builtins.isString(result));
    const s = builtins.stringSlice(result).?;
    try std.testing.expect(std.mem.eql(u8, s, "#(1 2 3)"));
}

test "eval printString on type instance" {
    const s = try testEvalMulti("Box : Object ( val ). Box new printString");
    try std.testing.expect(builtins.isString(s.result));
    const str = builtins.stringSlice(s.result).?;
    try std.testing.expect(std.mem.eql(u8, str, "a Box"));
}

test "eval doesNotUnderstand: hook" {
    const s = try testEvalMulti(
        \\Proxy : Object ( log ).
        \\Proxy log [^log].
        \\Proxy log: v [log := v].
        \\Proxy doesNotUnderstand: sel [log := sel. ^42].
        \\p := Proxy new. p log: 'none'.
        \\p bogusMessage.
        \\p log
    );
    try std.testing.expect(builtins.isString(s.result));
    const str = builtins.stringSlice(s.result).?;
    try std.testing.expect(std.mem.eql(u8, str, "bogusMessage"));
}

test "parser source position tracking" {
    const source = "hello\nworld\n  x";
    const pos1 = parser_mod.Parser.posToLineCol(source, 0);
    try std.testing.expectEqual(@as(usize, 1), pos1.line);
    try std.testing.expectEqual(@as(usize, 1), pos1.col);

    const pos2 = parser_mod.Parser.posToLineCol(source, 6);
    try std.testing.expectEqual(@as(usize, 2), pos2.line);
    try std.testing.expectEqual(@as(usize, 1), pos2.col);

    const pos3 = parser_mod.Parser.posToLineCol(source, 14);
    try std.testing.expectEqual(@as(usize, 3), pos3.line);
    try std.testing.expectEqual(@as(usize, 3), pos3.col);
}
