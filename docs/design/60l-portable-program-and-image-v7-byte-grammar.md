# 60L — PortableProgram v1 and Image-v7 Byte Grammar

> **Status:** Accepted and physically frozen 2026-08-25. K0 is complete; K1-K5
> runtime adoption completed 2026-08-26, and K6 compatibility and bounded-
> churn acceptance completed 2026-08-27.
> D1 was accepted 2026-08-25: image-v7 uses the source-independent portable
> image-code authority
> derived in §9.4 rather than retaining an opaque source-derived
> DispatchGeneration digest. D2 was accepted the same day: a v7 CodeObject has
> the exact two-word all-Oop image payload `[PortableProgramRef, provenance]`;
> arity, capture schema, and digest are derived. D3 was also accepted: a
> CodeBinding's publication generation is live process metadata and is not
> serialized; image recovery publishes a successor generation. D4 was accepted:
> PortableProgram uses fixed-width node and operand tables in canonical
> left-to-right postorder rather than a recursive node stream. D5 was accepted:
> binding, global, Symbol, and selector names are nonempty NUL-free bytes and
> need not be UTF-8 or source-spellable. D6 was accepted: every IEEE-754
> binary64 bit pattern, including all NaNs, is admitted and preserved exactly.
> D7 was accepted: an explicit ordered capture schema is semantic and may
> contain unused or repeated outer bindings; source compilation still emits the
> minimal free-variable schema in first-use order. No semantic decision remains
> open. On 2026-08-25 the production codecs, separate Python encoder, frozen
> representative fixtures, and independent reader described in §13 landed.
> The stored-golden, generated-boundary, malformed, and bounded-property matrix
> closes K0. Mapped and forced-rebuild execution remain K5 gates under §11's
> frozen boot-admission contract.
> This record is the K0 physical-format gate required by
> [60K — Reified Portable CodeObjects](60k-portable-codeobjects.md). It freezes
> no Runtime implementation. It is the sole
> authority for the exact PortableProgram v1, portable-program inventory,
> CodeBinding inventory, materialization selection, and image-v7 bytes. The
> common canonical-value framing is inherited byte-for-byte from
> [60I — Stackless Artifact Byte Grammar](60i-stackless-artifact-byte-grammar.md).
> Image-v5 and image-v6 remain frozen legacy formats.

## 1. Decision in one paragraph

PortableProgram v1 is a pointer-free canonical postorder tree encoded as six
required sections: metadata, lexical bindings, statements, nodes, operands,
and bytes. Image-v7 stores byte-unique PortablePrograms separately from
ordinary CodeObject identities, stores current CodeBindings directly, and
stores one materialization plan that binds both inventories. A CodeObject heap
object refers to one inventory row; a portable Block vtable locator refers to
the heap identity of the same CodeObject held in payload word zero. The three
portable records determine a reproducible source-independent image-code
authority. Native code remains an optional physical projection bound to that
authority, while live vtable dictionaries remain dispatch authority.

## 2. Scope and non-goals

This record specifies:

- the exact PortableProgram v1 canonical bytes and semantic admission rules;
- the exact portable-program inventory, CodeBinding inventory, and
  materialization-selection bytes;
- the exact image-v7 header, records, tagged fields, heap constraints, record
  order, authority derivation, and preflight order;
- deterministic resource ceilings, independent-reader obligations, fixtures,
  and malformed-input coverage; and
- the cross-format agreement required before mapping native code or mutating
  the heap or Store.

It does not specify:

- language-visible CodeObject or CodeNode selector names beyond 60K;
- compiler IR, Frame layout, native symbol spelling, or generated C;
- a PortableProgram interpreter;
- the process-local materialization repository data structure;
- the native registration or resolver ABI used by a v7 artifact; or
- a compatibility reinterpretation of any v5/v6 byte.

The private native ABI may change while v7 is implemented, but its complete
declaration remains covered by the Runtime ABI hash. The durable values below
contain no native pointer, Oop, `usize`, function address, registration token,
or process-local generation lease.

## 3. Shared encoding rules

All integers are little-endian. All additions, multiplications, alignments,
prefix sums, and host-width conversions use checked arithmetic. SHA-256 hashes
the exact complete bytes named by the field, never a decoded value, native
struct, hex rendering, or host collection.

PortableProgram, PortableProgramInventory, CodeBindingInventory, and
CodeMaterializationSelection use 60I §5's exact 64-byte common prefix and exact
32-byte directory entries. In particular:

- the domain field is 32 bytes and includes the specified terminal NUL followed
  by zero padding;
- `prefix_bytes == 64`, `directory_entry_bytes == 32`, and `reserved == 0`;
- required flags are zero in every v1 format below;
- section kinds are contiguous from one and occur in the specified order;
- required empty sections remain present;
- sections start at their earliest eight-byte-aligned canonical offsets;
- alignment padding is zero, and there is no trailing padding;
- fixed records are decoded from byte slices, never by casting native structs;
  and
- every variable arena is consumed completely in its specified visitation
  order without interning, aliasing, terminators, or omitted duplicates.

The exact domains are:

| Format | Non-padding domain bytes | Zero padding | Schema version |
|---|---|---:|---:|
| PortableProgram | `birnam.portable-program.v1\0` | 5 | 1 |
| PortableProgramInventory | `birnam.portable-inventory.v1\0` | 3 | 1 |
| CodeBindingInventory | `birnam.code-bindings.v1\0` | 8 | 1 |
| CodeMaterializationSelection | `birnam.code-selection.v1\0` | 7 | 1 |

The table is normative, including the terminal NUL and padding counts.

## 4. PortableProgram v1 physical grammar

**Accepted 2026-08-25.** PortableProgram uses fixed-width tables with expression
nodes in canonical left-to-right postorder and explicit operand indices. It does
not use a recursive length-delimited node stream. This makes structural bounds,
single ownership, cycles, sharing, and stable reflected node indices decidable
before CodeNode allocation.

### 4.1 Sections and placement

PortableProgram v1 has exactly six sections:

| Kind | Section | Stride | Canonical contents |
|---:|---|---:|---|
| 1 | Metadata | 32 | exactly one program record |
| 2 | Bindings | 24 | dense parameters, captures, then lets |
| 3 | Statements | 16 | top-level body statements in evaluation order |
| 4 | Nodes | 40 | expression nodes in canonical postorder |
| 5 | Operands | 4 | node or binding indices in node-record order |
| 6 | Bytes | 1 | binding names followed by node-owned bytes |

Let `B`, `S`, `N`, `O`, and `Y` be the Binding, Statement, Node, Operand, and
Bytes counts. The directory occupies bytes `0x40..0x100`; Metadata starts at
`0x100`. Subsequent sections are packed by the shared rule:

```text
Metadata   offset = 0x100,                    length = 32
Bindings   offset = 0x120,                    length = 24 * B
Statements offset = align_up(previous_end,8), length = 16 * S
Nodes      offset = previous_end,             length = 40 * N
Operands   offset = previous_end,             length = 4 * O
Bytes      offset = align_up(previous_end,8),  length = Y
total_bytes = Bytes.offset + Y
```

Bindings, Statements, and Nodes always end on eight-byte boundaries. The only
possible internal padding is four zero bytes before Bytes when `O` is odd. The
canonical empty program has `B = S = N = O = Y = 0`, total length `288`
(`0x120`), and Metadata at `0x100`.

### 4.2 Metadata record

The one 32-byte Metadata record is:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 4 | semantic version | `1` |
| `0x04` | 4 | parameter count | Block invocation arity |
| `0x08` | 4 | capture count | ordered capture-schema size |
| `0x0c` | 4 | binding count | equals `B` |
| `0x10` | 4 | statement count | equals `S` |
| `0x14` | 4 | node count | equals `N` |
| `0x18` | 4 | operand count | equals `O` |
| `0x1c` | 4 | reserved | zero |

Parameter and capture counts sum to no more than `B`. A zero-statement body is
valid and evaluates to `nil`. Every nonempty body evaluates to its final
statement's value; a `let` statement has its initializer's value.

### 4.3 Binding records

Each Binding record is exactly 24 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 4 | kind | `1` Parameter, `2` Capture, `3` Let |
| `0x04` | 4 | kind-local ordinal | dense zero-based ordinal within its kind |
| `0x08` | 4 | name length | Bytes slice length |
| `0x0c` | 4 | defining statement | `u32::MAX` for Parameter/Capture; statement index for Let |
| `0x10` | 8 | reserved | zero |

The Binding record index is its semantic binding identity. Records are exactly:

1. all Parameters in signature order;
2. all Captures in the explicit semantic schema order; then
3. all Lets in defining-statement order.

Every Parameter and Capture ordinal equals its position within the relevant
prefix. Every Let ordinal starts at zero and increases by one. A Let record's
defining statement names that binding in a Let Statement record, and each Let
Statement names exactly one Let record. Duplicate reflected names are allowed;
binding identity, never spelling, resolves lexical access.

Names are nonempty byte strings containing no NUL. They need not be UTF-8 or
source-spellable. This permits public CodeNode construction without turning the
current parser's identifier grammar into a hidden object-authority boundary.
This opaque-byte policy was accepted 2026-08-25. Inspection uses escaped byte
rendering when current Birnam source syntax cannot spell a name; optional
SourceProvenance, not PortableProgram, owns exact authoring fidelity.

### 4.4 Statement records

Each Statement record is exactly 16 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 4 | kind | `1` Expression, `2` Let |
| `0x04` | 4 | root node | in-range Node index |
| `0x08` | 4 | binding index | `u32::MAX` for Expression; matching Let binding for Let |
| `0x0c` | 4 | reserved | zero |

Statements occur in source evaluation order. A Let binding is not in scope in
its own initializer and becomes visible only after its Statement completes.

### 4.5 Node records

Each Node record is exactly 40 bytes:

| Offset | Width | Field | Meaning |
|---:|---:|---|---|
| `0x00` | 4 | kind | closed NodeKind enum below |
| `0x04` | 4 | required flags | zero in v1 |
| `0x08` | 4 | operand count | number of following logical Operand values owned by this node |
| `0x0c` | 4 | data length | number of following logical Bytes owned by this node |
| `0x10` | 8 | payload | kind-specific fixed value |
| `0x18` | 4 | auxiliary 0 | kind-specific count or index |
| `0x1c` | 4 | auxiliary 1 | kind-specific count or index |
| `0x20` | 8 | reserved | zero |

