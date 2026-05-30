//! Phase 9.5.4: bundled ELF64 reader (Linux dynamic symbol enumeration).
//!
//! Scope: ELF64 little-endian only — covers x86_64 / aarch64 Linux. Big-
//! endian or 32-bit ELF returns an error. Per Phase 9.5 decision-doc B1:
//! enumerates `.dynsym` (always present in shared libs; matches the
//! `dlsym`-callable view) rather than `.symtab` (debug-only, often
//! stripped from production libs).
//!
//! References: System V ABI — Executable and Linkable Format (ELF)
//! Specification; `man 5 elf`.

const std = @import("std");
const loader = @import("loader.zig");

// -- ELF magic + class -----------------------------------------------------

/// First 4 bytes of every ELF file. Reads as 0x464C457F in little-endian
/// (the same value `loader.openSymbols` reads when sniffing magic).
pub const MAGIC: u32 = 0x464C457F;

pub const EI_NIDENT: usize = 16;
pub const ELFCLASS64: u8 = 2;
pub const ELFDATA2LSB: u8 = 1;
pub const ELFDATA2MSB: u8 = 2;

// -- File header (ELF64) ---------------------------------------------------

pub const Ehdr = extern struct {
    e_ident: [EI_NIDENT]u8,
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
};

// -- Section header (ELF64) ------------------------------------------------

pub const Shdr = extern struct {
    sh_name: u32,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u64,
    sh_entsize: u64,
};

pub const SHT_DYNSYM: u32 = 11;

// -- Symbol entry (ELF64) --------------------------------------------------

pub const Sym = extern struct {
    st_name: u32,
    st_info: u8,
    st_other: u8,
    st_shndx: u16,
    st_value: u64,
    st_size: u64,
};

pub const SHN_UNDEF: u16 = 0;
pub const SHN_ABS: u16 = 0xFFF1;

pub const STT_NOTYPE: u8 = 0;
pub const STT_OBJECT: u8 = 1;
pub const STT_FUNC: u8 = 2;
pub const STT_SECTION: u8 = 3;
pub const STT_FILE: u8 = 4;

pub const STB_LOCAL: u8 = 0;
pub const STB_GLOBAL: u8 = 1;
pub const STB_WEAK: u8 = 2;

fn stType(info: u8) u8 {
    return info & 0x0F;
}

fn stBind(info: u8) u8 {
    return info >> 4;
}

// -- Errors ----------------------------------------------------------------

pub const Error = error{
    TooSmall,
    BadMagic,
    Not64Bit,
    NotLittleEndian,
    NoDynsym,
    Truncated,
};

// -- Reader / iterator -----------------------------------------------------

pub const Reader = struct {
    data: []const u8,

    pub fn init(data: []const u8) Error!Reader {
        if (data.len < @sizeOf(Ehdr)) return error.TooSmall;
        const magic: *align(1) const u32 = @ptrCast(data.ptr);
        if (magic.* != MAGIC) return error.BadMagic;
        const ident = data[0..EI_NIDENT];
        if (ident[4] != ELFCLASS64) return error.Not64Bit;
        if (ident[5] != ELFDATA2LSB) return error.NotLittleEndian;
        return .{ .data = data };
    }

    pub fn header(self: *const Reader) *align(1) const Ehdr {
        return @ptrCast(self.data.ptr);
    }

    fn shdrAt(self: *const Reader, idx: u16) ?*align(1) const Shdr {
        const h = self.header();
        const off = @as(usize, h.e_shoff) + @as(usize, idx) * @as(usize, h.e_shentsize);
        if (off + @sizeOf(Shdr) > self.data.len) return null;
        return @ptrCast(self.data.ptr + off);
    }

    /// Locate the `.dynsym` section header. Returns null if the binary
    /// has no dynamic symbol table (rare — static-only archives, raw
    /// object files).
    fn findDynsym(self: *const Reader) ?*align(1) const Shdr {
        const h = self.header();
        var i: u16 = 0;
        while (i < h.e_shnum) : (i += 1) {
            const sh = self.shdrAt(i) orelse return null;
            if (sh.sh_type == SHT_DYNSYM) return sh;
        }
        return null;
    }

    /// Walk `.dynsym` yielding callable symbols.
    pub fn symbols(self: *const Reader) Error!SymbolIter {
        const sh = self.findDynsym() orelse return error.NoDynsym;
        if (sh.sh_entsize == 0) return error.NoDynsym;
        const sym_table_bytes = @as(usize, sh.sh_size);
        if (@as(usize, sh.sh_offset) + sym_table_bytes > self.data.len) return error.Truncated;
        // String table sits at the section indexed by sh_link.
        const strtab_sh = self.shdrAt(@intCast(sh.sh_link)) orelse return error.Truncated;
        if (@as(usize, strtab_sh.sh_offset) + @as(usize, strtab_sh.sh_size) > self.data.len) return error.Truncated;
        return .{
            .data = self.data,
            .dynsym = sh,
            .strtab = strtab_sh,
            .index = 0,
        };
    }
};

