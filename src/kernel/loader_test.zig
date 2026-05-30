const std = @import("std");
const testing = std.testing;
const builtin = @import("builtin");

const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const bootstrap = @import("bootstrap.zig");
const loader = @import("loader.zig");
const macho_mod = @import("macho.zig");
const elf_mod = @import("elf.zig");
const kernel_eids = @import("kernel_eids.zig");

const is_darwin = builtin.os.tag == .macos;
const HELLO_DYLIB = "zig-out/lib/libbirnam_hello.dylib";

// -- Fact-dispatched reader registry ----------------------------------------

test "bootstrap registers both Mach-O and ELF readers as facts" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fact_store.FactStore.init(a);
    var table = object_table.ObjectTable.init(a);
    var interner = interner_mod.Interner.init(a);
    _ = try bootstrap.boot(a, &store, &table, &interner);

    var macho_it = store.entitiesMatching(
        kernel_eids.ATTR_READER_MAGIC,
        ref_mod.mkSmallInt(@intCast(macho_mod.MAGIC_64)).?,
    );
    try testing.expect(macho_it.next() != null);

    var elf_it = store.entitiesMatching(
        kernel_eids.ATTR_READER_MAGIC,
        ref_mod.mkSmallInt(@intCast(elf_mod.MAGIC)).?,
    );
    try testing.expect(elf_it.next() != null);
}

test "loader.openSymbols dispatches to Mach-O reader by magic" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fact_store.FactStore.init(a);
    var table = object_table.ObjectTable.init(a);
    var interner = interner_mod.Interner.init(a);
    _ = try bootstrap.boot(a, &store, &table, &interner);

    const bytes = macho_mod.readFile(a, HELLO_DYLIB) catch return error.SkipZigTest;
    var it = try loader.openSymbols(a, &store, bytes);
    defer it.deinit();

    var found_init = false;
    var n: usize = 0;
    while (it.next()) |sym| {
        n += 1;
        if (std.mem.eql(u8, sym.name, "_birnam_module_init")) {
            try testing.expectEqual(loader.Kind.section, sym.kind);
            try testing.expect(sym.external);
            found_init = true;
        }
    }
    try testing.expect(n > 0);
    try testing.expect(found_init);
}

test "loader.openSymbols rejects bytes with unknown magic" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fact_store.FactStore.init(a);
    var table = object_table.ObjectTable.init(a);
    var interner = interner_mod.Interner.init(a);
    _ = try bootstrap.boot(a, &store, &table, &interner);

    const garbage = [_]u8{ 0xDE, 0xAD, 0xBE, 0xEF, 0, 0, 0, 0 };
    const r = loader.openSymbols(a, &store, &garbage);
    try testing.expectError(error.NoReaderForMagic, r);
}

// -- ELF reader: structural sanity ------------------------------------------
//
// On macOS we don't have ELF dylibs to feed; we synthesize a tiny ELF64
// little-endian header to validate magic + class checks. Full symbol
// enumeration is exercised when the linux CI gate flips and ELF dylibs are
// available on disk.

test "elf.Reader rejects non-ELF magic" {
    const garbage = [_]u8{0xAA} ** 128;
    try testing.expectError(error.BadMagic, elf_mod.Reader.init(&garbage));
}

test "elf.Reader rejects 32-bit ELF" {
    var bytes: [@sizeOf(elf_mod.Ehdr)]u8 = .{0} ** @sizeOf(elf_mod.Ehdr);
    // ELF magic
    bytes[0] = 0x7F;
    bytes[1] = 'E';
    bytes[2] = 'L';
    bytes[3] = 'F';
    // EI_CLASS = 1 (32-bit)
    bytes[4] = 1;
    // EI_DATA = little
    bytes[5] = elf_mod.ELFDATA2LSB;
    try testing.expectError(error.Not64Bit, elf_mod.Reader.init(&bytes));
}

test "elf.Reader rejects big-endian ELF64" {
    var bytes: [@sizeOf(elf_mod.Ehdr)]u8 = .{0} ** @sizeOf(elf_mod.Ehdr);
    bytes[0] = 0x7F;
    bytes[1] = 'E';
    bytes[2] = 'L';
    bytes[3] = 'F';
    bytes[4] = elf_mod.ELFCLASS64;
    bytes[5] = elf_mod.ELFDATA2MSB;
    try testing.expectError(error.NotLittleEndian, elf_mod.Reader.init(&bytes));
}

test "elf.Reader accepts well-formed minimal ELF64 LE header" {
    var bytes: [@sizeOf(elf_mod.Ehdr)]u8 = .{0} ** @sizeOf(elf_mod.Ehdr);
    bytes[0] = 0x7F;
    bytes[1] = 'E';
    bytes[2] = 'L';
    bytes[3] = 'F';
    bytes[4] = elf_mod.ELFCLASS64;
    bytes[5] = elf_mod.ELFDATA2LSB;
    var reader = try elf_mod.Reader.init(&bytes);
    const ident = reader.header().e_ident;
    try testing.expectEqualSlices(u8, &.{ 0x7F, 'E', 'L', 'F' }, ident[0..4]);
}

test "elf.Reader.symbols errors NoDynsym when no .dynsym section present" {
    var bytes: [@sizeOf(elf_mod.Ehdr)]u8 = .{0} ** @sizeOf(elf_mod.Ehdr);
    bytes[0] = 0x7F;
    bytes[1] = 'E';
    bytes[2] = 'L';
    bytes[3] = 'F';
    bytes[4] = elf_mod.ELFCLASS64;
    bytes[5] = elf_mod.ELFDATA2LSB;
    // e_shnum = 0 (no section headers) → findDynsym returns null
    var reader = try elf_mod.Reader.init(&bytes);
    try testing.expectError(error.NoDynsym, reader.symbols());
}
