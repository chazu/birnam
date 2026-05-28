const std = @import("std");
const nodes = @import("nodes.zig");
const engine = @import("engine.zig");
const bootstrap_parser = @import("bootstrap.zig");
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

fn log(msg: [*:0]const u8) void {
    shell_log(msg);
}

// Module entry point, called by the shell.
// Not exported during testing to avoid collision with id module's export.
comptime {
    if (!@import("builtin").is_test) {
        @export(&birnam_module_init, .{ .name = "birnam_module_init" });
    }
}
fn birnam_module_init(ctx: *ShellContext) callconv(.c) void {
    shell_log = ctx.log;
    log("peg: initializing...");

    const api_ptr = ctx.import_value("id_api");
    if (api_ptr == null) {
        log("peg: ERROR - id_api not found (is id module loaded first?)");
        return;
    }

    id_api = @ptrCast(@alignCast(api_ptr));
    nodes.init(id_api);
    log("peg: node vtables created");
    log("peg: ready");
}

// -- Tests ------------------------------------------------------------------

// Tests use the "id" import added by build.zig to access bootstrap/api directly.
// id.zig re-exports the needed sub-modules.

fn testInit() *const IdApi {
    const id = @import("id");
    _ = id.bootstrap.init();
    id.api.populate();
    // Cast through anyopaque to bridge the two identical-layout IdApi struct types
    // (peg's local definition vs id's definition)
    const ptr: *const IdApi = @ptrCast(&id.api.id_api);
    nodes.init(ptr);
    return ptr;
}

test "bootstrap parser: simple literal grammar" {
    const api = testInit();

    const src = "start = 'hello'";
    var parser = bootstrap_parser.PegParser.init(src, api);
    const grammar = parser.parseGrammar();
    try std.testing.expect(grammar != null);

    const g = nodes.asGrammar(grammar).?;
    try std.testing.expect(g.rule_count == 1);
    try std.testing.expect(g.start_rule != null);
}

test "bootstrap parser: arithmetic grammar" {
    const api = testInit();

    const src =
        \\expr   = term ('+' term)*
        \\term   = factor ('*' factor)*
        \\factor = [0-9]+ / '(' expr ')'
    ;
    var parser = bootstrap_parser.PegParser.init(src, api);
    const grammar = parser.parseGrammar();
    try std.testing.expect(grammar != null);

    const g = nodes.asGrammar(grammar).?;
    try std.testing.expect(g.rule_count == 3);
}

test "engine: match simple literal" {
    const api = testInit();

    const src = "start = 'hello'";
    var parser = bootstrap_parser.PegParser.init(src, api);
    const grammar = parser.parseGrammar();
    try std.testing.expect(grammar != null);

    var eng = engine.Parser.init("hello", grammar, api);
    try std.testing.expect(eng.parse());
    try std.testing.expect(eng.pos == 5);
}

test "engine: literal mismatch" {
    const api = testInit();

    const src = "start = 'hello'";
    var parser = bootstrap_parser.PegParser.init(src, api);
    const grammar = parser.parseGrammar();

    var eng = engine.Parser.init("world", grammar, api);
    try std.testing.expect(!eng.parse());
}

test "engine: alternation" {
    const api = testInit();

    const src = "start = 'foo' / 'bar'";
    var parser = bootstrap_parser.PegParser.init(src, api);
    const grammar = parser.parseGrammar();

    var eng1 = engine.Parser.init("foo", grammar, api);
    try std.testing.expect(eng1.parse());

    var eng2 = engine.Parser.init("bar", grammar, api);
    try std.testing.expect(eng2.parse());

    var eng3 = engine.Parser.init("baz", grammar, api);
    try std.testing.expect(!eng3.parse());
}

test "engine: star repetition" {
    const api = testInit();

    const src = "start = 'a'*";
    var parser = bootstrap_parser.PegParser.init(src, api);
    const grammar = parser.parseGrammar();

    var eng1 = engine.Parser.init("", grammar, api);
    try std.testing.expect(eng1.parse());
    try std.testing.expect(eng1.pos == 0);

    var eng2 = engine.Parser.init("aaab", grammar, api);
    try std.testing.expect(eng2.parse());
    try std.testing.expect(eng2.pos == 3);
}

