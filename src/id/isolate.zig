//! Blocking process-isolate substrate, replacement policy mechanism, and
//! mailbox-safe live rollout transport (designs 48–50, I1a/I1b/L2).
//!
//! This module owns the versioned binary wire format and the process-local
//! handle/failure representations.  Process launch and the worker loop are kept
//! here as well so the host only needs to provide image recovery and phase
//! transitions.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const IdApi = idapi.IdApi;
const ca = std.heap.c_allocator;

pub const magic = "BISO";
pub const version: u16 = 1;
pub const header_len: usize = 20;
pub const max_frame: usize = 16 * 1024 * 1024;
pub const max_depth: usize = 64;
pub const max_container: usize = 65_536;
pub const max_nodes: usize = 1_000_000;

pub const FrameKind = enum(u8) {
    ready = 1,
    boot_error = 2,
    request = 3,
    reply_ok = 4,
    reply_error = 5,
    stop = 6,
    stopped = 7,
    protocol_error = 8,
    evolution_request = 9,
    evolution_reply = 10,
};

pub const Header = struct {
    kind: FrameKind,
    request_id: u64,
    payload_len: u32,
};

pub const CodecError = error{
    BadMagic,
    BadVersion,
    BadFlags,
    UnknownKind,
    InvalidRequestId,
    FrameTooLarge,
    Truncated,
    TrailingBytes,
    UnknownTag,
    TooDeep,
    TooManyNodes,
    ContainerTooLarge,
    InvalidUtf8,
    InvalidMapKey,
    DuplicateMapKey,
    UnknownRecordSchema,
    InvalidRecord,
    CyclicValue,
    NonTransferable,
    OutOfMemory,
};

pub fn encodeHeader(h: Header) CodecError![header_len]u8 {
    if (h.payload_len > max_frame) return error.FrameTooLarge;
    try validateRequestId(h.kind, h.request_id);
    var out: [header_len]u8 = [_]u8{0} ** header_len;
    @memcpy(out[0..4], magic);
    std.mem.writeInt(u16, out[4..6], version, .big);
    out[6] = @intFromEnum(h.kind);
    out[7] = 0;
    std.mem.writeInt(u64, out[8..16], h.request_id, .big);
    std.mem.writeInt(u32, out[16..20], h.payload_len, .big);
    return out;
}

pub fn decodeHeader(bytes: []const u8) CodecError!Header {
    if (bytes.len < header_len) return error.Truncated;
    if (!std.mem.eql(u8, bytes[0..4], magic)) return error.BadMagic;
    if (std.mem.readInt(u16, bytes[4..6], .big) != version) return error.BadVersion;
    if (bytes[7] != 0) return error.BadFlags;
    const kind = std.meta.intToEnum(FrameKind, bytes[6]) catch return error.UnknownKind;
    const request_id = std.mem.readInt(u64, bytes[8..16], .big);
    const payload_len = std.mem.readInt(u32, bytes[16..20], .big);
    if (payload_len > max_frame) return error.FrameTooLarge;
    try validateRequestId(kind, request_id);
    return .{ .kind = kind, .request_id = request_id, .payload_len = payload_len };
}

fn validateRequestId(kind: FrameKind, id: u64) CodecError!void {
    if ((kind == .ready or kind == .boot_error) and id != 0) return error.InvalidRequestId;
    if (kind != .ready and kind != .boot_error and kind != .protocol_error and id == 0)
        return error.InvalidRequestId;
    if (id > std.math.maxInt(i64)) return error.InvalidRequestId;
}

const Tag = enum(u8) {
    nil = 0,
    false_ = 1,
    true_ = 2,
    int = 3,
    float = 4,
    string = 5,
    symbol = 6,
    vector = 7,
    map = 8,
    record = 9,
};

const Encoder = struct {
    api: *IdApi,
    buf: std.ArrayListUnmanaged(u8) = .{},
    active: std.AutoHashMapUnmanaged(usize, void) = .{},
    nodes: usize = 0,

    fn deinit(self: *Encoder) void {
        self.buf.deinit(ca);
        self.active.deinit(ca);
    }

    fn reserve(self: *Encoder, n: usize) CodecError!void {
        if (self.buf.items.len > max_frame or n > max_frame - self.buf.items.len)
            return error.FrameTooLarge;
        self.buf.ensureUnusedCapacity(ca, n) catch return error.OutOfMemory;
    }
    fn byte(self: *Encoder, v: u8) CodecError!void {
        try self.reserve(1);
        self.buf.appendAssumeCapacity(v);
    }
    fn putU16(self: *Encoder, v: u16) CodecError!void {
        try self.reserve(2);
        var x: [2]u8 = undefined;
        std.mem.writeInt(u16, &x, v, .big);
        self.buf.appendSliceAssumeCapacity(&x);
    }
    fn putU32(self: *Encoder, v: u32) CodecError!void {
        try self.reserve(4);
        var x: [4]u8 = undefined;
        std.mem.writeInt(u32, &x, v, .big);
        self.buf.appendSliceAssumeCapacity(&x);
    }
    fn putU64(self: *Encoder, v: u64) CodecError!void {
        try self.reserve(8);
        var x: [8]u8 = undefined;
        std.mem.writeInt(u64, &x, v, .big);
        self.buf.appendSliceAssumeCapacity(&x);
    }
    fn bytes(self: *Encoder, data: []const u8) CodecError!void {
        if (data.len > max_frame) return error.FrameTooLarge;
        try self.putU32(@intCast(data.len));
        try self.reserve(data.len);
        self.buf.appendSliceAssumeCapacity(data);
    }
    fn countNode(self: *Encoder) CodecError!void {
        if (self.nodes == max_nodes) return error.TooManyNodes;
        self.nodes += 1;
    }

    fn value(self: *Encoder, value_: Oop, depth: usize) CodecError!void {
        if (depth > max_depth) return error.TooDeep;
        try self.countNode();
        if (value_ == self.api.nilObj) return self.byte(@intFromEnum(Tag.nil));
        if (value_ == self.api.falseObj) return self.byte(@intFromEnum(Tag.false_));
        if (value_ == self.api.trueObj) return self.byte(@intFromEnum(Tag.true_));
        if (self.api.isSmallInt(value_) != 0) {
            try self.byte(@intFromEnum(Tag.int));
            return self.putU64(@bitCast(self.api.smallIntValue(value_)));
        }
        if (self.api.isFloat(value_) != 0) {
            try self.byte(@intFromEnum(Tag.float));
            return self.putU64(@bitCast(self.api.floatValue(value_)));
        }
        const vt = self.api.vtableOf(value_);
        if (vt == self.api.string_vt) {
            var len: usize = 0;
            const ptr = self.api.stringBytes(value_, &len);
            try self.byte(@intFromEnum(Tag.string));
            return self.bytes(ptr[0..len]);
        }
        if (vt == self.api.symbol_vt) {
            const name = std.mem.span(self.api.symbolName(value_));
            if (!std.unicode.utf8ValidateSlice(name)) return error.InvalidUtf8;
            try self.byte(@intFromEnum(Tag.symbol));
            return self.bytes(name);
        }

        const vector_vt = self.api.import("VectorVt");
        const map_vt = self.api.import("MapVt");
        if (vt != vector_vt and vt != map_vt) return error.NonTransferable;
        const address = @intFromPtr(value_.?);
        if (self.active.contains(address)) return error.CyclicValue;
        self.active.put(ca, address, {}) catch return error.OutOfMemory;
        defer _ = self.active.remove(address);

        const size_o = self.api.send(value_, self.api.intern("size"), &.{}, 0);
        if (self.api.isSmallInt(size_o) == 0) return error.NonTransferable;
        const signed_size = self.api.smallIntValue(size_o);
        if (signed_size < 0 or signed_size > max_container) return error.ContainerTooLarge;
        const size: usize = @intCast(signed_size);
        if (vt == vector_vt) {
            try self.byte(@intFromEnum(Tag.vector));
            try self.putU32(@intCast(size));
            for (0..size) |i| {
                const item = self.api.send(value_, self.api.intern("at:"), &[_]Oop{self.api.mkSmallInt(@intCast(i))}, 1);
                try self.value(item, depth + 1);
            }
            return;
        }

        try self.byte(@intFromEnum(Tag.map));
        try self.putU32(@intCast(size));
        const keys = self.api.send(value_, self.api.intern("keys"), &.{}, 0);
        for (0..size) |i| {
            const key = self.api.send(keys, self.api.intern("at:"), &[_]Oop{self.api.mkSmallInt(@intCast(i))}, 1);
            if (!validMapKey(self.api, key)) return error.InvalidMapKey;
            try self.value(key, depth + 1);
            const val = self.api.send(value_, self.api.intern("at:"), &[_]Oop{key}, 1);
            try self.value(val, depth + 1);
        }
    }
};

