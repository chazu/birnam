//! Proof for rung 3c (part 2): the birnam → C emitter.
//!
//! Boots the whole front end (peg → lang → tcc → codegen), then evaluates birnam
//! arithmetic expressions: each is parsed, emitted as Method-ABI C, compiled by
//! libtcc, and run — the answer comes back from native code through the id api.

const std = @import("std");
const id = @import("id.zig");
const peg = @import("peg.zig");
const lang = @import("lang.zig");
const tcc = @import("tcc.zig");
const codegen = @import("codegen.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

fn setup() *idapi.IdApi {
    const api = id.id_init();
    peg.bringUp(api);
    lang.bringUp(api);
    tcc.bringUp(api);
    codegen.bringUp(api);
    return api;
}

fn eval(api: *idapi.IdApi, src: []const u8) Oop {
    const Codegen = api.import("Codegen").?;
    return api.send(Codegen, api.intern("eval:"), &[_]Oop{api.mkString(src.ptr, src.len)}, 1);
}
fn evalInt(api: *idapi.IdApi, src: []const u8) !i64 {
    const r = eval(api, src);
    if (r == api.nilObj) return error.EvalFailed;
    return api.smallIntValue(r);
}

test "codegen 3c.2: birnam arithmetic → C → native → value" {
    const api = setup();

    try std.testing.expectEqual(@as(i64, 42), try evalInt(api, "42"));
    try std.testing.expectEqual(@as(i64, 7), try evalInt(api, "[3 + 4]"));
    try std.testing.expectEqual(@as(i64, 14), try evalInt(api, "[[3 + 4] * 2]"));
    try std.testing.expectEqual(@as(i64, 5), try evalInt(api, "[[10 - 3] - 2]"));
    try std.testing.expectEqual(@as(i64, 25), try evalInt(api, "[100 / 4]"));
    try std.testing.expectEqual(@as(i64, -8), try evalInt(api, "[-10 + 2]"));

    // Deeper nesting recurses correctly.
    try std.testing.expectEqual(@as(i64, 20), try evalInt(api, "[[2 * 3] + [7 * 2]]"));
}

test "codegen 3c.2: out-of-subset input returns nil (no crash)" {
    const api = setup();

    try std.testing.expect(eval(api, "hello") == api.nilObj); // bare ident
    try std.testing.expect(eval(api, "[n printString]") == api.nilObj); // unary send
    try std.testing.expect(eval(api, "[1 to: 5]") == api.nilObj); // keyword send
    try std.testing.expect(eval(api, "@#$") == api.nilObj); // parse failure
}