test "engine: plus repetition" {
    const api = testInit();

    const src = "start = 'a'+";
    var parser = bootstrap_parser.PegParser.init(src, api);
    const grammar = parser.parseGrammar();

    var eng1 = engine.Parser.init("", grammar, api);
    try std.testing.expect(!eng1.parse());

    var eng2 = engine.Parser.init("aaa", grammar, api);
    try std.testing.expect(eng2.parse());
    try std.testing.expect(eng2.pos == 3);
}

test "engine: query (optional)" {
    const api = testInit();

    const src = "start = 'a'?";
    var parser = bootstrap_parser.PegParser.init(src, api);
    const grammar = parser.parseGrammar();

    var eng1 = engine.Parser.init("", grammar, api);
    try std.testing.expect(eng1.parse());
    try std.testing.expect(eng1.pos == 0);

    var eng2 = engine.Parser.init("a", grammar, api);
    try std.testing.expect(eng2.parse());
    try std.testing.expect(eng2.pos == 1);
}

test "engine: peek-for (no consumption)" {
    const api = testInit();

    const src = "start = &'a' .";
    var parser = bootstrap_parser.PegParser.init(src, api);
    const grammar = parser.parseGrammar();

    var eng1 = engine.Parser.init("a", grammar, api);
    try std.testing.expect(eng1.parse());
    try std.testing.expect(eng1.pos == 1);

    var eng2 = engine.Parser.init("b", grammar, api);
    try std.testing.expect(!eng2.parse());
}

test "engine: peek-not (no consumption)" {
    const api = testInit();

    const src = "start = !'a' .";
    var parser = bootstrap_parser.PegParser.init(src, api);
    const grammar = parser.parseGrammar();

    var eng1 = engine.Parser.init("b", grammar, api);
    try std.testing.expect(eng1.parse());
    try std.testing.expect(eng1.pos == 1);

    var eng2 = engine.Parser.init("a", grammar, api);
    try std.testing.expect(!eng2.parse());
}

test "engine: character class" {
    const api = testInit();

    const src = "start = [a-z]+";
    var parser = bootstrap_parser.PegParser.init(src, api);
    const grammar = parser.parseGrammar();

    var eng = engine.Parser.init("hello123", grammar, api);
    try std.testing.expect(eng.parse());
    try std.testing.expect(eng.pos == 5);
}

test "engine: dot matches any" {
    const api = testInit();

    const src = "start = .+";
    var parser = bootstrap_parser.PegParser.init(src, api);
    const grammar = parser.parseGrammar();

    var eng = engine.Parser.init("xyz", grammar, api);
    try std.testing.expect(eng.parse());
    try std.testing.expect(eng.pos == 3);
}

test "engine: multi-rule grammar (arithmetic)" {
    const api = testInit();

    const src =
        \\expr   = term ('+' term)*
        \\term   = factor ('*' factor)*
        \\factor = [0-9]+
    ;
    var parser = bootstrap_parser.PegParser.init(src, api);
    const grammar = parser.parseGrammar();
    try std.testing.expect(grammar != null);

    const g = nodes.asGrammar(grammar).?;
    try std.testing.expect(g.rule_count == 3);

    // First test: just a single digit (should match via factor directly)
    var eng2 = engine.Parser.init("42", grammar, api);
    const r2 = eng2.parse();
    try std.testing.expect(r2);
    try std.testing.expect(eng2.pos == 2);

    // Now test with operators
    var eng1 = engine.Parser.init("1+2*3", grammar, api);
    try std.testing.expect(eng1.parse());
    try std.testing.expect(eng1.pos == 5);
}

test "engine: capture" {
    const api = testInit();

    const src = "start = <[a-z]+>";
    var parser = bootstrap_parser.PegParser.init(src, api);
    const grammar = parser.parseGrammar();

    var eng = engine.Parser.init("hello123", grammar, api);
    try std.testing.expect(eng.parse());
    try std.testing.expect(eng.capture_count == 1);
    try std.testing.expect(eng.captures[0].begin == 0);
    try std.testing.expect(eng.captures[0].end == 5);
}