fn validMapKey(api: *IdApi, key: Oop) bool {
    return key == api.nilObj or key == api.trueObj or key == api.falseObj or
        api.isSmallInt(key) != 0 or api.vtableOf(key) == api.string_vt or
        api.vtableOf(key) == api.symbol_vt;
}

pub fn encodeValue(api: *IdApi, value: Oop) CodecError![]u8 {
    var encoder: Encoder = .{ .api = api };
    errdefer encoder.deinit();
    try encoder.value(value, 0);
    encoder.active.deinit(ca);
    return encoder.buf.toOwnedSlice(ca) catch error.OutOfMemory;
}

const Decoder = struct {
    api: *IdApi,
    bytes: []const u8,
    pos: usize = 0,
    nodes: usize = 0,

    fn take(self: *Decoder, n: usize) CodecError![]const u8 {
        if (n > self.bytes.len - self.pos) return error.Truncated;
        const out = self.bytes[self.pos .. self.pos + n];
        self.pos += n;
        return out;
    }
    fn byte(self: *Decoder) CodecError!u8 {
        return (try self.take(1))[0];
    }
    fn getU16(self: *Decoder) CodecError!u16 {
        return std.mem.readInt(u16, (try self.take(2))[0..2], .big);
    }
    fn getU32(self: *Decoder) CodecError!u32 {
        return std.mem.readInt(u32, (try self.take(4))[0..4], .big);
    }
    fn getU64(self: *Decoder) CodecError!u64 {
        return std.mem.readInt(u64, (try self.take(8))[0..8], .big);
    }
    fn sized(self: *Decoder) CodecError![]const u8 {
        const len = try self.getU32();
        if (len > max_frame) return error.FrameTooLarge;
        return self.take(len);
    }
    fn countNode(self: *Decoder) CodecError!void {
        if (self.nodes == max_nodes) return error.TooManyNodes;
        self.nodes += 1;
    }
    fn value(self: *Decoder, depth: usize) CodecError!Oop {
        if (depth > max_depth) return error.TooDeep;
        try self.countNode();
        const tag = std.meta.intToEnum(Tag, try self.byte()) catch return error.UnknownTag;
        return switch (tag) {
            .nil => self.api.nilObj,
            .false_ => self.api.falseObj,
            .true_ => self.api.trueObj,
            .int => self.api.mkSmallInt(@bitCast(try self.getU64())),
            .float => self.api.mkFloat(@bitCast(try self.getU64())),
            .string => blk: {
                const str = try self.sized();
                break :blk self.api.mkString(str.ptr, str.len);
            },
            .symbol => blk: {
                const name = try self.sized();
                if (!std.unicode.utf8ValidateSlice(name) or std.mem.indexOfScalar(u8, name, 0) != null)
                    return error.InvalidUtf8;
                const z = ca.allocSentinel(u8, name.len, 0) catch return error.OutOfMemory;
                defer ca.free(z);
                @memcpy(z[0..name.len], name);
                break :blk self.api.intern(z.ptr);
            },
            .vector => self.vector(depth),
            .map => self.map(depth),
            .record => return error.UnknownRecordSchema,
        };
    }
    fn vector(self: *Decoder, depth: usize) CodecError!Oop {
        const count = try self.getU32();
        if (count > max_container) return error.ContainerTooLarge;
        var out = self.api.send(self.api.import("Vector"), self.api.intern("new"), &.{}, 0);
        for (0..count) |_| {
            const item = try self.value(depth + 1);
            out = self.api.send(out, self.api.intern("add:"), &[_]Oop{item}, 1);
        }
        return out;
    }
    fn map(self: *Decoder, depth: usize) CodecError!Oop {
        const count = try self.getU32();
        if (count > max_container) return error.ContainerTooLarge;
        var out = self.api.send(self.api.import("Map"), self.api.intern("new"), &.{}, 0);
        for (0..count) |_| {
            const key = try self.value(depth + 1);
            if (!validMapKey(self.api, key)) return error.InvalidMapKey;
            if (self.api.send(out, self.api.intern("includesKey:"), &[_]Oop{key}, 1) == self.api.trueObj)
                return error.DuplicateMapKey;
            const val = try self.value(depth + 1);
            out = self.api.send(out, self.api.intern("at:put:"), &[_]Oop{ key, val }, 2);
        }
        return out;
    }
};

pub fn decodeValue(api: *IdApi, bytes: []const u8) CodecError!Oop {
    var decoder: Decoder = .{ .api = api, .bytes = bytes };
    const out = try decoder.value(0);
    if (decoder.pos != bytes.len) return error.TrailingBytes;
    return out;
}

pub const TransportError = error{
    Timeout,
    EndOfStream,
    TruncatedFrame,
    WriteFailed,
    ReadFailed,
    Protocol,
    OutOfMemory,
};

pub const Frame = struct {
    header: Header,
    payload: []u8,

    pub fn deinit(self: Frame) void {
        ca.free(self.payload);
    }
};

fn remainingMs(start: std.time.Instant, budget_ms: u32) TransportError!i32 {
    const now = std.time.Instant.now() catch return error.Timeout;
    const elapsed_ms = now.since(start) / std.time.ns_per_ms;
    if (elapsed_ms >= budget_ms) return error.Timeout;
    return @intCast(@min(budget_ms - elapsed_ms, std.math.maxInt(i32)));
}

fn waitFd(fd: std.posix.fd_t, events: i16, start: std.time.Instant, budget_ms: u32) TransportError!void {
    var fds = [_]std.posix.pollfd{.{ .fd = fd, .events = events, .revents = 0 }};
    const n = std.posix.poll(&fds, try remainingMs(start, budget_ms)) catch return error.ReadFailed;
    if (n == 0) return error.Timeout;
    if ((fds[0].revents & events) == 0) return error.ReadFailed;
}

fn readExact(fd: std.posix.fd_t, dest: []u8, budget_ms: u32, allow_clean_eof: bool) TransportError!void {
    const start = std.time.Instant.now() catch return error.Timeout;
    var pos: usize = 0;
    while (pos < dest.len) {
        try waitFd(fd, std.posix.POLL.IN, start, budget_ms);
        const n = std.posix.read(fd, dest[pos..]) catch |e| switch (e) {
            error.WouldBlock => continue,
            else => return error.ReadFailed,
        };
        if (n == 0) return if (pos == 0 and allow_clean_eof) error.EndOfStream else error.TruncatedFrame;
        pos += n;
    }
}

fn writeExact(fd: std.posix.fd_t, data: []const u8, budget_ms: u32) TransportError!void {
    const start = std.time.Instant.now() catch return error.Timeout;
    var pos: usize = 0;
    while (pos < data.len) {
        try waitFd(fd, std.posix.POLL.OUT, start, budget_ms);
        const n = std.posix.send(fd, data[pos..], std.c.MSG.NOSIGNAL) catch |e| switch (e) {
            error.WouldBlock => continue,
            else => return error.WriteFailed,
        };
        if (n == 0) return error.WriteFailed;
        pos += n;
    }
}

pub fn writeFrame(fd: std.posix.fd_t, kind: FrameKind, request_id: u64, payload: []const u8, timeout_ms: u32) TransportError!void {
    const header = encodeHeader(.{ .kind = kind, .request_id = request_id, .payload_len = @intCast(payload.len) }) catch return error.Protocol;
    try writeExact(fd, &header, timeout_ms);
    if (payload.len != 0) try writeExact(fd, payload, timeout_ms);
}

