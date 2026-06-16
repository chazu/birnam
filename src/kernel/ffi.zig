//! Foreign function dispatch (Phase 9; Phase 19 Track B: libffi-backed).
//!
//! Two layers:
//!   1. Thin Zig extern decls for the libffi C API (ffi_prep_cif / ffi_call).
//!   2. Sig-string parser + invoke(addr, sig, args) Ref marshaler.
//!
//! Sig strings (a birnam convention, originally dyncall's DCsigchar):
//!   B = bool, c = char, s = short, i = int, l = long, L = long long,
//!   f = float, d = double, p = pointer, Z = c-string (alias of p),
//!   v = void, `u`-prefix = unsigned (uc us ui ul uL), `{...}` = struct by
//!   value. `)` separates args from return type. `e` marks the start of
//!   variadic args.
//!
//! libffi marshals every C-ABI shape including struct-by-value on arm64-apple
//! (which dyncall 1.4 could not), and will back closures for Phase 19.2.

const std = @import("std");
const builtin = @import("builtin");
const ref_mod = @import("ref.zig");
const boxed = @import("boxed.zig");
const strings = @import("strings.zig");

/// Phase 19 Track B: libffi marshals aggregates (struct-by-value) on every
/// platform it supports, INCLUDING arm64-apple — so the dyncall-era arm64 gate
/// is gone. The sig parser understands `{...}` everywhere; the call path builds
/// an `ffi_type` of FFI_TYPE_STRUCT and libffi handles ABI placement.
pub const aggregates_supported = true;

pub const Ref = ref_mod.Ref;

// -- libffi C API extern decls ---------------------------------------------
//
// libffi describes a call with an `ffi_type` per arg/return + a prepped
// `ffi_cif` (call interface); `ffi_call` then marshals from an array of
// pointers-to-values into registers/stack per the ABI. Replaces dyncall's
// push-imperative VM. `ffi_cif` carries platform-specific extra fields, so we
// treat it as an opaque byte buffer (`CifStorage`) sized generously.

pub const FFI_TYPE_STRUCT: c_ushort = 13; // from ffitarget.h / ffi.h

/// ffi_abi — aarch64: FFI_FIRST_ABI=0, FFI_SYSV=1, FFI_DEFAULT_ABI=FFI_SYSV.
pub const FFI_DEFAULT_ABI: c_uint = 1;

/// ffi_status.
pub const FFI_OK: c_int = 0;

pub const ffi_type = extern struct {
    size: usize,
    alignment: c_ushort,
    type: c_ushort,
    elements: ?[*]?*ffi_type,
};

// Opaque storage for an ffi_cif. Real struct (aarch64) is ~40 bytes incl.
// FFI_EXTRA_CIF_FIELDS; 128 aligned(16) is safe headroom across platforms.
pub const CifStorage = [128]u8;

// libffi's built-in scalar type descriptors (globals defined in types.c).
pub extern "c" var ffi_type_void: ffi_type;
pub extern "c" var ffi_type_uint8: ffi_type;
pub extern "c" var ffi_type_sint8: ffi_type;
pub extern "c" var ffi_type_uint16: ffi_type;
pub extern "c" var ffi_type_sint16: ffi_type;
pub extern "c" var ffi_type_uint32: ffi_type;
pub extern "c" var ffi_type_sint32: ffi_type;
pub extern "c" var ffi_type_uint64: ffi_type;
pub extern "c" var ffi_type_sint64: ffi_type;
pub extern "c" var ffi_type_float: ffi_type;
pub extern "c" var ffi_type_double: ffi_type;
pub extern "c" var ffi_type_pointer: ffi_type;

pub extern "c" fn ffi_prep_cif(cif: *anyopaque, abi: c_uint, nargs: c_uint, rtype: *ffi_type, atypes: [*]?*ffi_type) c_int;
pub extern "c" fn ffi_prep_cif_var(cif: *anyopaque, abi: c_uint, nfixedargs: c_uint, ntotalargs: c_uint, rtype: *ffi_type, atypes: [*]?*ffi_type) c_int;
pub extern "c" fn ffi_call(cif: *anyopaque, fnp: *const anyopaque, rvalue: ?*anyopaque, avalue: [*]?*anyopaque) void;

// Phase 19.2 — closures (birnam callbacks). `ffi_closure_alloc` returns BOTH the
// closure object and a separate executable `code` location (W^X split); C calls
// through `code`. The handler `fun` runs on every C call with pointers to the
// args + return slot. libffi handles the Apple-Silicon JIT memory (MAP_JIT)
// internally via its trampoline table.
pub const ClosureFn = *const fn (cif: *anyopaque, ret: *anyopaque, args: [*]?*anyopaque, user_data: ?*anyopaque) callconv(.c) void;
pub extern "c" fn ffi_closure_alloc(size: usize, code: *?*anyopaque) ?*anyopaque;
pub extern "c" fn ffi_closure_free(closure: *anyopaque) void;
pub extern "c" fn ffi_prep_closure_loc(closure: *anyopaque, cif: *anyopaque, fun: ClosureFn, user_data: ?*anyopaque, codeloc: *anyopaque) c_int;

// -- Sig-string parser ------------------------------------------------------

pub const Kind = enum {
    void_,
    bool_,
    char,
    short,
    int,
    long,
    long_long,
    float,
    double,
    pointer,
    cstring,
    /// Phase 18: a by-value struct. Layout carried out-of-band in `Sig`'s
    /// `arg_aggrs[i]` / `ret_aggr`. Source syntax: `{...}` of scalar sigchars.
    aggregate,
};

/// Scalar field size/alignment (LP64; matches natural C struct layout). Used by
/// `parseAggr` to compute field offsets for peek/poke + Phase 19.5. Aggregate/
/// void are not valid struct fields.
fn kindSize(k: Kind) usize {
    return switch (k) {
        .bool_, .char => 1,
        .short => 2,
        .int, .float => 4,
        .long, .long_long, .double, .pointer, .cstring => 8,
        .void_, .aggregate => 0,
    };
}

