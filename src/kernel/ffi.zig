//! Phase 9: dyncall-backed foreign function dispatch.
//!
//! Two layers:
//!   1. Thin Zig extern decls for the dyncall C API.
//!   2. Sig-string parser + invoke(addr, sig, args) Ref marshaler.
//!
//! Sig strings follow dyncall convention (DCsigchar):
//!   B = bool, c = char, s = short, i = int, l = long, L = long long,
//!   f = float, d = double, p = pointer, Z = c-string (alias of p),
//!   v = void. `)` separates args from return type. `e` at the call site
//!   marks the start of variadic args.
//!
//! Phase 9 scope: pass-by-value scalars + pointer + c-string. Aggregates
//! (structs by value) deferred to 9.5+.

const std = @import("std");
const builtin = @import("builtin");
const ref_mod = @import("ref.zig");
const boxed = @import("boxed.zig");
const strings = @import("strings.zig");

/// Phase 18 prelude: struct-by-value relies on dyncall's aggregate vtable
/// (`dcArgAggr` / `dcBeginCallAggr` / `dcCallAggr`). Vendored dyncall 1.4
/// implements those for x86_64 but leaves them NULL on arm64-Apple (see
/// `dyncall_callvm_arm64_apple.c`). Calling through a NULL vtable slot segfaults,
/// so on arm64 we refuse aggregate calls (return NIL) until dyncall is upgraded
/// (≥1.5) or AAPCS aggregate classification is hand-rolled — Phase 19 backlog.
/// The sig parser still understands `{...}` everywhere; only marshaling is gated.
pub const aggregates_supported = builtin.cpu.arch != .aarch64;

pub const Ref = ref_mod.Ref;

// -- dyncall C API extern decls --------------------------------------------

pub const DCCallVM = opaque {};

pub const DC_CALL_C_DEFAULT: c_int = 0;
pub const DC_CALL_C_ELLIPSIS: c_int = 100;
pub const DC_CALL_C_ELLIPSIS_VARARGS: c_int = 101;

pub extern "c" fn dcNewCallVM(size: usize) ?*DCCallVM;
pub extern "c" fn dcFree(vm: *DCCallVM) void;
pub extern "c" fn dcReset(vm: *DCCallVM) void;
pub extern "c" fn dcMode(vm: *DCCallVM, mode: c_int) void;

pub extern "c" fn dcArgBool(vm: *DCCallVM, v: c_int) void;
pub extern "c" fn dcArgChar(vm: *DCCallVM, v: u8) void;
pub extern "c" fn dcArgShort(vm: *DCCallVM, v: c_short) void;
pub extern "c" fn dcArgInt(vm: *DCCallVM, v: c_int) void;
pub extern "c" fn dcArgLong(vm: *DCCallVM, v: c_long) void;
pub extern "c" fn dcArgLongLong(vm: *DCCallVM, v: c_longlong) void;
pub extern "c" fn dcArgFloat(vm: *DCCallVM, v: f32) void;
pub extern "c" fn dcArgDouble(vm: *DCCallVM, v: f64) void;
pub extern "c" fn dcArgPointer(vm: *DCCallVM, v: ?*const anyopaque) void;

pub extern "c" fn dcCallVoid(vm: *DCCallVM, fnp: *const anyopaque) void;
pub extern "c" fn dcCallBool(vm: *DCCallVM, fnp: *const anyopaque) c_int;
pub extern "c" fn dcCallChar(vm: *DCCallVM, fnp: *const anyopaque) u8;
pub extern "c" fn dcCallShort(vm: *DCCallVM, fnp: *const anyopaque) c_short;
pub extern "c" fn dcCallInt(vm: *DCCallVM, fnp: *const anyopaque) c_int;
pub extern "c" fn dcCallLong(vm: *DCCallVM, fnp: *const anyopaque) c_long;
pub extern "c" fn dcCallLongLong(vm: *DCCallVM, fnp: *const anyopaque) c_longlong;
pub extern "c" fn dcCallFloat(vm: *DCCallVM, fnp: *const anyopaque) f32;
pub extern "c" fn dcCallDouble(vm: *DCCallVM, fnp: *const anyopaque) f64;
pub extern "c" fn dcCallPointer(vm: *DCCallVM, fnp: *const anyopaque) ?*anyopaque;