pub fn readFrame(fd: std.posix.fd_t, timeout_ms: u32) TransportError!Frame {
    var raw: [header_len]u8 = undefined;
    try readExact(fd, &raw, timeout_ms, true);
    const header = decodeHeader(&raw) catch return error.Protocol;
    const payload = ca.alloc(u8, header.payload_len) catch return error.OutOfMemory;
    errdefer ca.free(payload);
    if (payload.len != 0) try readExact(fd, payload, timeout_ms, false);
    const empty = header.kind == .ready or header.kind == .stop or header.kind == .stopped;
    if (empty != (payload.len == 0)) return error.Protocol;
    return .{ .header = header, .payload = payload };
}

const error_schemas = [_][]const u8{
    "birnam.boot-error.v1",
    "birnam.application-error.v1",
    "birnam.protocol-error.v1",
};

fn encodeErrorRecord(api: *IdApi, schema: []const u8, kind: Oop, message: Oop) CodecError![]u8 {
    var e: Encoder = .{ .api = api };
    errdefer e.deinit();
    try e.byte(@intFromEnum(Tag.record));
    if (schema.len > std.math.maxInt(u16)) return error.FrameTooLarge;
    try e.putU16(@intCast(schema.len));
    try e.reserve(schema.len);
    e.buf.appendSliceAssumeCapacity(schema);
    try e.putU16(2);
    if (api.vtableOf(kind) != api.symbol_vt or api.vtableOf(message) != api.string_vt)
        return error.InvalidRecord;
    try e.value(kind, 1);
    try e.value(message, 1);
    e.active.deinit(ca);
    return e.buf.toOwnedSlice(ca) catch error.OutOfMemory;
}

const DecodedError = struct { kind: Oop, message: Oop };
fn decodeErrorRecord(api: *IdApi, schema: []const u8, bytes: []const u8) CodecError!DecodedError {
    var d: Decoder = .{ .api = api, .bytes = bytes };
    const tag = std.meta.intToEnum(Tag, try d.byte()) catch return error.UnknownTag;
    if (tag != .record) return error.InvalidRecord;
    const name_len = try d.getU16();
    const name = try d.take(name_len);
    if (!std.mem.eql(u8, name, schema)) return error.UnknownRecordSchema;
    if (try d.getU16() != 2) return error.InvalidRecord;
    const kind = try d.value(1);
    const message = try d.value(1);
    if (api.vtableOf(kind) != api.symbol_vt or api.vtableOf(message) != api.string_vt)
        return error.InvalidRecord;
    if (d.pos != bytes.len) return error.TrailingBytes;
    return .{ .kind = kind, .message = message };
}

const evolution_activate_schema = "birnam.evolution-activate.v1";
const evolution_rollback_schema = "birnam.evolution-rollback.v1";
const evolution_receipt_schema = "birnam.evolution-receipt.v1";

fn encodeRecordValues(api: *IdApi, schema: []const u8, values: []const Oop) CodecError![]u8 {
    var e: Encoder = .{ .api = api };
    errdefer e.deinit();
    try e.byte(@intFromEnum(Tag.record));
    if (schema.len > std.math.maxInt(u16) or values.len > std.math.maxInt(u16)) return error.FrameTooLarge;
    try e.putU16(@intCast(schema.len));
    try e.reserve(schema.len);
    e.buf.appendSliceAssumeCapacity(schema);
    try e.putU16(@intCast(values.len));
    for (values) |value| try e.value(value, 1);
    e.active.deinit(ca);
    return e.buf.toOwnedSlice(ca) catch error.OutOfMemory;
}

fn makeVector(api: *IdApi, values: []const Oop) Oop {
    var out = api.send(api.import("Vector"), api.intern("new"), &.{}, 0);
    for (values) |value| out = api.send(out, api.intern("add:"), &[_]Oop{value}, 1);
    return out;
}

fn vectorValues(api: *IdApi, value: Oop, expected: usize, out: []Oop) bool {
    if (api.vtableOf(value) != api.import("VectorVt") or out.len != expected) return false;
    const size = api.send(value, api.intern("size"), &.{}, 0);
    if (api.isSmallInt(size) == 0 or api.smallIntValue(size) != expected) return false;
    for (out, 0..) |*slot, i|
        slot.* = api.send(value, api.intern("at:"), &[_]Oop{api.mkSmallInt(@intCast(i))}, 1);
    return true;
}

fn isDigest(api: *IdApi, value: Oop) bool {
    if (api.vtableOf(value) != api.string_vt) return false;
    var len: usize = 0;
    const bytes = api.stringBytes(value, &len)[0..len];
    if (bytes.len != 64) return false;
    for (bytes) |b| if (!std.ascii.isDigit(b) and !(b >= 'a' and b <= 'f')) return false;
    return true;
}

fn contentAddressMatches(api: *IdApi, location: Oop, checksum: Oop) bool {
    if (api.vtableOf(location) != api.string_vt or !isDigest(api, checksum)) return false;
    var path_len: usize = 0;
    var checksum_len: usize = 0;
    const path = api.stringBytes(location, &path_len)[0..path_len];
    const digest = api.stringBytes(checksum, &checksum_len)[0..checksum_len];
    const base = std.fs.path.basename(path);
    return base.len == 70 and std.mem.eql(u8, base[0..64], digest) and std.mem.eql(u8, base[64..], ".dylib");
}

pub fn encodeEvolutionRequest(api: *IdApi, request: Oop) CodecError![]u8 {
    var short: [2]Oop = undefined;
    if (vectorValues(api, request, 2, &short) and short[0] == api.intern("rollback")) {
        if (!isDigest(api, short[1])) return error.InvalidRecord;
        return encodeRecordValues(api, evolution_rollback_schema, short[1..2]);
    }
    var fields: [7]Oop = undefined;
    if (!vectorValues(api, request, 7, &fields) or fields[0] != api.intern("activate")) return error.InvalidRecord;
    if (api.vtableOf(fields[1]) != api.symbol_vt or !isDigest(api, fields[2]) or
        (fields[3] != api.nilObj and api.vtableOf(fields[3]) != api.string_vt) or
        !isDigest(api, fields[4]) or !contentAddressMatches(api, fields[5], fields[4]) or !isDigest(api, fields[6]))
        return error.InvalidRecord;
    return encodeRecordValues(api, evolution_activate_schema, fields[1..7]);
}

pub const DecodedEvolution = struct { request: Oop, operation: Oop, module: Oop, accepted_base: Oop };

pub fn decodeEvolutionRequest(api: *IdApi, bytes: []const u8) CodecError!DecodedEvolution {
    var d: Decoder = .{ .api = api, .bytes = bytes };
    const tag = std.meta.intToEnum(Tag, try d.byte()) catch return error.UnknownTag;
    if (tag != .record) return error.InvalidRecord;
    const name = try d.take(try d.getU16());
    const count = try d.getU16();
    if (std.mem.eql(u8, name, evolution_rollback_schema)) {
        if (count != 1) return error.InvalidRecord;
        const expected = try d.value(1);
        if (!isDigest(api, expected) or d.pos != bytes.len) return error.InvalidRecord;
        const op = api.intern("rollback");
        return .{ .request = makeVector(api, &.{ op, expected }), .operation = op, .module = api.nilObj, .accepted_base = expected };
    }
    if (!std.mem.eql(u8, name, evolution_activate_schema) or count != 6) return error.UnknownRecordSchema;
    var fields: [6]Oop = undefined;
    for (&fields) |*field| field.* = try d.value(1);
    if (d.pos != bytes.len or api.vtableOf(fields[0]) != api.symbol_vt or !isDigest(api, fields[1]) or
        (fields[2] != api.nilObj and api.vtableOf(fields[2]) != api.string_vt) or
        !isDigest(api, fields[3]) or !contentAddressMatches(api, fields[4], fields[3]) or !isDigest(api, fields[5]))
        return error.InvalidRecord;
    const op = api.intern("activate");
    return .{
        .request = makeVector(api, &.{ op, fields[0], fields[1], fields[2], fields[3], fields[4], fields[5] }),
        .operation = op,
        .module = fields[0],
        .accepted_base = fields[5],
    };
}