Node kinds and exact field use are:

| Kind | Name | Operands | Data | Payload | Aux 0 | Aux 1 |
|---:|---|---|---|---|---|---|
| 1 | Integer | 0 | 0 | signed `i64` two's-complement bits | 0 | 0 |
| 2 | FloatBits | 0 | 0 | exact IEEE-754 binary64 bits | 0 | 0 |
| 3 | String | 0 | decoded String bytes | 0 | 0 | 0 |
| 4 | Symbol | 0 | Symbol name bytes | 0 | 0 | 0 |
| 5 | Nil | 0 | 0 | 0 | 0 | 0 |
| 6 | True | 0 | 0 | 0 | 0 | 0 |
| 7 | False | 0 | 0 | 0 | 0 | 0 |
| 8 | LexicalRead | 0 | 0 | 0 | binding index | 0 |
| 9 | GlobalRead | 0 | global-name bytes | 0 | 0 | 0 |
| 10 | Send | receiver then arguments | selector bytes | 0 | language arity | 0 |
| 11 | BlockLiteral | outer binding indices | complete nested PortableProgram | 0 | nested parameter count | nested capture count |

Every field shown as `0` is exactly zero. String data may be empty and may
contain any byte. Symbol, global, selector, and binding-name data is nonempty
and contains no NUL because the current Symbol/global floor owns NUL-terminated
names; the NUL is never stored. **Accepted 2026-08-25:** these values need not be
UTF-8 or spellable by the current parser. The ordinary runtime Symbol/send
surface, not source grammar, is the PortableProgram admission boundary.

**Accepted 2026-08-25.** All 64-bit floating encodings are admitted, including
signed zero, infinities, signalling NaNs, quiet NaNs, and every NaN payload.
Portable semantics preserve the exact bits. A backend that cannot preserve one
admitted bit pattern is not compatible with PortableProgram v1; it may not
normalize the value silently. Materialization uses a bit-preserving constructor
or byte copy for values that cannot safely pass through a source literal or
floating-point operation.

Selector arity is derived exactly as the current Birnam send surface derives
it: a nonempty selector containing one or more `:` bytes has arity equal to its
colon count; otherwise a selector whose first byte is ASCII alphabetic or `_`
has arity zero; every other nonempty selector has arity one. No UTF-8 or parser-
spellability requirement is added. Send operand count is exactly arity plus
one, with receiver first and arguments in left-to-right evaluation order.

### 4.6 Operand and Bytes visitation

The Operands section is the concatenation of each Node record's operands in
Node order. Send operands are Node indices. BlockLiteral operands are binding
indices in the nested program's capture order. Other nodes own no operands.
The checked prefix sum of all Node operand counts must consume exactly `O`.
Equal operand sequences are serialized again.

The Bytes section is constructed in this exact order:

1. every Binding name in Binding-record order; then
2. every Node's data in Node-record order.

The checked prefix sums of Binding name lengths and Node data lengths consume
exactly `Y`. Equal byte values, including equal nested programs, are serialized
again. There are no stored offsets, terminators, gaps, aliases, or per-value
length prefixes.

### 4.7 Tree, scope, and capture canonicalization

Nodes encode a tree, not a general DAG. Node records are the concatenated
left-to-right postorder traversals of Statement roots in Statement order.
Admission reconstructs those traversals and requires the visited Node indices
to be exactly `0..N`; every Node therefore has exactly one parent edge or one
Statement-root edge. Every Send child index is less than the Send's index.
Cycles, shared children, orphan nodes, reordered independent subtrees, and
forward child references reject.

LexicalRead resolves a Parameter, Capture, or previously completed Let in the
same PortableProgram. A BlockLiteral capture operand likewise names an outer
binding in scope at the literal. The nested program contains exactly as many
Capture bindings as the BlockLiteral has operands; Aux 0 and Aux 1 equal the
nested program's Metadata counts. Each operand supplies exactly one schema
entry in order. Distinct entries may name the same outer binding.

**Accepted 2026-08-25.** Capture schema order is explicit semantic
ordinary-object state. A Capture binding may be read zero, one, or many times;
unused entries and repeated outer operands are valid and remain visible through
`captureCount` and `captureAt:`. Public construction and byte re-encoding
preserve the supplied schema order exactly. Source compilation remains compact:
it emits only actual free variables, ordered by first occurrence under the same
Statement-order, left-to-right expression traversal.

## 5. PortableProgram semantic admission

Admission is iterative and completes before CodeNode allocation or native
compilation:

1. validate the common prefix, exact domain/version/length/flags, directory,
   section placement, padding, strides, counts, and bounds;
2. validate Metadata and its agreement with directory counts;
3. derive and validate Binding names, kinds, partitions, ordinals, and Let
   relationships;
4. validate Statement tags and references;
5. derive Node operand/data slices and validate every closed tag shape;
6. recursively admit each embedded PortableProgram under aggregate limits;
7. validate selector arity and nested signature/capture agreement;
8. validate canonical postorder, single ownership, lexical scope, Let timing,
   and exact capture-schema/operand agreement; and
9. re-encode independently in debug/tests and require byte equality.