// -- dyncall aggregate (struct-by-value) C API -----------------------------
// Phase 18 prelude. `dcAggrField` is variadic in C (the `...` carries a
// nested DCaggr* only when type == 'A'); we never pass nested aggregates, so
// declaring it with its named args is ABI-correct on the platforms dyncall
// supports (named args use the normal register convention regardless).

pub const DCaggr = opaque {};

pub extern "c" fn dcNewAggr(max_field_count: usize, size: usize) ?*DCaggr;
pub extern "c" fn dcFreeAggr(ag: *DCaggr) void;
pub extern "c" fn dcAggrField(ag: *DCaggr, type_: c_char, offset: c_int, array_len: usize) void;
pub extern "c" fn dcCloseAggr(ag: *DCaggr) void;
pub extern "c" fn dcBeginCallAggr(vm: *DCCallVM, ag: *const DCaggr) void;
pub extern "c" fn dcArgAggr(vm: *DCCallVM, ag: *const DCaggr, value: *const anyopaque) void;
pub extern "c" fn dcCallAggr(vm: *DCCallVM, fnp: *const anyopaque, ag: *const DCaggr, ret: *anyopaque) ?*anyopaque;

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

/// Scalar field size/alignment (LP64; matches natural C struct layout, which
/// is also what dyncall assumes). Aggregate/void are not valid struct fields.
fn kindSize(k: Kind) usize {
    return switch (k) {
        .bool_, .char => 1,
        .short => 2,
        .int, .float => 4,
        .long, .long_long, .double, .pointer, .cstring => 8,
        .void_, .aggregate => 0,
    };
}

/// dyncall field sigchar for a scalar Kind (note: our `.long` is C `long` →
/// dyncall 'j'; `.long_long` → 'l'; this differs from our top-level sig chars
/// 'l'/'L', which are our own convention).
fn dcFieldChar(k: Kind) u8 {
    return switch (k) {
        .bool_ => 'B',
        .char => 'c',
        .short => 's',
        .int => 'i',
        .long => 'j',
        .long_long => 'l',
        .float => 'f',
        .double => 'd',
        .pointer, .cstring => 'p',
        .void_, .aggregate => 'i', // unreachable for valid scalar fields
    };
}

