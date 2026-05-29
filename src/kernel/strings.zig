//! Tagged-pointer string Ref encoding.
//!
//! A string Ref uses tag `100` (one of the reserved codes). The upper 61
//! bits store `ptr >> 3` where `ptr` is the address of a `StringHeader`
//! on the heap. 8-aligned allocator guarantees the low 3 bits are zero
//! before tagging.
//!
//! Layout in memory:
//!   StringHeader { len: u64 }  followed inline by `len` bytes
//!
//! Strings are immutable after creation. Equality at the Ref level is
//! pointer-equal after intern (Phase 4.5 §interner); arbitrary string
//! Refs compare byte-equal via `equals`.

const std = @import("std");
const ref_mod = @import("ref.zig");

pub const Ref = ref_mod.Ref;

/// Tag code for strings. Sits in the reserved 100..111 block.
pub const STRING_TAG: u3 = 0b100;

pub const StringHeader = extern struct {
    len: u64,
    // followed by `len` bytes inline
};

/// Allocate a string on the heap and return a tagged Ref.
pub fn mkString(allocator: std.mem.Allocator, src: []const u8) !Ref {
    const total = @sizeOf(StringHeader) + src.len;
    const buf = try allocator.alignedAlloc(u8, .of(StringHeader), total);
    const header: *StringHeader = @ptrCast(@alignCast(buf.ptr));
    header.len = src.len;
    const payload = buf.ptr + @sizeOf(StringHeader);
    @memcpy(payload[0..src.len], src);
    return encode(header);
}

pub fn encode(header: *const StringHeader) Ref {
    const addr: u64 = @intFromPtr(header);
    std.debug.assert((addr & 0b111) == 0);
    return addr | @as(u64, STRING_TAG);
}

pub fn isString(r: Ref) bool {
    return (r & ref_mod.TAG_MASK) == STRING_TAG;
}

pub fn untag(r: Ref) *const StringHeader {
    std.debug.assert(isString(r));
    const addr = r & ~@as(u64, 0b111);
    return @ptrFromInt(addr);
}

/// Borrow the raw bytes from a string Ref.
pub fn bytes(r: Ref) []const u8 {
    const h = untag(r);
    const base: [*]const u8 = @ptrCast(h);
    return base[@sizeOf(StringHeader) .. @sizeOf(StringHeader) + h.len];
}

/// Byte-equality across two string Refs.
pub fn equals(a: Ref, b: Ref) bool {
    if (a == b) return true;
    return std.mem.eql(u8, bytes(a), bytes(b));
}
