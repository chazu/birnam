# 60O — Image-v8 Identity Preservation

> **Status:** Accepted and implemented 2026-08-27, including the full `Word` /
> `ForeignAddress` semantic split below.
>
> This design supersedes image-v7 and CodeBindingInventory v1. Birnam makes
> a clean exact-current cut: no v7 reader or writer remains, and old images or
> native image artifacts are not compatibility inputs.

> **Current Oop runtime admission (2026-10-26):** the runtime ABI hash
> now uses `Id-oop-image-v3`. Image format 8 adds the immediate-reference locator
> below. Older runtime images and `BIRNAMO1` native bundles are rejected.

## 1. Problem

Image-v7 encoded Integer, Float, and String values inline at reference sites.
Loading therefore allocated an equal replacement at every occurrence. That is
not identity preservation in Birnam: these values are boxed Oops, may be shared,
may own private vtables, and may be CodeBinding targets. In particular, the
exported `Integer` prototype is also a boxed zero, so classifying it by value
before export identity reconstructed a fresh zero and detached its restored
method dictionary from the ordinary Integer send surface.

The failure is in the general image object model, not in Integer dispatch.

## 2. Identity rule

Every durable Oop reference has exactly one of these meanings:

1. null and the Nil, True, and False singletons use their canonical tags;
2. Symbols use their bytes and are re-interned;
3. exported objects use `ExportRef` and resolve through the ordinary export
   table; or
4. every other Oop uses one global `HeapRef` identity.

Repeated references to one noncanonical Oop use the same HeapRef. Equal but
distinct boxed values use different HeapRefs. There is no occurrence census,
promotion threshold, identity annotation, or compiler policy.

CodeBindingInventory v2 permits Nil, True, False, Symbol, HeapRef, and ExportRef
for target and implementation. Provenance permits Nil, HeapRef, and ExportRef.
Inline Integer and String fields are removed. A binding and the live heap graph
therefore name the same target and implementation Oops.

## 3. Heap grammar

Image-v8 retains the v7 record envelope, ordering, authority derivation, global
heap numbering, CodeObject contract, portable Block contract, and deterministic
bounds. The fixed header version becomes 8.

Ordinary Oop fields are:

```text
Nil                     := 0x00
True                    := 0x01
False                   := 0x02
Symbol                  := 0x05 || length:u32-le || bytes
HeapRef                 := 0x07 || heap_id:u32-le
Null                    := 0x08
ExportRef               := 0x09 || length:u32-le || bytes
PortableProgramRef      := 0x0b || program_index:u32-le
PortableBlockVtableRef  := 0x0c || code_object_heap_id:u32-le
```

Tag `0x03` is a raw 64-bit payload word and is valid only in an ordinary heap
object slot whose admitted layout says `SLOT_RAW`. Tags `0x04` and `0x06` are
not image-v8 fields.

Four constructor locators give boxed floor values a portable payload without
serializing native layout or String pointers:

```text
BoxedInteger  := 0x0d || value:i64-le
BoxedFloat    := 0x0e || IEEE-754-bits:u64-le
BoxedWord     := 0x0f || value:u64-le
BoxedString   := 0x10 || length:u32-le || bytes
```

A constructor locator is valid only as a HeapObject locator and requires
`word_count == 0`. Loading allocates exactly one box with `mkSmallInt`,
`mkFloat`, `mkWord`, or `mkString`, stores it in that heap identity's relocation
slot, and never exposes its native payload words to the image. If a restored
CodeBinding targets the box, ordinary binding installation gives that same box
its private vtable.

VtableLocator is therefore ExportRef, PortableBlockVtableRef, or one of the
four constructor locators. Constructor locators and PortableProgramRef reject
in ordinary fields, Facts, and CodeBindingInventory.

### Immediate Oop references

```text
ImmediateOop := 0x13 || length:u32-le (= 8) || value:u64-le
```

The value is an unsigned 63-bit immediate Oop value. This locator occupies a
logical heap identity row with zero fields, but loading reconstructs the immediate
reference without allocating a box. Repeated references use that row's `HeapRef`.
The locator is rejected in ordinary fields and cannot have private behavior links.
It is distinct from raw slot words (`0x03`) and identity-bearing boxed Word values
(`0x0f`). Image owns this encoding; Id acquires no payload classification.

