//! Phase 11 — bytecode encoder.
//!
//! Encode + decode helpers for the opcode set locked in ADR-11 addendum.
//! Used by the lang-module compiler (11.2) to emit method bytestreams,
//! and by the VM dispatch loop (11.3) to decode operands as it walks the
//! stream. Owns the wire format — any change here must bump the
//! bytecode-format-version tag in the image manifest (Phase 12.1).

const std = @import("std");
const vm = @import("vm.zig");

pub const Opcode = vm.Opcode;

// -- Encoder -----------------------------------------------------------------

/// Mutable byte buffer for emitting bytecode during compile. Caller owns
/// the backing allocator; the buffer grows as needed.
pub const Encoder = struct {
    buf: std.ArrayList(u8),

    pub fn init(_: std.mem.Allocator) Encoder {
        return .{ .buf = std.ArrayList(u8){} };
    }

    pub fn deinit(self: *Encoder, allocator: std.mem.Allocator) void {
        self.buf.deinit(allocator);
    }

    /// Current write offset — also the operand for forward JUMP/BRANCH
    /// patching once the target is known.
    pub fn pos(self: *const Encoder) usize {
        return self.buf.items.len;
    }

    pub fn toOwnedSlice(self: *Encoder, allocator: std.mem.Allocator) ![]u8 {
        return self.buf.toOwnedSlice(allocator);
    }

    // -- No-operand opcodes -------------------------------------------------

    pub fn pop(self: *Encoder, allocator: std.mem.Allocator) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.POP));
    }
    pub fn loadSelf(self: *Encoder, allocator: std.mem.Allocator) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.LOAD_SELF));
    }
    pub fn loadTrue(self: *Encoder, allocator: std.mem.Allocator) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.LOAD_CONST_TRUE));
    }
    pub fn loadFalse(self: *Encoder, allocator: std.mem.Allocator) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.LOAD_CONST_FALSE));
    }
    pub fn loadNil(self: *Encoder, allocator: std.mem.Allocator) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.LOAD_CONST_NIL));
    }
    pub fn ret(self: *Encoder, allocator: std.mem.Allocator) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.RETURN));
    }

    // -- Single-byte operands ----------------------------------------------

    pub fn pushLocal(self: *Encoder, allocator: std.mem.Allocator, slot: u8) !void {
        try self.buf.appendSlice(allocator, &.{ @intFromEnum(Opcode.PUSH_LOCAL), slot });
    }
    pub fn storeLocal(self: *Encoder, allocator: std.mem.Allocator, slot: u8) !void {
        try self.buf.appendSlice(allocator, &.{ @intFromEnum(Opcode.STORE_LOCAL), slot });
    }
    pub fn returnFromMethod(self: *Encoder, allocator: std.mem.Allocator, depth: u8) !void {
        try self.buf.appendSlice(allocator, &.{ @intFromEnum(Opcode.RETURN_FROM_METHOD), depth });
    }

    // -- u16 operands -------------------------------------------------------

    pub fn pushConst(self: *Encoder, allocator: std.mem.Allocator, pool_idx: u16) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.PUSH_CONST));
        try appendU16(&self.buf, allocator, pool_idx);
    }
    pub fn makeBlock(self: *Encoder, allocator: std.mem.Allocator, pool_idx: u16) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.MAKE_BLOCK));
        try appendU16(&self.buf, allocator, pool_idx);
    }
    pub fn jump(self: *Encoder, allocator: std.mem.Allocator, offset: i16) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.JUMP));
        try appendI16(&self.buf, allocator, offset);
    }
    pub fn branchIfFalse(self: *Encoder, allocator: std.mem.Allocator, offset: i16) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.BRANCH_IF_FALSE));
        try appendI16(&self.buf, allocator, offset);
    }

    // -- i32 operand --------------------------------------------------------

    pub fn pushSmallInt(self: *Encoder, allocator: std.mem.Allocator, n: i32) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.PUSH_SMALLINT_I32));
        try appendI32(&self.buf, allocator, n);
    }

    // -- SEND opcodes -------------------------------------------------------

    pub fn send0(self: *Encoder, allocator: std.mem.Allocator, selector_eid: u32, site_idx: u16) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.SEND_0));
        try appendU32(&self.buf, allocator, selector_eid);
        try appendU16(&self.buf, allocator, site_idx);
    }
    pub fn send1(self: *Encoder, allocator: std.mem.Allocator, selector_eid: u32, site_idx: u16) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.SEND_1));
        try appendU32(&self.buf, allocator, selector_eid);
        try appendU16(&self.buf, allocator, site_idx);
    }
    pub fn sendN(self: *Encoder, allocator: std.mem.Allocator, selector_eid: u32, arg_count: u8, site_idx: u16) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.SEND_N));
        try appendU32(&self.buf, allocator, selector_eid);
        try self.buf.append(allocator, arg_count);
        try appendU16(&self.buf, allocator, site_idx);
    }

    // -- INVOKE_BLOCK ------------------------------------------------------

    pub fn invokeBlock(self: *Encoder, allocator: std.mem.Allocator, arg_count: u8, site_idx: u16) !void {
        try self.buf.append(allocator, @intFromEnum(Opcode.INVOKE_BLOCK));
        try self.buf.append(allocator, arg_count);
        try appendU16(&self.buf, allocator, site_idx);
    }

    // -- Forward-jump patching ---------------------------------------------

    /// Emit a JUMP with placeholder offset; returns the absolute position
    /// of the offset bytes so the caller can patch once the target pc is
    /// known. Same for `branchIfFalse_forward`.
    pub fn jump_forward(self: *Encoder, allocator: std.mem.Allocator) !usize {
        try self.buf.append(allocator, @intFromEnum(Opcode.JUMP));
        const patch_pos = self.buf.items.len;
        try appendI16(&self.buf, allocator, 0);
        return patch_pos;
    }
    pub fn branchIfFalse_forward(self: *Encoder, allocator: std.mem.Allocator) !usize {
        try self.buf.append(allocator, @intFromEnum(Opcode.BRANCH_IF_FALSE));
        const patch_pos = self.buf.items.len;
        try appendI16(&self.buf, allocator, 0);
        return patch_pos;
    }

    /// Patch a 2-byte signed offset emitted by `jump_forward` /
    /// `branchIfFalse_forward`. `offset` is pc-relative from the byte
    /// AFTER the operand (matches decoder convention).
    pub fn patchJump(self: *Encoder, patch_pos: usize, target_pc: usize) !void {
        const after_operand = patch_pos + 2;
        const delta: i64 = @as(i64, @intCast(target_pc)) - @as(i64, @intCast(after_operand));
        if (delta < std.math.minInt(i16) or delta > std.math.maxInt(i16)) return error.JumpOutOfRange;
        writeI16At(self.buf.items, patch_pos, @intCast(delta));
    }
};

