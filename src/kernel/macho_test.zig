const std = @import("std");
const testing = std.testing;
const builtin = @import("builtin");

const macho = @import("macho.zig");

const is_darwin = builtin.os.tag == .macos;
const TEST_DYLIB = "zig-out/lib/libbirnam_hello.dylib";

test "macho: header magic check" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const data = macho.readFile(a, TEST_DYLIB) catch return error.SkipZigTest;
    var reader = try macho.Reader.init(data);
    try testing.expectEqual(macho.MAGIC_64, reader.header().magic);
}

test "macho: BadMagic for non-Mach-O bytes" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();

    const garbage = [_]u8{0xAA} ** 64;
    const r = macho.Reader.init(&garbage);
    try testing.expectError(error.BadMagic, r);
}

test "macho: TooSmall for byte slice shorter than header" {
    const tiny = [_]u8{ 0xCF, 0xFA, 0xED };
    const r = macho.Reader.init(&tiny);
    try testing.expectError(error.TooSmall, r);
}

test "macho: SymbolIter finds _birnam_module_init in the hello dylib" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const data = macho.readFile(a, TEST_DYLIB) catch return error.SkipZigTest;
    var reader = try macho.Reader.init(data);
    var it = try reader.symbols();

    var found_init_runtime = false;
    var n_symbols: usize = 0;
    while (it.next()) |sym| {
        n_symbols += 1;
        if (std.mem.eql(u8, sym.name, "_birnam_module_init") and sym.kind == .section and sym.external) {
            found_init_runtime = true;
        }
    }
    try testing.expect(n_symbols > 0);
    try testing.expect(found_init_runtime);
}

test "macho: SymbolIter yields some external section symbols" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const data = macho.readFile(a, TEST_DYLIB) catch return error.SkipZigTest;
    var reader = try macho.Reader.init(data);
    var it = try reader.symbols();

    var section_count: usize = 0;
    var external_count: usize = 0;
    while (it.next()) |sym| {
        if (sym.kind == .section) section_count += 1;
        if (sym.external) external_count += 1;
    }
    try testing.expect(section_count > 0);
    try testing.expect(external_count > 0);
}