/// Map a scalar `Kind` (+ unsigned flag) to libffi's built-in type descriptor.
/// libffi's typed descriptors encode signedness directly, so the dyncall-era
/// bit-preserving-narrow + sign/zero-extend hacks are gone — the right ffi_type
/// makes libffi extend correctly on its own. C `long` is 8 bytes (LP64).
fn ffiTypeForKind(k: Kind, unsigned: bool) *ffi_type {
    return switch (k) {
        .void_ => &ffi_type_void,
        .bool_ => &ffi_type_uint8, // C _Bool is 1 byte
        .char => if (unsigned) &ffi_type_uint8 else &ffi_type_sint8,
        .short => if (unsigned) &ffi_type_uint16 else &ffi_type_sint16,
        .int => if (unsigned) &ffi_type_uint32 else &ffi_type_sint32,
        .long, .long_long => if (unsigned) &ffi_type_uint64 else &ffi_type_sint64,
        .float => &ffi_type_float,
        .double => &ffi_type_double,
        .pointer, .cstring => &ffi_type_pointer,
        .aggregate => &ffi_type_void, // unreachable — aggregates use Aggr.ensureFfiType
    };
}

/// A parsed by-value aggregate layout. `offsets[i]` is field `i`'s byte offset
/// (for peek/poke + Phase 19.5); `size` is the total. The libffi struct
/// descriptor (`ty` + its NULL-terminated `elements`) is built lazily; libffi
/// fills `ty.size`/`ty.alignment` during `ffi_prep_cif`.
pub const Aggr = struct {
    fields: []Kind,
    offsets: []u32,
    size: usize,
    elements: ?[]?*ffi_type = null, // owned; len = fields.len + 1 (NULL-terminated)
    ty: ffi_type = .{ .size = 0, .alignment = 0, .type = FFI_TYPE_STRUCT, .elements = null },

    pub fn deinit(self: *Aggr, allocator: std.mem.Allocator) void {
        if (self.elements) |e| allocator.free(e);
        allocator.free(self.fields);
        allocator.free(self.offsets);
    }

    /// Build (and cache) the libffi struct `ffi_type`. Returns null on OOM.
    /// Field signedness is irrelevant to layout, so the signed variant is used.
    pub fn ensureFfiType(self: *Aggr, allocator: std.mem.Allocator) ?*ffi_type {
        if (self.elements != null) return &self.ty;
        const elems = allocator.alloc(?*ffi_type, self.fields.len + 1) catch return null;
        for (self.fields, 0..) |fk, i| elems[i] = ffiTypeForKind(fk, false);
        elems[self.fields.len] = null; // libffi requires a NULL terminator
        self.elements = elems;
        self.ty = .{ .size = 0, .alignment = 0, .type = FFI_TYPE_STRUCT, .elements = elems.ptr };
        return &self.ty;
    }
};

pub const Sig = struct {
    args: []Kind,
    ret: Kind,
    /// Position in `args` (0-indexed) where variadic varargs begin. null
    /// means the signature is not variadic. Marker char in source: `e`.
    variadic_at: ?usize = null,
    /// Phase 18: parallel to `args` — non-null exactly where `args[i] ==
    /// .aggregate`. Empty (`&.{}`) for sigs with no struct args.
    arg_aggrs: []?*Aggr = &.{},
    /// Phase 18: non-null iff `ret == .aggregate`.
    ret_aggr: ?*Aggr = null,
    /// Phase 19.6: parallel to `args` — true where the arg is an *unsigned*
    /// integer (`u`-prefixed sigchar: `uc us ui ul uL`). Sign is carried beside
    /// the width `Kind` rather than as new `Kind` variants; under libffi it just
    /// selects the unsigned `ffi_type` (`ffiTypeForKind`), which handles arg
    /// width + return extension on its own. Parallel to `args` (always
    /// `args.len` long, like `arg_aggrs`); `&.{}` only on the default literal.
    arg_unsigned: []bool = &.{},
    /// Phase 19.6: true iff the return type is an unsigned integer.
    ret_unsigned: bool = false,

    pub fn deinit(self: *Sig, allocator: std.mem.Allocator) void {
        for (self.arg_aggrs) |maybe| {
            if (maybe) |ag| {
                ag.deinit(allocator);
                allocator.destroy(ag);
            }
        }
        if (self.arg_aggrs.len != 0) allocator.free(self.arg_aggrs);
        if (self.arg_unsigned.len != 0) allocator.free(self.arg_unsigned);
        if (self.ret_aggr) |ag| {
            ag.deinit(allocator);
            allocator.destroy(ag);
        }
        allocator.free(self.args);
    }
};

pub const SigParseError = error{
    UnknownSigchar,
    MissingReturnSeparator,
    EmptySig,
    OutOfMemory,
};

fn kindOf(c: u8) ?Kind {
    return switch (c) {
        'v' => .void_,
        'B' => .bool_,
        'c' => .char,
        's' => .short,
        'i' => .int,
        'l' => .long,
        'L' => .long_long,
        'f' => .float,
        'd' => .double,
        'p' => .pointer,
        'Z' => .cstring,
        else => null,
    };
}

/// Phase 19.6: only integer widths may carry an unsigned `u` prefix. bool/
/// float/double/pointer/cstring/void/aggregate reject it (a `uf`/`up` sig is a
/// parse error, not a silently-ignored prefix).
fn isIntKind(k: Kind) bool {
    return switch (k) {
        .char, .short, .int, .long, .long_long => true,
        else => false,
    };
}

/// Phase 19.6: parse one scalar sigchar at `s[i]`, honoring an optional `u`
/// unsigned prefix (`uc us ui ul uL`). `next` is the index of the LAST char
/// consumed (so a caller's `i = next` + loop `+= 1` lands past it). Returns null
/// on an unknown char or a `u` prefix on a non-integer type.
fn scalarAt(s: []const u8, i: usize) ?struct { kind: Kind, unsigned: bool, next: usize } {
    if (s[i] == 'u') {
        if (i + 1 >= s.len) return null; // dangling 'u'
        const k = kindOf(s[i + 1]) orelse return null;
        if (!isIntKind(k)) return null;
        return .{ .kind = k, .unsigned = true, .next = i + 1 };
    }
    const k = kindOf(s[i]) orelse return null;
    return .{ .kind = k, .unsigned = false, .next = i };
}

