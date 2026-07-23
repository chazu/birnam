//! Deep-input and long-sequence robustness for the explicit Rust PEG matcher.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;
const depth = 4096;
const long_input_len = 100_000;

fn send(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8, args: []const Oop) Oop {
    return api.send(receiver, api.intern(selector), args.ptr, args.len);
}

fn string(api: *idapi.IdApi, bytes: []const u8) Oop {
    return api.mkString(bytes.ptr, bytes.len);
}

fn loadRustPeg(api: *idapi.IdApi) !void {
    const path = std.posix.getenv("BIRNAM_RUST_PEG_DYLIB") orelse
        return error.MissingRustPegDylib;
    const path_z = try std.heap.c_allocator.dupeZ(u8, path);
    defer std.heap.c_allocator.free(path_z);
    const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustPegDlopenFailed;
    const raw_init = api.dlsym(handle, "module_init") orelse return error.RustPegModuleInitMissing;
    const init: ModuleInit = @ptrCast(@alignCast(raw_init));
    _ = init(api, null);
}

test "Rust PEG handles deep rule chains and long repetition without stack overflow" {
    const api = floor.api();
    try loadRustPeg(api);

    const Peg = api.import("Peg") orelse return error.PegNotExported;
    const grammar = send(api, Peg, "newGrammar", &.{});
    const literal = send(api, Peg, "literal:", &.{string(api, "x")});

    var previous = api.intern("deep0");
    _ = send(api, grammar, "rule:is:", &.{ previous, literal });
    var name_buffer: [32]u8 = undefined;
    for (1..depth) |index| {
        const name = std.fmt.bufPrintZ(&name_buffer, "deep{d}", .{index}) catch unreachable;
        const current = api.intern(name.ptr);
        const reference = send(api, Peg, "ruleRef:", &.{previous});
        _ = send(api, grammar, "rule:is:", &.{ current, reference });
        previous = current;
    }

    var timer = try std.time.Timer.start();
    const root = send(api, grammar, "parse:from:", &.{ string(api, "x"), previous });
    const deep_elapsed = timer.read();
    const cst_vt = api.import("PegCstVt") orelse return error.CstVtNotExported;
    try std.testing.expectEqual(cst_vt, api.vtableOf(root));
    try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(send(api, root, "end", &.{})));

    var chain_len: usize = 1;
    var node = root;
    while (api.smallIntValue(send(api, node, "childCount", &.{})) != 0) {
        try std.testing.expectEqual(@as(i64, 1), api.smallIntValue(send(api, node, "childCount", &.{})));
        node = send(api, node, "childAt:", &.{api.mkSmallInt(0)});
        chain_len += 1;
    }
    try std.testing.expectEqual(depth, chain_len);

    const source = try std.testing.allocator.alloc(u8, long_input_len);
    defer std.testing.allocator.free(source);
    @memset(source, 'x');
    const any = send(api, Peg, "anyByte", &.{});
    const repeated = send(api, any, "plus", &.{});
    const long_rule = api.intern("long");
    _ = send(api, grammar, "rule:is:", &.{ long_rule, repeated });
    timer.reset();
    const long_root = send(api, grammar, "parse:from:", &.{ string(api, source), long_rule });
    const long_elapsed = timer.read();
    try std.testing.expectEqual(cst_vt, api.vtableOf(long_root));
    try std.testing.expectEqual(@as(i64, long_input_len), api.smallIntValue(send(api, long_root, "end", &.{})));

    std.debug.print(
        "rust-peg-stress depth={d} long_bytes={d} deep_ns={d} long_ns={d}\n",
        .{ chain_len, long_input_len, deep_elapsed, long_elapsed },
    );
    try std.testing.expect(deep_elapsed < 5 * std.time.ns_per_s);
    try std.testing.expect(long_elapsed < 5 * std.time.ns_per_s);
}
