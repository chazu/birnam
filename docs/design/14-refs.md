# Tagged Pointers and the Ref Encoding

A `Ref` is the universal value type in Birnam: one machine word that may hold an
unboxed integer, an entity id, a pointer to a materialized object, a string, or a
symbol. This page explains the encoding, the No-NULL discipline it enforces, and
the "-1 trick" that gives materialized objects their layout.

> Implementation: `src/kernel/ref.zig`. Design: `VISION.md` §Object Model →
> Identity and References.

## One word, three low bits of tag

```zig
pub const Ref = u64;
```

Pointers are 8-byte aligned on 64-bit targets, so the low 3 bits of an aligned
address are always zero — free to use as a type tag. `ref.zig` defines:

| Tag (`0bxxx`) | Meaning | Upper 61 bits hold… |
|---|---|---|
| `000` `pointer` | raw pointer to a materialized object | the aligned address itself |
| `001` `smallint` | unboxed `SmallInteger` | sign-extended 61-bit integer |
| `010` `eid` | a cold object — query the fact store | the entity id |
| `011` `keyword` | interned keyword / immediate symbol | the keyword id |
| `100` `string` | pointer to a `StringHeader` | the address (see `strings.zig`) |
| `101..111` | reserved for future immediates | — |

Two constants drive encode/decode: `TAG_MASK = 0b111` and `VALUE_SHIFT = 3`.

### SmallInt

```zig
pub const SMALLINT_MIN: i64 = -(1 << 60);
pub const SMALLINT_MAX: i64 =  (1 << 60) - 1;
```

`mkSmallInt(n)` left-shifts by 3 and ORs in the `001` tag, returning `null` if
`n` doesn't fit 61 signed bits (the caller boxes a full-width integer/float
instead). `untagSmallInt` uses an *arithmetic* right shift so sign is preserved.
This is the *idst*/Smalltalk unboxed-int tradition — the common case (small
counters, indices, loop variables) never allocates.

### EID

`mkEid(eid)` shifts left by 3 and ORs the `010` tag; `untagEid` shifts back. An
eid Ref means "this object is cold — look it up in the fact store / object
table." `EID_MAX = (1 << 61) - 1`.

### Pointer

`mkPointer(ptr)` asserts 8-byte alignment (the tag bits must already be zero) and
returns the address verbatim — tag `000` means "the bits *are* a pointer." This
is the hot-object representation: the Ref points straight at a materialized
struct.

## NIL is a value, not a null word

A subtle but load-bearing point:

```zig
pub const NIL: Ref = mkEid(0);   // bit pattern 0b010, decimal 2
```

`NIL` is the **eid-0 entity** — a real, dispatchable object (the "nothing
object"). It is deliberately **not** the all-zero machine word. Zero-initialized
memory reads as a null *pointer* (tag `000`, address 0), which fails loudly on
dereference — exactly what you want from uninitialized memory. `NIL`, by
contrast, is a valid object you can send messages to (`[nil isNil] → true`).

This encoding is what makes the [No-NULL discipline](11-fact-model.md)
mechanical rather than aspirational:

- **`nil`-the-object is freely storable and returnable.** `{a: nil}` stores the
  eid-0 Ref; a lookup that legitimately holds nil returns nil.
- **Absence is a separate channel**, never a value. The Zig API surface uses
  `?Ref` for "not found" (Zig's optional `null`, *not* a stored fact value) — for
  example `ValueCell.current_v: ?Ref`, where `null` means "retracted or never
  asserted." Surface code routes absence through `ifAbsent:`, an empty query
  result, or a raise.
- Because absence rides `?Ref`/`ifAbsent:` while `nil` stays a genuine value,
  *stored-nil and absent remain exactly distinguishable.* That exactness is the
  Date payoff: sound 2-valued diffs, no 3VL trap.

## The -1 trick (Piumarta's bargain, preserved)

For any materialized object (a tag-`000` Ref), the vtable lives at `obj[-1]` —
one slot *before* the object's data. There is no header word and no object
envelope: **the object IS its slot vector.** A single backward indirection finds
the vtable; a single forward indexed load reads a field.

```
        obj[-1]  obj[0]   obj[1]   ...  obj[N-1]
        ┌──────┬────────┬────────┬─────┬────────┐
        │ vt   │ Ref_0  │ Ref_1  │ ... │ Ref_{N-1}
        └──────┴────────┴────────┴─────┴────────┘
                ▲
                Ref (tag 000) points here
```

User-defined objects materialize as a uniform `[vt][Ref_0 … Ref_{N-1}]` array:

- `N` = the number of attributes in that object's *shape*.
- Slot order = attributes sorted by eid (deterministic; no per-instance metadata
  table). The shape is derivable from the vtable's facts — no separate shape
  cache.
- Every slot is one tagged word. SmallInts live inline via tag `001`;
  strings/floats are Refs to heap entities.
- Field access is one indirect load; shape mutation grows/shrinks the array and
  re-asserts facts — no layout-cache to invalidate.

The kernel's own fixed structs (Vtable, Closure, Selector, Vector, Assoc) keep
their C-ABI extern-struct shapes — they are the only fixed-layout
materializations. Everything user-defined is uniform-Ref.

Cold objects have *no* slot vector at all — they are facts in the store. The
equivalent of "poke any bit of this object" is "assert/retract any fact about
this eid." Same power, different shape. The transition between the two
representations is [promotion/demotion](25-promotion.md).

## Where to go next

- How the value lives in a fact: [The Fact Model](11-fact-model.md).
- The hot/cold transition: [Promotion](25-promotion.md) (Part 2).
- Why this layout matters for speed: [The Performance Model](18-performance-model.md).