/// Phase 19.5: decode a whole scalar type NAME (a struct-field type symbol like
/// `i` / `ui` / `d` / `p`) into a Kind + unsigned flag. Returns null unless the
/// name is exactly one scalar token (no trailing chars, no aggregate/void).
pub fn kindFromName(name: []const u8) ?struct { kind: Kind, unsigned: bool } {
    if (name.len == 0) return null;
    const sc = scalarAt(name, 0) orelse return null;
    if (sc.next != name.len - 1) return null; // must consume the whole name
    if (sc.kind == .void_) return null;
    return .{ .kind = sc.kind, .unsigned = sc.unsigned };
}

/// Phase 19.5: byte size of a scalar field type (for struct layout).
pub fn scalarSize(k: Kind) usize {
    return kindSize(k);
}

/// Phase 19.5: read a scalar of `kind` at raw address `addr` into a Ref —
/// powers struct-field getters (`[pt x]`). Reuses the callback unmarshal path.
pub fn peekKind(allocator: std.mem.Allocator, addr: usize, kind: Kind, unsigned: bool) Ref {
    return cArgToRef(allocator, kind, unsigned, @ptrFromInt(addr));
}

/// Phase 19.5: write `value` as a scalar of `kind` to raw address `addr` —
/// powers struct-field setters (`[pt x: 3]`). Returns false on a type mismatch.
/// Width only; signedness doesn't affect the stored bytes.
pub fn pokeKind(addr: usize, kind: Kind, value: Ref) bool {
    switch (kind) {
        .bool_ => {
            if (!ref_mod.isSmallInt(value)) return false;
            @as(*u8, @ptrFromInt(addr)).* = if (ref_mod.untagSmallInt(value) != 0) 1 else 0;
        },
        .char => {
            if (!ref_mod.isSmallInt(value)) return false;
            @as(*u8, @ptrFromInt(addr)).* = @truncate(@as(u64, @bitCast(ref_mod.untagSmallInt(value))));
        },
        .short => {
            if (!ref_mod.isSmallInt(value)) return false;
            @as(*align(1) u16, @ptrFromInt(addr)).* = @truncate(@as(u64, @bitCast(ref_mod.untagSmallInt(value))));
        },
        .int => {
            if (!ref_mod.isSmallInt(value)) return false;
            @as(*align(1) u32, @ptrFromInt(addr)).* = @truncate(@as(u64, @bitCast(ref_mod.untagSmallInt(value))));
        },
        .long, .long_long => {
            if (!ref_mod.isSmallInt(value)) return false;
            @as(*align(1) u64, @ptrFromInt(addr)).* = @bitCast(ref_mod.untagSmallInt(value));
        },
        .float => {
            if (!boxed.isFloat(value)) return false;
            @as(*align(1) f32, @ptrFromInt(addr)).* = @floatCast(boxed.untagFloat(value));
        },
        .double => {
            if (!boxed.isFloat(value)) return false;
            @as(*align(1) f64, @ptrFromInt(addr)).* = boxed.untagFloat(value);
        },
        .pointer, .cstring => {
            if (ref_mod.isNil(value)) {
                @as(*align(1) usize, @ptrFromInt(addr)).* = 0;
            } else if (boxed.isFfiPtr(value)) {
                @as(*align(1) usize, @ptrFromInt(addr)).* = boxed.untagFfiPtr(value);
            } else return false;
        },
        .void_, .aggregate => return false,
    }
    return true;
}

fn alignUp(n: usize, a: usize) usize {
    return (n + a - 1) & ~(a - 1);
}

/// Parse a `{...}` aggregate beginning at `s[start] == '{'`. Returns the heap
/// `Aggr` (with natural-alignment offsets computed) and the index of the
/// closing `}`. Scalar fields only in v1 — nested aggregates/arrays rejected.
fn parseAggr(allocator: std.mem.Allocator, s: []const u8, start: usize) SigParseError!struct { aggr: *Aggr, close: usize } {
    var fields = std.ArrayList(Kind){};
    defer fields.deinit(allocator);
    var offsets = std.ArrayList(u32){};
    defer offsets.deinit(allocator);

    var cursor: usize = 0;
    var max_align: usize = 1;
    var j = start + 1;
    while (j < s.len and s[j] != '}') : (j += 1) {
        const fk = kindOf(s[j]) orelse return error.UnknownSigchar;
        if (fk == .void_) return error.UnknownSigchar; // no void fields
        const sz = kindSize(fk);
        const al = sz; // scalar align == size
        const off = alignUp(cursor, al);
        try offsets.append(allocator, @intCast(off));
        try fields.append(allocator, fk);
        cursor = off + sz;
        if (al > max_align) max_align = al;
    }
    if (j >= s.len) return error.MissingReturnSeparator; // unterminated '{'
    if (fields.items.len == 0) return error.EmptySig; // empty struct

    const aggr = try allocator.create(Aggr);
    aggr.* = .{
        .fields = try fields.toOwnedSlice(allocator),
        .offsets = try offsets.toOwnedSlice(allocator),
        .size = alignUp(cursor, max_align),
    };
    return .{ .aggr = aggr, .close = j };
}

pub fn parseSig(allocator: std.mem.Allocator, s: []const u8) SigParseError!Sig {
    if (s.len == 0) return error.EmptySig;
    var args = std.ArrayList(Kind){};
    defer args.deinit(allocator);
    var arg_aggrs = std.ArrayList(?*Aggr){};
    defer arg_aggrs.deinit(allocator);
    // Phase 19.6: parallel to `args` (always args.len long, like `arg_aggrs`).
    var arg_unsigned = std.ArrayList(bool){};
    defer arg_unsigned.deinit(allocator);
    // On any error after aggregates have been created, free them.
    errdefer for (arg_aggrs.items) |maybe| {
        if (maybe) |ag| {
            ag.deinit(allocator);
            allocator.destroy(ag);
        }
    };

    var variadic_at: ?usize = null;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        const c = s[i];
        if (c == ')') {
            if (i + 1 >= s.len) return error.MissingReturnSeparator;
            var ret_kind: Kind = undefined;
            var ret_aggr: ?*Aggr = null;
            var ret_unsigned = false;
            if (s[i + 1] == '{') {
                const parsed = try parseAggr(allocator, s, i + 1);
                ret_kind = .aggregate;
                ret_aggr = parsed.aggr;
            } else {
                const sc = scalarAt(s, i + 1) orelse return error.UnknownSigchar;
                ret_kind = sc.kind;
                ret_unsigned = sc.unsigned;
            }
            const owned_args = try args.toOwnedSlice(allocator);
            const owned_aggrs = try arg_aggrs.toOwnedSlice(allocator);
            const owned_unsigned = try arg_unsigned.toOwnedSlice(allocator);
            return .{
                .args = owned_args,
                .ret = ret_kind,
                .variadic_at = variadic_at,
                .arg_aggrs = owned_aggrs,
                .ret_aggr = ret_aggr,
                .arg_unsigned = owned_unsigned,
                .ret_unsigned = ret_unsigned,
            };
        }
        if (c == 'e') {
            variadic_at = args.items.len;
            continue;
        }
        if (c == '{') {
            const parsed = try parseAggr(allocator, s, i);
            try args.append(allocator, .aggregate);
            try arg_aggrs.append(allocator, parsed.aggr);
            try arg_unsigned.append(allocator, false);
            i = parsed.close; // loop's `+= 1` steps past the '}'
            continue;
        }
        const sc = scalarAt(s, i) orelse return error.UnknownSigchar;
        try args.append(allocator, sc.kind);
        try arg_aggrs.append(allocator, null);
        try arg_unsigned.append(allocator, sc.unsigned);
        i = sc.next; // loop's `+= 1` steps past the width char (handles `u` prefix)
    }
    return error.MissingReturnSeparator;
}

