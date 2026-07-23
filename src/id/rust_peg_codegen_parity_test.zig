//! Generated-C parity corpus for the unchanged Zig Codegen rung.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const artifact_inspector = @import("artifact_inspector.zig");
const store = @import("store.zig");
const peg = @import("peg.zig");
const lang = @import("lang.zig");
const tcc = @import("tcc.zig");
const codegen = @import("codegen.zig");
const prelude = @import("prelude.zig");
const collection = @import("collection.zig");
const err_mod = @import("error.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;

const sources = [_][]const u8{
    "42",
    "[1 + 2]",
    "let x := 10; [x + 2]",
    "[[1 to: 3] inject: 0 into: [:sum :n | [sum + n]]]",
    "#[1 2 3]",
    "{\"a\" -> 1, \"b\" -> 2}",
    "[:n | [n + 1]]",
    "// comment\nlet x := 1;\n[x + 2]",
};

fn bringPeg(api: *idapi.IdApi) !void {
    if (std.posix.getenv("BIRNAM_RUST_PEG_DYLIB")) |path| {
        const path_z = try std.heap.c_allocator.dupeZ(u8, path);
        defer std.heap.c_allocator.free(path_z);
        const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustPegDlopenFailed;
        const raw_init = api.dlsym(handle, "module_init") orelse return error.RustPegModuleInitMissing;
        const init: ModuleInit = @ptrCast(@alignCast(raw_init));
        _ = init(api, null);
    } else {
        peg.bringUp(api);
    }
}

fn setup() *idapi.IdApi {
    const api = floor.api();
    artifact_inspector.bringUp(api);
    store.bringUp(api);
    bringPeg(api) catch @panic("Rust PEG failed to load");
    lang.bringUp(api);
    tcc.bringUp(api);
    codegen.bringUp(api);
    prelude.bringUp(api);
    collection.bringUp(api);
    err_mod.bringUp(api);
    return api;
}

fn emitDigest(api: *idapi.IdApi) ![std.crypto.hash.sha2.Sha256.digest_length]u8 {
    var hash = std.crypto.hash.sha2.Sha256.init(.{});
    for (sources) |source| {
        hash.update(source);
        hash.update(&.{0});
        var emitted = codegen.emitProgramForTesting(api.mkString(source.ptr, source.len), false) orelse
            return error.EmitFailed;
        defer emitted.deinit(std.heap.c_allocator);
        hash.update(emitted.items);
        hash.update(&.{0});
    }
    var digest: [std.crypto.hash.sha2.Sha256.digest_length]u8 = undefined;
    hash.final(&digest);
    return digest;
}

test "Rust PEG preserves generated-C output for the frozen codegen corpus" {
    const api = setup();
    const digest = try emitDigest(api);
    const digest_text = std.fmt.bytesToHex(digest, .lower);
    std.debug.print("rust-peg-codegen-parity digest={s}\n", .{&digest_text});

    const expected_text = std.posix.getenv("BIRNAM_RUST_PEG_CODEGEN_DIGEST") orelse return;
    if (expected_text.len == 0) return;
    try std.testing.expectEqualStrings(expected_text, &digest_text);
}
