//! Frozen CST-shape parity corpus for the explicit Rust PEG backend.
//!
//! The test walks the same unchanged Lang grammar used by the production
//! language rung and hashes every successful rule name, half-open byte span,
//! child count, and child ordering. Failed parses contribute their
//! furthest-failure offset. The expected digest is generated once from the
//! canonical Zig PEG and then enforced when this test is pointed at Rust PEG.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const peg = @import("peg.zig");
const lang = @import("lang.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;

const cases = [_][]const u8{
    "let n := 30",
    "let fizzy := [:n | [[n mod: 3] = 0]]",
    "[System println: [classify value: n]]",
    "[1 to: 20 do: [:n | [System println: n]]]",
    "let primes := [[2 to: n] select: [:k | [isPrime value: k]]]",
    "[[k > 1] & [[hasDivisor value: k] isNil]]",
    "[[\"a\" , \"b\"] printString]",
    "[x ifTrue: [| \"yes\"] ifFalse: [| \"no\"]]",
    "[10 % 3]",
    "[a @ b]",
    "[x ! y]",
    "[2 ^ 3]",
    "[a @% b]",
    "[obj respondsTo: #%]",
    "[obj respondsTo: #@]",
    "[1 + 2]",
    "[a ? b]",
    "// a fizzbuzz fragment\nlet fizzy := [:n | [[n mod: 3] = 0]];\nlet buzzy := [:n | [[n mod: 5] = 0]];\n[System println: [fizzy value: 15]]",
    "let a := 1; let b := 2;",
    "#[]",
    "#[1 2 3]",
    "#[#[1 2] #[3 4]]",
    "#[[1 + 1] x \"s\"]",
    "let v := #[10 20 30]",
    "{}",
    "{\"a\" -> 1, \"b\" -> 2}",
    "{\"a\" -> 1 \"b\" -> 2}",
    "{\"a\" -> 1,}",
    "let m := {1 -> 10, 2 -> 20}",
    "{#x -> 1, #y -> 2}",
    "{#at:put: -> 1, #foo: -> 2}",
    "#[ #[?p #city #Boston] ]",
    "[db findAll: ?p where: #[ #[?p #city #Boston] ]]",
    "?p",
    "let v := ?city",
    "#double",
    "let := 30",
    "[n printString",
    "@#$%",
    "[justAVar]",
    "let n := 30 garbage",
};

const Digest = struct {
    // FNV-1a over an explicit tagged stream. The tags keep adjacent fields
    // unambiguous while remaining independent of pointer identity.
    state: u64 = 0xcbf29ce484222325,

    fn byte(self: *Digest, value: u8) void {
        self.state ^= value;
        self.state *%= 0x100000001b3;
    }

    fn putU64(self: *Digest, value: u64) void {
        var encoded = value;
        for (0..8) |_| {
            self.byte(@truncate(encoded));
            encoded >>= 8;
        }
    }

    fn bytes(self: *Digest, value: []const u8) void {
        self.putU64(value.len);
        for (value) |b| self.byte(b);
    }
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

fn bringLang(api: *idapi.IdApi) !void {
    if (std.posix.getenv("BIRNAM_RUST_LANG_DYLIB")) |path| {
        const path_z = try std.heap.c_allocator.dupeZ(u8, path);
        defer std.heap.c_allocator.free(path_z);
        const handle = api.dlopen(path_z.ptr, 1) orelse return error.RustLangDlopenFailed;
        const raw_init = api.dlsym(handle, "module_init") orelse return error.RustLangModuleInitMissing;
        const init: ModuleInit = @ptrCast(@alignCast(raw_init));
        _ = init(api, null);
    } else {
        lang.bringUp(api);
    }
}

fn send0(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8) Oop {
    return api.send(receiver, api.intern(selector), &.{}, 0);
}

fn send1(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8, arg: Oop) Oop {
    return api.send(receiver, api.intern(selector), &.{arg}, 1);
}

fn walk(api: *idapi.IdApi, node: Oop, digest: *Digest, depth: usize) void {
    if (depth > 4096) @panic("CST parity depth exceeded guard");

    digest.byte('N');
    var name_len: usize = 0;
    const name = api.stringBytes(send0(api, node, "ruleName"), &name_len);
    digest.bytes(name[0..name_len]);
    digest.putU64(@intCast(api.smallIntValue(send0(api, node, "start"))));
    digest.putU64(@intCast(api.smallIntValue(send0(api, node, "end"))));

    const child_count = api.smallIntValue(send0(api, node, "childCount"));
    if (child_count < 0) @panic("CST parity returned negative child count");
    digest.putU64(@intCast(child_count));
    var index: i64 = 0;
    while (index < child_count) : (index += 1) {
        const child = send1(api, node, "childAt:", api.mkSmallInt(index));
        if (child == api.nilObj) @panic("CST parity childAt: returned nil");
        walk(api, child, digest, depth + 1);
    }
}

fn parse(api: *idapi.IdApi, source: []const u8) Oop {
    const lang_obj = api.import("Lang") orelse @panic("Lang was not exported");
    const source_obj = api.mkString(source.ptr, source.len);
    return send1(api, lang_obj, "parse:", source_obj);
}

fn corpusDigest(api: *idapi.IdApi) u64 {
    var digest = Digest{};
    for (cases) |source| {
        digest.byte('C');
        digest.bytes(source);
        const root = parse(api, source);
        if (root == api.nilObj) {
            digest.byte('F');
            const lang_obj = api.import("Lang").?;
            const failure = send0(api, lang_obj, "furthestFail");
            digest.putU64(@intCast(api.smallIntValue(failure)));
        } else {
            digest.byte('S');
            walk(api, root, &digest, 0);
        }
    }
    return digest.state;
}

test "Rust PEG preserves the frozen Lang CST corpus" {
    const api = floor.api();
    try bringPeg(api);
    try bringLang(api);

    const digest = corpusDigest(api);
    std.debug.print("rust-peg-cst-parity digest={x:0>16}\n", .{digest});

    const expected_text = std.posix.getenv("BIRNAM_RUST_PEG_CST_DIGEST") orelse return;
    var expected: u64 = 0;
    for (expected_text) |character| {
        const digit = switch (character) {
            '0'...'9' => character - '0',
            'a'...'f' => character - 'a' + 10,
            'A'...'F' => character - 'A' + 10,
            else => return error.InvalidRustPegCstDigest,
        };
        expected = (expected << 4) | digit;
    }
    try std.testing.expectEqual(expected, digest);
}