// -- lifecycle --------------------------------------------------------------

/// libffi is stateless (no persistent VM), so there is nothing to tear down.
/// Kept as a no-op so existing call sites / tests don't have to change.
pub fn shutdown() void {}

// -- libffi arg marshaling --------------------------------------------------

/// Write `value` into an 8-byte arg holder per `kind`, type-punned. libffi
/// reads `sizeof(ffi_type)` bytes from `&holder`, and the chosen `ffi_type`
/// (via `ffiTypeForKind`) handles sub-word sign/zero extension into registers
/// — so we just deposit the value at the right width (low bytes on LE). Returns
/// false on a type mismatch. nil → NULL pointer. `.aggregate`/cstring-string
/// are handled in the invoke loop (they don't use a scalar holder).
fn marshalScalar(kind: Kind, value: Ref, holder: *u64) bool {
    switch (kind) {
        .void_ => return false,
        .bool_ => {
            if (!ref_mod.isSmallInt(value)) return false;
            @as(*u8, @ptrCast(holder)).* = if (ref_mod.untagSmallInt(value) != 0) 1 else 0;
        },
        .char => {
            if (!ref_mod.isSmallInt(value)) return false;
            @as(*u8, @ptrCast(holder)).* = @truncate(@as(u64, @bitCast(ref_mod.untagSmallInt(value))));
        },
        .short => {
            if (!ref_mod.isSmallInt(value)) return false;
            @as(*u16, @ptrCast(holder)).* = @truncate(@as(u64, @bitCast(ref_mod.untagSmallInt(value))));
        },
        .int => {
            if (!ref_mod.isSmallInt(value)) return false;
            @as(*u32, @ptrCast(holder)).* = @truncate(@as(u64, @bitCast(ref_mod.untagSmallInt(value))));
        },
        .long, .long_long => {
            if (!ref_mod.isSmallInt(value)) return false;
            @as(*u64, @ptrCast(holder)).* = @bitCast(ref_mod.untagSmallInt(value));
        },
        .float => {
            if (!boxed.isFloat(value)) return false;
            @as(*f32, @ptrCast(holder)).* = @floatCast(boxed.untagFloat(value));
        },
        .double => {
            if (!boxed.isFloat(value)) return false;
            @as(*f64, @ptrCast(holder)).* = boxed.untagFloat(value);
        },
        .pointer, .cstring => {
            // nil → NULL pointer (nil-is-value); else an FfiPtr (raw addr) or a
            // Phase 19.2 callback (its C-callable code location).
            if (ref_mod.isNil(value)) {
                @as(*?*const anyopaque, @ptrCast(holder)).* = null;
                return true;
            }
            if (boxed.isFfiPtr(value)) {
                @as(*usize, @ptrCast(holder)).* = boxed.untagFfiPtr(value);
            } else if (boxed.isCallback(value)) {
                @as(*usize, @ptrCast(holder)).* = boxed.callbackCode(value);
            } else return false;
        },
        .aggregate => return false,
    }
    return true;
}

// -- libffi return unwrapping -----------------------------------------------

/// Phase 9.5.5 stale check: a pointer Ref whose owning Module was unloaded
/// carries a `[box_eid :ffi-stale? ?]` fact. Refuse to marshal it (the C side
/// would dereference freed memory). Only meaningful for pointer/cstring args.
fn isStalePtr(store: *fact_store_mod.FactStore, kind: Kind, value: Ref) bool {
    if (kind != .pointer and kind != .cstring) return false;
    if (!boxed.isFfiPtr(value)) return false;
    const box_eid = boxed.ffiPtrBoxEid(value);
    if (box_eid == 0) return false;
    return store.currentValue(box_eid, kernel_eids.ATTR_FFI_STALE) != null;
}

/// Turn libffi's return buffer (`rbuf`, an 8-byte `ffi_arg`-or-float slot) into
/// a birnam Ref for the scalar/pointer/cstring return kinds. void + aggregate
/// returns are handled in `invokeWithOwner` (they need a special rvalue). For
/// integer returns libffi already sign/zero-extended into the full `ffi_arg`
/// per the return `ffi_type`, so a single `@bitCast` to i64 is correct for both
/// signed and unsigned widths — the old per-width extend hacks are gone.
///
/// `owner_mod_eid` (Phase 9.5.5): when non-zero, a returned `.pointer` is boxed
/// via `mkFfiPtrOwned` so Module unload can find + stale it; else unowned.
fn unwrapReturn(
    allocator: std.mem.Allocator,
    store: ?*fact_store_mod.FactStore,
    table: ?*object_table_mod.ObjectTable,
    ret: Kind,
    rbuf: u64,
    owner_mod_eid: u64,
) Ref {
    switch (ret) {
        .bool_, .char, .short, .int, .long, .long_long => {
            return ref_mod.mkSmallInt(@bitCast(rbuf)) orelse ref_mod.NIL;
        },
        .float => {
            const r: f32 = @bitCast(@as(u32, @truncate(rbuf)));
            return boxed.mkFloat(allocator, @floatCast(r)) catch ref_mod.NIL;
        },
        .double => {
            const r: f64 = @bitCast(rbuf);
            return boxed.mkFloat(allocator, r) catch ref_mod.NIL;
        },
        .pointer => {
            const addr_ret: usize = @intCast(rbuf);
            if (owner_mod_eid != 0 and store != null and table != null) {
                return boxed.mkFfiPtrOwned(allocator, store.?, table.?, addr_ret, owner_mod_eid) catch ref_mod.NIL;
            }
            return boxed.mkFfiPtr(allocator, addr_ret) catch ref_mod.NIL;
        },
        .cstring => {
            // A `Z` return is a C `char*`; copy it into a birnam string. NULL →
            // nil (EOF / absence). The buffer is the callee's; we only read it.
            if (rbuf == 0) return ref_mod.NIL;
            const cptr: [*:0]const u8 = @ptrFromInt(@as(usize, @intCast(rbuf)));
            return strings.mkString(allocator, std.mem.span(cptr)) catch ref_mod.NIL;
        },
        .void_, .aggregate => return ref_mod.NIL, // handled by the caller
    }
}

