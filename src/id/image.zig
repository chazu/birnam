//! The image — the running system made durable (docs/40).
//!
//! An image is NOT a log you replay: it is the live system's store-reachable
//! state serialized so a fresh process can map it back and start sending
//! messages (docs/40 §1). The durability line is the store boundary — the
//! store's facts (globals are facts), the interner (the durable anchor every
//! reference relinks against), and the heap graph transitively reachable from
//! fact values. What the system cannot name is garbage by definition and does
//! not survive.
//!
//! SLICE 3 lands the image itself in steps against a format that is append-ready
//! from day one (docs/40 §2 — "the image is a compacted journal; the journal is
//! an image in progress"):
//!   - step 1 (THIS): the segment format + skeleton. A header carrying the
//!     format version, architecture tag, and the runtime-shim ABI validity hash
//!     (the boot_validity gate, docs/40 §3a/§4), followed by a stream of framed
//!     records read until EOF (so appending is just writing more records). Save
//!     writes header-only; load validates it. `[System saveImage:]` /
//!     `[System loadImage:]` are wired; `birnam-id foo.image` boots through load.
//!   - step 2: the interner + fact + counter records.
//!   - step 3: the heap-graph record (reachability walk via the slice-2 layout
//!     descriptors; swizzle + relink-by-name on load).
//!   - step 4 (SLICE 4): compiled units as mapped code sections; the validity
//!     hash arbitrates map-vs-replay. Until then code survives by REPLAY from
//!     the method source the store already mirrors (docs/40 §4, §7.4).
//!
//! Reference-only salvage from the retired kernel lives in `src/attic/image.zig`
//! (the fact-level v1 and the hard-won interner-relink lesson).

const std = @import("std");
const builtin = @import("builtin");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const IdApi = idapi.IdApi;
const ca = std.heap.c_allocator;

var g_api: *IdApi = undefined;

// -- On-disk format ----------------------------------------------------------
//
// Header @0 (fixed, 28 bytes):
//   magic     [8]u8  "BIRNAMIM"
//   format    u32    FORMAT_VERSION (bump on any header/record layout change)
//   endian    u32    ENDIAN_TAG (detect/refuse a foreign byte order)
//   arch      u32    ARCH_TAG (code sections are per-arch; data is portable)
//   abi_hash  u64    the runtime-shim ABI validity gate (docs/40 §3a/§4)
// Then a stream of framed records, read until EOF:
//   record    u32 kind, u64 length, [length]u8 payload
// No end marker — EOF terminates. Appending = write more records at EOF; that
// is the journal write-mode the same format supports (docs/40 §2).

pub const MAGIC = "BIRNAMIM";
pub const FORMAT_VERSION: u32 = 1;
const ENDIAN_TAG: u32 = 0x01020304;
const HEADER_LEN: usize = 8 + 4 + 4 + 4 + 8;

/// Architecture tag — code sections are per-arch (docs/40 §4); a mismatch forces
/// the replay path for code in slice 4. Data sections are portable regardless.
const ARCH_TAG: u32 = switch (builtin.cpu.arch) {
    .aarch64 => 1,
    .x86_64 => 2,
    else => 0,
};

/// The runtime-shim ABI validity gate (docs/40 §3a/§4: the old boot_validity
/// idea). Folds a manual ABI version with the `birnam_*` shim NAME list, kept in
/// sync with `tcc.zig`'s `runtime_syms` + codegen's `birnam_global`. On load a
/// mismatch means the image's compiled sections cannot be trusted and code must
/// degrade to replay (wired in slice 4); the data sections stay portable.
const RUNTIME_ABI_VERSION: u32 = 1;
const shim_names = [_][]const u8{
    "birnam_send",       "birnam_intern",        "birnam_mkSmallInt",
    "birnam_smallIntValue", "birnam_mkFloat",    "birnam_floatValue",
    "birnam_mkString",   "birnam_nil",           "birnam_true",
    "birnam_false",      "birnam_objectVt",      "birnam_proto",
    "birnam_method",     "birnam_alloc",         "birnam_cstr",
    "birnam_free_cstr",  "birnam_layoutAllOops", "birnam_global",
};
pub fn abiHash() u64 {
    var h: u64 = 0xcbf29ce484222325; // FNV-1a
    for (shim_names) |nm| for (nm) |b| {
        h ^= b;
        h *%= 0x100000001b3;
    };
    var v = RUNTIME_ABI_VERSION;
    var i: usize = 0;
    while (i < 4) : (i += 1) {
        h ^= @as(u8, @truncate(v));
        h *%= 0x100000001b3;
        v >>= 8;
    }
    return h;
}

/// Record kinds — the section vocabulary. Step 1 writes none; later steps append
/// them. An unknown kind on load is skipped by its framed length (forward-compat).
const RecordKind = enum(u32) {
    interner = 1, // symbol names, in interner order (step 2, the durable anchor)
    facts = 2, // the store's (e,a,v,tx,op) triple stream (step 2)
    heap = 3, // the reachable object graph (step 3)
    code = 4, // compiled units (slice 4)
    _,
};

// -- Writer ------------------------------------------------------------------

const Buf = std.ArrayListUnmanaged(u8);

fn putU32(buf: *Buf, v: u32) void {
    var tmp: [4]u8 = undefined;
    std.mem.writeInt(u32, &tmp, v, .little);
    buf.appendSlice(ca, &tmp) catch @panic("image: oom");
}
fn putU64(buf: *Buf, v: u64) void {
    var tmp: [8]u8 = undefined;
    std.mem.writeInt(u64, &tmp, v, .little);
    buf.appendSlice(ca, &tmp) catch @panic("image: oom");
}
fn putBytes(buf: *Buf, bytes: []const u8) void {
    buf.appendSlice(ca, bytes) catch @panic("image: oom");
}

