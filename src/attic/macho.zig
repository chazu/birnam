//! Phase 9.1: bundled Mach-O 64-bit reader.
//!
//! Parses the header + load commands of a Mach-O 64-bit dylib and yields
//! `(name, addr, kind)` per symbol-table entry. Used by the FFI module's
//! lazy `Module>>symbols` enumerator (Phase 9.6).
//!
//! Scope: little-endian arm64 / x86_64 only (the two darwin architectures
//! birnam targets). Fat binaries are not detected — caller picks the right
//! slice. dyld shared-cache stubs (libSystem on Big Sur+) are NOT readable
//! as files; tests use a build-output dylib that lives on disk.
//!
//! References: <mach-o/loader.h>, <mach-o/nlist.h>.

const std = @import("std");

pub const MAGIC_64: u32 = 0xFEEDFACF;
pub const MAGIC_64_SWAPPED: u32 = 0xCFFAEDFE;

pub const LC_SYMTAB: u32 = 0x2;

pub const Header = extern struct {
    magic: u32,
    cputype: i32,
    cpusubtype: i32,
    filetype: u32,
    ncmds: u32,
    sizeofcmds: u32,
    flags: u32,
    reserved: u32,
};

pub const LoadCommand = extern struct {
    cmd: u32,
    cmdsize: u32,
};

pub const SymtabCommand = extern struct {
    cmd: u32,
    cmdsize: u32,
    symoff: u32,
    nsyms: u32,
    stroff: u32,
    strsize: u32,
};

pub const Nlist64 = extern struct {
    n_strx: u32,
    n_type: u8,
    n_sect: u8,
    n_desc: u16,
    n_value: u64,
};

/// Symbol type bitfield (n_type & N_TYPE):
pub const N_TYPE_MASK: u8 = 0x0E;
pub const N_STAB: u8 = 0xE0; // debug symbol if any bit set
pub const N_UNDF: u8 = 0x0;
pub const N_ABS: u8 = 0x2;
pub const N_SECT: u8 = 0xE; // defined in a section
pub const N_EXT: u8 = 0x01; // bit in n_type: external/global

pub const Kind = enum {
    /// Defined in a section (`N_SECT`). Common case for exported text/data.
    section,
    /// Absolute (`N_ABS`) — symbol value is a constant.
    absolute,
    /// Undefined (`N_UNDF`) — imported, not defined here.
    undefined_,
    /// Debug entry (`N_STAB`).
    stab,
    /// Anything not classified by the above masks.
    other,
};

pub const Symbol = struct {
    name: []const u8,
    addr: u64,
    kind: Kind,
    external: bool,
};

pub const Error = error{
    TooSmall,
    BadMagic,
    SwappedMagicUnsupported,
    NoSymtab,
    Truncated,
};

pub const Reader = struct {
    data: []const u8,

    pub fn init(data: []const u8) Error!Reader {
        if (data.len < @sizeOf(Header)) return error.TooSmall;
        const magic: *align(1) const u32 = @ptrCast(data.ptr);
        switch (magic.*) {
            MAGIC_64 => {},
            MAGIC_64_SWAPPED => return error.SwappedMagicUnsupported,
            else => return error.BadMagic,
        }
        return .{ .data = data };
    }

    pub fn header(self: *const Reader) *align(1) const Header {
        return @ptrCast(self.data.ptr);
    }

    fn findSymtab(self: *const Reader) ?*align(1) const SymtabCommand {
        const h = self.header();
        var off: usize = @sizeOf(Header);
        var i: u32 = 0;
        while (i < h.ncmds) : (i += 1) {
            if (off + @sizeOf(LoadCommand) > self.data.len) return null;
            const lc: *align(1) const LoadCommand = @ptrCast(self.data.ptr + off);
            if (lc.cmd == LC_SYMTAB) {
                if (off + @sizeOf(SymtabCommand) > self.data.len) return null;
                return @ptrCast(self.data.ptr + off);
            }
            off += lc.cmdsize;
        }
        return null;
    }

    pub fn symbols(self: *const Reader) Error!SymbolIter {
        const st = self.findSymtab() orelse return error.NoSymtab;
        const sym_table_bytes = @as(usize, st.nsyms) * @sizeOf(Nlist64);
        if (@as(usize, st.symoff) + sym_table_bytes > self.data.len) return error.Truncated;
        if (@as(usize, st.stroff) + @as(usize, st.strsize) > self.data.len) return error.Truncated;
        return .{
            .data = self.data,
            .symtab = st,
            .index = 0,
        };
    }
};