/// Top-level invocation: build the arg `ffi_type`s + value holders, prep the
/// `ffi_cif`, `ffi_call`, unwrap the return. Per-call (no persistent VM); the
/// cif lives on the stack since libffi is stateless.
///
/// Phase 9.5.5: `store`/`owner_mod_eid` are threaded through so per-arg
/// stale-pointer checks (`:ffi-stale?`) and per-return owned-pointer
/// boxing both work. Passing `store = null` or `owner_mod_eid = 0`
/// preserves Phase 9 ownerless behavior.
pub fn invoke(
    allocator: std.mem.Allocator,
    addr: usize,
    sig: Sig,
    args: []const Ref,
) Ref {
    return invokeWithOwner(allocator, null, null, addr, sig, args, 0);
}

pub fn invokeWithOwner(
    allocator: std.mem.Allocator,
    store: ?*fact_store_mod.FactStore,
    table: ?*object_table_mod.ObjectTable,
    addr: usize,
    sig: Sig,
    args: []const Ref,
    owner_mod_eid: u64,
) Ref {
    if (args.len != sig.args.len) return ref_mod.NIL;
    const n = sig.args.len;

    // Per-call scratch: one `ffi_type*` and one 8-byte type-punned value holder
    // per arg, plus the `avalue` pointer array libffi reads. `.cstring` string
    // args get a NUL-terminated copy whose lifetime must span the (synchronous)
    // call. All freed on return via the defers.
    const atypes = allocator.alloc(?*ffi_type, n) catch return ref_mod.NIL;
    defer allocator.free(atypes);
    const holders = allocator.alloc(u64, n) catch return ref_mod.NIL;
    defer allocator.free(holders);
    const avalues = allocator.alloc(?*anyopaque, n) catch return ref_mod.NIL;
    defer allocator.free(avalues);
    var scratch: std.ArrayList([]u8) = .{};
    defer {
        for (scratch.items) |buf| allocator.free(buf);
        scratch.deinit(allocator);
    }

    for (sig.args, args, 0..) |kind, value, i| {
        if (kind == .aggregate) {
            // By-value struct arg: an FfiPtr to its bytes; libffi reads the
            // struct directly through this pointer per the struct `ffi_type`.
            const aggr = sig.arg_aggrs[i] orelse return ref_mod.NIL;
            atypes[i] = aggr.ensureFfiType(allocator) orelse return ref_mod.NIL;
            if (!boxed.isFfiPtr(value)) return ref_mod.NIL;
            avalues[i] = @ptrFromInt(boxed.untagFfiPtr(value));
            continue;
        }
        const uns = sig.arg_unsigned.len != 0 and sig.arg_unsigned[i];
        atypes[i] = ffiTypeForKind(kind, uns);
        if (kind == .cstring and strings.isString(value)) {
            const s = strings.bytes(value);
            const buf = allocator.alloc(u8, s.len + 1) catch return ref_mod.NIL;
            @memcpy(buf[0..s.len], s);
            buf[s.len] = 0;
            scratch.append(allocator, buf) catch {
                allocator.free(buf);
                return ref_mod.NIL;
            };
            @as(*?*const anyopaque, @ptrCast(&holders[i])).* = buf.ptr;
            avalues[i] = &holders[i];
            continue;
        }
        if (store) |st| {
            if (isStalePtr(st, kind, value)) return ref_mod.NIL;
        }
        if (!marshalScalar(kind, value, &holders[i])) return ref_mod.NIL;
        avalues[i] = &holders[i];
    }

    const rtype = if (sig.ret == .aggregate)
        ((sig.ret_aggr orelse return ref_mod.NIL).ensureFfiType(allocator) orelse return ref_mod.NIL)
    else
        ffiTypeForKind(sig.ret, sig.ret_unsigned);

    var cif: CifStorage align(16) = undefined;
    const status = if (sig.variadic_at) |nfixed|
        ffi_prep_cif_var(&cif, FFI_DEFAULT_ABI, @intCast(nfixed), @intCast(n), rtype, atypes.ptr)
    else
        ffi_prep_cif(&cif, FFI_DEFAULT_ABI, @intCast(n), rtype, atypes.ptr);
    if (status != FFI_OK) return ref_mod.NIL;

    const fnp: *const anyopaque = @ptrFromInt(addr);

    if (sig.ret == .void_) {
        ffi_call(&cif, fnp, null, avalues.ptr);
        return ref_mod.NIL;
    }
    if (sig.ret == .aggregate) {
        // By-value struct return: libffi writes into a libc buffer we hand back
        // as an FfiPtr (caller peeks fields + frees via `[p free]`). `ffi_prep_cif`
        // filled `ty.size`; fall back to the parser's size if 0.
        const aggr = sig.ret_aggr.?;
        const sz = if (aggr.ty.size != 0) aggr.ty.size else aggr.size;
        const buf = std.c.malloc(sz) orelse return ref_mod.NIL;
        @memset(@as([*]u8, @ptrCast(buf))[0..sz], 0);
        ffi_call(&cif, fnp, buf, avalues.ptr);
        return boxed.mkFfiPtr(allocator, @intFromPtr(buf)) catch ref_mod.NIL;
    }

    // Scalar/pointer return: libffi widens into an 8-byte `ffi_arg` slot.
    var rbuf: u64 align(16) = 0;
    ffi_call(&cif, fnp, &rbuf, avalues.ptr);
    return unwrapReturn(allocator, store, table, sig.ret, rbuf, owner_mod_eid);
}

