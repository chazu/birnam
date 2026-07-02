# FFI: Shimless C Interop

Birnam can load any `.so`/`.dylib`, enumerate its symbols, and wrap C functions
*without writing a shim* — the wraps are facts (persistent, queryable, diffable).
`src/kernel/ffi.zig` is the libffi marshaling layer; `src/kernel/ffi_module.zig`
is the `Module` object and the deferred-dlopen resolution model.

> Implementation: `src/kernel/ffi.zig`, `src/kernel/ffi_module.zig`. Vendored
> libffi 3.4.6 (Phase 19 swapped out the original dyncall 1.4 — see
> [Phase 19](roadmap)). Concept: [The Vision](10-vision.md) tenet 8.
> Walkthrough: [Wrapping a C Library](35-ffi-walkthrough.md) (Part 3).

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
libffi `ffi_type` descriptor.

## Calling — marshaling

`invokeWithOwner` builds a libffi call: it validates arity, marks the variadic
boundary at `variadic_at`, marshals each argument by kind into the libffi arg
array, and dispatches `ffi_call`:

- Scalars — `untagSmallInt` → the matching C width.
- Floats — `untagFloat` → f32/f64.
- Pointers / cstrings — a `nil` argument becomes a NULL pointer; an FFI pointer is
  untagged and cast; a Birnam *string* passed for a `Z` parameter is copied to a
  scratch NUL-terminated buffer (freed after the synchronous call returns).
- Aggregates — struct-by-value args/returns marshal through a built `ffi_type`
  descriptor; libffi handles the platform ABI (including arm64-Apple's by-value
  rules) so no per-target hand-rolling is needed.

The return is marshaled back: scalars to SmallInt/Float, pointers to a boxed
`FFI_PTR_VT`, a `Z` cstring to a Birnam string (NULL → nil), an aggregate to a
malloc'd buffer returned as an FFI pointer.

The arity cap is **255** (the `u8` arg-count operand in `SEND_N`): the old per-arity
shims are gone, replaced by a single variadic `shimAny` that reads the count.
`buildWrapClosure` parses the sig, asserts the transient `:ffi-addr`/`:ffi-sig`
facts, and builds a `CLOSURE_VT` closure whose data is the wrap eid.

### Aggregates on arm64-Apple

`aggregates_supported = true`. Phase 19 swapped dyncall 1.4 (whose arm64-apple
aggregate vtable slots were NULL — struct-by-value would have segfaulted) for
libffi 3.4.6, which marshals structs by value on arm64-Apple correctly (HFA and
mixed structs verified). The historical gate is gone.

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

**Argument order** (easy to get backwards): `[mod wrap: #birnamSelector as:
"c_symbol_name" signature: "sig"]`. The first argument is the **birnam selector
symbol** to install (use a trailing-colon symbol like `#sqrt:` for a 1-arg call,
a bare `#errnoPtr` for a 0-arg call); the second is the **C symbol name string**
to `dlsym`. Passing the C-name string into the `wrap:` slot is the common
mistake — it now returns an `ffi-wrap-bad-selector` ERROR_VT entity rather than a
silent `nil`.

`resolveModule` runs (binds the wraps) at file-in (`:in`) commit, image `:load`,
AOT startup, **and after each interactive REPL line** — so a `wrap:` followed by
a call works in a single REPL session, not only via `:in`.

### Symbol enumeration

`[mod symbols]` reads `:ffi-path`, runs the [Mach-O/ELF reader](2e-images-aot.md)
via `loader.openSymbols`, and asserts one entity per symbol
(`:ffi-symbol-name`/`-addr`/`-kind`/`-external`/`-module`). The Module gets
`:ffi-has-symbol` (`:many`).

### Pointer manipulation — `FFI_PTR_VT` methods

For out-params and opaque-pointer deref without a shim: `Module alloc: n`
(malloc'd zeroed bytes → FFI pointer), and on `FFI_PTR_VT`: `free`, `address`,
`isNull`, and a width-complete peek/poke family (byte-offset based,
alignment-safe via `align(1)`):

| C width | peek (signed) | peek (unsigned) | poke |
|---|---|---|---|
| 8-bit  | — (`peekByte:` is unsigned 0..255) | `peekByte:` | `pokeByte:at:` |
| 16-bit | `peekShort:`  | `peekUShort:` | `pokeShort:at:` |
| 32-bit | `peekInt32:`  | `peekUInt32:` | `pokeInt32:at:` |
| 64-bit | `peekInt:`    | —             | `pokeInt:at:` |
| f64    | `peekDouble:` |               | `pokeDouble:at:` |
| ptr    | `peekPtr:` → FfiPtr |         | `pokePtr:at:` (FfiPtr/nil→NULL) |
| string | `peekString:` (NUL-terminated), `peekBytes:len:` (fixed) | | `pokeBytes:at:` |

Signed peeks sign-extend, `U` peeks zero-extend, into a SmallInt. Pokes truncate
to the low N bytes. **Match the width to the C type**: a C `int` out-param (e.g.
`errno` via `__error()`) is 4 bytes — use `peekInt32:`, not `peekInt:` (the
8-byte read would pull in adjacent bytes and overflow the 61-bit SmallInt → `nil`).
Sub-byte access (flags, bit-fields) is not addressable in C — peek the containing
width then mask/shift with the SmallInt bitwise ops (`bitAnd:`/`bitOr:`/`bitXor:`/
`bitNot`/`shiftLeft:`/`shiftRight:`, also `<<`/`>>`).

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