pub const SymbolIter = struct {
    data: []const u8,
    symtab: *align(1) const SymtabCommand,
    index: u32,

    pub fn next(self: *SymbolIter) ?Symbol {
        if (self.index >= self.symtab.nsyms) return null;
        const entries_ptr = self.data.ptr + self.symtab.symoff;
        const entry: *align(1) const Nlist64 = @ptrCast(entries_ptr + @as(usize, self.index) * @sizeOf(Nlist64));
        self.index += 1;

        // Resolve name via string table.
        const str_base = self.data.ptr + self.symtab.stroff;
        const str_end = self.symtab.strsize;
        var name: []const u8 = "";
        if (entry.n_strx < str_end) {
            const start = entry.n_strx;
            var end: u32 = start;
            while (end < str_end and str_base[end] != 0) : (end += 1) {}
            name = str_base[start..end];
        }

        const kind: Kind = blk: {
            if ((entry.n_type & N_STAB) != 0) break :blk .stab;
            switch (entry.n_type & N_TYPE_MASK) {
                N_UNDF => break :blk .undefined_,
                N_ABS => break :blk .absolute,
                N_SECT => break :blk .section,
                else => break :blk .other,
            }
        };

        return .{
            .name = name,
            .addr = entry.n_value,
            .kind = kind,
            .external = (entry.n_type & N_EXT) != 0,
        };
    }
};

// -- loader.SymbolIter adapter (Phase 9.5.4) -------------------------------

const loader = @import("loader.zig");

/// Per-iteration context held by the heap-allocated adapter. Carries the
/// owned `Reader` (which itself just borrows the file bytes) and the
/// underlying nlist-walking iter.
const MachoCtx = struct {
    reader: Reader,
    inner: SymbolIter,
};

fn mapKind(k: Kind) loader.Kind {
    return switch (k) {
        .section => .section,
        .absolute => .absolute,
        .undefined_ => .undefined_,
        .stab, .other => .other,
    };
}

fn machoNext(ctx_raw: *anyopaque) ?loader.Symbol {
    const ctx: *MachoCtx = @ptrCast(@alignCast(ctx_raw));
    while (ctx.inner.next()) |sym| {
        // Drop STAB (debug) and empty-name entries — readers expose only
        // callable symbols per Phase 9.5 decision-doc B1.
        if (sym.kind == .stab) continue;
        if (sym.name.len == 0) continue;
        return .{
            .name = sym.name,
            .addr = sym.addr,
            .kind = mapKind(sym.kind),
            .external = sym.external,
        };
    }
    return null;
}

fn machoDeinit(ctx_raw: *anyopaque, allocator: std.mem.Allocator) void {
    const ctx: *MachoCtx = @ptrCast(@alignCast(ctx_raw));
    allocator.destroy(ctx);
}

/// Format-reader entrypoint registered with `loader.registerReader` at boot.
pub fn openSymbolsAdapter(
    allocator: std.mem.Allocator,
    bytes: []const u8,
) anyerror!loader.SymbolIter {
    var reader = try Reader.init(bytes);
    const inner = try reader.symbols();
    const ctx = try allocator.create(MachoCtx);
    ctx.* = .{ .reader = reader, .inner = inner };
    return .{
        .ctx = @ptrCast(ctx),
        .next_fn = &machoNext,
        .deinit_fn = &machoDeinit,
        .allocator = allocator,
    };
}

// -- File-loading convenience -----------------------------------------------

pub fn readFile(allocator: std.mem.Allocator, path: []const u8) ![]u8 {
    var file = try std.fs.cwd().openFile(path, .{});
    defer file.close();
    const stat = try file.stat();
    const buf = try allocator.alloc(u8, stat.size);
    _ = try file.readAll(buf);
    return buf;
}
