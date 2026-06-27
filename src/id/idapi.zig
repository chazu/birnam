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
};

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
