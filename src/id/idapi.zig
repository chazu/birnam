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
    /// mkSmallInt(v) -> an Integer (a boxed i64 heap value); smallIntValue reads
    /// it; isSmallInt tests for it. NAMES ARE HISTORICAL: immediates were removed
    /// in the prototype refounding (docs/design/41 §A), so an Integer is now an
    /// ordinary heap object like a Float — no tag. Kept named to avoid churning
    /// ~180 call sites.
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

    /// vtableOf(obj) -> the object's vtable (the `[-1]` header word; every object
    /// has one now that immediates are gone — see docs/design/41 §A).
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
    /// setTraceHook(vt, hook): install a per-vtable trace hook. When the collector
    /// traces a live INSTANCE of `vt`, it calls `hook(instance, mark)` so the
    /// instance can mark Oops the conservative payload scan can't see — those held
    /// in non-id memory (e.g. the store's fact ArrayList). Unlike a root provider
    /// (a global root set), a trace hook fires PER reachable instance, so the
    /// instance's external Oops survive iff the instance itself does.
    setTraceHook: *const fn (Oop, TraceHook) callconv(.c) void,
    /// gcSetAuto(on): enable/disable automatic collection (on by default once a
    /// stack bottom is set). When on, an allocation that crosses the internal
    /// byte threshold since the last collection triggers one. Disable for
    /// deterministic manual-GC scenarios.
    gcSetAuto: *const fn (c_int) callconv(.c) void,

    // -- Float: the boxed-f64 floor value type (appended) --------------------
    /// The float vtable, and the boxed-f64 constructor / decoder / predicate.
    /// SmallInt owns the immediate tag, so a Float is a heap object with a single
    /// `f64` payload — the floor's only floating-point value. Higher layers add
    /// the Float protocol (prelude) and int↔float promotion.
    float_vt: Oop,
    mkFloat: *const fn (f64) callconv(.c) Oop,
    floatValue: *const fn (Oop) callconv(.c) f64,
    isFloat: *const fn (Oop) callconv(.c) c_int,

    // -- GC finalizers (F3; appended) ----------------------------------------
    /// setFinalizer(obj, fin): attach a finalizer to a collectable object. When
    /// the collector reclaims `obj` it calls `fin(obj)` first, so the object can
    /// release memory it owns OUTSIDE the id heap (a String's bytes, a TccUnit's
    /// native code). No-op on a permanent object or an untracked pointer. The
    /// finalizer must not allocate id objects (it runs mid-sweep).
    setFinalizer: *const fn (Oop, Finalizer) callconv(.c) void,

    // -- Exact payload layouts (F3 completed; docs/40 §3c — appended) --------
    /// setLayout(vt, kinds, n, rest): declare the exact payload layout of `vt`'s
    /// instances — which WORDS are Oops and which are raw bytes. `kinds` is a
    /// per-word array of slot kinds (`slot_raw`/`slot_oop`/`slot_code`) covering
    /// the first `n` words; every word past the prefix has kind `rest` (the
    /// variable-length tail: a trie node's slots, a block's captures). The kinds
    /// array must outlive the vtable (point it at a module static). With a
    /// layout set and precise mode on, the collector traces instances exactly;
    /// a future image save swizzles through the same map.
    setLayout: *const fn (Oop, ?[*]const u8, usize, u8) callconv(.c) void,
    /// gcSetPrecise(on): trace layout-bearing vtables' instances by their
    /// descriptors instead of the conservative payload scan (vtables without a
    /// layout stay conservative; stack/register roots stay conservative always —
    /// the Piumarta constraint is about C stacks, not heap payloads).
    gcSetPrecise: *const fn (c_int) callconv(.c) void,

    // -- image: heap serialization primitives (docs/40 §3c; appended) --------
    // The image save walks the store-reachable object graph and swizzles it; it
    // needs, per object, its payload extent, its vtable's exported NAME (the
    // relink-by-name anchor across a save/load), and each payload word's slot
    // kind (raw vs Oop vs code). All three read state only the floor holds
    // (`g_blocks`, the export env, the vtable layout descriptors).
    //
    /// objectPayloadSize(obj) -> the payload byte count of a tracked heap object
    /// (block size minus the vtable word); 0 for immediates, singletons, and
    /// anything untracked. Bounds the payload walk. Always a multiple of 8.
    objectPayloadSize: *const fn (Oop) callconv(.c) usize,
    /// exportedNameOf(obj) -> the NUL-terminated name `obj` was exported under in
    /// the id environment, or null if unexported. Vtables (and well-known
    /// singletons) relink by this name on load — the durable anchor (docs/40 §3c,
    /// the D5.1 relink-by-name spine).
    exportedNameOf: *const fn (Oop) callconv(.c) ?[*:0]const u8,
    /// slotKind(vt, word) -> the payload slot kind (`slot_raw`/`slot_oop`/
    /// `slot_code`) of `word` for instances of `vt`, per its layout descriptor;
    /// 0xFF if `vt` carries no layout (the caller then defers that object — an
    /// unknown layout can't be safely swizzled). Mirrors the tracer's per-word rule.
    slotKind: *const fn (Oop, usize) callconv(.c) u8,
    /// ensurePrivateVtable(obj) -> the vtable to install an object-specific method
    /// on: `obj`'s own family if `obj` OWNS it (a well-known prototype owns its
    /// family, so `defineMethod on: String` patches string_vt in place — extends
    /// every string), else a freshly split private one delegating to the current
    /// (extends only `obj`). The one target-selection primitive behind
    /// `defineMethod` — no family/instance fork above it (docs/design/41 §A).
    ensurePrivateVtable: *const fn (Oop) callconv(.c) Oop,
    /// setOwner(vt, proto) -> record `proto` as the prototype that owns family
    /// `vt`, so `defineMethod on: proto` patches `vt` in place (extends all its
    /// instances). Set once per well-known family at load; a well-known name is
    /// exported bound to its `proto`, not to `vt`.
    setOwner: *const fn (Oop, Oop) callconv(.c) void,

    /// setParent(vt, parent) -> re-point vtable `vt`'s delegation `parent`. Lets a
    /// library slot a shared abstract layer (e.g. `Collection`) between a family
    /// vtable and `object_vt` so a protocol authored once on the parent is found
    /// by every child via the ordinary `lookup` walk. Idempotent.
    setParent: *const fn (Oop, Oop) callconv(.c) void,

    // -- The MOP mirror seam (docs/design/42 §D1; appended) ------------------
    /// setMethodOverwriteHook(h): register a hook called `h(vt, sel)` just before
    /// an EXISTING (vt,sel) binding is rebound in a vtable dict. The store uses it
    /// to keep its `[vt sel m]` mirror from going stale (the consistency law: exact
    /// tracked Method, honest `#primitive` presence, or absence; never a stale or
    /// falsely-present value). Pass null to clear. The hook must be pure (no
    /// send/alloc) — it runs mid-install.
    setMethodOverwriteHook: *const fn (?MethodOverwriteHook) callconv(.c) void,
    /// vtableCount() -> the number of vtables created so far (the registry length).
    /// vtableAt(i) enumerates them: together they walk the whole type universe, so
    /// the store can backfill every registered real type's selectors into the
    /// mirror at init and image boot.
    vtableCount: *const fn () callconv(.c) usize,
    /// vtableAt(i) -> the i-th registered vtable, or null past the end.
    vtableAt: *const fn (usize) callconv(.c) Oop,
    /// vtableSelectorAt(vt, i) -> the i-th selector bound DIRECTLY on `vt` (a
    /// symbol), or null past the end. Enumerates the vtable's own method dict (not
    /// the delegated chain) — behind mirror backfill and protocol reflection.
    vtableSelectorAt: *const fn (Oop, usize) callconv(.c) Oop,
    /// vtableParent(vt) -> vt's delegation parent, or null at the top of the chain.
    /// Walk it to tell a real type (reaches `object_vt`) from an internal dict.
    vtableParent: *const fn (Oop) callconv(.c) Oop,
    /// mkWord(v) -> a boxed 64-bit machine word (opaque; a full C `long`/`void*`/
    /// unsigned that SmallInt's 63-bit tag can't hold without truncation, §F). The
    /// reader interprets the raw bits (signed long, pointer, unsigned).
    mkWord: *const fn (u64) callconv(.c) Oop,
    /// wordValue(o) -> the raw 64-bit payload of a Word.
    wordValue: *const fn (Oop) callconv(.c) u64,
    /// isWord(o) -> 1 if `o` is a Word, else 0.
    isWord: *const fn (Oop) callconv(.c) c_int,
    /// The Word family's vtable (extend it via `Word` the prototype, §F).
    word_vt: Oop,
    /// vtableOwner(vt) -> the object that OWNS `vt` as its private/family vtable
    /// (set by ensurePrivateVtable / the prototype wiring), or null if unowned.
    /// With vtableParent this lets the image tell a singleton's derived private
    /// vtable (owner == the object; serialize under the parent, installs rebuild
    /// it at boot) from an image-born vtable that must defer (a block's).
    vtableOwner: *const fn (Oop) callconv(.c) Oop,
};