Hash agreement is not semantic admission. A caller that receives both bytes
and an expected digest first admits the complete bytes, computes SHA-256 over
them, and then compares the digest.

## 6. PortableProgramInventory v1

The image inventory stores one copy of each byte-distinct program while
preserving distinct CodeObject Oop identities in Heap.

PortableProgramInventory has exactly two sections:

| Kind | Section | Stride | Canonical contents |
|---:|---|---:|---|
| 1 | ProgramRows | 48 | one row per byte-distinct PortableProgram |
| 2 | ProgramBytes | 1 | exact complete programs in row order |

Each ProgramRow is:

| Offset | Width | Field | Requirement |
|---:|---:|---|---|
| `0x00` | 32 | program SHA-256 | hash of exact complete program bytes |
| `0x20` | 4 | program length | nonzero byte count in ProgramBytes |
| `0x24` | 4 | required flags | zero |
| `0x28` | 4 | parameter count | equals admitted Metadata |
| `0x2c` | 4 | capture count | equals admitted Metadata |

Rows sort lexicographically by raw SHA-256 and then by complete program bytes.
Exact duplicate bytes reject; a deliberately injected hash collision remains
valid only when the bytes differ and is disambiguated by complete equality.
ProgramBytes is the non-interned concatenation in row order, and its prefix sum
must consume the complete section. The inventory set is exactly the closure of
(a) every program referenced by a Heap CodeObject and (b) every nested
PortableProgram embedded transitively by those programs. Missing closure rows
and extra unreachable rows both reject. This lets a stripped mapped image
construct a nested Block later without inventing executable meaning, requiring
a compiler, or retaining hidden code roots. The canonical empty inventory is
valid exactly when Heap contains no CodeObject.

The row index is an image-local `program_index`. It is not a CodeObject identity
and never substitutes for complete program bytes outside the exact inventory
whose SHA-256 qualifies it.

## 7. CodeBindingInventory v1

CodeBindingInventory stores the current portable method associations required
by 60K. It has exactly two sections:

| Kind | Section | Stride | Canonical contents |
|---:|---|---:|---|
| 1 | BindingRows | 32 | one row per current target/selector association |
| 2 | FieldBytes | 1 | four exact image Field encodings per row |

Each BindingRow is:

| Offset | Width | Field | Requirement |
|---:|---:|---|---|
| `0x00` | 4 | target field length | exactly one permitted Field |
| `0x04` | 4 | selector field length | exactly one Symbol Field |
| `0x08` | 4 | implementation field length | exactly one permitted Field |
| `0x0c` | 4 | provenance field length | exactly one permitted Field |
| `0x10` | 4 | selector arity | derived from Symbol bytes |
| `0x14` | 4 | required flags | zero |
| `0x18` | 8 | reserved | zero |

FieldBytes visits target, selector, implementation, and provenance for each row
in Binding-row order. It stores the exact image-v7 Field encodings specified in
§10. Target and implementation must be one of Nil, True, False, Integer,
Symbol, HeapRef, or ExportRef. Those forms preserve canonical singleton,
immediate, interned, heap, or exported identity across load. Inline Float and
String are forbidden here because reconstructing equal value bytes would not
preserve the particular object's private-vtable identity. Selector must be a
nonempty NUL-free Symbol. Provenance may be Nil, String, HeapRef, or ExportRef;
it never affects binding or materialization semantics.

Rows sort by the complete encoded target Field and then the selector's exact
Symbol bytes. Duplicate target/selector pairs reject. The row index is the
image-local `binding_index`.

The per-binding semantic digest used by §8 deliberately excludes provenance:

```text
SHA-256(
    "birnam.code-binding-row.v1\0" ||
    binding_index:u32-le ||
    target_field_length:u32-le || selector_field_length:u32-le ||
    implementation_field_length:u32-le || selector_arity:u32-le ||
    exact target Field || exact selector Field ||
    exact implementation Field
)
```

The quoted domain includes its terminal NUL. The index is safe in this digest
because canonical sorting makes it reproducible within the exact inventory.
The complete semantic digest of the binding set is:

```text
SHA-256(
    "birnam.code-binding-semantics.v1\0" ||
    binding_count:u32-le ||
    each per-binding digest in Binding-row order
)
```

Changing, adding, or stripping provenance therefore changes the complete
CodeBindingInventory bytes and their corruption-detection hash, but does not
change binding semantics, materialization choice, or portable image-code
authority.

**Accepted 2026-08-25.** A CodeBinding's current publication generation is
live carrier and lifetime metadata, not durable CodeBinding meaning. It does
not occur in CodeBindingInventory, provenance, materialization selection, or
portable image-code authority. Image recovery publishes the persisted current
association into a fresh process-local successor generation. Store alone owns
any retained semantic history under its configured retention policy.

## 8. CodeMaterializationSelection v1

This value is physical selection and artifact agreement, not portable program
meaning. It includes synchronous rows because image-v7 must locate their native
Methods as well as resumable entries. It deliberately excludes FrameDescriptor
hashes: a compatible rebuilding compiler may choose a different valid Frame
layout without changing the image's portable authority. The native artifact's
separately admitted manifest owns exact descriptor hashes and must project the
same selection rows.

It has exactly three sections:

| Kind | Section | Stride | Canonical contents |
|---:|---|---:|---|
| 1 | Metadata | 64 | exact hashes of the two input inventories |
| 2 | MaterializationRows | 64 | one selected execution disposition per subject |
| 3 | SelectorBytes | 1 | exact selector bytes in row order |

The one Metadata record is:

| Offset | Width | Field |
|---:|---:|---|
| `0x00` | 32 | SHA-256 of the complete PortableProgramInventory bytes |
| `0x20` | 32 | §7 complete CodeBinding semantic digest |

Each 64-byte MaterializationRow is:

| Offset | Width | Field | Requirement |
|---:|---:|---|---|
| `0x00` | 4 | kind | `1` NamedAdapter, `2` BlockBody |
| `0x04` | 4 | disposition | `1` Synchronous, `2` Resumable |
| `0x08` | 4 | subject index | binding index or program index according to kind |
| `0x0c` | 4 | language arity | selector-derived |
| `0x10` | 4 | capture count | zero for NamedAdapter; program value for BlockBody |
| `0x14` | 4 | selector length | SelectorBytes slice length |
| `0x18` | 4 | required flags | zero |
| `0x1c` | 4 | reserved | zero |
| `0x20` | 32 | semantic SHA-256 | binding-row digest or program digest |

Rows sort by kind and then subject index. Duplicate kind/subject pairs reject.
SelectorBytes concatenates exact selectors in row order without deduplication.

A NamedAdapter row exists exactly once for every CodeBinding row. Its subject
index names that binding, semantic digest is its §7 digest, language arity and
selector bytes equal the installed binding selector, and capture count is zero.

A BlockBody row exists exactly once for every program index. Its semantic
digest, parameter count, and capture count agree with the ProgramRow. Its
selector is `value` for arity zero and `value:` repeated exactly arity times
otherwise. This keeps every reachable CodeObject and every transitively nested
program constructible in a stripped mapped image. Implementations may omit a
native realization only when boot has admitted a compatible compiler capable
of realizing that exact selected row before a Block is published.

For a mapped or rebuilt Resumable row, the native manifest supplies one admitted
FrameDescriptor v2 whose implementation kind, language arity, driver argument
count, selector, and capture contract agree. Disposition is explicit physical
policy; no source annotation, selector spelling, target class, or current
implementation object silently chooses it.

## 9. Image-v7 outer grammar

### 9.1 Header and record envelope

Image-v7 retains the exact v5/v6 28-byte header and 12-byte record envelope:

| Header offset | Width | Field | v7 requirement |
|---:|---:|---|---|
| `0x00` | 8 | magic | exact ASCII `BIRNAMIM` |
| `0x08` | 4 | format version | `7` |
| `0x0c` | 4 | endian tag | `0x01020304` |
| `0x10` | 4 | architecture | existing architecture identifier |
| `0x14` | 8 | Runtime ABI | exact compatible Runtime ABI hash |

```text
Record := kind:u32-le || payload_length:u64-le || payload
```

Existing kinds retain their numbers. V7 forbids legacy Resumables kind 6 and
adds three kinds:

| Record kind | Value |
|---|---:|
| Facts | 2 |
| Heap | 3 |
| Code | 4 |
| Authority | 5 |
| Resumables | 6, forbidden in v7 |
| PortablePrograms | 7 |
| CodeBindings | 8 |
| Materializations | 9 |

### 9.2 Hashed canonical-value record payload

PortablePrograms, CodeBindings, and Materializations use one exact 40-byte
prefix followed by one complete common-framed value:

| Payload offset | Width | Field |
|---:|---:|---|
| `0x00` | 32 | SHA-256 of the complete following canonical value |
| `0x20` | 8 | canonical-value byte length `L` |
| `0x28` | `L` | exact complete canonical value |

The record payload length is exactly `40 + L`; the embedded `total_bytes` is
exactly `L`; no padding or suffix follows. PortablePrograms contains one
PortableProgramInventory, CodeBindings one CodeBindingInventory, and
Materializations one CodeMaterializationSelection.

### 9.3 Record order and cardinality

The only v7 record sequence is:

```text
Authority,
PortablePrograms,
CodeBindings,
Materializations,
Heap*,
Facts*,
Code?
```

The first four records exist exactly once even when their inventories are
empty. All Heap records precede all Facts records. Code is optional, unique,
and final. No other record kind is admitted. Unlike v5, a portable v7 image may
omit Code and rebuild from the complete portable records through an admitted
compiler. Boot, not structural preflight, rejects when neither compatible Code
nor a compatible compiler is available.

### 9.4 Portable image-code authority

**Accepted 2026-08-25.** Image-v7 uses a reproducible source-independent
portable image-code authority. It does not retain the v5/v6 source-derived
DispatchGeneration digest as an opaque recovery token. This changes the
version-qualified authority derivation, not the exact 64-byte lowercase-
hexadecimal Authority payload shape.

The Authority payload remains exactly 64 lowercase hexadecimal bytes, but v7
gives it a source-independent derivation:

```text
portable_programs_sha256 = PortablePrograms payload[0x00..0x20]
binding_semantics_sha256 = Materializations.Metadata[0x20..0x40]
materializations_sha256  = Materializations payload[0x00..0x20]

authority_raw = SHA-256(
    "birnam.portable-image-authority.v1\0" ||
    portable_programs_sha256 ||
    binding_semantics_sha256 ||
    materializations_sha256
)
```

The Authority payload is the exact lowercase hexadecimal spelling of
`authority_raw`. The quoted domain includes its terminal NUL. The
Materializations Metadata program hash must equal the PortablePrograms record
hash, and its binding hash must equal §7's recomputed provenance-free semantic
digest before authority is computed. This acyclic construction binds exact
program meaning, current binding semantics, and physical selection without
requiring SourceRevision or placing Authority inside a value from which
Authority is derived. CodeBinding provenance is checked by its own record hash
but remains outside executable authority.

If Code is present, its v5-compatible 64-byte generation-digest spelling equals
Authority byte-for-byte. The mapped artifact exports the exact three authority
components—program-inventory hash, provenance-free binding-semantics hash, and
materialization-selection hash—and the complete materialization selection;
admission compares all of them before any resolver or registration entry is
called. It neither consumes nor exports CodeBinding provenance.

### 9.5 Optional Code payload

The v7 Code payload retains v5's exact framing:

| Payload offset | Width | Field | Requirement |
|---:|---:|---|---|
| `0x00` | 8 | checksum | `wyhash(seed = 0, unit bytes)` |
| `0x08` | 64 | generation authority | byte-identical to Authority payload |
| `0x48` | remaining payload | native unit bytes | nonempty admitted artifact |

The Code payload length is at least 73 bytes and consumes the complete final
record. The checksum is corruption detection, not trusted-code admission. The
header's architecture and Runtime ABI, the portable-record hashes, the native
artifact's own exact manifest and descriptors, and its exported resolver and
registration ABI must all agree before the unit is allowed to publish anything.

## 10. Image-v7 Field and Heap grammar

### 10.1 Field additions

V7 retains field tags 0 through 9 byte-for-byte, forbids legacy tag 10, and
adds:

```text
PortableProgramRef      := 0x0b || program_index:u32-le
PortableBlockVtableRef  := 0x0c || code_object_heap_id:u32-le
```

Each is exactly five bytes. A `PortableProgramRef` is admitted only as word zero
of a v7 CodeObject heap object. It decodes to an immutable String containing
the exact program bytes selected from PortableProgramInventory; repeated
references may share that immutable String as a nonsemantic optimization.

A `PortableBlockVtableRef` is admitted wherever an existing image Field may
name an Oop and as a Heap vtable locator. It resolves through the named
CodeObject's program and required BlockBody materialization; no native address,
program index, digest, or generated export name accompanies it.

### 10.2 Global heap identity

Heap payload framing remains v5's:

```text
Heap := object_count:u32-le || HeapObject[object_count]
HeapObject := VtableLocator || word_count:u32-le || encoded_slots
```

Heap object identities are zero-based positions in the concatenation of every
Heap record's objects in record order. `HeapRef` and
`PortableBlockVtableRef` use that global identity. Forward references and
cycles remain valid.

VtableLocator is exactly ExportRef tag 9 or PortableBlockVtableRef tag 12. Tag
10 and every other Field tag reject in locator position.

### 10.3 CodeObject heap contract

**Accepted 2026-08-25.** The v7 CodeObject image payload is the minimal two-word
all-Oop representation below. Arity, capture schema, and digest remain semantic
CodeObject fields exposed through ordinary sends, but are derived from the one
immutable PortableProgram authority rather than stored as redundant heap words.

The exported vtable name `CodeObjectVt` identifies a v1 CodeObject in v7 Heap.
It has the exact all-Oop two-word payload:

```text
word 0  PortableProgramRef(program_index)
word 1  provenance: Nil | String | HeapRef | ExportRef
```

Its arity, capture schema, program bytes, and digest are derived from the
admitted PortableProgram and inventory row; the runtime may cache them but the
image never stores a second mutable authority. Distinct CodeObject heap objects
may refer to the same program index and retain distinct ordinary Oop identity.

### 10.4 Portable Block heap contract

A Heap object with `PortableBlockVtableRef(code_object_heap_id)` locator has an
all-Oop payload. Word zero is exactly `HeapRef(code_object_heap_id)`. The target
is an in-range `CodeObjectVt` object whose program is admitted, and:

```text
word_count == referenced_program.capture_count + 1
```

Words `1..word_count` are ordinary capture Fields in the referenced program's
canonical capture order. The referenced program index has exactly one
BlockBody materialization row. The locator/word-zero agreement is checked from
bytes before vtable resolution or heap allocation.

The exported zero-word `Block` family prototype is not a portable Block and
continues to use its ordinary exported `BlockVt` locator. No payload-size guess
distinguishes it.

### 10.5 CodeBindings and other fields

Every HeapRef in CodeBindingInventory is in range under the same global heap
identity. ExportRefs are nonempty, NUL-free names that resolve under the image's
ordinary export contract. The target and implementation object need not be
Blocks, but image save rejects any current callable implementation lacking its
own admitted durable reconstruction contract.

Facts use image-v6's exact retention-aware Facts payload and all unchanged Field
encodings. PortableBlockVtableRef is valid in a Fact. PortableProgramRef is not.

