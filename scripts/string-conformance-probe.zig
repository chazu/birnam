//! Backend-neutral R3.3 copied-byte String conformance client.

const std = @import("std");
const idapi = @import("idapi");

fn require(condition: bool, failure: anyerror) !void {
    if (!condition) return failure;
}

fn bytes(api: *idapi.IdApi, object: idapi.Oop) []const u8 {
    var length: usize = 0;
    const pointer = api.stringBytes(object, &length);
    return pointer[0..length];
}

pub fn main() !void {
    var args = std.process.args();
    _ = args.next();
    const dylib_path = args.next() orelse return error.MissingDylibPath;
    const label = args.next() orelse return error.MissingLabel;
    if (args.next() != null) return error.UnexpectedArgument;
    var dylib = try std.DynLib.open(dylib_path);
    defer dylib.close();
    const init = dylib.lookup(idapi.IdInitFn, "id_init") orelse return error.MissingIdInit;
    const api = init();

    const prototype = api.import("String");
    try require(prototype == api.vtableOwner(api.string_vt), error.StringPrototypeChanged);
    try require(api.vtableOf(prototype) == api.string_vt, error.StringPrototypeFamilyChanged);
    try require(bytes(api, prototype).len == 0, error.StringPrototypeNotEmpty);

    const empty = api.mkString("", 0);
    try require(empty != prototype, error.EmptyStringReusedPrototype);
    try require(bytes(api, empty).len == 0, error.EmptyStringNotEmpty);

    var source = [_]u8{ 0xff, 0x00, 'b', 'i', 'r', 'n', 'a', 'm' };
    const first = api.mkString(&source, source.len);
    const second = api.mkString(&source, source.len);
    try require(first != second, error.StringIdentityCollapsed);
    try require(api.vtableOf(first) == api.string_vt, error.StringVtableChanged);
    const first_pointer = bytes(api, first).ptr;
    source[0] = 0;
    source[1] = 1;
    try require(
        std.mem.eql(u8, bytes(api, first), &[_]u8{ 0xff, 0x00, 'b', 'i', 'r', 'n', 'a', 'm' }),
        error.StringDidNotCopyBinaryBytes,
    );
    try require(
        std.mem.eql(u8, bytes(api, second), &[_]u8{ 0xff, 0x00, 'b', 'i', 'r', 'n', 'a', 'm' }),
        error.StringInstancesShareCallerStorage,
    );

    for (0..4096) |index| {
        var small = [_]u8{ @truncate(index), @truncate(index >> 8), 0 };
        _ = api.mkString(&small, small.len);
    }
    try require(bytes(api, first).ptr == first_pointer, error.StringPointerMovedAfterGrowth);

    const allocator = std.heap.page_allocator;
    const large = try allocator.alloc(u8, 1024 * 1024);
    defer allocator.free(large);
    for (large, 0..) |*byte, index| byte.* = @truncate(index *% 131 +% 17);
    const large_string = api.mkString(large.ptr, large.len);
    const expected_first = large[0];
    const expected_middle = large[large.len / 2];
    const expected_last = large[large.len - 1];
    @memset(large, 0);
    const copied = bytes(api, large_string);
    try require(copied.len == 1024 * 1024, error.LargeStringLengthChanged);
    try require(copied[0] == expected_first, error.LargeStringStartChanged);
    try require(copied[copied.len / 2] == expected_middle, error.LargeStringMiddleChanged);
    try require(copied[copied.len - 1] == expected_last, error.LargeStringEndChanged);

    try require(api.import("String") == prototype, error.StringPrototypeReplaced);
    try require(api.vtableCount() == 11, error.StringAllocationCreatedVtable);
    std.debug.print("string-conformance: {s} preserved copied arbitrary bytes\n", .{label});
}
