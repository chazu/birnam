//! libid — the pure Piumarta object system, hand-written in Zig.
//!
//! A faithful port of the model in `~/dev/c/birnam/src/id.c` (Piumarta's "open,
//! extensible object model"). This is the FLOOR the siphon loads first — before
//! peg, before the compiler, and long before the fact store. There are **no
//! eids and no fact store here**: identity is the pointer, the vtable pointer
//! lives at `object[-1]`, selectors are interned symbol objects compared by
//! pointer, and a vtable carries its own method dict (`keys`/`values`) and a
//! `parent` for delegation.
//!
//! Bootstrap (the 5 messages): `lookup`, `addMethod`, `allocate`, `delegated`
//! are installed on the metavtable; `addMethod` is then used metacircularly to
//! install the rest. `bind` bottoms out when binding `lookup` on the metavtable.

const std = @import("std");
const idapi = @import("idapi.zig");

const ca = std.heap.c_allocator;

extern fn dlopen(path: [*:0]const u8, mode: c_int) ?*anyopaque;
extern fn dlsym(handle: ?*anyopaque, name: [*:0]const u8) ?*anyopaque;

/// Every object is referenced by an opaque pointer; its vtable pointer sits one
/// word *before* it (`object[-1]`). `Oop` = "ordinary object pointer".
pub const Oop = ?*anyopaque;