// -- Wrap registry + per-arity dispatch shims -------------------------------

const send_mod = @import("send.zig");
const fact_store_mod = @import("fact_store.zig");
const object_table_mod = @import("object_table.zig");
const closure_mod = @import("closure.zig");
const kernel_eids = @import("kernel_eids.zig");
const id_api = @import("id_api.zig");

// -- Phase 19.2: callbacks (birnam closure → C function pointer) -------------

/// Heap context behind a callback box (slot 1). Holds everything the trampoline
/// needs on each C-side call + everything teardown frees. The `cif` lives here
/// (not on a stack) because the live `ffi_closure` references it for the
/// callback's whole lifetime; `atypes` likewise outlives the cif.
pub const CallbackCtx = struct {
    closure: Ref, // the birnam closure invoked on each call
    sig: Sig, // parsed C signature (args drive unmarshal, ret drives marshal)
    atypes: []?*ffi_type,
    cif: CifStorage align(16),
    closure_handle: *anyopaque, // ffi_closure, freed via ffi_closure_free
    value_sel: u64, // value / value: / value:value: by arity
    n_args: usize,
    allocator: std.mem.Allocator,
};

/// Read one C arg (libffi hands `p` = a pointer to the arg's value) into a Ref,
/// per `kind`. Inverse of `marshalScalar`. Integer widths sign/zero-extend to
/// i64 per `unsigned`; an aggregate arg's `p` already points at the struct bytes.
fn cArgToRef(allocator: std.mem.Allocator, kind: Kind, unsigned: bool, p_opt: ?*anyopaque) Ref {
    const p = p_opt orelse return ref_mod.NIL;
    switch (kind) {
        .void_ => return ref_mod.NIL,
        .bool_ => return ref_mod.mkSmallInt(@as(*const u8, @ptrCast(p)).*) orelse ref_mod.NIL,
        .char => {
            const v: i64 = if (unsigned) @as(*const u8, @ptrCast(p)).* else @as(*const i8, @ptrCast(p)).*;
            return ref_mod.mkSmallInt(v) orelse ref_mod.NIL;
        },
        .short => {
            const v: i64 = if (unsigned) @as(*align(1) const u16, @ptrCast(p)).* else @as(*align(1) const i16, @ptrCast(p)).*;
            return ref_mod.mkSmallInt(v) orelse ref_mod.NIL;
        },
        .int => {
            const v: i64 = if (unsigned) @as(*align(1) const u32, @ptrCast(p)).* else @as(*align(1) const i32, @ptrCast(p)).*;
            return ref_mod.mkSmallInt(v) orelse ref_mod.NIL;
        },
        .long, .long_long => {
            const v: i64 = @bitCast(@as(*align(1) const u64, @ptrCast(p)).*);
            return ref_mod.mkSmallInt(v) orelse ref_mod.NIL;
        },
        .float => return boxed.mkFloat(allocator, @floatCast(@as(*align(1) const f32, @ptrCast(p)).*)) catch ref_mod.NIL,
        .double => return boxed.mkFloat(allocator, @as(*align(1) const f64, @ptrCast(p)).*) catch ref_mod.NIL,
        .pointer => {
            const addr = @as(*align(1) const usize, @ptrCast(p)).*;
            if (addr == 0) return ref_mod.NIL;
            return boxed.mkFfiPtr(allocator, addr) catch ref_mod.NIL;
        },
        .cstring => {
            const addr = @as(*align(1) const usize, @ptrCast(p)).*;
            if (addr == 0) return ref_mod.NIL;
            const cptr: [*:0]const u8 = @ptrFromInt(addr);
            return strings.mkString(allocator, std.mem.span(cptr)) catch ref_mod.NIL;
        },
        .aggregate => return boxed.mkFfiPtr(allocator, @intFromPtr(p)) catch ref_mod.NIL,
    }
}

/// Write the closure's result Ref into libffi's return slot per `kind`. Integer
/// returns occupy a full `ffi_arg` (8 bytes). A non-conforming result (wrong
/// type) writes a zero/NULL rather than corrupting the C caller.
fn refToCRet(kind: Kind, ret: *anyopaque, ref: Ref) void {
    switch (kind) {
        .void_, .aggregate => {},
        .bool_, .char, .short, .int, .long, .long_long => {
            const v: i64 = if (ref_mod.isSmallInt(ref)) ref_mod.untagSmallInt(ref) else 0;
            @as(*i64, @ptrCast(@alignCast(ret))).* = v;
        },
        .float => {
            const v: f32 = if (boxed.isFloat(ref)) @floatCast(boxed.untagFloat(ref)) else 0;
            @as(*f32, @ptrCast(@alignCast(ret))).* = v;
        },
        .double => {
            const v: f64 = if (boxed.isFloat(ref)) boxed.untagFloat(ref) else 0;
            @as(*f64, @ptrCast(@alignCast(ret))).* = v;
        },
        .pointer, .cstring => {
            const addr: usize = if (boxed.isFfiPtr(ref)) boxed.untagFfiPtr(ref) else if (boxed.isCallback(ref)) boxed.callbackCode(ref) else 0;
            @as(*usize, @ptrCast(@alignCast(ret))).* = addr;
        },
    }
}

/// libffi calls this on every C-side invocation of the callback's code pointer.
/// Unmarshal the C args → Refs, send the birnam closure (`value`/`value:`/
/// `value:value:`), marshal the result back. Re-entrant: the evaluator runs here
/// mid-C-call (e.g. inside `qsort`). user_data = the `*CallbackCtx`.
fn callbackTrampoline(cif_unused: *anyopaque, ret: *anyopaque, args: [*]?*anyopaque, user_data: ?*anyopaque) callconv(.c) void {
    _ = cif_unused;
    const ctx: *CallbackCtx = @ptrCast(@alignCast(user_data orelse return));
    const alloc = send_mod.kernelAllocator() orelse return;
    var argbuf: [2]Ref = .{ ref_mod.NIL, ref_mod.NIL }; // block protocol caps arity at 2
    var i: usize = 0;
    while (i < ctx.n_args and i < 2) : (i += 1) {
        const uns = ctx.sig.arg_unsigned.len != 0 and ctx.sig.arg_unsigned[i];
        argbuf[i] = cArgToRef(alloc, ctx.sig.args[i], uns, args[i]);
    }
    const result = send_mod.sendN(ctx.closure, ctx.value_sel, &argbuf, ctx.n_args);
    refToCRet(ctx.sig.ret, ret, result);
}

