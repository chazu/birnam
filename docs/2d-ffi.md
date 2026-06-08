# FFI: Shimless C Interop

Birnam can load any `.so`/`.dylib`, enumerate its symbols, and wrap C functions
*without writing a shim* — the wraps are facts (persistent, queryable, diffable).
`src/kernel/ffi.zig` is the dyncall marshaling layer; `src/kernel/ffi_module.zig`
is the `Module` object and the deferred-dlopen resolution model.

> Implementation: `src/kernel/ffi.zig`, `src/kernel/ffi_module.zig`. Vendored
> dyncall 1.4. Concept: [The Vision](10-vision.md) tenet 8. Walkthrough:
> [Wrapping a C Library](35-ffi-walkthrough.md) (Part 3).

## Signatures and sigchars — `ffi.zig`

A wrap's signature is a string of sigchars, args before `)`, return after:

| Char | C type | | Char | C type |
|---|---|---|---|---|
| `B` | bool (c_int) | | `f` | float (f32) |
| `c` | char (u8) | | `d` | double (f64) |
| `s` | short | | `p` | pointer |
| `i` | int | | `Z` | cstring (pointer alias, marshals strings) |
| `l` | long | | `v` | void |
| `L` | long long | | `{…}` | struct by value (aggregate) |
| | | | `)` | arg/return separator |
| | | | `e` | variadic marker (at an arg site) |

`parseSig("p)p")` produces a `Sig { args: []Kind, ret: Kind, variadic_at: ?usize,
arg_aggrs: []?*Aggr, ret_aggr: ?*Aggr }`. An `Aggr` (from `{fipc}`) carries the
field kinds, natural-alignment offsets, total tail-padded size, and a lazily-built
dyncall `DCaggr` descriptor.

## Calling — marshaling

`invokeWithOwner` drives a dyncall VM: it validates arity, switches to ellipsis
mode at `variadic_at`, and pushes each argument by kind via `pushArg`:

- Scalars — `untagSmallInt` → the matching `dcArg*`.
- Floats — `untagFloat` → f32/f64.
- Pointers / cstrings — a `nil` argument becomes a NULL pointer; an FFI pointer is
  untagged and cast; a Birnam *string* passed for a `Z` parameter is copied to a
  scratch NUL-terminated buffer (freed after the synchronous call returns).
- Aggregates — expect an FFI pointer, pushed via `dcArgAggr` (struct return is set
  up with `dcBeginCallAggr` *before* args).

`callAndWrap` dispatches the right `dcCall*` and marshals the return: scalars to
SmallInt/Float, pointers to a boxed `FFI_PTR_VT`, a `Z` cstring to a Birnam string
(NULL → nil), an aggregate to a malloc'd buffer returned as an FFI pointer.

The arity cap is **255** (the `u8` arg-count operand in `SEND_N`): the old per-arity
shims are gone, replaced by a single variadic `shimAny` that reads the count.
`buildWrapClosure` parses the sig, asserts the transient `:ffi-addr`/`:ffi-sig`
facts, and builds a `CLOSURE_VT` closure whose data is the wrap eid.

### The arm64-Apple aggregate limitation

`aggregates_supported = builtin.cpu.arch != .aarch64`. dyncall 1.4 ships NULL
aggregate-vtable slots on arm64-apple, so struct-by-value would segfault — it is
gated off there (returns cleanly), and is a Phase 19 backlog item (upgrade dyncall
or hand-roll the AAPCS path). Scalar/pointer/string calls work fine on arm64.

## The Module object — `ffi_module.zig`

### Deferred dlopen

`Module load: "lib"` does **not** dlopen. It asserts only the durable
`[mod :ffi-path "lib"]` (M_USER) and mints a per-instance delegate vtable. The
side-effecting open is deferred to `resolveModule`, run at REPL line-commit,
file-in commit, and image load. This is what lets a wrap be *declarative* — a fact
that survives [save/load](2e-images-aot.md) and relinks by name, never by address.

`resolveModule`:

- `ensureInstanceVt` — reuse the instance vt if its `:vtable` *fact* still exists
  (not whether it's in the object table — a demoted entity has no table entry but
  a live fact). This was the fix for a bug where every resolve re-minted the vt and
  dropped installed selectors.
- dlopen once per process: `std.c.dlopen(path, {LAZY, GLOBAL})`. **RTLD_GLOBAL** is
  required so dependencies loaded first satisfy a later library's flat-namespace
  references (the libtecla → libncurses case in
  [the walkthrough](35-ffi-walkthrough.md)).
- For each declarative wrap (`:ffi-wrap-symbol`, `:ffi-sig-str`,
  `:wrap-selector`, all M_USER): `dlsym` the symbol and `buildWrapClosure` it onto
  the instance vt. The wrap eids are **snapshotted before the loop** — asserting
  during iteration would invalidate the live HAMT iterator and silently drop
  wraps.

### Declaring a wrap

`wrap:as:signature:` is declarative: it mints a wrap entity with durable facts
(`:ffi-owner`, `:ffi-wrap-symbol`, `:ffi-sig-str`, `:wrap-selector`) and returns
self (chainable). No dlsym happens here — that waits for `resolveModule`.

### Symbol enumeration

`[mod symbols]` reads `:ffi-path`, runs the [Mach-O/ELF reader](2e-images-aot.md)
via `loader.openSymbols`, and asserts one entity per symbol
(`:ffi-symbol-name`/`-addr`/`-kind`/`-external`/`-module`). The Module gets
`:ffi-has-symbol` (`:many`).

### Pointer manipulation — `FFI_PTR_VT` methods

For out-params and opaque-pointer deref without a shim: `Module alloc: n`
(malloc'd zeroed bytes → FFI pointer), and on `FFI_PTR_VT`: `free`, `address`,
`isNull`, `peekInt:`/`peekByte:`/`peekDouble:`/`peekPtr:`/`peekString:` and the
`poke…:at:` counterparts (byte-offset based, alignment-safe).

### Unload and stale pointers

`[mod unload]` refuses while an FFI call is in flight (`ffi_depth > 0`), frees each
owned wrap's heap `Sig`, marks every owned entity `:ffi-stale?`, dlcloses, and
marks the Module stale. `pushArg` checks the stale flag so a call through a freed
library fails cleanly instead of crashing. Failure paths throughout return
`ERROR_VT` diagnostic entities (`:error-kind`, `:error-message`, context facts)
rather than panicking.

## Where to go next

- The end-to-end driving experience: [Wrapping a C Library](35-ffi-walkthrough.md) (Part 3).
- The readers symbol enumeration uses: [Images, Freeze, and AOT](2e-images-aot.md).
- Why demotion is FFI-pinned: [Promotion](25-promotion.md).
