//! Tagged reference type for birnam.
//!
//! A `Ref` is a single machine word. Low 3 bits = tag, identifying what
//! the upper 61 bits mean. 8-byte pointer alignment makes the low 3 bits
//! free on aligned addresses.
//!
//! Tag codes (locked in VISION.md §Object Model → Identity and References):
//!
//!   000  raw pointer to materialized libid struct  (hot objects)
//!   001  SmallInteger                              (unboxed ints, idst tradition)
//!   010  EID                                       (cold object — query fact store)
//!   011  keyword / immediate symbol                (future)
//!   100..111  reserved for future immediates
//!
//! No-NULL discipline (Codd/Date): `NIL` is a real entity at well-known
//! EID 0. It is NOT the all-zero machine word — zero-initialized memory
//! reads as a null pointer (tag 000, address 0), which is fail-loud on
//! dereference. `NIL` itself is `mkEid(0)`, the bit pattern `0b010`.

const std = @import("std");

pub const Ref = u64;

pub const Tag = enum(u3) {
    pointer = 0b000,
    smallint = 0b001,
    eid = 0b010,
    keyword = 0b011,
    // 0b100..0b111 reserved
};

pub const TAG_MASK: u64 = 0b111;
pub const VALUE_SHIFT: u6 = 3;

/// NIL = the nothing-entity at well-known EID 0.
/// Bit pattern: `0b010`. Decimal: 2.
pub const NIL: Ref = mkEid(0);

// -- Tag predicates -----------------------------------------------------------

pub fn tagOf(r: Ref) Tag {
    return @enumFromInt(@as(u3, @intCast(r & TAG_MASK)));
}

pub fn isPointer(r: Ref) bool {
    return (r & TAG_MASK) == @intFromEnum(Tag.pointer);
}

pub fn isSmallInt(r: Ref) bool {
    return (r & TAG_MASK) == @intFromEnum(Tag.smallint);
}

pub fn isEid(r: Ref) bool {
    return (r & TAG_MASK) == @intFromEnum(Tag.eid);
}

pub fn isKeyword(r: Ref) bool {
    return (r & TAG_MASK) == @intFromEnum(Tag.keyword);
}

pub fn isNil(r: Ref) bool {
    return r == NIL;
}

// -- Pointer encoding ---------------------------------------------------------

/// Tag bits already zero on 8-aligned address. Debug-asserts alignment.
pub fn mkPointer(ptr: *anyopaque) Ref {
    const addr: u64 = @intFromPtr(ptr);
    std.debug.assert((addr & TAG_MASK) == 0);
    return addr;
}

/// Caller must verify `isPointer(r)` first.
pub fn untagPointer(r: Ref) *anyopaque {
    std.debug.assert(isPointer(r));
    return @ptrFromInt(r);
}

// -- SmallInt encoding (idst tradition: shift-by-3 with sign extension) -------

pub const SMALLINT_MIN: i64 = -(@as(i64, 1) << 60);
pub const SMALLINT_MAX: i64 = (@as(i64, 1) << 60) - 1;

/// Returns null if value won't fit in 61 signed bits.
pub fn mkSmallInt(n: i64) ?Ref {
    if (n < SMALLINT_MIN or n > SMALLINT_MAX) return null;
    const shifted: i64 = n << VALUE_SHIFT;
    return @as(u64, @bitCast(shifted)) | @intFromEnum(Tag.smallint);
}

/// Arithmetic shift preserves sign. Caller must verify `isSmallInt(r)` first.
pub fn untagSmallInt(r: Ref) i64 {
    std.debug.assert(isSmallInt(r));
    return @as(i64, @bitCast(r)) >> VALUE_SHIFT;
}

// -- EID encoding -------------------------------------------------------------

pub const EID_MAX: u64 = (@as(u64, 1) << 61) - 1;

pub fn mkEid(eid: u64) Ref {
    std.debug.assert(eid <= EID_MAX);
    return (eid << VALUE_SHIFT) | @intFromEnum(Tag.eid);
}

pub fn untagEid(r: Ref) u64 {
    std.debug.assert(isEid(r));
    return r >> VALUE_SHIFT;
}

// -- Keyword encoding ---------------------------------------------------------

pub const KEYWORD_MAX: u64 = (@as(u64, 1) << 61) - 1;

pub fn mkKeyword(k: u64) Ref {
    std.debug.assert(k <= KEYWORD_MAX);
    return (k << VALUE_SHIFT) | @intFromEnum(Tag.keyword);
}

pub fn untagKeyword(r: Ref) u64 {
    std.debug.assert(isKeyword(r));
    return r >> VALUE_SHIFT;
}
