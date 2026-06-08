# Ref Encoding Internals

The concept page [Tagged Pointers and the Ref Encoding](14-refs.md) explains
*why* `Ref` is shaped the way it is. This page is the implementation reference
for `src/kernel/ref.zig` — the exact bit layout and the encode/decode helpers
every other file uses.

> Implementation: `src/kernel/ref.zig` (~126 LOC). Concept: [Refs](14-refs.md).

## Type and tags

```zig
pub const Ref = u64;

pub const Tag = enum(u3) {
    pointer  = 0b000,  // raw pointer to a materialized object (hot)
    smallint = 0b001,  // unboxed SmallInteger
    eid      = 0b010,  // entity id — cold object, query the store
    keyword  = 0b011,  // interned keyword / immediate symbol
    string   = 0b100,  // pointer to a StringHeader (see strings.zig)
    // 0b101..0b111 reserved
};

pub const TAG_MASK: u64 = 0b111;
pub const VALUE_SHIFT: u6 = 3;
```

The low 3 bits are the tag; the upper 61 bits are the payload. `tagOf(r)` masks;
the predicates `isPointer`/`isSmallInt`/`isEid`/`isKeyword`/`isNil` compare the
masked tag.

## NIL

```zig
pub const NIL: Ref = mkEid(0);   // bit pattern 0b010, decimal 2
```

`NIL` is the eid-0 entity, *not* the zero word. A zero word is a tag-`000`
null pointer — fail-loud on dereference, which is the correct behavior for
uninitialized memory. `isNil(r)` is `r == NIL`. See the
[No-NULL discipline](11-fact-model.md).

## SmallInt

```zig
pub const SMALLINT_MIN: i64 = -(1 << 60);
pub const SMALLINT_MAX: i64 =  (1 << 60) - 1;

pub fn mkSmallInt(n: i64) ?Ref         // null if out of 61-bit signed range
pub fn untagSmallInt(r: Ref) i64       // arithmetic shift — preserves sign
```

`mkSmallInt` left-shifts by 3 and ORs the `001` tag. Returning `?Ref` means the
caller boxes a full-width number (a `FLOAT_VT` box, see `boxed.zig`) when a value
won't fit. `untagSmallInt` uses an arithmetic right shift so negatives decode
correctly.

## EID and keyword

```zig
pub const EID_MAX: u64 = (1 << 61) - 1;
pub fn mkEid(eid: u64) Ref
pub fn untagEid(r: Ref) u64

pub const KEYWORD_MAX: u64 = (1 << 61) - 1;
pub fn mkKeyword(k: u64) Ref
pub fn untagKeyword(r: Ref) u64
```

Both are shift-by-3 plus the tag. An eid Ref is the cold representation of an
object; a keyword Ref is an immediate interned symbol.

## Pointer

```zig
pub fn mkPointer(ptr: *anyopaque) Ref   // debug-asserts 8-byte alignment
pub fn untagPointer(r: Ref) *anyopaque  // caller must verify isPointer first
```

`mkPointer` requires the tag bits already be zero (8-byte alignment) and returns
the address verbatim. A pointer Ref points directly at a materialized object's
slot 0; the vtable is at slot `-1` (the [-1 trick](14-refs.md)). The string tag
(`0b100`) is a *separate* pointer-like tag for `StringHeader` so dispatch never
tries to read a `-1` slot off a foreign string address; FFI pointers are likewise
boxed under `FFI_PTR_VT` rather than raw pointer Refs.

## Who uses what

- `mkSmallInt`/`untagSmallInt` — the VM arithmetic primitives, loop counters,
  `PUSH_SMALLINT_I32` bytecode.
- `mkEid`/`untagEid` — everywhere an object identity crosses a fact boundary
  (the `v` column, method-fact values, receivers in `send`).
- `mkPointer`/`untagPointer` — promotion (materialized structs), closures,
  kernel vtables.
- `tagOf` — `vtableEidOf` in [dispatch](26-dispatch.md) switches on it to resolve
  a receiver to its vtable.

## Where to go next

- The value at rest, inside a fact: [The Fact Store](22-fact-store.md).
- The value in motion, as a receiver: [Dispatch](26-dispatch.md).
- The materialized layout pointer Refs point into: [Promotion](25-promotion.md).