/// The method-overwrite hook (docs/42 §D1): `(vtable, selector)` of a binding about
/// to be rebound, so the store can retract a now-stale mirror fact before the dict
/// flips. Pure — no send/alloc (it runs mid-install, possibly reentrantly).
pub const MethodOverwriteHook = *const fn (Oop, Oop) callconv(.c) void;

/// Returned by `slotKind` when a vtable has no layout descriptor: the object
/// can't be swizzled and the image defers it (counted, never dropped).
pub const slot_unknown: u8 = 0xFF;

/// Slot kinds for `setLayout` payload descriptors: a raw (non-pointer) word, an
/// Oop (traced + swizzled), or a native-code pointer into a compiled unit
/// (ignored by the tracer — units are kept alive by their owning objects — but
/// serialized as (unit, offset) by the image save; docs/40 §3c).
pub const slot_raw: u8 = 0;
pub const slot_oop: u8 = 1;
pub const slot_code: u8 = 2;

/// The mark function handed to a root provider / trace hook: call it per root Oop.
pub const MarkFn = *const fn (Oop) callconv(.c) void;
/// A module's root provider: marks every Oop it keeps reachable in non-id memory.
pub const RootProvider = *const fn (MarkFn) callconv(.c) void;
/// A per-vtable trace hook (F3): marks an instance's Oops held in non-id memory.
pub const TraceHook = *const fn (Oop, MarkFn) callconv(.c) void;
/// A per-object finalizer (F3): called once, just before the object is reclaimed,
/// to release memory it owns outside the id heap. Receives the dying object.
pub const Finalizer = *const fn (Oop) callconv(.c) void;

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