/// Resolve the block-value selector for `n` args: `value` (0), `value:` (1),
/// `value:value:` (2). Birnam blocks support ≤2 params, so callbacks do too.
fn valueSelectorForArity(n: usize) ?u64 {
    return switch (n) {
        0 => kernel_eids.SEL_VALUE,
        1 => kernel_eids.SEL_VALUE_OF,
        2 => blk: {
            const store = send_mod.storePtr() orelse break :blk null;
            const table = send_mod.tablePtr() orelse break :blk null;
            const interner = id_api.internerPtr() orelse break :blk null;
            break :blk interner.intern(store, table, "value:value:") catch null;
        },
        else => null,
    };
}

/// Mint a callback: parse `sig_str`, build the cif, alloc + prep a libffi
/// closure bound to `closure_ref`, return a CALLBACK_VT box (slot 0 = code loc,
/// slot 1 = the heap ctx). Returns NIL on parse error, arity > 2, or any libffi
/// failure (with all partial allocations cleaned up). Lifetime: the GC roots the
/// birnam closure through the callback box — when the box is live, `gc.consider`
/// follows slot 1 (`*CallbackCtx`) and marks `ctx.closure`, so a persistent
/// callback keeps its closure alive without the caller holding a separate ref.
pub fn makeCallback(allocator: std.mem.Allocator, closure_ref: Ref, sig_str: []const u8) Ref {
    var sig = parseSig(allocator, sig_str) catch return ref_mod.NIL;
    const n = sig.args.len;
    if (n > 2) {
        sig.deinit(allocator);
        return ref_mod.NIL; // block-value protocol caps arity at 2
    }
    const value_sel = valueSelectorForArity(n) orelse {
        sig.deinit(allocator);
        return ref_mod.NIL;
    };

    const ctx = allocator.create(CallbackCtx) catch {
        sig.deinit(allocator);
        return ref_mod.NIL;
    };
    const atypes = allocator.alloc(?*ffi_type, if (n == 0) 1 else n) catch {
        sig.deinit(allocator);
        allocator.destroy(ctx);
        return ref_mod.NIL;
    };
    var ok = false;
    defer if (!ok) {
        sig.deinit(allocator);
        allocator.free(atypes);
        allocator.destroy(ctx);
    };

    var i: usize = 0;
    while (i < n) : (i += 1) {
        if (sig.args[i] == .aggregate) {
            const aggr = sig.arg_aggrs[i] orelse return ref_mod.NIL;
            atypes[i] = aggr.ensureFfiType(allocator) orelse return ref_mod.NIL;
        } else {
            const uns = sig.arg_unsigned.len != 0 and sig.arg_unsigned[i];
            atypes[i] = ffiTypeForKind(sig.args[i], uns);
        }
    }
    const rtype = if (sig.ret == .aggregate)
        ((sig.ret_aggr orelse return ref_mod.NIL).ensureFfiType(allocator) orelse return ref_mod.NIL)
    else
        ffiTypeForKind(sig.ret, sig.ret_unsigned);

    // ctx owns sig/atypes from here; populate then prep cif into ctx.cif.
    ctx.* = .{
        .closure = closure_ref,
        .sig = sig,
        .atypes = atypes,
        .cif = undefined,
        .closure_handle = undefined,
        .value_sel = value_sel,
        .n_args = n,
        .allocator = allocator,
    };
    if (ffi_prep_cif(&ctx.cif, FFI_DEFAULT_ABI, @intCast(n), rtype, atypes.ptr) != FFI_OK) return ref_mod.NIL;

    var code: ?*anyopaque = null;
    const handle = ffi_closure_alloc(256, &code) orelse return ref_mod.NIL;
    const codeloc = code orelse {
        ffi_closure_free(handle);
        return ref_mod.NIL;
    };
    if (ffi_prep_closure_loc(handle, &ctx.cif, &callbackTrampoline, ctx, codeloc) != FFI_OK) {
        ffi_closure_free(handle);
        return ref_mod.NIL;
    }
    ctx.closure_handle = handle;

    const box = boxed.mkCallback(allocator, @intFromPtr(codeloc), @intFromPtr(ctx)) catch {
        ffi_closure_free(handle);
        return ref_mod.NIL;
    };
    ok = true; // ctx now owned by the box; defer-cleanup disarmed
    return box;
}

/// Free a callback's libffi closure + heap ctx. Idempotent (clears slot 1, so a
/// second `[cb free]` is a no-op). Does NOT free the birnam closure (the GC owns
/// that).
pub fn freeCallback(callback_ref: Ref) void {
    const ctx_addr = boxed.callbackCtx(callback_ref);
    if (ctx_addr == 0) return;
    const ctx: *CallbackCtx = @ptrFromInt(ctx_addr);
    ffi_closure_free(ctx.closure_handle);
    const alloc = ctx.allocator;
    ctx.sig.deinit(alloc);
    alloc.free(ctx.atypes);
    alloc.destroy(ctx);
    boxed.clearCallbackCtx(callback_ref);
}

/// Resolve a wrap_eid (carried in `s.closure.data`) back to its addr +
/// parsed Sig. Returns null on any fact-store lookup miss.
fn resolveWrap(
    store: *fact_store_mod.FactStore,
    wrap_eid: u64,
) ?struct { addr: usize, sig: *const Sig } {
    const addr_ref = store.currentValue(wrap_eid, kernel_eids.ATTR_FFI_ADDR) orelse return null;
    if (!boxed.isFfiPtr(addr_ref)) return null;
    const addr = boxed.untagFfiPtr(addr_ref);
    const sig_ref = store.currentValue(wrap_eid, kernel_eids.ATTR_FFI_SIG) orelse return null;
    if (!ref_mod.isPointer(sig_ref)) return null;
    const sig: *const Sig = @ptrCast(@alignCast(ref_mod.untagPointer(sig_ref)));
    return .{ .addr = addr, .sig = sig };
}