/// A primitive method: receiver + a flat argument buffer. Pointer-based, no eids.
pub const Method = *const fn (self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop;

/// The word reserved before each payload for the vtable pointer.
const VT: usize = @sizeOf(usize);

/// A vtable is an object too (its own vtable pointer at [-1] is the metavtable).
/// `keys`/`values` are the parallel method-dictionary arrays; `parent` is the
/// delegation link (null = top of chain).
pub const Vtable = extern struct {
    size: usize,
    tally: usize,
    keys: ?[*]Oop,
    values: ?[*]Oop,
    parent: ?*Vtable,
};

/// An interned name. Selectors are `*Symbol`, compared by pointer identity.
pub const Symbol = extern struct {
    string: [*:0]u8,
};

/// An uninterned byte string — the floor's value type for source text and
/// literal bytes (distinct from `Symbol`, which is interned for selector
/// identity). Many modules need it (peg literals + source, lang, io), so it
/// lives in the floor alongside `Symbol`/`SmallInt`. The payload of a String
/// libid object; the bytes are c_allocator-owned.
pub const String = extern struct {
    ptr: [*]const u8,
    len: usize,
};

// -- The well-known objects (set by `boot`) ---------------------------------

pub var vtable_vt: *Vtable = undefined; // the vtable of vtables (self-describing)
pub var object_vt: *Vtable = undefined; // top of the delegation chain
pub var symbol_vt: *Vtable = undefined;
pub var string_vt: *Vtable = undefined; // the vtable of (uninterned) byte strings
pub var smallint_vt: *Vtable = undefined; // the vtable of tagged fixnums (F1)
pub var nil_vt: *Vtable = undefined;
pub var bool_vt: *Vtable = undefined; // shared by true_obj + false_obj

// The canonical singleton objects (boot-allocated, compared by pointer). These
// are NOT immediates — only SmallInt is (maru-style). They are distinct from the
// Zig `null` Oop, which stays *internal absence* (no-NULL rule, F1 / docs/34).
pub var nil_obj: Oop = undefined;
pub var true_obj: Oop = undefined;
pub var false_obj: Oop = undefined;
var symbol_list: *Vtable = undefined; // the intern table (keys = symbols)
var env: *Vtable = undefined; // the export environment (keys = symbols, values = objects)

pub var s_lookup: *Symbol = undefined;
pub var s_addMethod: *Symbol = undefined;
pub var s_allocate: *Symbol = undefined;
pub var s_delegated: *Symbol = undefined;
pub var s_dnu: *Symbol = undefined; // doesNotUnderstand:

var booted = false;

// -- Raw allocation + the `-1` vtable-pointer trick -------------------------
//
// Every heap object is tracked in `g_blocks` (keyed by its object/payload
// address) so the conservative collector can know each object's extent and
// whether it may be reclaimed. `kind` records that: instances are `.collectable`
// (alloc/mkString); vtables, symbols and singletons are `.permanent` (never
// swept, never traced — their only outgoing Oops live in the env/interner, which
// are scanned as roots, not via tracing). See the GC section below + docs/34 §F3.

const GcKind = enum(u8) { collectable, permanent };
const Block = struct { total: usize, kind: GcKind, mark: bool = false };
var g_blocks: std.AutoHashMapUnmanaged(usize, Block) = .{};

fn allocRaw(payload: usize, kind: GcKind) Oop {
    const total = (VT + payload + 7) & ~@as(usize, 7);
    const buf = ca.alignedAlloc(u8, .@"16", total) catch @panic("libid: oom");
    @memset(buf, 0);
    const obj_addr = @intFromPtr(buf.ptr) + VT;
    g_blocks.put(ca, obj_addr, .{ .total = total, .kind = kind }) catch @panic("libid: oom");
    return @ptrFromInt(obj_addr);
}

fn vtSlot(obj: Oop) *?*Vtable {
    return @ptrFromInt(@intFromPtr(obj.?) - VT);
}
pub fn vtableOf(obj: Oop) ?*Vtable {
    if (obj == null) return null; // internal absence has no vtable
    if ((@intFromPtr(obj.?) & 1) != 0) return smallint_vt; // tagged fixnum (F1)
    return vtSlot(obj).*;
}

// -- SmallInt: the one immediate (maru-style low-bit tag, F1) ----------------
// bit0 = 1 ⇒ a 63-bit signed fixnum (value in the high bits); bit0 = 0 ⇒ a heap
// pointer (16-byte alignment guarantees the low bit is free). A fixnum is never
// dereferenced — `vtableOf` maps it to `smallint_vt`.

pub fn isSmallInt(o: Oop) bool {
    return o != null and (@intFromPtr(o.?) & 1) != 0;
}
pub fn mkSmallInt(v: i64) Oop {
    const u: usize = @bitCast(v);
    return @ptrFromInt((u << 1) | 1);
}
pub fn smallIntValue(o: Oop) i64 {
    return @as(i64, @bitCast(@intFromPtr(o.?))) >> 1; // arithmetic shift restores sign
}

// -- String: an uninterned byte-string value (floor type) --------------------

inline fn asString(o: Oop) *String {
    return @ptrCast(@alignCast(o.?));
}
pub fn mkString(bytes: []const u8) Oop {
    const obj = allocRaw(@sizeOf(String), .collectable);
    setVtable(obj, string_vt);
    const dup = ca.dupe(u8, bytes) catch @panic("libid: oom");
    asString(obj).* = .{ .ptr = dup.ptr, .len = dup.len };
    return obj;
}
pub fn stringBytes(o: Oop) []const u8 {
    const s = asString(o);
    return s.ptr[0..s.len];
}
fn setVtable(obj: Oop, vt: ?*Vtable) void {
    vtSlot(obj).* = vt;
}

inline fn oop(p: anytype) Oop {
    return @ptrCast(p);
}
inline fn asVtable(o: Oop) *Vtable {
    return @ptrCast(@alignCast(o.?));
}

// -- send / bind (metacircular) ---------------------------------------------

/// Resolve `sel` on `rcv`'s vtable. Bottoms out when binding `lookup` on the
/// metavtable (else infinite regress); otherwise sends `lookup` to the vtable.
pub fn bind(rcv: Oop, sel: *Symbol) ?Method {
    const vt = vtableOf(rcv) orelse return null;
    const m: Oop = if (vt == vtable_vt and sel == s_lookup)
        vtableLookup(oop(vt), &[_]Oop{oop(sel)}, 1)
    else
        send1(oop(vt), s_lookup, oop(sel));
    if (m == null) return null;
    return @ptrCast(@alignCast(m));
}

pub fn send(rcv: Oop, sel: *Symbol, args: []const Oop) Oop {
    if (bind(rcv, sel)) |m| return m(rcv, args.ptr, args.len);

    // doesNotUnderstand: — re-dispatch an OVERRIDABLE message rather than trap.
    // The handler receives the original selector followed by the original args
    // (a stack buffer — no reified Message object yet; F1/F5 make that cheap).
    // Bottom-out: if doesNotUnderstand: itself is unbound, this is a genuine bug
    // (mirrors how `bind` bottoms out `lookup`).
    const handler = bind(rcv, s_dnu) orelse {
        std.debug.print("libid: doesNotUnderstand: {s} (and no doesNotUnderstand: handler)\n", .{sel.string});
        @panic("libid: method not found");
    };
    var buf: [1 + max_dnu_args]Oop = undefined;
    buf[0] = oop(sel);
    const k = @min(args.len, max_dnu_args);
    if (k > 0) @memcpy(buf[1 .. 1 + k], args[0..k]);
    return handler(rcv, &buf, 1 + k);
}

/// How many of the original args a failed send forwards to `doesNotUnderstand:`
/// (beyond the leading selector). Enough for real keyword sends; a reified
/// Message object lifts the cap later.
const max_dnu_args: usize = 16;

/// The default `doesNotUnderstand:` installed on `object_vt` — overridable. For
/// now it traps (as the old hard panic did) but as a *method*, so a prototype
/// can intercept first. Swappable once F7 gives errors a value/condition form.
fn objectDoesNotUnderstand(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const sel: *Symbol = @ptrCast(@alignCast(args[0]));
    std.debug.print("libid: doesNotUnderstand: {s}\n", .{sel.string});
    @panic("libid: doesNotUnderstand:");
}

// Thin arity helpers (no varargs packing — Zig slices do the job).
pub fn send0(rcv: Oop, sel: *Symbol) Oop {
    return send(rcv, sel, &[_]Oop{});
}
pub fn send1(rcv: Oop, sel: *Symbol, a: Oop) Oop {
    return send(rcv, sel, &[_]Oop{a});
}
pub fn send2(rcv: Oop, sel: *Symbol, a: Oop, b: Oop) Oop {
    return send(rcv, sel, &[_]Oop{ a, b });
}

// -- The primitive methods (installed on vtable_vt) -------------------------

/// `[vt lookup: key]` — find the method bound to `key`, walking `parent`.
fn vtableLookup(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const vt = asVtable(self);
    const key = args[0];
    if (vt.keys) |keys| {
        var i: usize = 0;
        while (i < vt.tally) : (i += 1) {
            if (keys[i] == key) return vt.values.?[i];
        }
    }
    if (vt.parent) |p| return send1(oop(p), s_lookup, key);
    return null;
}

/// `[vt addMethod: key with: method]` — bind `key` → `method` (overwrite if present).
fn vtableAddMethod(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const vt = asVtable(self);
    const key = args[0];
    const method = args[1];
    var i: usize = 0;
    while (i < vt.tally) : (i += 1) {
        if (vt.keys.?[i] == key) {
            vt.values.?[i] = method;
            return method;
        }
    }
    if (vt.tally == vt.size) growDict(vt);
    vt.keys.?[vt.tally] = key;
    vt.values.?[vt.tally] = method;
    vt.tally += 1;
    return method;
}

/// `[vt allocate: size]` — a fresh instance whose vtable is `vt`. `size` is the
/// payload byte count, passed pointer-encoded (libid is pre-SmallInt).
fn vtableAllocate(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const vt = asVtable(self);
    const size: usize = @intFromPtr(args[0]);
    const obj = allocRaw(size, .collectable);
    setVtable(obj, vt);
    return obj;
}

/// `[vt delegated]` — a fresh child vtable delegating to `vt`.
fn vtableDelegated(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    return oop(newVtable(asVtable(self)));
}

// -- Construction helpers (used by boot; not messages) ----------------------

fn growDict(vt: *Vtable) void {
    const ns = if (vt.size == 0) 4 else vt.size * 2;
    const nk = ca.alloc(Oop, ns) catch @panic("libid: oom");
    const nv = ca.alloc(Oop, ns) catch @panic("libid: oom");
    @memset(nk, null);
    @memset(nv, null);
    if (vt.keys) |k| std.mem.copyForwards(Oop, nk, k[0..vt.tally]);
    if (vt.values) |v| std.mem.copyForwards(Oop, nv, v[0..vt.tally]);
    vt.keys = nk.ptr;
    vt.values = nv.ptr;
    vt.size = ns;
}

/// Allocate a bare vtable object (vtable pointer = vtable_vt) with `parent`.
fn newVtable(parent: ?*Vtable) *Vtable {
    const obj = allocRaw(@sizeOf(Vtable), .permanent);
    // During boot, vtable_vt may not exist yet; `boot` patches the first ones.
    if (booted) setVtable(obj, vtable_vt);
    const vt = asVtable(obj);
    vt.size = 0;
    vt.tally = 0;
    vt.keys = null;
    vt.values = null;
    vt.parent = parent;
    return vt;
}

fn intern(string: [:0]const u8) *Symbol {
    var i: usize = 0;
    while (i < symbol_list.tally) : (i += 1) {
        const sym: *Symbol = @ptrCast(@alignCast(symbol_list.keys.?[i]));
        if (std.mem.orderZ(u8, sym.string, string) == .eq) return sym;
    }
    const obj = allocRaw(@sizeOf(Symbol), .permanent);
    setVtable(obj, symbol_vt);
    const sym = asSymbol(obj);
    sym.string = (ca.dupeZ(u8, string) catch @panic("libid: oom")).ptr;
    _ = vtableAddMethod(oop(symbol_list), &[_]Oop{ oop(sym), null }, 2);
    return sym;
}
inline fn asSymbol(o: Oop) *Symbol {
    return @ptrCast(@alignCast(o.?));
}

// -- Bootstrap --------------------------------------------------------------

/// Bring up the object system. After this, `send` is fully operational.
/// Idempotent: a second call (e.g. the id dylib re-loaded through the id api)
/// returns the same live system rather than re-bootstrapping fresh objects.
pub fn boot() void {
    if (booted) return;
    // vtable_vt: the metavtable, self-describing (its own vtable is itself).
    vtable_vt = newVtable(null);
    setVtable(oop(vtable_vt), vtable_vt);

    // object_vt: top of the delegation chain; vtables delegate to it.
    object_vt = newVtable(null);
    setVtable(oop(object_vt), vtable_vt);
    vtable_vt.parent = object_vt;

    symbol_vt = newVtable(object_vt);
    setVtable(oop(symbol_vt), vtable_vt);

    symbol_list = newVtable(null);
    setVtable(oop(symbol_list), vtable_vt);
    env = newVtable(null);
    setVtable(oop(env), vtable_vt);
    booted = true;

    // The fixnum vtable + the canonical singletons (F1). newVtable now auto-sets
    // their meta to vtable_vt (booted); each delegates to object_vt.
    smallint_vt = newVtable(object_vt);
    string_vt = newVtable(object_vt);
    nil_vt = newVtable(object_vt);
    bool_vt = newVtable(object_vt);
    nil_obj = allocRaw(0, .permanent);
    setVtable(nil_obj, nil_vt);
    true_obj = allocRaw(0, .permanent);
    setVtable(true_obj, bool_vt);
    false_obj = allocRaw(0, .permanent);
    setVtable(false_obj, bool_vt);

    s_lookup = intern("lookup");
    s_addMethod = intern("addMethod");
    s_allocate = intern("allocate");
    s_delegated = intern("delegated");
    s_dnu = intern("doesNotUnderstand:");

    // Install lookup + addMethod directly (send can't work without them), then
    // use addMethod *via send* (metacircular) for the rest.
    _ = vtableAddMethod(oop(vtable_vt), &[_]Oop{ oop(s_lookup), @constCast(@ptrCast(&vtableLookup)) }, 2);
    _ = vtableAddMethod(oop(vtable_vt), &[_]Oop{ oop(s_addMethod), @constCast(@ptrCast(&vtableAddMethod)) }, 2);
    _ = send2(oop(vtable_vt), s_addMethod, oop(s_allocate), @constCast(@ptrCast(&vtableAllocate)));
    _ = send2(oop(vtable_vt), s_addMethod, oop(s_delegated), @constCast(@ptrCast(&vtableDelegated)));

    // The default doesNotUnderstand: lives on object_vt (top of every delegation
    // chain), so every object inherits an overridable handler (F2 / docs/34).
    _ = send2(oop(object_vt), s_addMethod, oop(s_dnu), @constCast(@ptrCast(&objectDoesNotUnderstand)));
}

/// Intern a symbol from outside (the siphon / later modules need this).
pub fn symbol(string: [:0]const u8) *Symbol {
    return intern(string);
}

// -- The export environment (name -> object) — the re-import namespace -------

/// `export key -> value`: bind a name to an object so others can `import` it.
fn exportObj(key: [:0]const u8, value: Oop) Oop {
    const sym = intern(key);
    _ = vtableAddMethod(oop(env), &[_]Oop{ oop(sym), value }, 2);
    return value;
}
/// `import key`: the object previously exported under `key`, or null.
fn importObj(key: [:0]const u8) Oop {
    const sym = intern(key);
    if (env.keys) |keys| {
        var i: usize = 0;
        while (i < env.tally) : (i += 1) {
            if (keys[i] == oop(sym)) return env.values.?[i];
        }
    }
    return null;
}

// -- The IdApi table (callconv(.c) wrappers over the system) ----------------

fn dlopenC(path: [*:0]const u8, mode: c_int) callconv(.c) ?*anyopaque {
    return dlopen(path, mode);
}
fn dlsymC(handle: ?*anyopaque, name: [*:0]const u8) callconv(.c) ?*anyopaque {
    return dlsym(handle, name);
}
fn internC(string: [*:0]const u8) callconv(.c) Oop {
    return oop(intern(std.mem.span(string)));
}
fn protoC(base: Oop) callconv(.c) Oop {
    return oop(newVtable(asVtable(base)));
}
fn methodC(vt: Oop, sel: Oop, m: idapi.Method) callconv(.c) void {
    _ = vtableAddMethod(vt, &[_]Oop{ sel, @constCast(@ptrCast(m)) }, 2);
}
fn allocC(vt: Oop, size: usize) callconv(.c) Oop {
    const obj = allocRaw(size, .collectable);
    setVtable(obj, asVtable(vt));
    return obj;
}
fn importC(key: [*:0]const u8) callconv(.c) Oop {
    return importObj(std.mem.span(key));
}
fn exportC(key: [*:0]const u8, value: Oop) callconv(.c) Oop {
    return exportObj(std.mem.span(key), value);
}
fn isExportedC(key: [*:0]const u8) callconv(.c) c_int {
    return if (importObj(std.mem.span(key)) != null) 1 else 0;
}
fn sendC(rcv: Oop, sel: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    return send(rcv, @ptrCast(@alignCast(sel)), args[0..n]);
}
fn mkSmallIntC(v: i64) callconv(.c) Oop {
    return mkSmallInt(v);
}
fn smallIntValueC(o: Oop) callconv(.c) i64 {
    return smallIntValue(o);
}
fn isSmallIntC(o: Oop) callconv(.c) c_int {
    return if (isSmallInt(o)) 1 else 0;
}
fn mkStringC(ptr: [*]const u8, len: usize) callconv(.c) Oop {
    return mkString(ptr[0..len]);
}
fn stringBytesC(o: Oop, out_len: *usize) callconv(.c) [*]const u8 {
    const s = asString(o);
    out_len.* = s.len;
    return s.ptr;
}
fn symbolNameC(o: Oop) callconv(.c) [*:0]const u8 {
    return asSymbol(o).string;
}
fn vtableOfC(o: Oop) callconv(.c) Oop {
    return oop(vtableOf(o).?);
}

// -- GC: a self-contained conservative mark-sweep (F3) -----------------------
//
// Why conservative: birnam compiles methods to native code (libtcc) that holds
// live Oops in C locals/registers with no stack maps — a precise/moving collector
// could not find those roots. So we follow Piumarta: scan the stack + saved
// registers conservatively, treating any word that equals a live object's address
// as a root (false positives only over-retain — never unsafe). SmallInts (bit0=1)
// are skipped; references are always to an object's payload START (the Oop), so an
// exact-address table lookup suffices (no interior pointers).
//
// Only `.collectable` objects are swept. `.permanent` ones (vtables, symbols,
// singletons) are never freed and never traced: their only outgoing Oops live in
// the env / interner / vtable dicts, which are reached as ROOTS (the env+interner
// providers below), not by tracing. Modules holding Oops in non-id memory (the
// store's fact list) register their own root providers.

const MaxRootProviders = 32;
var g_root_providers: [MaxRootProviders]idapi.RootProvider = undefined;
var g_root_provider_count: usize = 0;
var g_stack_bottom: usize = 0;
var g_gc_worklist: std.ArrayListUnmanaged(usize) = .{};

extern fn _setjmp(buf: [*]usize) c_int;

fn gcMarkWord(w: usize) void {
    if (w == 0 or (w & 1) != 0) return; // null / SmallInt immediate
    const blk = g_blocks.getPtr(w) orelse return;
    if (blk.kind == .permanent or blk.mark) return;
    blk.mark = true;
    g_gc_worklist.append(ca, w) catch @panic("gc: oom");
}

fn gcMarkOopC(o: Oop) callconv(.c) void {
    gcMarkWord(@intFromPtr(o orelse return));
}

/// Conservatively scan an aligned [lo, hi) word range for roots.
fn gcScanRange(lo: usize, hi: usize) void {
    var a = (lo + 7) & ~@as(usize, 7);
    while (a + @sizeOf(usize) <= hi) : (a += @sizeOf(usize)) {
        gcMarkWord(@as(*const usize, @ptrFromInt(a)).*);
    }
}

/// Trace a marked object: conservatively scan its payload for more live pointers.
fn gcTrace(w: usize) void {
    const blk = g_blocks.get(w) orelse return;
    gcScanRange(w, w + (blk.total - VT));
}

// The env + interner keep Oops in their `keys`/`values` arrays (c_allocator
// memory), so they are scanned as roots — this is what keeps every EXPORTED
// object (factories, the store, facades) and every interned symbol alive.
fn gcMarkVtableDict(vt: *Vtable, comptime keys_too: bool) void {
    if (vt.values) |v| {
        var i: usize = 0;
        while (i < vt.tally) : (i += 1) gcMarkWord(@intFromPtr(v[i] orelse continue));
    }
    if (keys_too) if (vt.keys) |k| {
        var i: usize = 0;
        while (i < vt.tally) : (i += 1) gcMarkWord(@intFromPtr(k[i] orelse continue));
    };
}

fn gcCollect() usize {
    if (g_stack_bottom == 0) return 0; // cannot find stack roots → refuse to sweep

    // Flush callee-saved registers to a stack buffer, then scan the whole stack
    // from here down to the recorded bottom (covers every caller frame's Oops).
    var jb: [256]usize align(16) = undefined;
    _ = _setjmp(&jb);
    const sp = @intFromPtr(&jb);
    gcScanRange(sp, g_stack_bottom);

    // Roots in non-id memory: the export env, the interner, and module providers.
    gcMarkVtableDict(env, false); // export values (keys are symbols = permanent)
    gcMarkVtableDict(symbol_list, true); // interned symbols
    var pi: usize = 0;
    while (pi < g_root_provider_count) : (pi += 1) g_root_providers[pi](&gcMarkOopC);

    // Trace transitively.
    while (g_gc_worklist.pop()) |w| gcTrace(w);

    // Sweep unmarked collectables (collect dead keys first — can't mutate while
    // iterating), then clear marks for next time.
    var dead: std.ArrayListUnmanaged(usize) = .{};
    defer dead.deinit(ca);
    var it = g_blocks.iterator();
    while (it.next()) |e| {
        if (e.value_ptr.kind == .collectable and !e.value_ptr.mark)
            dead.append(ca, e.key_ptr.*) catch @panic("gc: oom");
    }
    for (dead.items) |k| {
        const total = g_blocks.get(k).?.total;
        const base: [*]align(16) u8 = @ptrFromInt(k - VT);
        ca.free(base[0..total]);
        _ = g_blocks.remove(k);
    }
    var it2 = g_blocks.iterator();
    while (it2.next()) |e| e.value_ptr.mark = false;
    return dead.items.len;
}

fn gcSetStackBottomC(addr: usize) callconv(.c) void {
    g_stack_bottom = addr;
}
fn gcRegisterRootProviderC(p: idapi.RootProvider) callconv(.c) void {
    if (g_root_provider_count >= MaxRootProviders) @panic("gc: too many root providers");
    g_root_providers[g_root_provider_count] = p;
    g_root_provider_count += 1;
}
fn gcCollectC() callconv(.c) usize {
    return gcCollect();
}
fn gcLiveCountC() callconv(.c) usize {
    return g_blocks.count();
}

var g_api: idapi.IdApi = undefined;

/// `id_init()` — boot the object system, build the IdApi table, and EXPORT the
/// id layer's own objects so the siphon (or any module) can re-import them.
/// Matches the C `_libid_init`: the siphon dlsyms + calls this to get the API.
pub export fn id_init() callconv(.c) *idapi.IdApi {
    boot();
    g_api = .{
        .dlopen = &dlopenC,
        .dlsym = &dlsymC,
        .object_vt = oop(object_vt),
        .vtable_vt = oop(vtable_vt),
        .symbol_vt = oop(symbol_vt),
        .intern = &internC,
        .proto = &protoC,
        .method = &methodC,
        .alloc = &allocC,
        .import = &importC,
        .exportObj = &exportC,
        .isExported = &isExportedC,
        .send = &sendC,
        .smallint_vt = oop(smallint_vt),
        .nilObj = nil_obj,
        .trueObj = true_obj,
        .falseObj = false_obj,
        .mkSmallInt = &mkSmallIntC,
        .smallIntValue = &smallIntValueC,
        .isSmallInt = &isSmallIntC,
        .string_vt = oop(string_vt),
        .mkString = &mkStringC,
        .stringBytes = &stringBytesC,
        .symbolName = &symbolNameC,
        .vtableOf = &vtableOfC,
        .gcSetStackBottom = &gcSetStackBottomC,
        .gcRegisterRootProvider = &gcRegisterRootProviderC,
        .gcCollect = &gcCollectC,
        .gcLiveCount = &gcLiveCountC,
    };
    // Publish the id layer into its own environment — so it is re-importable.
    _ = exportObj("Object", oop(object_vt));
    _ = exportObj("Vtable", oop(vtable_vt));
    _ = exportObj("Symbol", oop(symbol_vt));
    return &g_api;
}

// -- Proof: the object system stands on its own -----------------------------

test "libid: metacircular bootstrap + a user method dispatches" {
    boot();

    // The closure is self-describing: vtable_vt's vtable is itself.
    try std.testing.expect(vtableOf(oop(vtable_vt)) == vtable_vt);
    try std.testing.expect(vtableOf(oop(object_vt)) == vtable_vt);
    try std.testing.expect(vtable_vt.parent == object_vt);

    // Define a Point vtable delegating to object, via SEND (metacircular path):
    //   point_vt = [object_vt delegated]
    const point_vt = asVtable(send0(oop(object_vt), s_delegated));
    try std.testing.expect(point_vt.parent == object_vt);

    // Install a method on it via send: [point_vt addMethod: #answer with: fn]
    const s_answer = intern("answer");
    _ = send2(oop(point_vt), s_addMethod, oop(s_answer), @constCast(@ptrCast(&answerImpl)));

    // Allocate an instance: [point_vt allocate: 0], then send it #answer.
    const p = send1(oop(point_vt), s_allocate, @ptrFromInt(0));
    try std.testing.expect(vtableOf(p) == point_vt);
    const result = send0(p, s_answer);
    try std.testing.expectEqual(@as(usize, 42), @intFromPtr(result));

    // Delegation works: #answer is found by walking point -> (its vt) -> dict;
    // and an unknown-on-point selector resolves up the chain to object_vt.
    // (object_vt has no methods yet, so we only assert the positive path here.)
}

fn answerImpl(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = args;
    _ = n;
    return @ptrFromInt(42);
}

test "libid: doesNotUnderstand: is an overridable message (F2)" {
    boot();

    // A prototype whose doesNotUnderstand: returns a sentinel instead of trapping.
    const proxy_vt = asVtable(send0(oop(object_vt), s_delegated));
    _ = send2(oop(proxy_vt), s_addMethod, oop(s_dnu), @constCast(@ptrCast(&dnuTo99)));
    const proxy = send1(oop(proxy_vt), s_allocate, @ptrFromInt(0));

    // Sending an unknown selector is intercepted by the override (no panic),
    // and the handler sees the original selector as its first argument.
    const unknown = intern("totallyUnknownSelector");
    const r = send0(proxy, unknown);
    try std.testing.expectEqual(@as(usize, 99), @intFromPtr(r));

    // The handler also received the forwarded argument (selector + args).
    last_dnu_sel = null;
    _ = send1(proxy, unknown, @ptrFromInt(7));
    try std.testing.expect(last_dnu_sel == oop(unknown));
    try std.testing.expectEqual(@as(usize, 7), last_dnu_arg);
}

test "libid: SmallInt is the one immediate (F1)" {
    boot();

    const a = mkSmallInt(42);
    try std.testing.expect(isSmallInt(a));
    try std.testing.expectEqual(@as(i64, 42), smallIntValue(a));
    try std.testing.expectEqual(@as(i64, -7), smallIntValue(mkSmallInt(-7))); // sign round-trips
    try std.testing.expectEqual(@as(i64, 0), smallIntValue(mkSmallInt(0)));

    // A fixnum's vtable is smallint_vt — found by tag, no `[-1]` deref.
    try std.testing.expect(vtableOf(a) == smallint_vt);

    // Heap objects (incl. the singletons) are NOT smallints; nil is a real
    // object distinct from the Zig null Oop (internal absence).
    try std.testing.expect(!isSmallInt(oop(object_vt)));
    try std.testing.expect(!isSmallInt(nil_obj));
    try std.testing.expect(vtableOf(nil_obj) == nil_vt);
    try std.testing.expect(nil_obj != null);
    try std.testing.expect(true_obj != false_obj and vtableOf(true_obj) == bool_vt);
}

test "libid: String is an uninterned byte-string floor value" {
    boot();
    const s = mkString("hello world");
    try std.testing.expect(vtableOf(s) == string_vt);
    try std.testing.expectEqualStrings("hello world", stringBytes(s));
    // Distinct objects for equal bytes (uninterned), unlike symbols.
    const s2 = mkString("hello world");
    try std.testing.expect(s != s2);
    try std.testing.expectEqualStrings(stringBytes(s), stringBytes(s2));
    // Not a smallint; a real heap object.
    try std.testing.expect(!isSmallInt(s));
}

// -- GC tests (F3) -----------------------------------------------------------

noinline fn makeGarbage(count: usize) void {
    var i: usize = 0;
    var o: Oop = null;
    while (i < count) : (i += 1) o = allocC(oop(object_vt), 0); // each overwrites the last
    std.mem.doNotOptimizeAway(&o);
}

var g_test_roots: [3]Oop = .{ null, null, null };
fn testRootProvider(mark: idapi.MarkFn) callconv(.c) void {
    for (g_test_roots) |r| if (r != null) mark(r);
}

test "libid: GC reclaims unreachable instances (F3)" {
    boot();
    var anchor: usize = 0;
    g_stack_bottom = @intFromPtr(&anchor);
    _ = &anchor;

    _ = gcCollect(); // settle
    const before = g_blocks.count();
    makeGarbage(2000);
    try std.testing.expect(g_blocks.count() >= before + 2000);

    const freed = gcCollect();
    try std.testing.expect(freed >= 1900); // nearly all (a little stack residue is ok)
    try std.testing.expect(g_blocks.count() <= before + 100);
}

test "libid: GC is a no-op until a stack bottom is set" {
    boot();
    g_stack_bottom = 0; // unset
    makeGarbage(100);
    const live = g_blocks.count();
    try std.testing.expectEqual(@as(usize, 0), gcCollect()); // refuses to sweep
    try std.testing.expectEqual(live, g_blocks.count());
}

test "libid: GC keeps root-provider objects and what they reach (F3)" {
    boot();
    var anchor: usize = 0;
    g_stack_bottom = @intFromPtr(&anchor);
    _ = &anchor;
    gcRegisterRootProviderC(&testRootProvider);

    // A container instance (one Oop slot) pointing at a child; only the container
    // is rooted — the child must survive via tracing.
    const child = allocC(oop(object_vt), 0);
    const container = allocC(oop(object_vt), @sizeOf(Oop));
    @as(*Oop, @ptrCast(@alignCast(container.?))).* = child;
    const kept = mkString("kept");
    g_test_roots = .{ container, kept, null };

    makeGarbage(500);
    _ = gcCollect();

    try std.testing.expect(g_blocks.contains(@intFromPtr(container.?)));
    try std.testing.expect(g_blocks.contains(@intFromPtr(child.?))); // reached via container
    try std.testing.expect(g_blocks.contains(@intFromPtr(kept.?)));
    try std.testing.expectEqualStrings("kept", stringBytes(kept));

    g_test_roots = .{ null, null, null }; // don't leak roots into later tests
}

test "libid: GC never sweeps permanents (vtables / symbols / singletons)" {
    boot();
    var anchor: usize = 0;
    g_stack_bottom = @intFromPtr(&anchor);
    _ = &anchor;

    const sym = oop(intern("a-kept-symbol"));
    const vt = protoC(oop(object_vt)); // a fresh permanent vtable
    makeGarbage(300);
    _ = gcCollect();

    try std.testing.expect(g_blocks.contains(@intFromPtr(sym.?)));
    try std.testing.expect(g_blocks.contains(@intFromPtr(vt.?)));
    try std.testing.expect(g_blocks.contains(@intFromPtr(nil_obj.?)));
}

var last_dnu_sel: Oop = null;
var last_dnu_arg: usize = 0;

fn dnuTo99(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    last_dnu_sel = args[0]; // the original selector
    last_dnu_arg = if (n > 1) @intFromPtr(args[1]) else 0; // first forwarded arg
    return @ptrFromInt(99);
}