var g_api: *IdApi = undefined;
pub var isolate_vt: Oop = null;
pub var isolate_obj: Oop = null;
pub var handle_vt: Oop = null;
pub var failure_vt: Oop = null;
pub var replacement_vt: Oop = null;
pub var replacement_obj: Oop = null;
pub var receipt_vt: Oop = null;
pub var receipt_obj: Oop = null;

const Handle = extern struct { token: u64 };
const Failure = extern struct {
    class: Oop,
    kind: Oop,
    message: Oop,
    exit_status: Oop,
    request_id: Oop,
};
const EvolutionReceipt = extern struct {
    operation: Oop,
    module: Oop,
    accepted_base: Oop,
    prior_digest: Oop,
    current_digest: Oop,
    outcome: Oop,
    error_kind: Oop,
    message: Oop,
};

fn failure(o: Oop) *Failure {
    return @ptrCast(@alignCast(o.?));
}
fn handle(o: Oop) *Handle {
    return @ptrCast(@alignCast(o.?));
}
fn receipt(o: Oop) *EvolutionReceipt {
    return @ptrCast(@alignCast(o.?));
}

pub fn makeReceipt(fields: [8]Oop) Oop {
    const out = g_api.alloc(receipt_vt, @sizeOf(EvolutionReceipt));
    receipt(out).* = .{
        .operation = fields[0],
        .module = fields[1],
        .accepted_base = fields[2],
        .prior_digest = fields[3],
        .current_digest = fields[4],
        .outcome = fields[5],
        .error_kind = fields[6],
        .message = fields[7],
    };
    return out;
}

fn receiptFields(value: Oop) [8]Oop {
    const r = receipt(value);
    return .{ r.operation, r.module, r.accepted_base, r.prior_digest, r.current_digest, r.outcome, r.error_kind, r.message };
}

fn validReceiptFields(api: *IdApi, fields: [8]Oop) bool {
    return api.vtableOf(fields[0]) == api.symbol_vt and
        (fields[1] == api.nilObj or api.vtableOf(fields[1]) == api.symbol_vt) and
        isDigest(api, fields[2]) and isDigest(api, fields[3]) and isDigest(api, fields[4]) and
        api.vtableOf(fields[5]) == api.symbol_vt and
        (fields[6] == api.nilObj or api.vtableOf(fields[6]) == api.symbol_vt) and
        api.vtableOf(fields[7]) == api.string_vt;
}

pub fn encodeEvolutionReceipt(api: *IdApi, value: Oop) CodecError![]u8 {
    const fields = receiptFields(value);
    if (!validReceiptFields(api, fields)) return error.InvalidRecord;
    return encodeRecordValues(api, evolution_receipt_schema, &fields);
}

pub fn decodeEvolutionReceipt(api: *IdApi, bytes: []const u8) CodecError!Oop {
    var d: Decoder = .{ .api = api, .bytes = bytes };
    const tag = std.meta.intToEnum(Tag, try d.byte()) catch return error.UnknownTag;
    if (tag != .record) return error.InvalidRecord;
    const name = try d.take(try d.getU16());
    if (!std.mem.eql(u8, name, evolution_receipt_schema) or try d.getU16() != 8) return error.UnknownRecordSchema;
    var fields: [8]Oop = undefined;
    for (&fields) |*field| field.* = try d.value(1);
    if (d.pos != bytes.len) return error.TrailingBytes;
    if (!validReceiptFields(api, fields)) return error.InvalidRecord;
    return makeReceipt(fields);
}

const Lifecycle = enum { starting, ready, busy, stopping, reap_pending, exited };
const Entry = struct {
    pid: std.posix.pid_t,
    fd: std.posix.fd_t,
    state: Lifecycle,
    next_request: u64 = 1,
    exit_status: ?u32 = null,
    orphaned: bool = false,
    term_at: ?std.time.Instant = null,
    kill_at: ?std.time.Instant = null,
};

// Deterministic process-fault injection for the acceptance suite. These hooks
// are intentionally environment-only: they do not add selectors or state to the
// Birnam API, and normal processes pay only the getenv checks on shutdown paths.
fn testFault(name: []const u8) bool {
    const faults = std.posix.getenv("BIRNAM_TEST_ISOLATE_FAULTS") orelse return false;
    var it = std.mem.splitScalar(u8, faults, ',');
    while (it.next()) |fault| if (std.mem.eql(u8, fault, name)) return true;
    return false;
}

fn cleanupStageBudget() u64 {
    return if (testFault("fast-escalation")) 20 * std.time.ns_per_ms else 2 * std.time.ns_per_s;
}

var registry: std.AutoHashMapUnmanaged(u64, Entry) = .{};
var next_token: u64 = 1;

fn registryEntry(token: u64) ?*Entry {
    return registry.getPtr(token);
}

fn finaliseHandle(o: Oop) callconv(.c) void {
    if (registryEntry(handle(o).token)) |entry| entry.orphaned = true;
}

fn mkFailure(class: [*:0]const u8, kind: [*:0]const u8, message: []const u8, status: ?u32, request_id: ?u64) Oop {
    const status_o = if (status) |s| g_api.mkSmallInt(@intCast(s)) else g_api.nilObj;
    const request_o = if (request_id) |r| g_api.mkSmallInt(@intCast(r)) else g_api.nilObj;
    return makeFailure(g_api.intern(class), g_api.intern(kind), g_api.mkString(message.ptr, message.len), status_o, request_o);
}

fn statusOf(entry: *Entry) ?u32 {
    if (entry.state == .exited) return entry.exit_status;
    const got = std.posix.waitpid(entry.pid, std.posix.W.NOHANG);
    if (got.pid == 0) return null;
    entry.exit_status = got.status;
    entry.state = .exited;
    if (entry.fd >= 0) {
        std.posix.close(entry.fd);
        entry.fd = -1;
    }
    return got.status;
}

fn terminate(entry: *Entry) void {
    if (statusOf(entry) != null) return;
    if (entry.fd >= 0) {
        std.posix.close(entry.fd);
        entry.fd = -1;
    }
    // A cooperative worker gets a bounded grace window after channel closure.
    // Only then do we escalate TERM and KILL, each with its own cleanup budget.
    const start = std.time.Instant.now() catch {
        entry.state = .reap_pending;
        return;
    };
    while (true) {
        const now = std.time.Instant.now() catch break;
        if (now.since(start) >= cleanupStageBudget()) break;
        if (statusOf(entry) != null) return;
        std.Thread.sleep(10 * std.time.ns_per_ms);
    }
    std.posix.kill(entry.pid, std.posix.SIG.TERM) catch {};
    entry.term_at = std.time.Instant.now() catch null;
    const term_start = std.time.Instant.now() catch {
        entry.state = .reap_pending;
        return;
    };
    while (true) {
        const now = std.time.Instant.now() catch break;
        if (now.since(term_start) >= cleanupStageBudget()) break;
        if (statusOf(entry) != null) return;
        std.Thread.sleep(10 * std.time.ns_per_ms);
    }
    std.posix.kill(entry.pid, std.posix.SIG.KILL) catch {};
    entry.kill_at = std.time.Instant.now() catch null;
    if (testFault("reap-pending")) {
        entry.state = .reap_pending;
        return;
    }
    const kill_start = std.time.Instant.now() catch {
        entry.state = .reap_pending;
        return;
    };
    while (true) {
        const now = std.time.Instant.now() catch break;
        if (now.since(kill_start) >= cleanupStageBudget()) break;
        if (statusOf(entry) != null) return;
        std.Thread.sleep(10 * std.time.ns_per_ms);
    }
    entry.state = .reap_pending;
}

extern var environ: [*:null]?[*:0]u8;
const worker_fd: std.posix.fd_t = 198;