pub const SymbolIter = struct {
    data: []const u8,
    dynsym: *align(1) const Shdr,
    strtab: *align(1) const Shdr,
    index: usize,

    pub fn next(self: *SymbolIter) ?loader.Symbol {
        const entsize = @as(usize, self.dynsym.sh_entsize);
        const count = @as(usize, self.dynsym.sh_size) / entsize;
        while (self.index < count) {
            const sym_off = @as(usize, self.dynsym.sh_offset) + self.index * entsize;
            self.index += 1;
            const entry: *align(1) const Sym = @ptrCast(self.data.ptr + sym_off);

            const stt = stType(entry.st_info);
            // Skip FILE / SECTION metadata, NOTYPE (often padding entry 0).
            if (stt == STT_FILE) continue;
            if (stt == STT_SECTION) continue;
            if (stt == STT_NOTYPE and entry.st_name == 0 and entry.st_value == 0) continue;

            // Resolve name via the dynsym-linked string table.
            const str_base = self.data.ptr + @as(usize, self.strtab.sh_offset);
            const str_end = @as(usize, self.strtab.sh_size);
            var name: []const u8 = "";
            if (entry.st_name < str_end) {
                const start: usize = entry.st_name;
                var end: usize = start;
                while (end < str_end and str_base[end] != 0) : (end += 1) {}
                name = str_base[start..end];
            }
            if (name.len == 0) continue;

            const kind: loader.Kind = switch (entry.st_shndx) {
                SHN_UNDEF => .undefined_,
                SHN_ABS => .absolute,
                else => .section,
            };
            const stb = stBind(entry.st_info);
            const external = stb == STB_GLOBAL or stb == STB_WEAK;

            return .{
                .name = name,
                .addr = entry.st_value,
                .kind = kind,
                .external = external,
            };
        }
        return null;
    }
};

// -- loader.SymbolIter adapter --------------------------------------------

const ElfCtx = struct {
    reader: Reader,
    inner: SymbolIter,
};

fn elfNext(ctx_raw: *anyopaque) ?loader.Symbol {
    const ctx: *ElfCtx = @ptrCast(@alignCast(ctx_raw));
    return ctx.inner.next();
}

fn elfDeinit(ctx_raw: *anyopaque, allocator: std.mem.Allocator) void {
    const ctx: *ElfCtx = @ptrCast(@alignCast(ctx_raw));
    allocator.destroy(ctx);
}

/// Format-reader entrypoint registered with `loader.registerReader` at boot.
pub fn openSymbolsAdapter(
    allocator: std.mem.Allocator,
    bytes: []const u8,
) anyerror!loader.SymbolIter {
    var reader = try Reader.init(bytes);
    const inner = try reader.symbols();
    const ctx = try allocator.create(ElfCtx);
    ctx.* = .{ .reader = reader, .inner = inner };
    return .{
        .ctx = @ptrCast(ctx),
        .next_fn = &elfNext,
        .deinit_fn = &elfDeinit,
        .allocator = allocator,
    };
}