/// Phase 15.E — free a wrap's heap `Sig` holder (struct + its `args` slice).
/// Safe targeted free, not arena-sweep: a Sig holds an internal slice that is
/// not Ref-reachable (so a sweep couldn't keep it), and it is referenced ONLY
/// via `[wrap :ffi-sig ptr]`, never captured. Callers invoke this exactly when
/// the wrap's Sig is being superseded (re-resolve) or torn down (module unload)
/// — both quiescent points (unload refuses while `ffi_depth > 0`). No-op when
/// the wrap has no current `:ffi-sig`. Uses the same allocator that created it.
pub fn freeSig(
    store: *fact_store_mod.FactStore,
    allocator: std.mem.Allocator,
    wrap_eid: u64,
) void {
    const sig_ref = store.currentValue(wrap_eid, kernel_eids.ATTR_FFI_SIG) orelse return;
    if (!ref_mod.isPointer(sig_ref)) return;
    const sig: *Sig = @ptrCast(@alignCast(ref_mod.untagPointer(sig_ref)));
    sig.deinit(allocator);
    allocator.destroy(sig);
    // Retract the now-dangling fact so `resolveWrap` returns null (the shim has
    // no stale-guard and would otherwise read freed memory on a post-unload
    // call). On re-resolve the caller asserts a fresh `:ffi-sig` right after.
    _ = store.retract(wrap_eid, kernel_eids.ATTR_FFI_SIG, sig_ref, kernel_eids.M_BOOTSTRAP) catch {};
}

fn shimDispatch(s: *send_mod.Send, args: []const Ref) Ref {
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    const table = send_mod.tablePtr() orelse return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const c = s.closure orelse return ref_mod.NIL;
    if (!ref_mod.isEid(c.data)) return ref_mod.NIL;
    const wrap_eid = ref_mod.untagEid(c.data);
    const w = resolveWrap(store, wrap_eid) orelse return ref_mod.NIL;
    // Phase 9.5.5: thread the wrap's owning Module through so return
    // pointers can be tagged with `:ffi-owner` and arg pointers can be
    // checked for `:ffi-stale?`. Owner = 0 keeps Phase 9 behavior.
    var owner_mod: u64 = 0;
    if (store.currentValue(wrap_eid, kernel_eids.ATTR_FFI_OWNER)) |owner_ref| {
        if (ref_mod.isEid(owner_ref)) owner_mod = ref_mod.untagEid(owner_ref);
    }
    return invokeWithOwner(alloc, store, table, w.addr, w.sig.*, args, owner_mod);
}

/// All FFI shims now share the variadic Imp signature; arity comes from
/// `n_args` and args are read from the buffer. The wrap's expected arity
/// is in its `:ffi-sig` slot, so distinct per-arity shims aren't needed.
fn shimAny(s: *send_mod.Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    return shimDispatch(s, args[0..n_args]);
}

pub const RegisterError = error{
    OutOfMemory,
    ArityTooHigh,
    SigParseFailure,
};

/// Resolve a single declarative wrap: parse `sig_str`, assert the TRANSIENT
/// resolved facts (`:ffi-addr`, `:ffi-sig` pointer) onto the EXISTING
/// `wrap_eid`, and build the dispatch closure. The wrap entity's durable
/// facts (`:ffi-wrap-symbol` / `:ffi-sig-str` / `:wrap-selector` / `:ffi-owner`)
/// are asserted earlier by `wrap:as:signature:`; this only adds the
/// process-local resolution. Returns the closure EID for the caller to install
/// via `[vt sel mkEid(closure_eid)]`.
///
/// Change 1: the old 0..3 arity cap is gone — `shimAny` is variadic and the
/// whole send path (`SEND_N` → `sendN`) carries arbitrary arity. The only
/// remaining ceiling is `SEND_N`'s u8 arg-count operand (`dispatch.zig`), so a
/// sig with >255 args genuinely cannot be dispatched and is still rejected.
pub fn buildWrapClosure(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    wrap_eid: u64,
    addr: usize,
    sig_str: []const u8,
) anyerror!u64 {
    var sig = parseSig(allocator, sig_str) catch return error.SigParseFailure;
    if (sig.args.len > 255) {
        sig.deinit(allocator);
        return error.ArityTooHigh;
    }
    // Phase 15.E: re-resolving an already-wrapped eid (image reload, relink)
    // supersedes its `:ffi-sig`; free the prior holder first so it doesn't leak.
    freeSig(store, allocator, wrap_eid);

    // Stash the parsed Sig on the heap so the shim can reference it via
    // pointer Ref. Transient (process-local); not persisted.
    const sig_holder = try allocator.create(Sig);
    sig_holder.* = sig;

    const addr_ref = try boxed.mkFfiPtr(allocator, addr);
    _ = try store.assert(wrap_eid, kernel_eids.ATTR_FFI_ADDR, addr_ref, kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(wrap_eid, kernel_eids.ATTR_FFI_SIG, ref_mod.mkPointer(@ptrCast(sig_holder)), kernel_eids.M_BOOTSTRAP);

    // All arities go through the variadic shim — Phase 10.2.5 unified Imp.
    const method_ptr: *const anyopaque = @ptrCast(&shimAny);

    // Allocate a Closure with .data = mkEid(wrap_eid). Match the kernel's
    // closure-as-libid-object layout: [vt_ref][method][data].
    const total = 3 * @sizeOf(Ref);
    const buf = try allocator.alignedAlloc(u8, .of(Ref), total);
    @memset(buf, 0);
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = ref_mod.mkEid(kernel_eids.CLOSURE_VT);
    const c: *closure_mod.Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    c.method = method_ptr;
    c.data = ref_mod.mkEid(wrap_eid);
    const closure_eid = table.alloc();
    try table.put(closure_eid, ref_mod.mkPointer(@ptrCast(c)));
    return closure_eid;
}

/// Legacy one-shot: mint a fresh wrap entity then resolve it. Retained for
/// direct callers/tests that bypass the declarative `wrap:as:signature:` path.
pub fn registerWrap(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    addr: usize,
    sig_str: []const u8,
) anyerror!u64 {
    const wrap_eid = table.alloc();
    return buildWrapClosure(allocator, store, table, wrap_eid, addr, sig_str);
}