fn oopText(api: *IdApi, value: Oop) ?[]const u8 {
    if (api.vtableOf(value) == api.symbol_vt) return std.mem.span(api.symbolName(value));
    if (api.vtableOf(value) == api.string_vt) {
        var len: usize = 0;
        const p = api.stringBytes(value, &len);
        return p[0..len];
    }
    return null;
}

fn arityOne(selector: []const u8) bool {
    const colons = std.mem.count(u8, selector, ":");
    if (colons != 0) return colons == 1;
    return selector.len != 0 and !std.ascii.isAlphabetic(selector[0]);
}

fn spawnWorker(image: []const u8, receiver: []const u8, selector: []const u8) !Entry {
    const exe = try std.fs.selfExePathAlloc(ca);
    defer ca.free(exe);
    const absolute_image = try std.fs.realpathAlloc(ca, image);
    defer ca.free(absolute_image);

    var sv: [2]std.posix.fd_t = undefined;
    if (std.c.socketpair(std.c.AF.UNIX, std.c.SOCK.STREAM, 0, &sv) != 0) return error.SocketPairFailed;
    errdefer {
        std.posix.close(sv[0]);
        std.posix.close(sv[1]);
    }
    _ = try std.posix.fcntl(sv[0], std.posix.F.SETFD, std.posix.FD_CLOEXEC);
    _ = try std.posix.fcntl(sv[1], std.posix.F.SETFD, std.posix.FD_CLOEXEC);
    var one: c_int = 1;
    if (std.c.setsockopt(sv[0], std.c.SOL.SOCKET, std.c.SO.NOSIGPIPE, &one, @sizeOf(c_int)) != 0)
        return error.SocketOptionFailed;
    if (std.c.setsockopt(sv[1], std.c.SOL.SOCKET, std.c.SO.NOSIGPIPE, &one, @sizeOf(c_int)) != 0)
        return error.SocketOptionFailed;

    const exe_z = try ca.dupeZ(u8, exe);
    defer ca.free(exe_z);
    const image_z = try ca.dupeZ(u8, absolute_image);
    defer ca.free(image_z);
    const receiver_z = try ca.dupeZ(u8, receiver);
    defer ca.free(receiver_z);
    const selector_z = try ca.dupeZ(u8, selector);
    defer ca.free(selector_z);
    const fd_z = try std.fmt.allocPrintSentinel(ca, "{d}", .{worker_fd}, 0);
    defer ca.free(fd_z);

    var actions: std.c.posix_spawn_file_actions_t = undefined;
    if (std.c.posix_spawn_file_actions_init(&actions) != 0) return error.SpawnSetupFailed;
    defer _ = std.c.posix_spawn_file_actions_destroy(&actions);
    if (std.c.posix_spawn_file_actions_addinherit_np(&actions, std.posix.STDIN_FILENO) != 0) return error.SpawnSetupFailed;
    if (std.c.posix_spawn_file_actions_addinherit_np(&actions, std.posix.STDOUT_FILENO) != 0) return error.SpawnSetupFailed;
    if (std.c.posix_spawn_file_actions_addinherit_np(&actions, std.posix.STDERR_FILENO) != 0) return error.SpawnSetupFailed;
    if (std.c.posix_spawn_file_actions_adddup2(&actions, sv[1], worker_fd) != 0) return error.SpawnSetupFailed;
    if (std.c.posix_spawn_file_actions_addclose(&actions, sv[0]) != 0) return error.SpawnSetupFailed;
    if (std.c.posix_spawn_file_actions_addclose(&actions, sv[1]) != 0) return error.SpawnSetupFailed;
    var attr: std.c.posix_spawnattr_t = undefined;
    if (std.c.posix_spawnattr_init(&attr) != 0) return error.SpawnSetupFailed;
    defer _ = std.c.posix_spawnattr_destroy(&attr);
    if (std.c.posix_spawnattr_setflags(&attr, std.c.POSIX_SPAWN.CLOEXEC_DEFAULT) != 0) return error.SpawnSetupFailed;
    const argv = [_:null]?[*:0]const u8{
        exe_z.ptr, "--worker", image_z.ptr, receiver_z.ptr, selector_z.ptr, "--channel-fd", fd_z.ptr,
    };
    var pid: std.posix.pid_t = undefined;
    if (std.c.posix_spawn(&pid, exe_z.ptr, &actions, &attr, &argv, @ptrCast(environ)) != 0)
        return error.SpawnFailed;
    std.posix.close(sv[1]);
    return .{ .pid = pid, .fd = sv[0], .state = .starting };
}

fn newHandle(entry: Entry) ?Oop {
    if (next_token == 0) return null;
    const token = next_token;
    if (next_token == std.math.maxInt(u64)) next_token = 0 else next_token += 1;
    registry.put(ca, token, entry) catch return null;
    const out = g_api.alloc(handle_vt, @sizeOf(Handle));
    handle(out).token = token;
    g_api.setFinalizer(out, &finaliseHandle);
    return out;
}

fn startWorkerWithReadyTimeout(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    drainImpl();
    const receiver = oopText(g_api, args[0]) orelse return mkFailure("encode", "invalidReceiver", "receiver must be a Symbol or String", null, null);
    const selector = oopText(g_api, args[1]) orelse return mkFailure("encode", "invalidSelector", "selector must be a Symbol or String", null, null);
    const image = oopText(g_api, args[2]) orelse return mkFailure("encode", "invalidImage", "image must be a String", null, null);
    if (std.mem.indexOfScalar(u8, receiver, 0) != null or std.mem.indexOfScalar(u8, selector, 0) != null or std.mem.indexOfScalar(u8, image, 0) != null)
        return mkFailure("encode", "embeddedNul", "worker names and image path may not contain NUL", null, null);
    if (!arityOne(selector))
        return mkFailure("boot", "wrongArity", "worker selector must have arity one", null, null);
    if (g_api.isSmallInt(args[3]) == 0 or g_api.smallIntValue(args[3]) < 0)
        return mkFailure("encode", "invalidTimeout", "ready timeout must be a nonnegative Integer", null, null);
    const ready_timeout: u32 = @intCast(@min(g_api.smallIntValue(args[3]), std.math.maxInt(u32)));
    var entry = spawnWorker(image, receiver, selector) catch |e|
        return mkFailure("boot", "spawnFailed", @errorName(e), null, null);
    const ready = readFrame(entry.fd, ready_timeout) catch |e| {
        terminate(&entry);
        return mkFailure("boot", "handshakeFailed", @errorName(e), entry.exit_status, null);
    };
    defer ready.deinit();
    if (ready.header.kind == .boot_error) {
        const remote = decodeErrorRecord(g_api, error_schemas[0], ready.payload) catch null;
        terminate(&entry);
        if (remote) |r| return makeFailure(g_api.intern("boot"), r.kind, r.message, if (entry.exit_status) |s| g_api.mkSmallInt(@intCast(s)) else g_api.nilObj, g_api.nilObj);
        return mkFailure("boot", "invalidBootError", "worker returned a malformed boot error", entry.exit_status, null);
    }
    if (ready.header.kind != .ready) {
        terminate(&entry);
        return mkFailure("protocol", "expectedReady", "worker did not send ready", entry.exit_status, null);
    }
    entry.state = .ready;
    return newHandle(entry) orelse {
        terminate(&entry);
        return mkFailure("exit", "tokenExhausted", "isolate handle token space exhausted", entry.exit_status, null);
    };
}

fn startWorker(self: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    return startWorkerWithReadyTimeout(self, &[_]Oop{ args[0], args[1], args[2], g_api.mkSmallInt(10_000) }, 4);
}