## 4. Classification order

Save classification is semantic and fixed:

1. null and the three canonical singleton Oops;
2. Symbols, including Symbols that currently own a private vtable;
3. exported objects, before any boxed-family test;
4. nonexported Integer, Float, Word, and String family members as heap objects;
5. portable Block vtable references; and
6. other admitted heap objects.

Family recognition follows vtable ancestry so a particular boxed value remains
imageable after it acquires a private vtable. Export-first ordering keeps the
canonical `Integer`, `Float`, `Word`, and `String` prototypes canonical.

## 5. Admission and failure

Structural preflight validates constructor framing, locator-only placement,
zero word counts, binding reference ranges, and the unchanged CodeObject and
portable Block closure before allocation. Boot admission remains transactional.

Process-local capabilities and objects without an admitted durable layout still
reject save honestly. This design does not make raw pointers portable and does
not weaken the ordinary send surface.

### 5.1 Word and ForeignAddress

`Word` is exclusively a portable unsigned 64-bit numeric object. It remains a
boxed, identity-bearing, imageable value and is reconstructed by `BoxedWord`.
It is not a native pointer carrier.

`ForeignAddress` is the exported prototype of an ordinary, malleable object
family for process-local native-address capabilities. Each instance has two raw
machine words: the address and an ownership kind. The prototype is the only
canonical imageable member and resolves by `ExportRef`; every nonprototype
member, including one with a private vtable, rejects image saving as
`foreignAddressNotImageable`.

The ownership kinds are prototype, borrowed, and Alien-malloc-owned. TCC symbol
lookup, FFI pointer returns, and pointer peeks mint borrowed capabilities.
`Alien alloc:` mints an owned capability, installs a finalizer, and `free`
accepts only that kind, clears it before releasing memory, and rejects repeat or
borrowed release. This prevents double-free and reclaims an unreachable owned
buffer. It cannot prove external C code has stopped using a pointer; that
remains the explicit unsafe foreign-call contract.

Alien signature `p` exclusively consumes and returns `ForeignAddress`.
Signature `u` continues to consume and return portable Integer/Word numeric
values. TCC `addSymbol:value:` likewise requires `ForeignAddress`, and
`symbol:` returns one. Generated code uses dedicated
`birnam_mkForeignAddress`, `birnam_foreignAddressValue`, and
`birnam_isForeignAddress` shims; generated wrappers reject a non-address before
entering C. The frozen
IdApi is unchanged: one shared Rust helper builds this family through its
ordinary `proto`, `alloc`, layout, ancestry, and export operations.

These additions defined Runtime ABI v4 with exact hash
`0xcdb641f978540d45`. [Design 83](../archive/design/83-production-id-construction-and-lifetimes.md)
subsequently advances the runtime ABI to v5 (`0xb587f81ecfb08b15`) for exact-entry
resumable adapters, then v6 (`0x5582a42725e647a6`) for the smaller behavior
payload. Current image-v8 headers carry the v6 hash; the image format remains 8.

## 6. Acceptance

The implementation is complete only when executable mapped, rebuilt, and
resaved images prove all of the following:

- a method on each canonical Integer, Float, Word, and String prototype remains
  visible through ordinary sends;
- a method on one particular boxed value remains attached to that exact
  restored object and not to an equal fresh value;
- a shared boxed value remains identity-equal at all restored references;
- equal but distinct boxes remain distinct;
- each CodeBinding target and implementation is the same Oop used by the
  restored heap and Store facts;
- malformed constructor tags, lengths, placements, and word counts reject;
- `p` rejects Integer/Word carriers while `u` still round-trips portable
  unsigned values;
- owned ForeignAddresses free exactly once or at collection, borrowed ones
  cannot be freed, and every reachable instance rejects image saving;
- stripped rebuild, mapped boot, resave, and fixed-live-set churn gates pass.

The representation is proportional to the live graph. 60J's compaction and
reclamation rules remain responsible for bounded indefinite execution; v8
does not add historical roots.