/// A parsed by-value aggregate layout. `offsets[i]` is field `i`'s byte offset;
/// `size` is the total (tail-padded to struct alignment). `dc` is the dyncall
/// descriptor, built lazily on first use and freed in `deinit`.
pub const Aggr = struct {
    fields: []Kind,
    offsets: []u32,
    size: usize,
    dc: ?*DCaggr = null,

    pub fn deinit(self: *Aggr, allocator: std.mem.Allocator) void {
        if (self.dc) |d| dcFreeAggr(d);
        allocator.free(self.fields);
        allocator.free(self.offsets);
    }

    /// Build (and cache) the dyncall descriptor. Returns null on OOM.
    pub fn ensureDc(self: *Aggr) ?*DCaggr {
        if (self.dc) |d| return d;
        const d = dcNewAggr(self.fields.len, self.size) orelse return null;
        for (self.fields, self.offsets) |fk, off| {
            dcAggrField(d, @intCast(dcFieldChar(fk)), @intCast(off), 1);
        }
        dcCloseAggr(d);
        self.dc = d;
        return d;
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
    /// the width `Kind` rather than as new `Kind` variants, so `kindSize`/
    /// `dcFieldChar` (layout-only) stay untouched; sign matters only when
    /// marshaling a value wider than the signed range (bit-preserving narrow)
    /// and when zero- vs sign-extending a return. Parallel to `args` (always
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

// -- VM lifecycle ----------------------------------------------------------

/// Default VM stack size — generous enough for the Phase 9 test plan.
pub const DEFAULT_VM_SIZE: usize = 4096;

/// Single kernel-global VM pointer. dyncall is single-threaded; single VM
/// is enough until concurrency lands.
var g_vm: ?*DCCallVM = null;

pub fn ensureVm() ?*DCCallVM {
    if (g_vm) |v| return v;
    g_vm = dcNewCallVM(DEFAULT_VM_SIZE);
    return g_vm;
}

pub fn shutdown() void {
    if (g_vm) |v| {
        dcFree(v);
        g_vm = null;
    }
}

// -- Ref → dyncall arg marshaling -----------------------------------------

/// Push `value` into the VM as `kind`. Returns false on type mismatch OR
/// on a stale FFI pointer (Phase 9.5.5: pointer Refs whose owning Module
/// has been unloaded carry `[box_eid :ffi-stale? ?]` facts). Caller
/// distinguishes the two via `out_stale_box_eid` (0 = type mismatch,
/// non-zero = stale box).
fn pushArgChecked(
    store: *fact_store_mod.FactStore,
    vm: *DCCallVM,
    kind: Kind,
    unsigned: bool,
    value: Ref,
    out_stale_box_eid: *u64,
) bool {
    out_stale_box_eid.* = 0;
    if (kind == .pointer or kind == .cstring) {
        if (boxed.isFfiPtr(value)) {
            const box_eid = boxed.ffiPtrBoxEid(value);
            if (box_eid != 0) {
                if (store.currentValue(box_eid, kernel_eids.ATTR_FFI_STALE)) |_| {
                    out_stale_box_eid.* = box_eid;
                    return false;
                }
            }
        }
    }
    return pushArg(vm, kind, unsigned, value);
}

/// Push `value` into the VM as `kind`. Returns false on type mismatch so the
/// caller can abort + return NIL.
///
/// Phase 19.6: `unsigned` only affects the `short`/`int` arms — widths where a
/// positive birnam smallint can exceed the *signed* max (a range-checked
/// `@intCast` would panic). There we bit-preserve: truncate to the unsigned
/// width, then `@bitCast` to the signed width dyncall's arg fn expects (the
/// callee reads the same bytes back as unsigned). `char` already truncates;
/// `long`/`long_long` are i64-wide, so any representable smallint fits without
/// special handling.
fn pushArg(vm: *DCCallVM, kind: Kind, unsigned: bool, value: Ref) bool {
    switch (kind) {
        .void_ => return false,
        .bool_ => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgBool(vm, @intCast(@as(c_int, @intCast(ref_mod.untagSmallInt(value)))));
        },
        .char => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgChar(vm, @intCast(@as(u8, @truncate(@as(u64, @bitCast(ref_mod.untagSmallInt(value)))))));
        },
        .short => {
            if (!ref_mod.isSmallInt(value)) return false;
            const v = ref_mod.untagSmallInt(value);
            if (unsigned) {
                dcArgShort(vm, @bitCast(@as(u16, @truncate(@as(u64, @bitCast(v))))));
            } else {
                dcArgShort(vm, @intCast(v));
            }
        },
        .int => {
            if (!ref_mod.isSmallInt(value)) return false;
            const v = ref_mod.untagSmallInt(value);
            if (unsigned) {
                dcArgInt(vm, @bitCast(@as(u32, @truncate(@as(u64, @bitCast(v))))));
            } else {
                dcArgInt(vm, @intCast(v));
            }
        },
        .long => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgLong(vm, @intCast(ref_mod.untagSmallInt(value)));
        },
        .long_long => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgLongLong(vm, @intCast(ref_mod.untagSmallInt(value)));
        },
        .float => {
            if (!boxed.isFloat(value)) return false;
            dcArgFloat(vm, @floatCast(boxed.untagFloat(value)));
        },
        .double => {
            if (!boxed.isFloat(value)) return false;
            dcArgDouble(vm, boxed.untagFloat(value));
        },
        .pointer, .cstring => {
            // Change 3: nil marshals to a NULL pointer (nil-is-value). Gives
            // callers `start: nil` for optional `char*` / `void*` args.
            if (ref_mod.isNil(value)) {
                dcArgPointer(vm, null);
                return true;
            }
            // Birnam strings for `.cstring` are handled in the invoke loop
            // (they need a scratch NUL-terminated copy whose lifetime spans
            // the call). By the time we reach here, a `.cstring` value is
            // expected to be an FfiPtr (raw `char*`); `.pointer` always is.
            if (!boxed.isFfiPtr(value)) return false;
            const addr = boxed.untagFfiPtr(value);
            dcArgPointer(vm, @ptrFromInt(addr));
        },
        // Handled in the invoke loop (needs the per-arg Aggr), never here.
        .aggregate => return false,
    }
    return true;
}