fn requestWithTimeout(self: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    drainImpl();
    const entry = registryEntry(handle(self).token) orelse return mkFailure("stopped", "unknownHandle", "unknown isolate handle", null, null);
    if (entry.state != .ready) return mkFailure(if (entry.state == .busy) "busy" else "stopped", "notReady", "isolate is not ready", entry.exit_status, null);
    if (g_api.isSmallInt(args[1]) == 0 or g_api.smallIntValue(args[1]) < 0)
        return mkFailure("encode", "invalidTimeout", "timeout must be a nonnegative Integer", null, null);
    const timeout: u32 = @intCast(@min(g_api.smallIntValue(args[1]), std.math.maxInt(u32)));
    const service_start = std.time.Instant.now() catch return mkFailure("timeout", "clockUnavailable", "monotonic clock unavailable", null, null);
    const payload = encodeValue(g_api, args[0]) catch |e| return mkFailure("encode", "nonTransferable", @errorName(e), null, null);
    defer ca.free(payload);
    const now = std.time.Instant.now() catch return mkFailure("timeout", "clockUnavailable", "monotonic clock unavailable", null, null);
    const elapsed_ms = now.since(service_start) / std.time.ns_per_ms;
    if (elapsed_ms >= timeout)
        return mkFailure("timeout", "deadline", "request deadline expired before transmission", null, null);
    const remaining: u32 = @intCast(timeout - elapsed_ms);
    if (entry.next_request > std.math.maxInt(i64)) {
        entry.state = .stopping;
        terminate(entry);
        return mkFailure("protocol", "requestIdExhausted", "request id space exhausted", entry.exit_status, null);
    }
    const id = entry.next_request;
    entry.next_request += 1;
    entry.state = .busy;
    writeFrame(entry.fd, .request, id, payload, remaining) catch |e| {
        entry.state = .stopping;
        terminate(entry);
        return mkFailure(if (e == error.Timeout) "timeout" else if (entry.exit_status != null) "exit" else "transport", @errorName(e), "request write failed", entry.exit_status, id);
    };
    const reply = readFrame(entry.fd, remaining) catch |e| {
        entry.state = .stopping;
        terminate(entry);
        return mkFailure(if (e == error.Timeout) "timeout" else if (entry.exit_status != null) "exit" else "transport", @errorName(e), "request read failed", entry.exit_status, id);
    };
    defer reply.deinit();
    if (reply.header.request_id != id) {
        entry.state = .stopping;
        terminate(entry);
        return mkFailure("protocol", "mismatchedRequestId", "reply request id did not match", entry.exit_status, id);
    }
    if (reply.header.kind == .reply_ok) {
        const result = decodeValue(g_api, reply.payload) catch |e| {
            entry.state = .stopping;
            terminate(entry);
            return mkFailure("protocol", "invalidReply", @errorName(e), entry.exit_status, id);
        };
        entry.state = .ready;
        return result;
    }
    if (reply.header.kind == .reply_error) {
        const remote = decodeErrorRecord(g_api, error_schemas[1], reply.payload) catch {
            entry.state = .stopping;
            terminate(entry);
            return mkFailure("protocol", "invalidApplicationError", "malformed application error", entry.exit_status, id);
        };
        entry.state = .ready;
        return makeFailure(g_api.intern("application"), remote.kind, remote.message, g_api.nilObj, g_api.mkSmallInt(@intCast(id)));
    }
    entry.state = .stopping;
    terminate(entry);
    return mkFailure("protocol", "unexpectedReply", "unexpected reply frame", entry.exit_status, id);
}

fn requestDefault(self: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    return requestWithTimeout(self, &[_]Oop{ args[0], g_api.mkSmallInt(5_000) }, 2);
}

fn evolveWithTimeout(self: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    drainImpl();
    const entry = registryEntry(handle(self).token) orelse return mkFailure("stopped", "unknownHandle", "unknown isolate handle", null, null);
    if (entry.state != .ready) return mkFailure(if (entry.state == .busy) "busy" else "stopped", "notReady", "isolate is not ready", entry.exit_status, null);
    if (g_api.isSmallInt(args[1]) == 0 or g_api.smallIntValue(args[1]) < 0)
        return mkFailure("encode", "invalidTimeout", "timeout must be a nonnegative Integer", null, null);
    const timeout: u32 = @intCast(@min(g_api.smallIntValue(args[1]), std.math.maxInt(u32)));
    const service_start = std.time.Instant.now() catch return mkFailure("timeout", "clockUnavailable", "monotonic clock unavailable", null, null);
    const payload = encodeEvolutionRequest(g_api, args[0]) catch |e|
        return mkFailure("encode", "invalidEvolutionRequest", @errorName(e), null, null);
    defer ca.free(payload);
    const now = std.time.Instant.now() catch return mkFailure("timeout", "clockUnavailable", "monotonic clock unavailable", null, null);
    const elapsed_ms = now.since(service_start) / std.time.ns_per_ms;
    if (elapsed_ms >= timeout)
        return mkFailure("timeout", "deadline", "evolution deadline expired before transmission", null, null);
    const remaining: u32 = @intCast(timeout - elapsed_ms);
    if (entry.next_request > std.math.maxInt(i64)) {
        entry.state = .stopping;
        terminate(entry);
        return mkFailure("protocol", "requestIdExhausted", "request id space exhausted", entry.exit_status, null);
    }
    const id = entry.next_request;
    entry.next_request += 1;
    entry.state = .busy;
    writeFrame(entry.fd, .evolution_request, id, payload, remaining) catch |e| {
        entry.state = .stopping;
        terminate(entry);
        return mkFailure(if (e == error.Timeout) "timeout" else if (entry.exit_status != null) "exit" else "transport", @errorName(e), "evolution write failed", entry.exit_status, id);
    };
    const reply = readFrame(entry.fd, remaining) catch |e| {
        entry.state = .stopping;
        terminate(entry);
        return mkFailure(if (e == error.Timeout) "timeout" else if (entry.exit_status != null) "exit" else "transport", @errorName(e), "evolution read failed", entry.exit_status, id);
    };
    defer reply.deinit();
    if (reply.header.request_id != id or reply.header.kind != .evolution_reply) {
        entry.state = .stopping;
        terminate(entry);
        return mkFailure("protocol", "invalidEvolutionReply", "worker returned an invalid evolution reply", entry.exit_status, id);
    }
    const result = decodeEvolutionReceipt(g_api, reply.payload) catch |e| {
        entry.state = .stopping;
        terminate(entry);
        return mkFailure("protocol", "invalidEvolutionReceipt", @errorName(e), entry.exit_status, id);
    };
    entry.state = .ready;
    return result;
}

fn stopWorkerWithTimeout(self: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    drainImpl();
    const entry = registryEntry(handle(self).token) orelse return self;
    if (entry.state == .exited) return self;
    if (entry.state == .reap_pending) {
        if (statusOf(entry) != null) return self;
        return mkFailure("exit", "reapPending", "worker has not yet been reaped", null, null);
    }
    if (entry.state != .ready) return mkFailure("busy", "notReady", "isolate is not ready", entry.exit_status, null);
    if (g_api.isSmallInt(args[0]) == 0 or g_api.smallIntValue(args[0]) < 0)
        return mkFailure("encode", "invalidTimeout", "stop timeout must be a nonnegative Integer", null, null);
    const timeout: u32 = @intCast(@min(g_api.smallIntValue(args[0]), std.math.maxInt(u32)));
    const id = entry.next_request;
    if (id > std.math.maxInt(i64)) return mkFailure("protocol", "requestIdExhausted", "request id space exhausted", null, null);
    entry.next_request += 1;
    entry.state = .stopping;
    writeFrame(entry.fd, .stop, id, &.{}, timeout) catch {};
    const reply = readFrame(entry.fd, timeout) catch null;
    if (reply) |frame| {
        defer frame.deinit();
        if (frame.header.kind != .stopped or frame.header.request_id != id) {
            terminate(entry);
            return mkFailure("protocol", "invalidStopReply", "invalid stopped reply", entry.exit_status, id);
        }
    }
    terminate(entry);
    if (entry.state == .reap_pending) return mkFailure("exit", "reapPending", "worker has not yet been reaped", null, id);
    return self;
}

fn stopWorker(self: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    _ = args;
    return stopWorkerWithTimeout(self, &[_]Oop{g_api.mkSmallInt(2_000)}, 1);
}

