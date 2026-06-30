//! IdApi — the id object system's C-ABI, modelled on Piumarta's `struct __libid`
//! (`~/dev/c/birnam` @ d35caee/753ffb1). It is a **vtable of function pointers**
//! returned by libid's `id_init` entrypoint. The siphon raw-loads libid ONCE to
//! get this table, then does everything through it — including re-importing (or
//! re-loading) the id layer itself, because module loading (`dlopen`/`dlsym`) and
//! the object namespace (`import`/`export`) are part of the API.
//!
//! No eids, no fact store. Append-only (keep the layout stable across the dylib
//! boundary); unused trailing slots can be added later without breaking modules.

pub const Oop = ?*anyopaque;

/// A primitive method: receiver + a flat argument buffer (pointer-based).
pub const Method = *const fn (self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop;

pub const IdApi = extern struct {
    // -- bootstrap: module loading IS an id-api operation --------------------
    // So even the id dylib can be (re)loaded *through* the id api.
    dlopen: *const fn ([*:0]const u8, c_int) callconv(.c) ?*anyopaque,
    dlsym: *const fn (?*anyopaque, [*:0]const u8) callconv(.c) ?*anyopaque,

    // -- globals: the well-known objects -------------------------------------
    object_vt: Oop,
    vtable_vt: Oop,
    symbol_vt: Oop,

    // -- allocation ----------------------------------------------------------
    /// intern(cstr) -> the unique symbol object for that string.
    intern: *const fn ([*:0]const u8) callconv(.c) Oop,
    /// proto(base) -> a fresh vtable delegating to `base` (install methods on it,
    /// allocate instances from it). The Piumarta `proto`.
    proto: *const fn (Oop) callconv(.c) Oop,
    /// method(type, selector, impl) -> bind a primitive method on `type`.
    method: *const fn (Oop, Oop, Method) callconv(.c) void,
    /// alloc(type, payload_bytes) -> a fresh instance whose vtable is `type`.
    alloc: *const fn (Oop, usize) callconv(.c) Oop,

    // -- environment: the name -> object namespace (the re-import mechanism) --
    import: *const fn ([*:0]const u8) callconv(.c) Oop,
    exportObj: *const fn ([*:0]const u8, Oop) callconv(.c) Oop,
    isExported: *const fn ([*:0]const u8) callconv(.c) c_int,

    // -- messaging -----------------------------------------------------------
    send: *const fn (Oop, Oop, [*]const Oop, usize) callconv(.c) Oop,

    // -- immediates + canonical singletons (F1; appended — ABI stays stable) -
    /// The fixnum vtable, and the boot-time singletons nil / true / false. The
    /// singletons are heap objects compared by pointer (only SmallInt is an
    /// immediate); each is distinct from the Zig `null` Oop (internal absence).
    smallint_vt: Oop,
    nilObj: Oop,
    trueObj: Oop,
    falseObj: Oop,
    /// mkSmallInt(v) -> a tagged 63-bit fixnum; smallIntValue decodes it;
    /// isSmallInt tests the tag. Pointer-encoded ints are gone above the floor.
    mkSmallInt: *const fn (i64) callconv(.c) Oop,
    smallIntValue: *const fn (Oop) callconv(.c) i64,
    isSmallInt: *const fn (Oop) callconv(.c) c_int,

    // -- String: the uninterned byte-string value type (appended) ------------
    /// mkString(ptr, len) -> a String object owning a copy of the bytes;
    /// stringBytes(obj, &len) -> the byte pointer (len written out). For source
    /// text + literal bytes — distinct from interned symbols.
    string_vt: Oop,
    mkString: *const fn ([*]const u8, usize) callconv(.c) Oop,
    stringBytes: *const fn (Oop, *usize) callconv(.c) [*]const u8,

    // -- reflection: a symbol's interned text (appended) ---------------------
    /// symbolName(sym) -> the NUL-terminated string a symbol was interned from.
    /// The reverse of `intern`; lets higher layers inspect selectors (e.g. count
    /// keyword parts for define-method).
    symbolName: *const fn (Oop) callconv(.c) [*:0]const u8,

    /// vtableOf(obj) -> the object's vtable (tag-aware: SmallInt → smallint_vt).
    /// Lets a library reach the type of a value to install protocol on it (e.g.
    /// the shared bool vtable behind true/false).
    vtableOf: *const fn (Oop) callconv(.c) Oop,

    // -- GC: a self-contained conservative mark-sweep (F3; appended) ---------
    // The collector reclaims INSTANCE objects (alloc/mkString); vtables, symbols
    // and the singletons are permanent. Roots = the C stack + saved registers +
    // registered root providers; tracing is conservative (every payload word that
    // looks like a live-object pointer is followed). See `docs/34` §F3.
    //
    /// gcSetStackBottom(addr): record the outermost stack address to scan up to
    /// (the address of a local near the program's entry). The collector scans
    /// from its own frame to here; if unset (0) `gcCollect` is a no-op (it cannot
    /// safely find stack roots). Pass `@intFromPtr(&local)`.
    gcSetStackBottom: *const fn (usize) callconv(.c) void,
    /// gcRegisterRootProvider(p): register a callback that, at collection time,
    /// calls the passed mark function on each live Oop the module holds in non-id
    /// (e.g. `c_allocator` ArrayList) memory — the store's facts especially.
    gcRegisterRootProvider: *const fn (RootProvider) callconv(.c) void,
    /// gcCollect() -> the number of instance objects reclaimed. No-op (0) until a
    /// stack bottom is set. Safe to call between top-level evaluations.
    gcCollect: *const fn () callconv(.c) usize,
    /// gcLiveCount() -> the number of tracked (live) heap objects — for tests and
    /// introspection.
    gcLiveCount: *const fn () callconv(.c) usize,
};

/// The mark function handed to a root provider: call it on each root Oop.
pub const MarkFn = *const fn (Oop) callconv(.c) void;
/// A module's root provider: marks every Oop it keeps reachable in non-id memory.
pub const RootProvider = *const fn (MarkFn) callconv(.c) void;

/// libid's module entrypoint: `id_init()` returns the API table (cf. `_libid_init`).
pub const IdInitFn = *const fn () callconv(.c) *IdApi;

/// A *reloadable* module's entrypoint. The siphon threads an opaque per-module
/// `state` pointer: `null` on first load, the SAME pointer on every reload, so a
/// module's private (non-id) state survives a hot code reload. The module returns
/// the (possibly newly-allocated) state for the siphon to hold and pass back.
/// id-shaped state can instead ride `api.import`/`api.export`; this is the escape
/// hatch for arbitrary state. libid itself is the floor (loaded once via
/// `id_init`), so this leaves its reflectivity untouched.
pub const ModuleInitFn = *const fn (*IdApi, ?*anyopaque) callconv(.c) ?*anyopaque;