// -- Decoder helpers ---------------------------------------------------------

pub fn readOpcode(bytes: []const u8, pc: usize) Opcode {
    return @enumFromInt(bytes[pc]);
}
pub fn readU8(bytes: []const u8, pc: usize) u8 {
    return bytes[pc];
}
pub fn readU16(bytes: []const u8, pc: usize) u16 {
    return std.mem.readInt(u16, bytes[pc..][0..2], .little);
}
pub fn readI16(bytes: []const u8, pc: usize) i16 {
    return std.mem.readInt(i16, bytes[pc..][0..2], .little);
}
pub fn readU32(bytes: []const u8, pc: usize) u32 {
    return std.mem.readInt(u32, bytes[pc..][0..4], .little);
}
pub fn readI32(bytes: []const u8, pc: usize) i32 {
    return std.mem.readInt(i32, bytes[pc..][0..4], .little);
}

// -- internal helpers --------------------------------------------------------

fn appendU16(buf: *std.ArrayList(u8), allocator: std.mem.Allocator, v: u16) !void {
    var bytes: [2]u8 = undefined;
    std.mem.writeInt(u16, &bytes, v, .little);
    try buf.appendSlice(allocator, &bytes);
}
fn appendI16(buf: *std.ArrayList(u8), allocator: std.mem.Allocator, v: i16) !void {
    var bytes: [2]u8 = undefined;
    std.mem.writeInt(i16, &bytes, v, .little);
    try buf.appendSlice(allocator, &bytes);
}
fn appendU32(buf: *std.ArrayList(u8), allocator: std.mem.Allocator, v: u32) !void {
    var bytes: [4]u8 = undefined;
    std.mem.writeInt(u32, &bytes, v, .little);
    try buf.appendSlice(allocator, &bytes);
}
fn appendI32(buf: *std.ArrayList(u8), allocator: std.mem.Allocator, v: i32) !void {
    var bytes: [4]u8 = undefined;
    std.mem.writeInt(i32, &bytes, v, .little);
    try buf.appendSlice(allocator, &bytes);
}

fn writeI16At(bytes: []u8, offset: usize, v: i16) void {
    std.mem.writeInt(i16, bytes[offset..][0..2], v, .little);
}