fn handleState(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    const entry = registryEntry(handle(self).token) orelse return g_api.intern("stopped");
    _ = statusOf(entry);
    return g_api.intern(switch (entry.state) {
        .reap_pending => "reapPending",
        else => @tagName(entry.state),
    });
}

fn drainImpl() void {
    var it = registry.iterator();
    while (it.next()) |row| {
        const entry = row.value_ptr;
        if (entry.orphaned and entry.state != .exited) {
            if (entry.fd >= 0) {
                std.posix.close(entry.fd);
                entry.fd = -1;
            }
            std.posix.kill(entry.pid, std.posix.SIG.TERM) catch {};
            entry.term_at = std.time.Instant.now() catch null;
            entry.state = .reap_pending;
        }
        if (entry.state == .reap_pending and statusOf(entry) == null) {
            if (entry.kill_at == null and entry.term_at != null) {
                const now = std.time.Instant.now() catch continue;
                if (now.since(entry.term_at.?) >= 2 * std.time.ns_per_s) {
                    std.posix.kill(entry.pid, std.posix.SIG.KILL) catch {};
                    entry.kill_at = now;
                }
            }
        }
    }
}

pub fn drain() callconv(.c) void {
    drainImpl();
}
pub fn shutdown() callconv(.c) void {
    var it = registry.iterator();
    while (it.next()) |row| if (row.value_ptr.state != .exited) terminate(row.value_ptr);
}

const PhaseFn = *const fn () callconv(.c) c_int;

fn currentDigest(api: *IdApi) ?Oop {
    const codegen = api.import("Codegen") orelse return null;
    const digest = api.send(codegen, api.intern("currentGenerationDigest"), &.{}, 0);
    return if (isDigest(api, digest)) digest else null;
}

fn writeEvolutionResult(api: *IdApi, fd: c_int, id: u64, decoded: DecodedEvolution, prior: Oop, outcome: Oop, error_kind: Oop, message: []const u8) bool {
    const current = currentDigest(api) orelse prior;
    const result = makeReceipt(.{
        decoded.operation,
        decoded.module,
        decoded.accepted_base,
        prior,
        current,
        outcome,
        error_kind,
        api.mkString(message.ptr, message.len),
    });
    const payload = encodeEvolutionReceipt(api, result) catch return false;
    defer ca.free(payload);
    writeFrame(fd, .evolution_reply, id, payload, 10_000) catch return false;
    return true;
}

fn workerEvolution(api: *IdApi, fd: c_int, frame: Frame, begin_quiescence: PhaseFn, end_quiescence: PhaseFn) bool {
    if (testFault("evolution-delay")) std.Thread.sleep(200 * std.time.ns_per_ms);
    const decoded = decodeEvolutionRequest(api, frame.payload) catch |e| {
        const msg = @errorName(e);
        const payload = encodeErrorRecord(api, error_schemas[2], api.intern("invalidEvolutionRequest"), api.mkString(msg.ptr, msg.len)) catch return false;
        defer ca.free(payload);
        writeFrame(fd, .protocol_error, frame.header.request_id, payload, 10_000) catch {};
        return false;
    };
    const prior = currentDigest(api) orelse return false;
    const globals = api.import("Globals") orelse
        return writeEvolutionResult(api, fd, frame.header.request_id, decoded, prior, api.intern("rejected"), api.intern("evolutionUnavailable"), "worker image has no Globals facade");
    const live = api.send(globals, api.intern("at:"), &[_]Oop{api.intern("LiveModules")}, 1);
    if (live == api.nilObj)
        return writeEvolutionResult(api, fd, frame.header.request_id, decoded, prior, api.intern("rejected"), api.intern("evolutionUnavailable"), "worker image has no LiveModules facade");
    const prepared = api.send(live, api.intern("prepareEvolution:"), &[_]Oop{decoded.request}, 1);
    if (api.send(prepared, api.intern("isError"), &.{}, 0) == api.trueObj) {
        const kind = api.send(prepared, api.intern("kind"), &.{}, 0);
        const message = api.send(prepared, api.intern("message"), &.{}, 0);
        var len: usize = 0;
        const bytes = if (api.vtableOf(message) == api.string_vt) api.stringBytes(message, &len)[0..len] else "evolution preparation failed";
        return writeEvolutionResult(api, fd, frame.header.request_id, decoded, prior, api.intern("rejected"), kind, bytes);
    }
    const intents = api.send(globals, api.intern("at:"), &[_]Oop{api.intern("EvolutionIntent")}, 1);
    if (intents == api.nilObj or api.send(intents, api.intern("isIntent:"), &[_]Oop{prepared}, 1) != api.trueObj)
        return writeEvolutionResult(api, fd, frame.header.request_id, decoded, prior, api.intern("rejected"), api.intern("invalidEvolutionIntent"), "worker policy did not produce an evolution intent");
    if (begin_quiescence() == 0)
        return writeEvolutionResult(api, fd, frame.header.request_id, decoded, prior, api.intern("rejected"), api.intern("invalidRuntimePhase"), "worker could not enter evolution quiescence");
    const committed = api.send(prepared, api.intern("fulfill"), &.{}, 0);
    if (end_quiescence() == 0) return false;
    if (api.send(committed, api.intern("isError"), &.{}, 0) == api.trueObj) {
        const kind = api.send(committed, api.intern("kind"), &.{}, 0);
        const message = api.send(committed, api.intern("message"), &.{}, 0);
        var len: usize = 0;
        const bytes = if (api.vtableOf(message) == api.string_vt) api.stringBytes(message, &len)[0..len] else "evolution commit failed";
        return writeEvolutionResult(api, fd, frame.header.request_id, decoded, prior, api.intern("rejected"), kind, bytes);
    }
    const outcome = if (decoded.operation == api.intern("activate")) api.intern("activated") else api.intern("rolledBack");
    const message = if (decoded.operation == api.intern("activate")) "generation activated" else "generation rolled back";
    return writeEvolutionResult(api, fd, frame.header.request_id, decoded, prior, outcome, api.nilObj, message);
}

pub fn workerDriver(api: *IdApi, fd: c_int, receiver: Oop, selector: Oop, begin_quiescence: PhaseFn, end_quiescence: PhaseFn) callconv(.c) c_int {
    g_api = api;
    if (testFault("hard-stop")) {
        const ignore_term: std.posix.Sigaction = .{
            .handler = .{ .handler = std.posix.SIG.IGN },
            .mask = std.posix.sigemptyset(),
            .flags = 0,
        };
        std.posix.sigaction(std.posix.SIG.TERM, &ignore_term, null);
    }
    writeFrame(fd, .ready, 0, &.{}, 10_000) catch return 1;
    while (true) {
        const frame = readFrame(fd, std.math.maxInt(u32)) catch return 1;
        defer frame.deinit();
        switch (frame.header.kind) {
            .request => {
                const arg = decodeValue(api, frame.payload) catch |e| {
                    const msg = @errorName(e);
                    const payload = encodeErrorRecord(api, error_schemas[2], api.intern("invalidPayload"), api.mkString(msg.ptr, msg.len)) catch return 1;
                    defer ca.free(payload);
                    writeFrame(fd, .protocol_error, frame.header.request_id, payload, 10_000) catch {};
                    return 1;
                };
                const result = api.send(receiver, selector, &[_]Oop{arg}, 1);
                const is_error = api.send(result, api.intern("isError"), &.{}, 0) == api.trueObj;
                if (is_error) {
                    const kind = api.send(result, api.intern("kind"), &.{}, 0);
                    const message = api.send(result, api.intern("message"), &.{}, 0);
                    const payload = encodeErrorRecord(api, error_schemas[1], kind, message) catch return 1;
                    defer ca.free(payload);
                    writeFrame(fd, .reply_error, frame.header.request_id, payload, 10_000) catch return 1;
                } else {
                    const payload = encodeValue(api, result) catch |e| {
                        const msg = @errorName(e);
                        const ep = encodeErrorRecord(api, error_schemas[1], api.intern("encodeFailed"), api.mkString(msg.ptr, msg.len)) catch return 1;
                        defer ca.free(ep);
                        writeFrame(fd, .reply_error, frame.header.request_id, ep, 10_000) catch return 1;
                        continue;
                    };
                    defer ca.free(payload);
                    writeFrame(fd, .reply_ok, frame.header.request_id, payload, 10_000) catch return 1;
                }
            },
            .stop => {
                if (testFault("hard-stop")) while (true) std.Thread.sleep(std.time.ns_per_s);
                writeFrame(fd, .stopped, frame.header.request_id, &.{}, 10_000) catch return 1;
                return 0;
            },
            .evolution_request => if (!workerEvolution(api, fd, frame, begin_quiescence, end_quiescence)) return 1,
            else => return 1,
        }
    }
}

