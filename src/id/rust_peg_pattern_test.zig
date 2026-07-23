//! Cross-language smoke for the Rust PEG pattern representation.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;
const Identity = *const fn () callconv(.c) [*:0]const u8;

fn send(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8, args: []const Oop) Oop {
    return api.send(receiver, api.intern(selector), args.ptr, args.len);
}

fn string(api: *idapi.IdApi, bytes: []const u8) Oop {
    return api.mkString(bytes.ptr, bytes.len);
}

fn expectPattern(api: *idapi.IdApi, pattern: Oop, pattern_vt: Oop) !void {
    try std.testing.expect(pattern != null);
    try std.testing.expectEqual(pattern_vt, api.vtableOf(pattern));
}

test "Rust PEG patterns and grammars expose the frozen construction protocol" {
    const api = floor.api();
    const path = std.posix.getenv("BIRNAM_RUST_PEG_DYLIB") orelse
        return error.MissingRustPegDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);

    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustPegDlopenFailed;
    const raw_identity = api.dlsym(handle, "birnam_peg_rust_identity") orelse
        return error.RustPegIdentityMissing;
    const identity: Identity = @ptrCast(@alignCast(raw_identity));
    try std.testing.expectEqualStrings("rust-peg-n3.3-matcher", std.mem.span(identity()));

    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustPegModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);

    const peg = api.import("Peg") orelse return error.RustPegNotExported;
    const pattern_vt = api.import("PegPatternVt") orelse return error.RustPegPatternVtNotExported;
    try std.testing.expect(api.import("PegFacadeVt") != null);
    try std.testing.expect(api.import("PegGrammarVt") != null);

    const literal = send(api, peg, "literal:", &.{string(api, "abc")});
    try expectPattern(api, literal, pattern_vt);
    try expectPattern(api, send(api, literal, "star", &.{}), pattern_vt);
    try expectPattern(api, send(api, literal, "plus", &.{}), pattern_vt);
    try expectPattern(api, send(api, literal, "opt", &.{}), pattern_vt);
    try expectPattern(api, send(api, literal, "negate", &.{}), pattern_vt);
    try expectPattern(api, send(api, literal, "lookahead", &.{}), pattern_vt);

    const sequence = send(api, peg, "seq", &.{});
    try expectPattern(api, sequence, pattern_vt);
    try std.testing.expectEqual(sequence, send(api, sequence, "add:", &.{literal}));

    const choice = send(api, peg, "choice", &.{});
    try expectPattern(api, choice, pattern_vt);
    try std.testing.expectEqual(choice, send(api, choice, "add:", &.{literal}));

    const charset = send(api, peg, "charset", &.{});
    try expectPattern(api, charset, pattern_vt);
    try std.testing.expectEqual(
        charset,
        send(api, charset, "from:to:", &.{ api.mkSmallInt('A'), api.mkSmallInt('Z') }),
    );

    const rule = send(api, peg, "ruleRef:", &.{api.intern("start")});
    try expectPattern(api, rule, pattern_vt);

    const grammar_vt = api.import("PegGrammarVt") orelse return error.RustPegGrammarVtNotExported;
    const grammar = send(api, peg, "newGrammar", &.{});
    try std.testing.expectEqual(grammar_vt, api.vtableOf(grammar));
    try std.testing.expectEqual(grammar, send(api, grammar, "rule:is:", &.{ api.intern("start"), literal }));
    try std.testing.expectEqual(grammar, send(api, grammar, "rule:is:", &.{ string(api, "tail"), rule }));
    try std.testing.expectEqual(@as(i64, 0), api.smallIntValue(send(api, grammar, "furthestFail", &.{})));
    try std.testing.expectEqual(grammar_vt, api.vtableOf(send(api, peg, "grammar", &.{})));

    const cst_vt = api.import("PegCstVt") orelse return error.RustPegCstVtNotExported;
    const root = send(api, grammar, "parse:from:", &.{ string(api, "abc"), api.intern("start") });
    try std.testing.expectEqual(cst_vt, api.vtableOf(root));
    var name_len: usize = 0;
    const name_bytes = api.stringBytes(send(api, root, "ruleName", &.{}), &name_len);
    try std.testing.expectEqualStrings("start", name_bytes[0..name_len]);
    try std.testing.expectEqual(@as(i64, 0), api.smallIntValue(send(api, root, "start", &.{})));
    try std.testing.expectEqual(@as(i64, 3), api.smallIntValue(send(api, root, "end", &.{})));
    try std.testing.expectEqual(@as(i64, 0), api.smallIntValue(send(api, root, "childCount", &.{})));

    const ab = send(api, peg, "literal:", &.{string(api, "ab")});
    const a = send(api, peg, "literal:", &.{string(api, "a")});
    const c = send(api, peg, "literal:", &.{string(api, "c")});
    const alternatives = send(api, peg, "choice", &.{});
    _ = send(api, alternatives, "add:", &.{ab});
    _ = send(api, alternatives, "add:", &.{a});
    const backtracking = send(api, peg, "seq", &.{});
    _ = send(api, backtracking, "add:", &.{alternatives});
    _ = send(api, backtracking, "add:", &.{c});
    _ = send(api, grammar, "rule:is:", &.{ api.intern("backtracking"), backtracking });
    const backtracked = send(api, grammar, "parse:from:", &.{ string(api, "ac"), api.intern("backtracking") });
    try std.testing.expectEqual(cst_vt, api.vtableOf(backtracked));
    try std.testing.expectEqual(@as(i64, 2), api.smallIntValue(send(api, backtracked, "end", &.{})));

    const lookahead = send(api, peg, "seq", &.{});
    _ = send(api, lookahead, "add:", &.{send(api, a, "lookahead", &.{} )});
    _ = send(api, lookahead, "add:", &.{a});
    _ = send(api, grammar, "rule:is:", &.{ api.intern("lookahead"), lookahead });
    const looked = send(api, grammar, "parse:from:", &.{ string(api, "a"), api.intern("lookahead") });
    try std.testing.expectEqual(cst_vt, api.vtableOf(looked));

    const digit = send(api, peg, "literal:", &.{string(api, "a")});
    const nested = send(api, peg, "ruleRef:", &.{api.intern("digit")});
    _ = send(api, grammar, "rule:is:", &.{ api.intern("digit"), digit });
    _ = send(api, grammar, "rule:is:", &.{ api.intern("nested"), nested });
    const nested_root = send(api, grammar, "parse:from:", &.{ string(api, "a"), api.intern("nested") });
    try std.testing.expectEqual(cst_vt, api.vtableOf(nested_root));
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(send(api, nested_root, "childCount", &.{})));
    const nested_child = send(api, nested_root, "childAt:", &.{api.mkSmallInt(0)});
    try std.testing.expectEqual(cst_vt, api.vtableOf(nested_child));
    var child_name_len: usize = 0;
    const child_name = api.stringBytes(send(api, nested_child, "ruleName", &.{}), &child_name_len);
    try std.testing.expectEqualStrings("digit", child_name[0..child_name_len]);

    const failed = send(api, grammar, "parse:from:", &.{ string(api, "ax"), api.intern("start") });
    try std.testing.expect(api.vtableOf(failed) != cst_vt);
    try std.testing.expectEqual(@as(i64, 0), api.smallIntValue(send(api, grammar, "furthestFail", &.{})));

    const missing_rule = send(api, peg, "ruleRef:", &.{api.intern("missing")});
    _ = send(api, grammar, "rule:is:", &.{ api.intern("undefined"), missing_rule });
    const undefined_result = send(api, grammar, "parse:from:", &.{ string(api, ""), api.intern("undefined") });
    try std.testing.expect(api.vtableOf(undefined_result) != cst_vt);

    const recursive = send(api, peg, "ruleRef:", &.{api.intern("recursive")});
    _ = send(api, grammar, "rule:is:", &.{ api.intern("recursive"), recursive });
    const recursive_result = send(api, grammar, "parse:from:", &.{ string(api, ""), api.intern("recursive") });
    try std.testing.expect(api.vtableOf(recursive_result) != cst_vt);
}
