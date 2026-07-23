//! Whole checked-in Birnam source-corpus parity through an explicit PEG dylib.

const std = @import("std");
const floor = @import("floor_adapter_test.zig");
const peg = @import("peg.zig");
const lang = @import("lang.zig");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ModuleInit = *const fn (*idapi.IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;
const roots = [_][]const u8{ "docs", "examples", "modules", "src/id" };

const Digest = struct {
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

fn send0(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8) Oop {
    return api.send(receiver, api.intern(selector), &.{}, 0);
}

fn send1(api: *idapi.IdApi, receiver: Oop, selector: [*:0]const u8, arg: Oop) Oop {
    return api.send(receiver, api.intern(selector), &.{arg}, 1);
}

fn walkCst(api: *idapi.IdApi, node: Oop, digest: *Digest, depth: usize) void {
    if (depth > 4096) @panic("source-corpus CST depth exceeded guard");
    digest.byte('N');
    var name_len: usize = 0;
    const name = api.stringBytes(send0(api, node, "ruleName"), &name_len);
    digest.bytes(name[0..name_len]);
    digest.putU64(@intCast(api.smallIntValue(send0(api, node, "start"))));
    digest.putU64(@intCast(api.smallIntValue(send0(api, node, "end"))));
    const child_count = api.smallIntValue(send0(api, node, "childCount"));
    if (child_count < 0) @panic("source-corpus CST returned negative child count");
    digest.putU64(@intCast(child_count));
    var index: i64 = 0;
    while (index < child_count) : (index += 1) {
        const child = send1(api, node, "childAt:", api.mkSmallInt(index));
        if (child == api.nilObj) @panic("source-corpus CST childAt: returned nil");
        walkCst(api, child, digest, depth + 1);
    }
}

fn collectPaths(allocator: std.mem.Allocator) !std.ArrayList([]u8) {
    var paths: std.ArrayList([]u8) = .{};
    errdefer {
        for (paths.items) |path| allocator.free(path);
        paths.deinit(allocator);
    }

    for (roots) |root| {
        var dir = try std.fs.cwd().openDir(root, .{ .iterate = true });
        var walker = try dir.walk(allocator);
        while (try walker.next()) |entry| {
            if (entry.kind != .file or !std.mem.endsWith(u8, entry.path, ".birnam")) continue;
            const path = try std.fs.path.join(allocator, &.{ root, entry.path });
            try paths.append(allocator, path);
        }
        walker.deinit();
        dir.close();
    }

    std.mem.sort([]u8, paths.items, {}, struct {
        fn lessThan(_: void, left: []u8, right: []u8) bool {
            return std.mem.lessThan(u8, left, right);
        }
    }.lessThan);
    return paths;
}

test "Rust PEG preserves the complete checked-in Birnam source corpus" {
    const api = floor.api();
    try bringPeg(api);
    lang.bringUp(api);
    const lang_obj = api.import("Lang") orelse return error.LangNotExported;

    var paths = try collectPaths(std.testing.allocator);
    defer {
        for (paths.items) |path| std.testing.allocator.free(path);
        paths.deinit(std.testing.allocator);
    }
    try std.testing.expectEqual(@as(usize, 76), paths.items.len);

    var digest = Digest{};
    for (paths.items) |path| {
        const source = try std.fs.cwd().readFileAlloc(std.testing.allocator, path, 8 << 20);
        defer std.testing.allocator.free(source);
        digest.byte('F');
        digest.bytes(path);
        digest.bytes(source);

        const root = send1(api, lang_obj, "parse:", api.mkString(source.ptr, source.len));
        if (root == api.nilObj) {
            digest.byte('F');
            digest.putU64(@intCast(api.smallIntValue(send0(api, lang_obj, "furthestFail"))));
        } else {
            digest.byte('S');
            walkCst(api, root, &digest, 0);
        }
    }

    std.debug.print("rust-peg-source-corpus digest={x:0>16} files={d}\n", .{ digest.state, paths.items.len });
    const expected_text = std.posix.getenv("BIRNAM_RUST_PEG_SOURCE_DIGEST") orelse return;
    if (expected_text.len == 0) return;
    var expected: u64 = 0;
    for (expected_text) |character| {
        const digit = switch (character) {
            '0'...'9' => character - '0',
            'a'...'f' => character - 'a' + 10,
            'A'...'F' => character - 'A' + 10,
            else => return error.InvalidRustPegSourceDigest,
        };
        expected = (expected << 4) | digit;
    }
    try std.testing.expectEqual(expected, digest.state);
}