pub fn workerBootError(api: *IdApi, fd: c_int, kind_z: [*:0]const u8, message_ptr: [*]const u8, message_len: usize) callconv(.c) void {
    g_api = api;
    const payload = encodeErrorRecord(api, error_schemas[0], api.intern(kind_z), api.mkString(message_ptr, message_len)) catch return;
    defer ca.free(payload);
    writeFrame(fd, .boot_error, 0, payload, 2_000) catch {};
}

pub fn makeFailure(class: Oop, kind: Oop, message: Oop, status: Oop, request_id: Oop) Oop {
    const out = g_api.alloc(failure_vt, @sizeOf(Failure));
    failure(out).* = .{ .class = class, .kind = kind, .message = message, .exit_status = status, .request_id = request_id };
    return out;
}

fn failureIsError(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return g_api.trueObj;
}
fn failureClass(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return failure(self).class;
}
fn failureKind(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return failure(self).kind;
}
fn failureMessage(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return failure(self).message;
}
fn failureStatus(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return failure(self).exit_status;
}
fn failureRequestId(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return failure(self).request_id;
}

fn receiptOperation(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return receipt(self).operation;
}
fn receiptModule(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return receipt(self).module;
}
fn receiptAcceptedBase(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return receipt(self).accepted_base;
}
fn receiptPriorDigest(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return receipt(self).prior_digest;
}
fn receiptCurrentDigest(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return receipt(self).current_digest;
}
fn receiptOutcome(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return receipt(self).outcome;
}
fn receiptErrorKind(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return receipt(self).error_kind;
}
fn receiptMessage(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return receipt(self).message;
}
fn receiptIsError(self: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    return if (receipt(self).outcome == g_api.intern("rejected")) g_api.trueObj else g_api.falseObj;
}

// Private process-lifecycle fixtures. Application dispatch is covered with a
// named receiver restored from the smoke image; these two native hooks retain
// deterministic coverage of post-transmission timeout and abrupt child exit.
fn fixtureSleep(_: Oop, args: [*]const Oop, _: usize) callconv(.c) Oop {
    if (g_api.isSmallInt(args[0]) != 0 and g_api.smallIntValue(args[0]) > 0)
        std.Thread.sleep(@as(u64, @intCast(g_api.smallIntValue(args[0]))) * std.time.ns_per_ms);
    return args[0];
}

fn fixtureCrash(_: Oop, _: [*]const Oop, _: usize) callconv(.c) Oop {
    std.c.abort();
}

pub fn bringUp(api: *IdApi) void {
    g_api = api;
    const reloading = api.isExported("Isolate") != 0;
    if (reloading) {
        isolate_vt = api.import("IsolateVt");
        isolate_obj = api.import("Isolate");
        handle_vt = api.import("IsolateHandleVt");
        failure_vt = api.import("IsolateFailureVt");
        replacement_vt = api.import("IsolateReplacementVt");
        replacement_obj = api.import("IsolateReplacementPrototype");
        receipt_vt = api.import("EvolutionReceiptVt");
        receipt_obj = api.import("EvolutionReceiptPrototype");
    } else {
        isolate_vt = api.proto(api.object_vt);
        isolate_obj = api.alloc(isolate_vt, 0);
        handle_vt = api.proto(api.object_vt);
        failure_vt = api.proto(api.import("ErrorVt"));
        replacement_vt = api.proto(api.object_vt);
        replacement_obj = api.alloc(replacement_vt, 0);
        receipt_vt = api.proto(api.object_vt);
        receipt_obj = api.alloc(receipt_vt, 0);
    }
    api.setLayout(isolate_vt, null, 0, idapi.slot_raw);
    api.setLayout(handle_vt, null, 0, idapi.slot_raw);
    api.setLayout(failure_vt, null, 0, idapi.slot_oop);
    api.setLayout(replacement_vt, null, 0, idapi.slot_raw);
    api.setLayout(receipt_vt, null, 0, idapi.slot_oop);
    api.method(isolate_vt, api.intern("start:selector:image:"), &startWorker);
    api.method(isolate_vt, api.intern("start:selector:image:timeout:"), &startWorkerWithReadyTimeout);
    api.method(isolate_vt, api.intern("_isolateSleep:"), &fixtureSleep);
    api.method(isolate_vt, api.intern("_isolateCrash:"), &fixtureCrash);
    api.method(handle_vt, api.intern("request:timeout:"), &requestWithTimeout);
    api.method(handle_vt, api.intern("request:"), &requestDefault);
    api.method(handle_vt, api.intern("evolve:timeout:"), &evolveWithTimeout);
    api.method(handle_vt, api.intern("stop"), &stopWorker);
    api.method(handle_vt, api.intern("stopWithTimeout:"), &stopWorkerWithTimeout);
    api.method(handle_vt, api.intern("state"), &handleState);
    api.method(failure_vt, api.intern("isError"), &failureIsError);
    api.method(failure_vt, api.intern("class"), &failureClass);
    api.method(failure_vt, api.intern("kind"), &failureKind);
    api.method(failure_vt, api.intern("message"), &failureMessage);
    api.method(failure_vt, api.intern("exitStatus"), &failureStatus);
    api.method(failure_vt, api.intern("requestId"), &failureRequestId);
    api.method(receipt_vt, api.intern("operation"), &receiptOperation);
    api.method(receipt_vt, api.intern("module"), &receiptModule);
    api.method(receipt_vt, api.intern("acceptedBaseDigest"), &receiptAcceptedBase);
    api.method(receipt_vt, api.intern("priorDigest"), &receiptPriorDigest);
    api.method(receipt_vt, api.intern("currentDigest"), &receiptCurrentDigest);
    api.method(receipt_vt, api.intern("outcome"), &receiptOutcome);
    api.method(receipt_vt, api.intern("errorKind"), &receiptErrorKind);
    api.method(receipt_vt, api.intern("message"), &receiptMessage);
    api.method(receipt_vt, api.intern("isError"), &receiptIsError);
    if (!reloading) {
        _ = api.exportObj("IsolateVt", isolate_vt);
        _ = api.exportObj("Isolate", isolate_obj);
        _ = api.exportObj("IsolateHandleVt", handle_vt);
        _ = api.exportObj("IsolateFailureVt", failure_vt);
        _ = api.exportObj("IsolateReplacementVt", replacement_vt);
        _ = api.exportObj("IsolateReplacementPrototype", replacement_obj);
        _ = api.exportObj("EvolutionReceiptVt", receipt_vt);
        _ = api.exportObj("EvolutionReceiptPrototype", receipt_obj);
    }
    const globals = api.import("Globals");
    if (globals != null)
        _ = api.send(globals, api.intern("at:put:"), &[_]Oop{ api.intern("IsolateReplacement"), replacement_obj }, 2);
    if (globals != null)
        _ = api.send(globals, api.intern("at:put:"), &[_]Oop{ api.intern("EvolutionReceipt"), receipt_obj }, 2);
}

fn moduleInit(api: *IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    bringUp(api);
    return state;
}
comptime {
    if (!@import("builtin").is_test) {
        @export(&moduleInit, .{ .name = "module_init" });
        @export(&workerDriver, .{ .name = "birnam_isolate_worker_driver" });
        @export(&workerBootError, .{ .name = "birnam_isolate_worker_boot_error" });
        @export(&drain, .{ .name = "birnam_isolate_drain" });
        @export(&shutdown, .{ .name = "birnam_isolate_shutdown" });
    }
}
