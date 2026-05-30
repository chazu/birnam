//! Phase 9.5.4: format-agnostic binary loader dispatch.
//!
//! Per Phase 9.5 decision-doc B2: fact-dispatched reader registry. Each
//! format reader (macho.zig, elf.zig, future PE/COFF/...) self-registers
//! at boot via `registerReader`, asserting two facts at a fresh entity:
//!
//!   [reader_eid :reader-magic <SmallInt-of-first-4-bytes>]
//!   [reader_eid :reader-impl  <pointer-Ref to OpenSymbolsFn>]
//!
//! `openSymbols(allocator, store, bytes)` sniffs the input's first 4 bytes,
//! queries `[?r :reader-magic <magic>]` via AVET, calls the impl. No
//! hardcoded switch in the kernel; adding a new format is a fact
//! assertion. COLA-correct.
//!
//! Both readers return a `SymbolIter` that already filters out debug /
//! local-only entries (per B1: callable-symbols contract).

const std = @import("std");
const ref_mod = @import("ref.zig");
const fact_store_mod = @import("fact_store.zig");
const object_table_mod = @import("object_table.zig");
const kernel_eids = @import("kernel_eids.zig");

pub const Ref = ref_mod.Ref;

// -- Shared Symbol contract -------------------------------------------------

/// What kind of binding a symbol carries. Both Mach-O and ELF readers
/// normalize their native enums into this set. STAB / file-local /
/// debug-only entries are dropped at the iter level — they never surface.
pub const Kind = enum {
    /// Defined in a section. Most calls (`sqrt`, `printf`, ...) are this.
    section,
    /// Absolute value (constant).
    absolute,
    /// Referenced but defined elsewhere — needs dynamic resolution.
    undefined_,
    /// Anything not classified by the above.
    other,
};

pub const Symbol = struct {
    /// String slice borrowed from the underlying file bytes. Caller may
    /// `dupe` if the bytes won't outlive the symbol use.
    name: []const u8,
    addr: u64,
    kind: Kind,
    external: bool,
};

/// Function signature each format reader registers. Allocator is used by
/// the iterator for its own state (it MUST deinit on `SymbolIter.deinit`).
/// `bytes` lifetime must cover all `.next()` calls.
pub const OpenSymbolsFn = *const fn (
    allocator: std.mem.Allocator,
    bytes: []const u8,
) anyerror!SymbolIter;

/// Format-agnostic iterator. Each reader supplies a context pointer and a
/// pair of function pointers — Zig's idiomatic vtable-by-hand pattern.
pub const SymbolIter = struct {
    ctx: *anyopaque,
    next_fn: *const fn (ctx: *anyopaque) ?Symbol,
    deinit_fn: *const fn (ctx: *anyopaque, allocator: std.mem.Allocator) void,
    allocator: std.mem.Allocator,

    pub fn next(self: *SymbolIter) ?Symbol {
        return self.next_fn(self.ctx);
    }

    pub fn deinit(self: *SymbolIter) void {
        self.deinit_fn(self.ctx, self.allocator);
    }
};

// -- Registry ---------------------------------------------------------------

pub const Error = error{
    TooSmall,
    NoReaderForMagic,
    BadReaderFact,
};

/// Heap-allocated box for the reader fn pointer. Function pointers don't
/// always satisfy birnam's 8-byte pointer-Ref alignment assertion, so we
/// stash the fn ptr in a stable 8-aligned struct and tag the box's address.
pub const ImplBox = struct {
    f: OpenSymbolsFn,
};

/// Register a reader. Bootstrap calls this once per format at boot.
pub fn registerReader(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    magic: u32,
    impl_fn: OpenSymbolsFn,
) !void {
    const box = try allocator.create(ImplBox);
    box.* = .{ .f = impl_fn };
    const reader_eid = table.alloc();
    _ = try store.assert(
        reader_eid,
        kernel_eids.ATTR_READER_MAGIC,
        ref_mod.mkSmallInt(@intCast(magic)).?,
        kernel_eids.M_BOOTSTRAP,
    );
    _ = try store.assert(
        reader_eid,
        kernel_eids.ATTR_READER_IMPL,
        ref_mod.mkPointer(@ptrCast(box)),
        kernel_eids.M_BOOTSTRAP,
    );
}

/// Read first 4 bytes as a little-endian u32, look up the reader entity,
/// invoke its impl.
pub fn openSymbols(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    bytes: []const u8,
) !SymbolIter {
    if (bytes.len < 4) return error.TooSmall;
    const magic: u32 = std.mem.readInt(u32, bytes[0..4], .little);
    var it = store.entitiesMatching(
        kernel_eids.ATTR_READER_MAGIC,
        ref_mod.mkSmallInt(@intCast(magic)).?,
    );
    while (it.next()) |reader_eid| {
        const impl_ref = store.currentValue(reader_eid, kernel_eids.ATTR_READER_IMPL) orelse return error.BadReaderFact;
        if (!ref_mod.isPointer(impl_ref)) return error.BadReaderFact;
        const box: *const ImplBox = @ptrCast(@alignCast(ref_mod.untagPointer(impl_ref)));
        return box.f(allocator, bytes);
    }
    return error.NoReaderForMagic;
}