## 11. Image-v7 structural-preflight and boot-admission boundary

**Accepted 2026-08-25.** Admission is deliberately split. K0
`structural_preflight` is a pure durable-byte operation. It performs no native
mapping, compilation, resolver call, heap allocation, Symbol interning,
CodeBinding publication, Store mutation, or user send. It validates in this
order:

1. exact header, architecture, Runtime ABI, record framing, record order, and
   cardinality;
2. all three hashed-value prefixes, lengths, complete hashes, common prefixes,
   section directories, bounds, and canonical admission;
3. Materialization Metadata agreement and derived Authority spelling;
4. complete Heap framing, global object count, all Field tags, export names,
   layouts that can be validated without materialization, and all reference
   ranges;
5. every CodeObject payload and PortableProgramRef;
6. every portable Block locator, word-zero reference, capture count, and the
   complete transitive BlockBody-row coverage;
7. every CodeBinding field, target/selector uniqueness, implementation
   imageability, and required NamedAdapter row;
8. exact Materialization row coverage, selectors, arities, semantic hashes,
   and dispositions; and
9. Code framing, checksum, and authority spelling if Code exists.

K5 `boot_admission` consumes an already structurally admitted image. If Code
exists it checks the mapped artifact's exported inventory hashes, complete
selection, manifest bytes, descriptors, resolver inventory, and registration
ABI. Otherwise it admits a compatible compiler capable of rebuilding the same
authority. It rejects before boot commits to reconstruction when neither path
is available.

Only after complete `boot_admission` does boot stage allocations and
materializations in 60K §12.2's order. Any failure releases staged native
mappings and ordinary allocations, publishes no CodeBinding, and leaves Store
unchanged. Structural validity alone is never executable admission.

## 12. Deterministic bounds

These are admission ceilings, not claims about available memory:

| Quantity | Maximum |
|---|---:|
| one common-framed canonical value | 64 MiB |
| one PortableProgram Node count | 1,048,576 |
| one PortableProgram Binding or Statement count | 262,144 |
| parameters or captures in one program | 4,096 each |
| operands or selector-derived language arity at one node | 4,096 |
| one name, Symbol, global, or selector | 1 MiB |
| one String literal | 16 MiB |
| one PortableProgram Bytes section | 32 MiB |
| expression-tree depth | 4,096 |
| nested PortableProgram depth | 4,096 |
| byte-distinct programs in one inventory | 262,144 |
| CodeBindings in one image | 262,144 |
| CodeObjects or portable Blocks in one image | 1,048,576 each |
| Materialization rows | 524,288; exactly programs plus CodeBindings |
| aggregate v7 portable canonical bytes | 256 MiB |
| transitive PortableProgram bytes admitted from one root | 64 MiB |
| complete image bytes | 1 GiB |

Counters for nested programs, transitive bytes, fields, references, and
materializations are cumulative even when exact bytes repeat. Implementations
must parse nested programs iteratively or otherwise prove that the accepted
depth cannot exhaust the native stack.

## 13. Canonical fixtures and independent reader

**Implemented proof slice, 2026-08-25.**
[`abi/fixtures/portable-code-v1`](../../abi/fixtures/portable-code-v1/README.md)
contains two independent encoders' byte-identical representatives for all four
common values and image-v7. The frozen corpus covers thirteen PortablePrograms,
five program inventories, four binding inventories, four materialization
selections, and six images. Besides empty and mixed stripped images, retained
Facts, and final Code framing, it covers non-UTF-8 names/selectors, a nonempty
nested Block, duplicate program elision, Block and non-Block implementations,
shared CodeObjects, repeated Blocks, and cyclic captures. Two generated
boundary programs cover every maximum individual byte length and the complete
32 MiB Bytes-section ceiling. The independent reader imports neither production
codec nor fixture builder, validates 102 targeted faults, and rejects 27,847
non-value prefixes across every compact golden. Production nested-program
admission and the independent reader both walk nested programs iteratively
under the frozen depth and aggregate limits.

The Code fixture contains a synthetic checked unit. It proves the Code record,
checksum, and authority bytes, not native manifest/export agreement or a
successful mapping. Compiler admission, real mapped load, forced rebuild, and
runtime CodeObject/Block materialization remain later 60K gates. They are
deliberately not K0 evidence and their absence does not weaken the frozen
structural contract.

### 13.1 Evidence classes

**Accepted 2026-08-25.** K0 uses three explicit evidence classes:

1. **Stored goldens** are compact, committed, admissible exact-byte values.
   Two independent encoders reproduce them byte-for-byte.
2. **Generated boundary vectors** have committed deterministic construction
   recipes plus pinned exact lengths and SHA-256 values. Production and
   independent encoders reproduce and fully admit the bytes during tests, but
   maximum-size repetitive binaries are not stored in the repository.
3. **Synthetic algorithm tests** exercise states that cannot be represented by
   valid canonical bytes. In particular, digest ordering is tested with an
   injected equal-digest seam because no SHA-256 collision is known. Synthetic
   rows are never admitted as PortableProgramInventory fixtures.

The malformed corpus is also recipe-based: committed independent mutations,
truncation loops, and bounded generators produce rejected inputs from the
stored goldens and generated vectors. Hundreds of redundant corrupt binaries
need not be stored. Every reported case identifies its source recipe and
expected rejection stage.