fn writeHeader(buf: *Buf) void {
    putBytes(buf, MAGIC);
    putU32(buf, FORMAT_VERSION);
    putU32(buf, ENDIAN_TAG);
    putU32(buf, ARCH_TAG);
    putU64(buf, abiHash());
}

/// Begin a framed record: write its kind and a length placeholder, returning the
/// offset of the placeholder. Call `endRecord` after appending the payload to
/// backpatch the length. Keeps records self-delimiting without a section table.
fn beginRecord(buf: *Buf, kind: RecordKind) usize {
    putU32(buf, @intFromEnum(kind));
    const len_at = buf.items.len;
    putU64(buf, 0); // placeholder, backpatched by endRecord
    return len_at;
}
fn endRecord(buf: *Buf, len_at: usize) void {
    const payload_len = buf.items.len - (len_at + 8);
    std.mem.writeInt(u64, buf.items[len_at..][0..8], @intCast(payload_len), .little);
}

/// Serialize the store-reachable system to `path`. Step 1: header only (an empty
/// but valid image); steps 2–4 append the interner/fact/heap/code records here.
pub fn saveImage(path: []const u8) bool {
    var buf: Buf = .{};
    defer buf.deinit(ca);
    writeHeader(&buf);
    // steps 2–3: appendInterner(&buf); appendFacts(&buf); appendHeap(&buf);
    const file = std.fs.cwd().createFile(path, .{}) catch return false;
    defer file.close();
    file.writeAll(buf.items) catch return false;
    return true;
}

// -- Reader ------------------------------------------------------------------

const Reader = struct {
    b: []const u8,
    i: usize = 0,

    fn u32v(self: *Reader) ?u32 {
        if (self.i + 4 > self.b.len) return null;
        const v = std.mem.readInt(u32, self.b[self.i..][0..4], .little);
        self.i += 4;
        return v;
    }
    fn u64v(self: *Reader) ?u64 {
        if (self.i + 8 > self.b.len) return null;
        const v = std.mem.readInt(u64, self.b[self.i..][0..8], .little);
        self.i += 8;
        return v;
    }
    fn take(self: *Reader, n: usize) ?[]const u8 {
        if (self.i + n > self.b.len) return null;
        const s = self.b[self.i .. self.i + n];
        self.i += n;
        return s;
    }
};

/// Validate the header. Returns false on a bad magic, an unreadable header, a
/// foreign byte order, or an incompatible format version. The ABI/arch mismatch
/// is NOT fatal here (slice 4 degrades code to replay on mismatch); step 1 has no
/// code sections, so any well-formed same-format header loads.
fn readHeader(r: *Reader) bool {
    const magic = r.take(MAGIC.len) orelse return false;
    if (!std.mem.eql(u8, magic, MAGIC)) return false;
    const fmt = r.u32v() orelse return false;
    if (fmt != FORMAT_VERSION) return false;
    const endian = r.u32v() orelse return false;
    if (endian != ENDIAN_TAG) return false;
    _ = r.u32v() orelse return false; // arch — recorded, checked in slice 4
    _ = r.u64v() orelse return false; // abi hash — recorded, checked in slice 4
    return true;
}

/// Map an image back into the running system. Step 1: validate the header and
/// walk the (empty) record stream; steps 2–4 rebuild interner/facts/heap/code.
pub fn loadImage(path: []const u8) bool {
    const bytes = std.fs.cwd().readFileAlloc(ca, path, 1 << 30) catch return false;
    defer ca.free(bytes);
    var r = Reader{ .b = bytes };
    if (!readHeader(&r)) return false;
    while (r.i < r.b.len) {
        const kind = r.u32v() orelse return false;
        const len = r.u64v() orelse return false;
        const payload = r.take(@intCast(len)) orelse return false;
        _ = payload; // steps 2–4 dispatch on the payload; step 1 skips every record
        switch (@as(RecordKind, @enumFromInt(kind))) {
            // steps 2–3: .interner, .facts, .heap; slice 4: .code
            else => {}, // unknown/not-yet-handled record: skip by its framed length
        }
    }
    return true;
}

// -- birnam surface: [System saveImage:] / [System loadImage:] ---------------

fn pathArg(o: Oop, len: *usize) ?[*]const u8 {
    if (o == null or g_api.vtableOf(o) != g_api.string_vt) return null;
    return g_api.stringBytes(o, len);
}

/// `[System saveImage: path]` — serialize the running system to `path`; answers
/// self on success, nil on failure (bad path arg or write error).
fn sysSaveImage(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var len: usize = 0;
    const p = pathArg(a[0], &len) orelse return g_api.nilObj;
    return if (saveImage(p[0..len])) self else g_api.nilObj;
}

/// `[System loadImage: path]` — map an image back in; answers self on success,
/// nil on failure. `birnam-id foo.image` uses this on boot.
fn sysLoadImage(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var len: usize = 0;
    const p = pathArg(a[0], &len) orelse return g_api.nilObj;
    return if (loadImage(p[0..len])) self else g_api.nilObj;
}

// -- bringup -----------------------------------------------------------------

pub fn bringUp(api: *IdApi) void {
    g_api = api;
    // The System vtable is exported by the prelude module (loaded before us). We
    // extend it — save/load are System protocol (docs/40 §1: `[System saveImage:]`).
    const system_vt = api.import("SystemVt");
    api.method(system_vt, api.intern("saveImage:"), &sysSaveImage);
    api.method(system_vt, api.intern("loadImage:"), &sysLoadImage);
}

fn moduleInit(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}
comptime {
    if (!@import("builtin").is_test) @export(&moduleInit, .{ .name = "module_init" });
}