/// Dispatch the appropriate dcCallX based on return kind. Returns the raw
/// C-side value boxed/tagged into a birnam Ref.
///
/// `owner_mod_eid` (Phase 9.5.5): when non-zero, returned pointer values
/// (`.pointer` / `.cstring`) are boxed via `mkFfiPtrOwned`, asserting
/// `[box :ffi-owner mod]` so Module unload can find + stale them. When
/// zero, fall back to unowned `mkFfiPtr` (Phase 9 behavior).
fn callAndWrap(
    allocator: std.mem.Allocator,
    store: ?*fact_store_mod.FactStore,
    table: ?*object_table_mod.ObjectTable,
    vm: *DCCallVM,
    ret: Kind,
    ret_aggr: ?*Aggr,
    ret_unsigned: bool,
    addr: usize,
    owner_mod_eid: u64,
) Ref {
    const fnp: *const anyopaque = @ptrFromInt(addr);
    switch (ret) {
        .void_ => {
            dcCallVoid(vm, fnp);
            return ref_mod.NIL;
        },
        .bool_ => {
            const r = dcCallBool(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .char => {
            // Phase 19.6: `uc` zero-extends (0..255); signed `c` sign-extends.
            const r = dcCallChar(vm, fnp);
            const v: i64 = if (ret_unsigned) @as(u8, @bitCast(r)) else @intCast(r);
            return ref_mod.mkSmallInt(v) orelse ref_mod.NIL;
        },
        .short => {
            const r = dcCallShort(vm, fnp);
            const v: i64 = if (ret_unsigned) @as(u16, @bitCast(r)) else @intCast(r);
            return ref_mod.mkSmallInt(v) orelse ref_mod.NIL;
        },
        .int => {
            const r = dcCallInt(vm, fnp);
            const v: i64 = if (ret_unsigned) @as(u32, @bitCast(r)) else @intCast(r);
            return ref_mod.mkSmallInt(v) orelse ref_mod.NIL;
        },
        .long => {
            const r = dcCallLong(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .long_long => {
            const r = dcCallLongLong(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .float => {
            const r = dcCallFloat(vm, fnp);
            return boxed.mkFloat(allocator, @floatCast(r)) catch ref_mod.NIL;
        },
        .double => {
            const r = dcCallDouble(vm, fnp);
            return boxed.mkFloat(allocator, r) catch ref_mod.NIL;
        },
        .pointer => {
            const r = dcCallPointer(vm, fnp);
            const addr_ret: usize = @intFromPtr(r);
            if (owner_mod_eid != 0 and store != null and table != null) {
                return boxed.mkFfiPtrOwned(allocator, store.?, table.?, addr_ret, owner_mod_eid) catch ref_mod.NIL;
            }
            return boxed.mkFfiPtr(allocator, addr_ret) catch ref_mod.NIL;
        },
        .cstring => {
            // Change 4: a `Z` return is a C `char*`; copy it into a birnam
            // string. NULL return → nil (EOF / absence). The C buffer is the
            // callee's (e.g. libtecla's internal line buffer); we only read it,
            // so there is nothing to free here.
            const r = dcCallPointer(vm, fnp);
            if (r == null) return ref_mod.NIL;
            const cptr: [*:0]const u8 = @ptrCast(r);
            return strings.mkString(allocator, std.mem.span(cptr)) catch ref_mod.NIL;
        },
        .aggregate => {
            // Phase 18: a by-value struct return. `dcBeginCallAggr` was already
            // issued in invokeWithOwner (it must precede arg pushing). Allocate
            // a libc buffer of the struct's size, let dyncall write into it, and
            // hand back an FfiPtr — the caller reads fields via peek and frees
            // via `[p free]`.
            const aggr = ret_aggr orelse return ref_mod.NIL;
            const dc = aggr.ensureDc() orelse return ref_mod.NIL;
            const buf = std.c.malloc(aggr.size) orelse return ref_mod.NIL;
            @memset(@as([*]u8, @ptrCast(buf))[0..aggr.size], 0);
            _ = dcCallAggr(vm, fnp, dc, buf);
            return boxed.mkFfiPtr(allocator, @intFromPtr(buf)) catch ref_mod.NIL;
        },
    }
}

/// Top-level invocation. Resolves the kernel-global VM, configures variadic
/// mode if the sig is variadic, marshals args, calls, wraps return.
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

    // Phase 18: refuse aggregate calls where dyncall can't marshal them (see
    // `aggregates_supported`). Bail BEFORE touching the VM so no NULL vtable
    // slot is ever dereferenced.
    if (!aggregates_supported) {
        if (sig.ret == .aggregate) return ref_mod.NIL;
        for (sig.args) |k| {
            if (k == .aggregate) return ref_mod.NIL;
        }
    }

    const vm = ensureVm() orelse return ref_mod.NIL;
    dcReset(vm);
    const is_variadic = sig.variadic_at != null;
    if (is_variadic) dcMode(vm, DC_CALL_C_ELLIPSIS);

    // Phase 18: a by-value struct return requires `dcBeginCallAggr` BEFORE any
    // args are pushed (dyncall protocol). Build the descriptor here so the
    // callAndWrap aggregate branch can reuse the cached `dc`.
    if (sig.ret == .aggregate) {
        const aggr = sig.ret_aggr orelse return ref_mod.NIL;
        const dc = aggr.ensureDc() orelse return ref_mod.NIL;
        dcBeginCallAggr(vm, dc);
    }

    // Change 2: birnam strings passed for a `.cstring` (`Z`) arg are marshaled
    // to NUL-terminated C buffers here. Each buffer must outlive the call (C
    // reads through the pointer during dcCall), so we hold them until after
    // callAndWrap, then free. dyncall only stores the pointer value, never the
    // bytes — safe because the call is synchronous. (A C function that stashes
    // the pointer past return would dangle; that is `.pointer` territory, where
    // the caller owns the buffer explicitly.)
    var scratch: std.ArrayList([]u8) = .{};
    defer {
        for (scratch.items) |buf| allocator.free(buf);
        scratch.deinit(allocator);
    }

    for (sig.args, args, 0..) |kind, value, i| {
        if (is_variadic and i == sig.variadic_at.?) {
            dcMode(vm, DC_CALL_C_ELLIPSIS_VARARGS);
        }
        if (kind == .aggregate) {
            // Phase 18: a by-value struct arg is passed as an FfiPtr to its
            // bytes (built via `Module alloc:` + `poke…:at:`). dyncall reads
            // `aggr.size` bytes through the pointer during the call.
            const aggr = sig.arg_aggrs[i] orelse return ref_mod.NIL;
            const dc = aggr.ensureDc() orelse return ref_mod.NIL;
            if (!boxed.isFfiPtr(value)) return ref_mod.NIL;
            dcArgAggr(vm, dc, @ptrFromInt(boxed.untagFfiPtr(value)));
            continue;
        }
        if (kind == .cstring and strings.isString(value)) {
            const s = strings.bytes(value);
            const buf = allocator.alloc(u8, s.len + 1) catch return ref_mod.NIL;
            @memcpy(buf[0..s.len], s);
            buf[s.len] = 0;
            scratch.append(allocator, buf) catch {
                allocator.free(buf);
                return ref_mod.NIL;
            };
            dcArgPointer(vm, buf.ptr);
            continue;
        }
        const uns = sig.arg_unsigned.len != 0 and sig.arg_unsigned[i];
        if (store) |s| {
            var stale_box: u64 = 0;
            if (!pushArgChecked(s, vm, kind, uns, value, &stale_box)) return ref_mod.NIL;
        } else {
            if (!pushArg(vm, kind, uns, value)) return ref_mod.NIL;
        }
    }
    return callAndWrap(allocator, store, table, vm, sig.ret, sig.ret_aggr, sig.ret_unsigned, addr, owner_mod_eid);
}

// -- Wrap registry + per-arity dispatch shims -------------------------------

const send_mod = @import("send.zig");
const fact_store_mod = @import("fact_store.zig");
const object_table_mod = @import("object_table.zig");
const closure_mod = @import("closure.zig");
const kernel_eids = @import("kernel_eids.zig");

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