Together these evidence classes must cover:

- empty program; every literal kind; signed integer extrema; positive and
  negative zero; infinities; quiet/signalling NaNs with distinct payloads;
- one Parameter, Capture, and Let; duplicate reflected names; lexical scope
  boundaries; empty and nonempty Blocks; nested captures; explicitly unused
  captures; repeated outer bindings in distinct capture entries; and zero-,
  unary-, binary-, and multi-keyword-arity Sends;
- binary Strings with NUL, non-UTF-8 names/selectors without NUL, repeated equal
  arena values, maximum admitted individual lengths, and alignment padding;
- empty, one-row, and duplicate-byte-elided program inventories, plus synthetic
  equal-digest ordering with bytewise collision resolution;
- empty, Block implementation, non-Block implementation, provenance, and
  multiple-target CodeBinding inventories;
- synchronous/resumable and NamedAdapter/BlockBody materialization rows, plus
  a rebuilt Resumable descriptor whose physical hash differs while selection
  and portable authority remain unchanged;
- data-only v7, compiler-rebuild v7, mapped-code v7, cyclic captures, multiple
  CodeObjects sharing one program, and repeated Blocks sharing one CodeObject;
  and
- the exact authority derivation and all cross-record hashes.

Every stored golden records exact bytes, length, SHA-256, decoded structure,
and round-trip equality. Every generated boundary vector records its recipe,
length, SHA-256, decoded structure, and round-trip equality. A small independent
reader in a different crate or language uses no production serializer/decoder
types and validates at least the empty, mixed nested, and complete code-framed
image fixtures plus every generated boundary vector.

## 14. Malformed-input matrix

The negative corpus includes at least:

- truncation at every prefix, directory, record, Field, and arena boundary;
- wrong domain/version/size/flags/section count/kind/order/stride/count/offset,
  nonzero reserved bytes or padding, overlap, gap, alias, overflow, and suffix;
- every unknown Binding, Statement, Node, Materialization, record, and Field
  tag;
- count disagreement, arena under/over-consumption, forbidden empty names,
  embedded NUL names, wrong selector arity, wrong fixed-zero fields, and every
  bound plus one;
- orphan/shared/cyclic/forward/reordered Nodes, out-of-scope reads, Let use in
  its initializer, missing/duplicate Let relationships, capture-count or
  capture-operand disagreement, and nested signature disagreement;
- malformed embedded programs, depth/aggregate exhaustion, Float bit drift,
  forged program hashes, unsorted inventory rows, exact duplicate programs,
  and injected digest collisions resolved incorrectly;
- malformed CodeBinding Field lengths, forbidden tags, duplicate
  target/selector rows, bad heap/export references, wrong per-binding digest,
  and provenance affecting semantics;
- missing/duplicate/extra Materialization rows, wrong coverage, kind/index,
  selector, arity, capture count, semantic digest, or disposition;
- legacy tag 10 or record 6 in v7, PortableProgramRef outside CodeObject word
  zero, invalid CodeObject layout, locator/word-zero mismatch, wrong Block word
  count, and a locator naming a non-CodeObject;
- missing/duplicate/reordered portable records, forged record hashes, forged or
  uppercase Authority, incorrect authority derivation, orphan Code, Code not
  final, and trailing image bytes; and
- a structurally valid stripped image with neither compatible native Code nor
  an admitted compiler, which passes structural preflight but fails boot with
  the explicit compatibility Error required by 60K.

Property tests generate bounded valid trees and images, round-trip them, and
mutate each owned scalar, tag, index, byte range, hash, and ordering relation.
The production reader and independent reader must agree on the complete K0
fixture corpus. At K5, compiler admission, mapped `boot_admission`, and the
forced-rebuild loader must agree on the same frozen authority vectors and all
applicable image fixtures.

## 15. Acceptance gates

60L is accepted only after:

1. the accompanying adversarial review is complete and every Critical or High
   finding is incorporated or explicitly rejected with rationale;
2. every physical choice called out in that review has been discussed and
   accepted one at a time;
3. 60K, 60J, and the 60 overview cross-link the accepted authority and delivery
   sequence without claiming implementation;
4. two independent encoders produce byte-identical stored goldens and matching
   generated boundary-vector lengths and SHA-256 values;
5. the independent reader and malformed-input matrix pass; and
6. mapped and forced-rebuild designs commit to the same frozen authority
   vectors and the `boot_admission` boundary before K1 implementation begins.

K0 does not require a native artifact to map or a compiler to execute. Those
are K5 implementation and acceptance gates. A synthetic nonempty Code unit is
valid K0 evidence only for final-record framing, checksum, and authority
spelling; it is not executable-admission evidence.

All six K0 gates closed on 2026-08-25. This froze the durable grammar and
authorized K1 implementation. K1-K5 completed 2026-08-26. Production now emits
a final nonempty Code record; ordinary boot maps it and forced or stripped boot
rebuilds from the same canonical inventories. The independent reader, frozen
checksums, runtime codec tests, and mapped/rebuilt lifecycle smoke agree on the
authority vectors. K6 compatibility and bounded-churn acceptance completed
2026-08-27.
