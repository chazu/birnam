# 60I — Stackless Artifact Byte Grammar

> Scope: versioned format/protocol reference. Historical migration stages and
> native implementation arrangements are not additional language requirements.
> Follow [INVARIANTS.md](../../INVARIANTS.md) for architecture; retain the
> specified bytes when reading or writing the documented format.

> **Status:** S2 physical-format design accepted and pre-implementation gate
> closed 2026-08-22.
> The semantic contents, identity layering, rooting, selection composition,
> manifest-index resolution, image-recovery behavior, and no-IdApi-growth
> boundary are accepted by
> [60E](../archive/2026-10-runtime-refactors/60e-resumable-registration-and-entry.md) and
> [60F](../archive/2026-10-runtime-refactors/60f-resumable-compiler-lowering.md). This record is the sole authority
> for their exact durable byte grammars and process-local registration-envelope
> ABI. The accepted cross-format constraints, exact records, schema sections,
> native ABI, image records, golden bytes, malformed matrix, independent
> conformance reader, and non-circular construction trace are fixed below.
> 60E's 2026-08-15 shared private target-resolution amendment
> changes no durable byte or native-envelope field: the unique entry thunk
> remains the registered Method carrier. S2
> implementation may rely on this closed physical-format contract.
>
> This record does not accept an implementation. Closing its design gates only
> makes an implementation unambiguous enough to begin and test.

## 1. Purpose and authority

The stackless design now has stable semantic answers for what an artifact must
say, but prose such as “contains PC records” or “uses fixed-width integers” is
not an independently implementable wire format. Without exact offsets, widths,
record sizes, bounds, and digest ranges, the first Rust codec would become an
accidental specification. That would make a replacement compiler reproduce one
implementation rather than implement a documented Birnam contract.

60I prevents that outcome. It owns the exact physical representation of:

1. the shared stable implementation key;
2. `ArtifactSelection v1`;
3. `GenerationSelection v1`;
4. `ResumableManifest v1`;
5. the versioned native registration envelope;
6. canonical FrameDescriptor layout v2 bytes; and
7. image-v5 `Resumables` framing and manifest-index references.

The surrounding records retain separate authority:

| Record | Authority |
|---|---|
| [40](../archive/design/40-the-image.md) | General image ownership, persistence, mapping, and recovery policy. |
| [49](../archive/design/49-live-module-evolution.md) | SourceRevision and logical DispatchGeneration identity and publication. |
| [60B](../archive/2026-10-runtime-refactors/60b-stackless-machine-and-frames.md) | Frame semantics, root safety, slot interpretation, and physical-code pins. |
| [60E](../archive/2026-10-runtime-refactors/60e-resumable-registration-and-entry.md) | Entry recognition, registration lifetime, implementation anchors, binding ownership, and recovery ordering. |
| [60F](../archive/2026-10-runtime-refactors/60f-resumable-compiler-lowering.md) | Eligibility, lowering, liveness, coloring, semantic descriptor contents, selection materialization, and cross-format agreement. |
| **60I** | Exact bytes, exact native envelope layout, structural validation, format limits, and conformance fixtures. |

If 60I and another record appear to disagree about bytes, 60I governs the
bytes. If they disagree about language behavior, lookup, rooting, lowering, or
lifetime, the semantic owner above governs and 60I must be corrected.

## 2. Non-goals and malleability boundary

60I does not define or constrain:

- Birnam source syntax or language-visible object layout;
- IdApi fields, Method ABI, lookup, inheritance, DNU, or metacircular policy;
- relational rules that choose execution disposition;
- compiler IR, data structures, liveness algorithm, or coloring heuristic;
- Rust collection types or iteration order;
- native code addresses as durable identity;
- suspension, Continuations, scheduling, or mapping reclamation; or
- a compatibility reader for unshipped pre-S2 experimental formats.

Canonical bytes are an interchange contract, not a hidden semantic collection.
A compiler may use any internal representation and emit a different valid Frame
layout. A Runtime may decode into vectors, trees, borrowed views, or another
index. Those choices are non-authoritative. Only the accepted canonical bytes,
their hashes, and the semantic validation rules cross implementations.

## 3. Identity layers and construction order

The byte formats preserve the acyclic identity separation accepted by 60F:

```text
SourceRevision + stable root key + lowering version
                         |
                         v
             portable implementation identity
                         |
                         v
               canonical descriptor bytes
                         |
                         v
                    descriptor SHA-256
                         |
                         v
source map + operations + carriers + GenerationSelection SHA-256
                         |
                         v
              DispatchGeneration base digest
                         |
                         v
          ResumableManifest authority field
                         |
                         v
                  final artifact checksum

mapping the artifact -> process-local CodeGeneration identity and lease
```

Neither a logical DispatchGeneration identity nor a process-local
CodeGeneration identity occurs in descriptor bytes. The final artifact checksum
does not feed back into a descriptor or DispatchGeneration digest.

The `u64` portable implementation identity is reproducible metadata for
descriptors, source maps, and diagnostics. It is not collision-free lookup
authority. A durable manifest key uses the logical authority, complete stable
source key, and descriptor SHA-256. A unit-local resolver and registration
operation use the exact canonical manifest entry index.

## 4. Cross-format schema ledger

This ledger identifies one semantic owner and every physical consumer before
field offsets are assigned.

| Information | Canonical owner | Producers | Consumers | Identity role |
|---|---|---|---|---|
| SourceRevision digest | Design 49; embedded through 60I's shared key | source/candidate projection | selection, manifest, generation composition, image recovery | durable source authority |
| root kind, root index, lexical block path, implementation kind | shared stable-key grammar | normalized inventory | selection, manifest, diagnostics | durable implementation location |
| execution disposition | ArtifactSelection or GenerationSelection | materializer/composer | compiler, activation, image replay | generation behavior provenance |
| portable `u64` implementation identity | descriptor and manifest fields | compiler | FrameView, source maps, diagnostics | reproducible metadata, never sole lookup authority |
| descriptor semantics | descriptor-v2 blob | lowering/layout | Runtime Frame construction and tracing | physical execution contract |
| descriptor SHA-256 | hash of exact descriptor bytes | compiler/loader verifier | manifest, dispatch carrier, registration | exact layout identity |
| DispatchGeneration base digest | Design 49 v2 | candidate/generation composition | manifest authority, image Authority | durable published-generation authority |
| manifest entry index | canonical manifest row position | manifest serializer | registration envelope, resolver, image references, Runtime catalog | artifact-local physical locator only |
| selector bytes and language arity | manifest entry | compiler | admission, diagnostics, binding reconciliation | semantic cross-check, not dispatch cache |
| implementation Oop | native envelope only | generated initializer | Runtime anchor table | process-local GC root input |
| thunk, StepMethod, physical code lease | native envelope only | mapper/initializer | Runtime registry | process-local execution capability |
| artifact revision scope and rows | ArtifactSelection | compilation boundary | candidate manifest, artifact admission, generation composition | immutable artifact provenance |
| complete active revision scope and rows | GenerationSelection | activation composition | DispatchGeneration, image, image compilation | immutable generation provenance |

No physical format may duplicate one of these values under a second definition
or derive it from a process-local address.

## 5. Common canonical wire model

### 5.1 Accepted baseline

Every durable 60I format obeys these already accepted constraints:

- it is a finite pointer-free byte sequence;
- integers have explicit fixed widths and little-endian encoding;
- it contains no Oop, native pointer, function pointer, `usize`, Runtime handle,
  registration token, anchor handle, or physical code lease;
- physical section offsets are measured from byte zero of the containing
  canonical value; typed references inside records use section-relative record
  indices where their schema requires an independently indexed target;
- sections are monotonic, non-overlapping, in their one canonical order, and
  validated with checked arithmetic;
- section starts are eight-byte aligned and canonical padding is zero;
- unused reserved fields and padding bits are zero;
- unknown required flags, unknown enum values, trailing bytes, and noncanonical
  ordering fail closed;
- `u32` is used for PCs, slots, logical-value identifiers, section indices,
  record indices, path ordinals, and manifest entry indices;
- `u32::MAX` is the only absent-index sentinel where an optional index is
  permitted;
- `u64` is used for portable implementation identities and bitmap words;
- bitmap slot `n` is bit `n % 64` of word `n / 64`, with unused high bits zero;
- SHA-256 hashes the exact complete canonical bytes, never a native struct,
  decoded object, hex rendering, or host collection; and
- parsers read from byte slices with explicit decoding rather than casting to
  packed or native structs.

Every independently hashed durable stackless value uses this closed-schema
physical shape:

1. one fixed-size common prefix;
2. one fixed-width section-directory entry for every schema-required section;
3. the schema-required sections in their one canonical order; and
4. one final variable-byte arena when the schema contains variable-width
   values.

The directory is a redundant structural index, not a TLV extension mechanism.
Each format version fixes the exact section count, section kinds, and section
order. Missing, additional, unknown, or reordered sections fail closed. The
directory does not permit arbitrary placement: the serializer packs sections
at their earliest aligned canonical offsets, and admission rejects gaps other
than required zero padding, overlaps, storage aliasing, and trailing bytes.

Every directory entry is exactly 32 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 4 | section kind | schema-local nonzero `u32` equal to this entry's required ordinal |
| `0x04` | 4 | record stride | schema-defined `u32`; `1` for the variable-byte arena |
| `0x08` | 4 | record count | `u32` number of records; byte count for the variable-byte arena |
| `0x0c` | 4 | reserved | zero |
| `0x10` | 8 | section offset | `u64` canonical absolute offset from byte zero |
| `0x18` | 8 | section byte length | `u64` checked product of record count and record stride |

Section kinds are local to a domain and schema version rather than entries in
a global registry. Kind zero is invalid. Each schema assigns contiguous kinds
starting at one in its required section order, so directory entry `i` has kind
`i + 1`. A schema table supplies the semantic name and exact stride for each
kind. Shared record subgrammars may still be byte-identical without making
their containing section kinds globally identical.

Every required section remains present when empty. A fixed-record empty section
retains its schema-defined nonzero stride and has zero count and length. An
empty variable-byte arena has stride one and zero count and length. Its offset
is the current canonical packing cursor. Multiple empty sections may therefore
have equal offsets; because they address no bytes, this is not storage aliasing.
Nonempty section ranges may never overlap or alias.

The serializer and admission validator compute section placement identically:

```text
cursor = 64 + checked(section_count * 32)

for entry in required_schema_order:
    expected_offset = align_up(cursor, 8)
    require(entry.offset == expected_offset)
    require(bytes[cursor..expected_offset] are zero)
    require(entry.length == checked(entry.count * entry.stride))
    cursor = checked(entry.offset + entry.length)

require(cursor == total_bytes)
```

This makes stored offsets, lengths, counts, and strides independently useful
for bounded access while requiring all of them to agree with the one canonical
packing. The variable-byte arena consequently has a representational maximum
of `u32::MAX` bytes; §9 will impose a substantially smaller deterministic
compiler and admission ceiling.

Variable values owned in canonical record order do not store starts. Each
schema defines one reference-visitation order by section, record, and field.
The serializer appends every value to its designated arena in that order,
without interning, deduplication, aliasing, terminators, per-value length
prefixes, or internal padding. The owning record stores only the `u32` record
count. Equal values are serialized again.

The decoder derives each start by checked prefix sum, advances by the stored
count, and requires the final cursor to equal the arena directory count. This
rejects gaps, overlaps, alternative storage orders, and unreferenced suffixes
without encoding redundant starts. A zero count denotes an empty value. If a
schema must distinguish absent from present-but-empty, its owning record uses
an explicit presence enum; arena values do not use `u32::MAX`. That sentinel is
reserved for schema-declared optional references to independently indexed
records.

Admission may retain the derived starts in owned immutable process-local
storage for constant-time Runtime access. Such decoded offsets are caches, not
durable identity or authority. No initializer, resolver, image reference, or
generation merge may use a raw arena before the complete value has passed this
scan.

Raw byte values have no common UTF-8 or text interpretation. Their owning field
defines validity. PathOrdinals uses the same concatenation rule with one
little-endian `u32` record per ordinal.

Padding exists only before a physical section whose start requires alignment.
The serializer emits the minimum bytes needed to reach the earliest
eight-byte-aligned start, every such byte is zero, and the hash covers them.
There is no arena-internal or discretionary trailing padding. Alignment before
a final required empty section remains structural padding before that section,
not trailing padding after it.

No common-framed value contains its own digest. Its digest is always:

```text
SHA256(bytes[0..total_bytes])
```

The hash covers the domain, prefix, directory, every section, and every
canonical padding byte. No field is skipped, blanked, zeroed for hashing, or
otherwise excluded. A digest field may name only a fully constructed dependency
that precedes the containing value in the acyclic construction order:

1. the stable source key and lowering version determine the portable
   implementation identity;
2. those compilation inputs determine complete FrameDescriptor bytes;
3. the descriptor SHA-256 hashes those exact complete bytes;
4. complete ArtifactSelection slices determine GenerationSelection bytes and
   their SHA-256;
5. the source map, portable operations, carrier dispositions, descriptor
   hashes, and GenerationSelection digest determine the DispatchGeneration
   authority;
6. that already-computed authority, stable keys, descriptor hashes, selectors,
   and arities determine ResumableManifest bytes and their SHA-256; and
7. completed artifact and image containers may commit those finished child
   digests but never feed a later checksum back into an earlier identity.

Digest placement is therefore fixed as follows:

| Value | Earlier digests carried inside it | External owner of its own digest |
|---|---|---|
| ArtifactSelection | SourceRevision identities | candidate manifest and artifact integrity |
| GenerationSelection | SourceRevision identities | DispatchGeneration construction and retained generation/image provenance |
| FrameDescriptor | none of its own bytes | ResumableManifest row and native registration envelope |
| ResumableManifest | DispatchGeneration authority and descriptor hashes | artifact/generation provenance and image Resumables record |
| native registration envelope | descriptor hash | no instance digest; its declaration is covered by the Runtime ABI hash |
| image Resumables record | authority, complete manifest, and manifest hash | outer image integrity |

SourceRevision and DispatchGeneration digests are earlier semantic identities,
not checksums of the value that carries them. A FrameDescriptor contains no
logical generation authority, so unchanged descriptor bytes may be used under a
later generation. A manifest does carry its generation authority and therefore
changes when that authority changes.

This common framing applies to ArtifactSelection, GenerationSelection,
ResumableManifest, and FrameDescriptor values. It does not replace the native
registration envelope's process-local C ABI or the outer image grammar; an
image `Resumables` record embeds the already-canonical manifest bytes without
reframing them.

The common prefix is exactly 64 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 32 | domain | exact format-and-version domain, including one terminating NUL and then zero padding |
| `0x20` | 4 | schema version | format-defined nonzero `u32` agreeing with the domain |
| `0x24` | 4 | prefix bytes | `u32` value `64` |
| `0x28` | 8 | total bytes | `u64` length of the complete canonical value, equal to the admitted slice length |
| `0x30` | 4 | required flags | format-defined `u32`; every unknown bit fails closed |
| `0x34` | 4 | section count | `u32` equal to the format version's exact required section count |
| `0x38` | 4 | directory-entry bytes | `u32` value `32` |
| `0x3c` | 4 | reserved | zero |

The domain field is a cryptographic type-and-version tag, not a module, IdApi,
or Runtime authority. Its non-padding bytes are a format-defined ASCII constant
of at most 31 bytes including exactly one terminating NUL. For example,
ArtifactSelection v1 uses `birnam.artifact-selection.v1\0`; the remaining three
bytes in the field are zero. Because the domain is part of the canonical value,
hashing the complete value domain-separates otherwise identical payloads from
different formats or schema versions. Admission compares all 32 bytes against
the expected constant before interpreting format-specific sections.

The four common-framed formats use these exact domain fields and numeric schema
versions:

| Format | Non-padding domain bytes | Zero-padding bytes | Schema version |
|---|---|---:|---:|
| ArtifactSelection | `birnam.artifact-selection.v1\0` | 3 | 1 |
| GenerationSelection | `birnam.generation-selection.v1\0` | 1 | 1 |
| ResumableManifest | `birnam.resumable-manifest.v1\0` | 3 | 1 |
| FrameDescriptor | `birnam.frame-descriptor.v2\0` | 5 | 2 |

These constants name physical schemas only. The native registration envelope
has its native ABI version and struct size rather than this durable prefix. The
outer image-v5 grammar supplies the `Resumables` record framing, while the
embedded manifest retains its own complete prefix and domain.

There is no separate common-framing version. Any incompatible framing change
changes canonical bytes and therefore requires a new format schema version and
domain. The numeric schema version deliberately repeats the version carried by
the domain so admission can reject a mismatched decoder or malformed value.

The section directory starts at byte 64 and occupies the checked product of
section count and 32 bytes. It therefore needs no stored offset. The first
section begins at the earliest eight-byte-aligned offset after the directory;
with the accepted prefix and entry widths this is exactly the first byte after
the directory.

Field-specific byte strings retain their semantic owner's byte rules. In
particular, a format does not impose UTF-8 on selectors merely because module
names or text diagnostics use UTF-8 elsewhere.

### 5.2 Locked format-specific decisions

Each schema below now fixes its required section list, allowed required-flag
mask, and format-specific metadata layout. The shared bounds and
compiler/admission distinction are frozen by §9. Golden bytes, the independent
reader, malformed inputs, complete boundary matrix, and construction trace now
prove the accepted layouts; none fills in an unspecified byte-layout choice.

The accepted framing uses fixed-size record-array sections plus at most one
trailing variable-byte arena per format under the length-only concatenation
rule above.

## 6. Shared stable implementation key

60F accepts this semantic key:

```text
source_revision_digest
root_kind = NonMethodProgram | MethodOperation
root_index
lexical_block_path = [u32 child ordinal ...]
implementation_kind = NamedAdapter | BlockBody
```

The physical stable-key record is exactly 48 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 32 | SourceRevision digest | Design 49's raw digest bytes |
| `0x20` | 4 | root kind | `u32`: 1 NonMethodProgram, 2 MethodOperation |
| `0x24` | 4 | root index | `u32` index in the normalized revision inventory |
| `0x28` | 4 | path count | `u32` number of complete lexical-path ordinals |
| `0x2c` | 4 | implementation kind | `u32`: 1 NamedAdapter, 2 BlockBody |

Enum value zero is invalid for root and implementation kinds, and every other
unlisted value fails closed. The stable key contains the corresponding ordinal
sequence derived by prefix-summing path counts in canonical row order. A named
adapter has an empty path. A block body carries its complete lexical path.
Format-specific validation must additionally prove that the root and every path
ordinal name an implementation in the declared SourceRevision.

ArtifactSelection, GenerationSelection, and ResumableManifest embed this record
verbatim and provide a schema-local PathOrdinals section of `u32` records. Paths
are concatenated once per canonical row in row order, without interning,
deduplication, or aliasing. The decoder derives each row's start from the sum of
all preceding path counts and requires complete section consumption. Empty
paths consume no records. Equal nonempty paths are serialized again.

Stable keys compare by SourceRevision digest, root kind, root index, the
dereferenced lexical path lexicographically by unsigned ordinal, and
implementation kind. No format-specific reinterpretation of a field is
permitted. Exact root, path-depth, and ordinal admission bounds remain part of
§9's limit table.

The portable implementation identity used by descriptors, manifests, source
maps, and diagnostics is derived exactly as follows:

```text
identity_digest = SHA256(
    "birnam.portable-implementation.v1\0"
    || stable_key_record[0..48]
    || each lexical path ordinal as little-endian u32
    || lowering_version as little-endian u32
)

portable_implementation_identity =
    little-endian u64 decoded from identity_digest[0..8]
```

`lowering_version` is nonzero. The path occurs once after the stable record even
though the record already carries its count. This is an unambiguous
domain-separated projection of the complete key and lowering version. A `u64`
collision remains valid for distinct complete stable keys and never selects an
entry; the complete key plus descriptor SHA-256 retains authority.

## 7. Format workbooks

Each subsection records accepted semantic content and the exact work still
needed. When complete, each gains a normative byte-offset table, compositional
grammar, validation table, and annotated golden fixture.

### 7.1 `ArtifactSelection v1`

The non-durable Birnam request admitted immediately before materialization is
the exact tagged seven-element Vector fixed by 60F §13.5.2. It is completely
copied before this codec runs; none of its Oops, Vector identity, or iteration
state enters the canonical bytes.

> **Physical layout accepted 2026-08-22.** Golden fixtures remain part of the
> §10 proof work and do not reopen this layout.

Accepted semantics:

- domain `birnam.artifact-selection.v1\0`, distinct from
  GenerationSelection;
- declares the complete sorted SourceRevision scope compiled into one artifact;
- contains exactly one row for every eligible implementation in that scope;
- fills omitted requests with `Synchronous`;
- each row contains the shared stable key and disposition;
- rows sort by revision, root kind, root index, block path, and implementation
  kind; and
- its SHA-256 is committed by candidate manifest v4 and covered by the artifact
  checksum.

The artifact exports the canonical static byte array as
`__birnam_artifact_selection_v1` and a native `u64` length as
`__birnam_artifact_selection_v1_size`. The length is copied with `memcpy` and
range-checked before the canonical bytes are read; the native length itself is
not part of the little-endian canonical value.

ArtifactSelection v1 has required flags zero and exactly three sections:

| Kind | Section | Stride | Canonical contents |
|---:|---|---:|---|
| 1 | CoveredRevisions | 32 | unique raw SourceRevision digests in lexicographic byte order |
| 2 | SelectionRows | 52 | shared stable-key record followed by disposition |
| 3 | PathOrdinals | 4 | little-endian `u32` ordinals concatenated once per row in row order |

It has no format-specific metadata section: the common prefix and directory
already carry every required scalar count and length. Any nonzero required flag
fails closed.

Let `R` be the CoveredRevisions count, `I` the SelectionRows count, and `P` the
PathOrdinals count. The complete physical placement is:

| Absolute offset or formula | Bytes | Contents |
|---|---:|---|
| `0x00` | 64 | §5 common prefix |
| `0x40` | 32 | directory entry 1: CoveredRevisions |
| `0x60` | 32 | directory entry 2: SelectionRows |
| `0x80` | 32 | directory entry 3: PathOrdinals |
| `0xa0` | `32 * R` | CoveredRevisions records |
| `0xa0 + 32 * R` | `52 * I` | SelectionRows records |
| `align_up(0xa0 + 32 * R + 52 * I, 8)` | `4 * P` | PathOrdinals records |

The only possible section padding is therefore four zero bytes between
SelectionRows and PathOrdinals when `I` is odd. `total_bytes` is exactly the
PathOrdinals offset plus `4 * P`; the value has no trailing alignment padding.
All arithmetic is checked before conversion to a host size.

The canonical empty scope has `R = I = P = 0`, `total_bytes = 160`, and all
three section offsets equal to `0xa0`. It is valid and means that the artifact
compiled no indexed SourceRevision scope. An empty scope may contain no rows or
path ordinals. A nonempty covered-revision scope may also have zero rows when
its normalized inventory contains no eligible implementation; this is distinct
from an empty scope and preserves complete artifact provenance.

Each SelectionRows record embeds §6's stable-key record at offset `0x00`, then
has this four-byte suffix:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x30` | 4 | disposition | `u32`: 0 Synchronous, 1 ResumableRequired |

Every row digest must appear in CoveredRevisions. The separate revision section
is authoritative for complete artifact scope and may therefore contain a
revision with no eligible implementation row. Rows use §6's semantic comparison
order; duplicate stable keys fail closed. Path storage follows §6 and is
validated against the PathOrdinals directory count.

Admission validates this format in the following format-specific order after
§8's common structural checks:

| Check | Required result |
|---|---|
| Prefix | exact ArtifactSelection domain, schema version 1, required flags 0, and section count 3 |
| Directory | kinds `1, 2, 3`; strides `32, 52, 4`; exact offsets and lengths from the formulas above |
| Limits | `R`, `I`, `P`, fixed-record aggregate, and `total_bytes` satisfy §9 before proportional allocation |
| Revision scope | CoveredRevisions is strictly increasing by raw digest bytes and contains no duplicate |
| Row fields | every root kind, root index, implementation kind, path count, and disposition is valid; `NonMethodProgram.root_index` is zero |
| Row shape | `NamedAdapter` uses `MethodOperation` and an empty path; `BlockBody` uses a nonempty complete lexical path |
| Row order | rows are strictly increasing under §6's stable-key comparison and contain no duplicate key |
| Path storage | checked prefix sums consume exactly `P` ordinals; no row refers beyond the section and no suffix remains |
| Scope membership | every row's SourceRevision digest occurs in CoveredRevisions |
| Inventory completeness | every eligible implementation in every covered normalized revision has exactly one row, and no row names an ineligible or absent implementation |
| Empty scope | `R = 0` requires `I = P = 0`; nonzero `R` with `I = P = 0` is valid only when the normalized eligible inventory is empty |
| External agreement | candidate manifest v4 carries the same covered-revision set and ArtifactSelection SHA-256; the mapped export is byte-identical to the admitted value |

These checks intentionally use complete stable keys rather than a compact
CoveredRevisions index. Repeating the 32-byte digest keeps each row
self-contained, preserves §6's shared key grammar verbatim, and lets generation
composition merge canonical rows without a schema-local dereference rule.

ArtifactSelection does not embed its own digest. Its SHA-256 covers the complete
canonical value and is committed externally by candidate manifest v4 and the
artifact checksum, avoiding self-referential hashing.

Candidate manifest v4 replaces v3's separator-sensitive lists with counted and
length-framed fields. The exact grammar accepted 2026-08-23 is:

```text
v4|runtime|N:name|source_digest|non_method_digest|method_digest|revision_digest|selection_sha256|R:<64-hex revision>*|D:<N:dependency>*|O:<N:target><N:selector><I|R><arity:>*
```

`N`, `R`, `D`, `O`, and `arity` are canonical unsigned decimal integers
terminated by `:`; leading zeroes are forbidden except for zero itself. `N:`
frames exactly that many following bytes. `R:` is followed by exactly `R`
concatenated 64-byte lowercase hexadecimal revision digests. `D:` is followed
by exactly `D` length-framed dependency tokens. `O:` is followed by exactly
`O` operation records; each record contains a length-framed target, a
length-framed selector, the one-byte action `I` or `R`, and a colon-terminated
arity. All five digest fields and every revision are lowercase hexadecimal.
Names, dependencies, targets, and selectors are nonempty bytes at or above
`0x21`; their length frames allow selector bytes such as `|`, `,`, and `;`
without ambiguity. No trailing byte is accepted. This text manifest is a
compatibility carrier rather than another common section-directory value: its
canonical selection authority remains the exported ArtifactSelection bytes,
whose digest and CoveredRevisions section must agree exactly with the v4
fields. Candidate manifest v3 remains inspectable only as pre-v4 compatibility
input and cannot be emitted by current compilation.

Selection-bearing candidate AOT output uses this same candidate-v4 envelope and
the same ArtifactSelection and ResumableManifest exports. It introduces no
second eager-load envelope or byte grammar: the prebuilt artifact is admitted
through CompiledUnit open/map/activate against its recorded predecessor.

Committed golden fixtures:

- [`abi/fixtures/stackless-s2`](../../tests/fixtures/stackless-s2/README.md)
  freezes the empty-scope, all-synchronous, mixed-disposition/nested-block, and
  malformed-scope bytes, exact lengths, SHA-256 values, annotated hexadecimal
  rendering, normalized inventory, and single-fault rejection stages required
  by §10.

Committed proof:

- the committed dependency-free reader independently accepts all three valid
  ArtifactSelection fixtures, rejects the malformed scope and nine named
  single-fault mutations at their required stages, and rejects truncation at
  every byte boundary. It also checks all selection ceilings, aggregate limits,
  checked `u64` addition/multiplication, and section-end overflow. Production
  decoder property fuzzing remains implementation evidence, not an open format
  decision.

### 7.2 `GenerationSelection v1`

> **Physical layout accepted 2026-08-22.** Golden fixtures remain part of the
> §10 proof work and do not reopen this layout.

Accepted semantics:

- domain `birnam.generation-selection.v1\0`, distinct from
  ArtifactSelection;
- declares the complete active DispatchGeneration SourceRevision scope;
- is the canonical union of artifact slices whose scopes partition that active
  set;
- depends only on the complete revision inventory and rows, not code-unit
  partitioning or native collection iteration;
- its SHA-256 enters `birnam.dispatch-base.v2`; and
- image v5 retains the exact accepted bytes.

GenerationSelection has required flags zero and repeats ArtifactSelection's
exact three-section inventory, record layouts, enum values, canonical ordering,
path concatenation, and validation rules. It has no format-specific metadata
section and does not embed its own digest. Only its domain and scope validation
differ: its CoveredRevisions section must equal the complete active source map,
and its rows are the partition-independent canonical union of the admitted
artifact slices.

Let `G` be the CoveredRevisions count, `I` the SelectionRows count, and `P` the
PathOrdinals count. Its complete placement is deliberately byte-isomorphic to
§7.1:

| Absolute offset or formula | Bytes | Contents |
|---|---:|---|
| `0x00` | 64 | §5 common prefix with the GenerationSelection domain |
| `0x40` | 32 | directory entry 1: CoveredRevisions |
| `0x60` | 32 | directory entry 2: SelectionRows |
| `0x80` | 32 | directory entry 3: PathOrdinals |
| `0xa0` | `32 * G` | CoveredRevisions records |
| `0xa0 + 32 * G` | `52 * I` | SelectionRows records |
| `align_up(0xa0 + 32 * G + 52 * I, 8)` | `4 * P` | PathOrdinals records |

As in ArtifactSelection, an odd `I` introduces exactly four zero padding bytes
before PathOrdinals, and `total_bytes` ends immediately after the final ordinal.
The canonical empty generation has `G = I = P = 0`, `total_bytes = 160`, and
all three section offsets equal to `0xa0`. It represents a DispatchGeneration
whose complete active source map is empty.

GenerationSelection carries no artifact identity, ArtifactSelection digest,
slice boundary, native path, mapping identity, or code-unit ordinal. Those
belong to the generation's separately retained immutable provenance. Including
one here would make the DispatchGeneration base digest change when identical
source revisions and dispositions were merely repartitioned into different
artifacts.

Admission and composition validate this format as follows after §8's common
structural checks:

| Check | Required result |
|---|---|
| Prefix | exact GenerationSelection domain, schema version 1, required flags 0, and section count 3 |
| Directory | kinds `1, 2, 3`; strides `32, 52, 4`; exact offsets and lengths from the formulas above |
| Limits | `G`, `I`, `P`, fixed-record aggregate, and `total_bytes` satisfy §9 before proportional allocation |
| Revision scope | CoveredRevisions is strictly increasing, duplicate-free, and byte-equal to the revision-digest set of the complete active source map |
| Input admission | every contributing ArtifactSelection has independently passed §7.1 structural, inventory, digest, and artifact-agreement checks |
| Scope partition | every active revision belongs to exactly one nonempty artifact scope; scopes neither overlap nor omit an active revision |
| Empty artifact scope | an empty ArtifactSelection owns no revision and contributes no row or ordinal; it is neutral to the canonical merge and cannot satisfy missing scope ownership |
| Canonical merge | one linear merge of the admitted sorted revision and row streams produces the sections; duplicate revision ownership or stable keys fail |
| Rows and paths | all §7.1 row-field, row-shape, ordering, path-consumption, scope-membership, and normalized-inventory checks hold over the complete generation |
| Empty generation | an empty active source map requires `G = I = P = 0`; a nonempty active map may have zero rows only when its complete normalized eligible inventory is empty |
| Partition independence | any valid slice partition with the same complete revisions and rows serializes byte-identical GenerationSelection bytes and SHA-256 |
| External agreement | the retained DispatchGeneration value, `birnam.dispatch-base.v2` input, image-v5 projection, and mapped/rebuilt recovery all use the exact admitted bytes and SHA-256 |

The canonical merge serializes a new common-framed value; it never concatenates
ArtifactSelection byte strings or copies their domain prefixes and directories.
The Runtime may retain slice boundaries for activation, rollback, mapping, and
diagnostics, but those boundaries have no effect on these bytes.

Committed golden fixtures:

- [`abi/fixtures/stackless-s2`](../../tests/fixtures/stackless-s2/README.md)
  freezes one-revision and two-revision GenerationSelection values plus the
  ArtifactSelection inputs for one-slice, multi-slice, reversed-order,
  empty-neutral, overlapping, and missing-revision composition cases. The
  independent composer requires every valid repartitioning to produce the same
  exact bytes and SHA-256, rejects overlap or omission before output, checks the
  odd-row zero padding, and rejects truncation at every output byte boundary.

Committed proof:

- the shared selection overflow, complete ceiling, composition, padding, and
  truncation matrix is committed and independently checked. Production decoder
  property fuzzing remains an implementation acceptance requirement.

### 7.3 `ResumableManifest v1`

> **Physical layout accepted 2026-08-22.** Golden fixtures remain part of the
> §10 proof work and do not reopen this layout.

Accepted semantics:

- one manifest-wide DispatchGeneration authority digest;
- one canonically sorted row per named adapter or block body;
- each row contains kind, portable implementation identity, descriptor
  SHA-256, selector bytes, selector-determined language arity, and the complete
  shared stable source key;
- the durable entry key is `(authority, stable source key, descriptor SHA-256)`;
- the canonical row index is the only physical resolver and registration
  locator; and
- duplicate durable keys, duplicate source keys within a kind, malformed arity,
  noncanonical order, and descriptor hashes not owned by the unit fail closed.

The artifact exports the canonical static byte array as
`__birnam_resumable_manifest_v1` and a native `u64` length as
`__birnam_resumable_manifest_v1_size`, with the same copied-and-range-checked
length rule as ArtifactSelection. A unit containing block entries exports:

```c
void *__birnam_resolve_block_vtable_v1(uint32_t manifest_entry_index);
```

The resolver is private artifact ABI, not an IdApi or language export.

StepMethod, entry-thunk, Prepare, and Abort symbols have internal linkage in
each unit. Their numeric suffix is manifest-local and therefore must not enter
the process-wide dynamic symbol namespace, where a previously mapped artifact
could interpose the same suffix. The exported private artifact ABI is limited
to the manifest bytes and size, initializer-context brackets, unit initializer,
and the block-vtable resolver when block entries exist.

An artifact with at least one manifest entry also exports the exact
initializer-context brackets:

```c
uint32_t __birnam_resumable_context_begin_v1(
    uint64_t generation_identity,
    uint64_t generation_lease);
uint32_t __birnam_resumable_context_end_v1(void);
```

These are artifact-private ABI, not Runtime ABI, IdApi, language exports, or
canonical artifact inputs. Each mapped artifact owns one artifact-local static
context containing the two `u64` values and a failure bit. The owner-thread
quiescence guard serializes every Begin/End bracket; dynamic TLS is deliberately
not used because TinyCC cannot correctly relocate it in a dlopened Mach-O image.
An inactive context
has both values zero. Begin rejects a zero operand or an already-active context
without changing it; otherwise it installs both values and clears the failure
bit. Generated Prepare reads this context, rejects if it is inactive, and sets
the failure bit if registration rejects. Abort likewise sets the bit if its
compensation rejects. End answers rejection when the context was inactive or
its failure bit was set, and clears all three fields on every call. The loader
must call End before leaving the same owner-thread quiescent staging operation
after every returning initializer path.

The exact-floor loader opens the Runtime staged-unit context first, then the
artifact context, and keeps both active across the frozen
`__birnam_unit_init(void)` plus Method-shaped `__ev` initialization sequence.
It closes the artifact context before asking the Runtime to commit. A rejected
program result, rejected context End, or any earlier post-stage admission
failure calls the loader-private
`birnam_id_cancel_resumable_unit_v1(void)` compensation seam; only an accepted
result and accepted End may call `birnam_id_finish_resumable_unit_v1(void)`.
Cancel and Finish both close the staged context, but Cancel unconditionally
uses the rollback journal even when every manifest row has been claimed. These
three exact-floor seams remain outside Runtime ABI v3 because generated code
imports none of them.

Owner-thread quiescence prevents concurrent loader operations from exchanging
physical capabilities, while artifact-local storage prevents two
simultaneously mapped units from sharing a context cell. Nesting initialization
of the same mapped artifact is rejected rather than stacked. The
physical identity and lease remain process-local and never enter the manifest,
descriptor, selection, image, or generation digest.

ResumableManifest v1 has required flags zero and exactly four sections:

| Kind | Section | Stride | Canonical contents |
|---:|---|---:|---|
| 1 | Authority | 32 | exactly one raw DispatchGeneration authority digest |
| 2 | ManifestEntries | 96 | one fixed record per resumable implementation |
| 3 | PathOrdinals | 4 | little-endian `u32` ordinals concatenated once per row in row order |
| 4 | SelectorBytes | 1 | exact selector bytes concatenated once per row in row order |

It has no additional metadata section. Any nonzero required flag fails closed.
Authority has exactly one record, including when ManifestEntries is empty. It
uses the raw 32-byte digest form required by Design 49 for hashing. At an
existing image boundary that carries the authority as 64 lowercase hexadecimal
bytes, preflight strictly validates and decodes that spelling to 32 bytes before
comparison; malformed, uppercase, or noncanonical text fails rather than being
normalized.

Let `N` be the ManifestEntries count, `P` the PathOrdinals count, and `S` the
SelectorBytes count. The complete physical placement is:

| Absolute offset or formula | Bytes | Contents |
|---|---:|---|
| `0x00` | 64 | §5 common prefix |
| `0x40` | 32 | directory entry 1: Authority |
| `0x60` | 32 | directory entry 2: ManifestEntries |
| `0x80` | 32 | directory entry 3: PathOrdinals |
| `0xa0` | 32 | directory entry 4: SelectorBytes |
| `0xc0` | 32 | the one Authority record |
| `0xe0` | `96 * N` | ManifestEntries records |
| `0xe0 + 96 * N` | `4 * P` | PathOrdinals records |
| `align_up(0xe0 + 96 * N + 4 * P, 8)` | `S` | SelectorBytes arena |

Because both Authority and ManifestEntries end on eight-byte boundaries, the
only possible section padding is four zero bytes before SelectorBytes when `P`
is odd. `total_bytes` is exactly the SelectorBytes offset plus `S`; there is no
trailing alignment padding. All placement and aggregate arithmetic is checked.

The canonical empty manifest has `N = P = S = 0`, `total_bytes = 224`, Authority
at `0xc0`, and the other three section offsets equal to `0xe0`. It remains
qualified by one non-optional DispatchGeneration authority and is valid exactly
when the matching ArtifactSelection contains no `ResumableRequired` row.

Each ManifestEntries record is exactly 96 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 48 | stable key | §6's byte-identical shared stable-key record |
| `0x30` | 8 | portable implementation identity | little-endian `u64`; diagnostic metadata, never a selection key |
| `0x38` | 32 | descriptor SHA-256 | raw hash of the exact complete descriptor bytes |
| `0x58` | 4 | selector length | `u32` byte count |
| `0x5c` | 4 | language arity | selector-determined `u32` argument count |

The stable key's implementation-kind field is the manifest entry kind; it is
not repeated. The manifest index is the record's canonical zero-based position
in ManifestEntries; it is likewise not stored inside the row.

Rows sort by implementation kind, SourceRevision digest, root kind, root index,
the dereferenced lexical path, and portable implementation identity. Duplicate
stable source keys within a kind fail even when their implementation identities
or descriptor hashes differ. Equal `u64` implementation identities on distinct
stable keys are valid and never make a row ambiguous.

After sorting, PathOrdinals uses §6's non-interned row-order concatenation.
SelectorBytes is built the same way: each row's exact selector bytes are
appended without a terminator, interning, deduplication, or aliasing, and the row
stores only their length. The decoder derives starts by checked prefix sum and
requires complete section consumption. Equal selectors are serialized again.
Selector bytes are not required to be UTF-8. Admission validates every derived
slice as the selector belonging to the stable source binding, recomputes its
language arity, and cross-checks the descriptor's driver argument contract.

For every ResumableRequired ArtifactSelection row the artifact contains exactly
one matching manifest entry; a Synchronous row contains none. The manifest does
not repeat the covered-revision inventory or ArtifactSelection digest because
the selection owns complete compilation scope and candidate admission
cross-checks the two formats. It also does not embed its own digest. Its
SHA-256 covers the complete canonical manifest and is committed externally by
the artifact, generation, and image records.

Admission validates this format in the following format-specific order after
§8's common structural checks:

| Check | Required result |
|---|---|
| Prefix | exact ResumableManifest domain, schema version 1, required flags 0, and section count 4 |
| Directory | kinds `1, 2, 3, 4`; strides `32, 96, 4, 1`; exact offsets and lengths from the formulas above |
| Limits | `N`, `P`, `S`, fixed-record aggregate, selector limits, descriptor aggregate, and `total_bytes` satisfy §9 before proportional allocation |
| Authority | Authority has count 1 and its raw digest equals the staged unit's already-computed `birnam.dispatch-base.v2` authority |
| Row fields | each stable key, portable identity, descriptor hash, selector length, and language arity is structurally valid; named and block row shapes obey §6 |
| Row order | rows are strictly ordered by implementation kind, stable source key, and portable implementation identity; duplicate stable source keys within a kind and duplicate durable entry keys fail |
| Portable identity | the `u64` agrees with the stable key and accepted lowering-version inputs; equal values on distinct stable keys remain valid and never select a row |
| Path storage | checked prefix sums consume exactly `P` ordinals, with no out-of-range row and no unreferenced suffix |
| Selector storage | checked prefix sums consume exactly `S` bytes; every derived slice is the exact selector for its stable source binding and no suffix remains |
| Arity | canonical selector interpretation reproduces `language arity`, which agrees with the descriptor's driver argument contract |
| Descriptor ownership | each raw hash names exact admitted descriptor-v2 bytes owned by this unit; their hash, portable identity, entry kind, and driver contract agree with the row |
| Selection agreement | every `ResumableRequired` ArtifactSelection row has exactly one entry and every `Synchronous` row has none; no manifest entry lies outside that artifact scope |
| Index assignment | a manifest index is exactly the row's zero-based canonical position and is absent from the row bytes |
| Block resolver | every embedded or requested index is bounds-checked; only a `BlockBody` row may resolve, and the returned vtable agrees with the staged unit and registered row |
| External agreement | the copied artifact export, staged-unit catalog, retained generation manifest, and image-v5 record use byte-identical manifest bytes and SHA-256 |

Named and block entries deliberately share one ManifestEntries section. The
implementation kind already carried by the stable key provides the discriminator,
while one global canonical row position remains sufficient for registration and
the block-vtable resolver. The manifest does not store its row index, descriptor
offset, selector offset, ArtifactSelection digest, or covered-revision scope.

Committed golden fixtures:

- [`abi/fixtures/stackless-s2`](../../tests/fixtures/stackless-s2/README.md)
  freezes empty, named-only, block-only, mixed, and non-UTF-8 selector manifests
  whose rows commit real independently checked descriptor hashes. It also
  freezes matching ArtifactSelection inputs and proves authority, identity,
  arity, descriptor ownership, selection agreement, seven disagreement
  mutations, exact hashes, and truncation at every boundary. The selector byte
  `ff` is canonically arity one and agrees with the Call descriptor.

Committed collision proof:

- portable-ID collision safety uses a model-level injected projection collision
  rather than an infeasible serialized SHA-256 witness. The independent checker
  proves two distinct complete stable keys remain distinct under equal `u64`
  projections and demonstrates that projection-keyed lookup would be invalid.
  A committed forged-ID binary separately replaces one row's recomputed value
  with another row's value and must reject at portable identity. This closes
  the safety property without weakening exact identity recomputation.

### 7.4 Native registration envelope v1

> **Physical ABI accepted 2026-08-22.** The declaration, operation contract,
> and supported-target layout below are normative. Golden ABI vectors remain
> part of §10 proof work and do not reopen this layout.

This is a process-local C ABI, not a durable little-endian value. It may contain
native pointers and an Oop, but the hashed Runtime ABI must cover its exact
declaration and platform layout.

Accepted semantic fields, in logical order:

- carrier version and struct size;
- `Prepare` or `Abort`, flags, and zero reserved fields;
- canonical manifest entry index;
- registration token;
- borrowed descriptor pointer and length;
- entry-thunk and StepMethod function pointers;
- optional named-adapter implementation Oop;
- physical CodeGeneration identity and lease;
- portable implementation identity;
- selector-determined language arity and descriptor driver argument count; and
- complete descriptor SHA-256.

The two operations close the registration/publication visibility gap. Registering
after ordinary binding installation would make an entry thunk reachable before
the Runtime owns its descriptor, StepMethod, implementation anchor, and physical
code lease. Registering irreversibly before installation would leak that owned
state when the binding is rejected. The required lifecycle is therefore:

```text
Prepare -> staged unpublished entry -> binding publication consumes it
                                  \-> rejected installation calls Abort
```

Prepare validates the exact row in the active staged-unit manifest, validates
and owns the descriptor, acquires the Runtime's code lease, installs the named
implementation anchor when required, and returns an opaque registration token.
It does not make the entry independently reachable. Abort requires both the
same manifest entry index and that token, releases only the still-unpublished
matching preparation, and cannot remove a consumed, replaced, or unrelated
entry. This token check makes stale and reentrant compensation fail closed.

There is no envelope Commit operation. Successful ordinary direct-binding
publication consumes a named staged entry through Runtime binding accounting;
unbound block entries remain retained by the staged manifest. The loader's
final staged-unit transaction verifies the complete manifest-indexed set and
atomically associates it with the DispatchGeneration. Unit rollback remains a
second all-or-nothing cleanup boundary, but it does not replace row-local Abort:
generated initialization may reject a binding before the unit transaction ends,
and that rejection must promptly and unambiguously release its row's ownership.

Prepare and Abort are private native resource-lifecycle operations. They add no
Birnam transaction semantics, IdApi field, relational operation, durable token,
or language-visible state.

The normative C declaration is:

```c
#include <stddef.h>
#include <stdint.h>

typedef void *BirnamOopV1;
typedef BirnamOopV1 (*BirnamMethodV1)(
    BirnamOopV1 receiver,
    BirnamOopV1 const *arguments,
    size_t argument_count);
typedef BirnamMethodV1 BirnamStepMethodV1;

enum {
    BIRNAM_REGISTRATION_CARRIER_VERSION_V1 = 1,
    BIRNAM_REGISTRATION_PREPARE_V1 = 1,
    BIRNAM_REGISTRATION_ABORT_V1 = 2,
    BIRNAM_REGISTRATION_OK_V1 = 0,
    BIRNAM_REGISTRATION_REJECTED_V1 = 1,
};

typedef struct BirnamResumableRegistrationV1 {
    uint32_t carrier_version;
    uint32_t struct_size;
    uint32_t operation;
    uint32_t flags;
    uint32_t manifest_entry_index;
    uint32_t reserved0;
    uint64_t registration_token;
    uint8_t const *descriptor_bytes;
    uint64_t descriptor_length;
    BirnamMethodV1 entry_thunk;
    BirnamStepMethodV1 step_method;
    BirnamOopV1 implementation_oop;
    uint64_t code_generation_identity;
    uint64_t code_generation_lease;
    uint64_t portable_implementation_identity;
    uint32_t language_arity;
    uint32_t driver_argument_count;
    uint8_t descriptor_sha256[32];
} BirnamResumableRegistrationV1;

uint32_t birnam_register_resumable_v1(
    BirnamResumableRegistrationV1 *envelope);
```

`BirnamStepMethodV1` is a semantically distinct name for 60C's Method-shaped
StepMethod, not a different calling convention. The Rust mirror uses `#[repr(C)]`,
`*const u8`, `Oop`, and nullable `Option<Method>` function-pointer fields; it
does not cast this record from arbitrary bytes or model a function pointer as a
data pointer.

The only supported S2 native target is the current `aarch64-apple-darwin`
little-endian ABI. Its exact layout is:

| Offset | Width | Field | ABI kind |
|---:|---:|---|---|
| `0x00` | 4 | `carrier_version` | `uint32_t` |
| `0x04` | 4 | `struct_size` | `uint32_t` |
| `0x08` | 4 | `operation` | `uint32_t` enum |
| `0x0c` | 4 | `flags` | zero `uint32_t` |
| `0x10` | 4 | `manifest_entry_index` | `uint32_t` |
| `0x14` | 4 | `reserved0` | zero `uint32_t` |
| `0x18` | 8 | `registration_token` | in/out opaque `uint64_t` |
| `0x20` | 8 | `descriptor_bytes` | borrowed data pointer |
| `0x28` | 8 | `descriptor_length` | `uint64_t` byte count |
| `0x30` | 8 | `entry_thunk` | Method function pointer |
| `0x38` | 8 | `step_method` | StepMethod function pointer |
| `0x40` | 8 | `implementation_oop` | optional Oop data pointer |
| `0x48` | 8 | `code_generation_identity` | opaque `uint64_t` identity |
| `0x50` | 8 | `code_generation_lease` | opaque generational `uint64_t` capability |
| `0x58` | 8 | `portable_implementation_identity` | `uint64_t` metadata |
| `0x60` | 4 | `language_arity` | `uint32_t` |
| `0x64` | 4 | `driver_argument_count` | `uint32_t` |
| `0x68` | 32 | `descriptor_sha256` | raw bytes |

The structure has size 136 (`0x88`) and alignment 8 with no implicit interior
or trailing padding. The C producer and Rust consumer both compile assertions
equivalent to:

```c
_Static_assert(sizeof(void *) == 8, "Birnam v1 Oop width");
_Static_assert(sizeof(size_t) == 8, "Birnam v1 size_t width");
_Static_assert(sizeof(BirnamMethodV1) == 8, "Birnam v1 Method width");
_Static_assert(_Alignof(BirnamResumableRegistrationV1) == 8,
               "Birnam registration alignment");
_Static_assert(sizeof(BirnamResumableRegistrationV1) == 136,
               "Birnam registration size");
_Static_assert(offsetof(BirnamResumableRegistrationV1, carrier_version) == 0x00,
               "carrier_version offset");
_Static_assert(offsetof(BirnamResumableRegistrationV1, registration_token) == 0x18,
               "registration_token offset");
_Static_assert(offsetof(BirnamResumableRegistrationV1, descriptor_bytes) == 0x20,
               "descriptor_bytes offset");
_Static_assert(offsetof(BirnamResumableRegistrationV1, entry_thunk) == 0x30,
               "entry_thunk offset");
_Static_assert(offsetof(BirnamResumableRegistrationV1, implementation_oop) == 0x40,
               "implementation_oop offset");
_Static_assert(offsetof(BirnamResumableRegistrationV1, code_generation_identity) == 0x48,
               "code_generation_identity offset");
_Static_assert(offsetof(BirnamResumableRegistrationV1, language_arity) == 0x60,
               "language_arity offset");
_Static_assert(offsetof(BirnamResumableRegistrationV1, descriptor_sha256) == 0x68,
               "descriptor_sha256 offset");
```

The implementation asserts every intervening field offset as well; the reduced
listing above is explanatory, not permission to omit those assertions. The
target ABI manifest separately asserts little-endian AArch64, Darwin, Mach-O,
64-bit data and function pointers, and the exact symbol signature. A port must
add and hash its complete layout fixture before it becomes supported.

Version 1 requires `carrier_version == 1` and `struct_size == 136` exactly. The
sized field detects mismatch; it is not permission to append unvalidated fields
under version 1. Any incompatible growth receives a new carrier version and
Runtime ABI hash.

Operation-specific values are:

| Field | Prepare | Abort |
|---|---|---|
| `operation` | `BIRNAM_REGISTRATION_PREPARE_V1` | `BIRNAM_REGISTRATION_ABORT_V1` |
| `flags`, `reserved0` | zero | zero |
| `manifest_entry_index` | in range for the active staged manifest | same exact index used by Prepare |
| `registration_token` | zero on entry; replaced with a nonzero token on success | matching nonzero token on entry; cleared to zero on success |
| `descriptor_bytes` | nonnull, correctly aligned for bytes, and readable for `descriptor_length` | null |
| `descriptor_length` | exact nonzero descriptor-v2 byte length within §9 limits | zero |
| `entry_thunk`, `step_method` | nonnull, distinct, and equal to the staged row's admitted symbols | null |
| `implementation_oop` | nonnull for `NamedAdapter`; null for `BlockBody` | null |
| `code_generation_identity` | nonzero and equal to the active staged unit | zero |
| `code_generation_lease` | nonzero valid capability for that exact identity and staged unit | zero |
| `portable_implementation_identity` | exact manifest and descriptor value; zero is not a sentinel | zero |
| `language_arity` | exact manifest/selector value within §9 | zero |
| `driver_argument_count` | exact descriptor value, 0 or 1 | zero |
| `descriptor_sha256` | exact hash of `descriptor_bytes[0..descriptor_length]` | 32 zero bytes |

The envelope memory is caller-owned and writable only because
`registration_token` is an in/out field. On success the shim changes that field
and no other. On any rejection it returns a nonzero value and leaves the entire
envelope unchanged; callers treat every nonzero value, including a future or
unknown value, as rejection. Stable detailed diagnostics belong to the staged-
unit context rather than this narrow C result code.

For Prepare, descriptor bytes, function pointers, the implementation Oop, and
the code-generation capability are borrowed for the call only. The caller keeps
the implementation Oop live for that duration. Before returning success, the
Runtime validates and owns decoded descriptor storage, anchors the named
implementation when present, copies the callable addresses and identities, and
acquires an independent physical-code lease from the supplied capability. It
retains no pointer to the envelope or descriptor export. Failure acquires no
lasting lease or anchor and publishes nothing.

The `uint64_t` code-generation lease is a Runtime/repository-issued generational
capability, not a pointer, durable identity, refcount, or ownership transfer. It
must resolve to the same physical generation as `code_generation_identity` and
the active staged-unit context. This permits stale-capability validation without
exposing a Rust allocation or repository-owned pointer through generated C.

Abort consults only its header, operation, manifest index, and token after
proving every Prepare-only field is zero or null. The pair `(manifest entry
index, registration token)` must name one still-unpublished preparation in the
same staged-unit context. A wrong index, stale or foreign token, already-consumed
entry, duplicate Abort, missing Runtime, or missing staged context rejects
without clearing the token or releasing any other entry. Successful Abort
releases that one preparation and clears the caller's token. The shim never
mutates `manifest_entry_index`, so the caller's unchanged field is the index
echo used by generated compensation logic.

The Runtime ABI hash does not hash compiler-dependent C source text. Its
registration-envelope input is the canonical tuple:

```text
domain = "birnam.registration-envelope.v1\0"
target = aarch64-apple-darwin, Mach-O, little-endian
symbol = birnam_register_resumable_v1
signature = u32(BirnamResumableRegistrationV1 *)
struct size = 136
struct alignment = 8
ordered fields = name, offset, width, alignment, ABI kind, operation nullability
constants = carrier version, operation values, status values
```

Runtime ABI v3 serializes that tuple exactly as:

```text
RegistrationAbiTupleV1 :=
    domain bytes including terminal NUL
    lp_ascii(target)
    lp_ascii(symbol)
    lp_ascii(signature)
    struct_size:u32-le
    struct_alignment:u32-le
    field_count:u32-le
    field[field_count]
    constant_count:u32-le
    constant[constant_count]

field :=
    lp_ascii(name)
    offset:u32-le
    width:u32-le
    alignment:u32-le
    lp_ascii(abi_kind)
    lp_ascii(operation_nullability)

constant := lp_ascii(name) || value:u32-le
lp_ascii(value) := byte_length:u32-le || exact ASCII bytes
```

There is no padding, terminator, or normalization inside a length-prefixed
string. The target spelling is
`aarch64-apple-darwin|Mach-O|little-endian`; symbol and signature spellings are
exactly those shown above. ABI-kind spellings are `u32`, `u64`,
`pointer<const-u8>`, `function-pointer<BirnamMethodV1>`,
`function-pointer<BirnamStepMethodV1>`, `BirnamOopV1`, and `bytes[32]` as
appropriate to the ordered declaration. Non-pointer fields use
`not-applicable`; descriptor and function pointers use
`prepare-nonnull-abort-null`; the implementation Oop uses
`prepare-named-nonnull-block-null-abort-null`. Constant names are the five exact
`BIRNAM_REGISTRATION_*_V1` C identifiers in declaration order.

Runtime ABI v3 preserves the existing FNV-1a-64 stream: starting with offset
basis `0xcbf29ce484222325`, it hashes each byte of the existing ordered shim-name
list without separators, then the appended name
`birnam_register_resumable_v1`, then every tuple byte, then version bytes
`03 00 00 00`. The prime remains `0x100000001b3` with wrapping `u64`
multiplication. The frozen tuple is 1,539 bytes, SHA-256
`d180f7b2aa164885a63f1cf2347a2a3802bd06eaf42c3f919fbf6f2f16ddc047`.
The resulting Runtime ABI v3 hash is `0x6c3b67f0ef1614b3`, written into an
image header as little-endian bytes `b3 14 16 ef f0 67 3b 6c`.

The exact legacy name prefix, before the appended registration symbol, is:

```text
birnam_send, birnam_intern, birnam_mkSmallInt, birnam_smallIntValue,
birnam_mkFloat, birnam_floatValue, birnam_mkString, birnam_nil,
birnam_true, birnam_false, birnam_objectVt, birnam_proto, birnam_method,
birnam_alloc, birnam_cstr, birnam_free_cstr, birnam_layoutAllOops,
birnam_global, birnam_candidate_validate_binding
```

The checked-in ABI fixture contains the exact C declaration, exhaustive layout
assertions, and three 136-byte native vectors: named Prepare, block Prepare, and
Abort. The independent checker covers null envelope, every truncation/size
mismatch, version, operation, flags, reserved data, index, token, pointer,
length, symbol nullability and distinction, implementation nullability,
identity/lease, arity, driver-count, and hash failures. It also proves the
success contract changes only the token field. Wrong-target and exported-symbol
signature checks belong to the target ABI manifest rather than an instance
envelope. Those vectors provide evidence for this accepted declaration; they
do not defer any field or nullability decision.

The independent checker parses every tuple component, rejects truncation at
every byte boundary and eight named mutations, and recomputes the exact v3 hash
from the legacy shim stream, appended symbol, tuple bytes, and version.

### 7.5 FrameDescriptor layout v2

> **Physical layout accepted 2026-08-22.** This descriptor is immutable shared
> frame-type, control-flow, rooting, and source-map metadata. It is not copied
> into each activation. Golden fixtures remain part of §10 proof work and do not
> reopen the layout.

Accepted semantic sections, in order:

1. header, portable identities, counts, and section information;
2. receiver, argument, implementation, and capture entry locations;
3. PC records;
4. per-PC root bitmaps;
5. call-site records and call-staging descriptions;
6. edge records;
7. parallel-copy pairs and clear bitmaps; and
8. logical-value and source-map records.

An edge copy list has parallel semantics. Runtime commit reads all sources into
descriptor-sized preallocated transfer scratch, writes destinations, applies
result initialization and clears, and only then advances the root-state PC.
Descriptor bytes contain no logical DispatchGeneration or physical
CodeGeneration identity.

FrameDescriptor v2 has required flags zero and exactly fifteen sections. All
sections are present even when their count is zero:

| Kind | Directory offset | Section | Stride | Canonical contents |
|---:|---:|---|---:|---|
| 1 | `0x40` | Metadata | 64 | exactly one descriptor metadata record |
| 2 | `0x60` | EntryCaptureSlots | 4 | one Oop-slot index per capture in canonical capture order |
| 3 | `0x80` | PCRecords | 32 | one record for every dense PC in PC order |
| 4 | `0xa0` | RootBitmapWords | 8 | dense per-PC root-map words |
| 5 | `0xc0` | CallSites | 48 | Call and TailCall sites in source-PC order |
| 6 | `0xe0` | Edges | 32 | Continue and Call transitions in `(source PC, target PC)` order |
| 7 | `0x100` | CopyPairs | 8 | edge-owned parallel physical-slot copies |
| 8 | `0x120` | ClearBitmapWords | 8 | dense per-edge clear-map words |
| 9 | `0x140` | SourceFiles | 48 | source revision, extent, and diagnostic path ownership |
| 10 | `0x160` | SourceSpans | 24 | half-open byte spans into SourceFiles |
| 11 | `0x180` | LexicalBindings | 32 | stable lexical identities and source names |
| 12 | `0x1a0` | LogicalValues | 32 | deterministic IR values and binding associations |
| 13 | `0x1c0` | LogicalUses | 16 | source-relevant uses grouped by logical value |
| 14 | `0x1e0` | PCValueLocations | 16 | per-PC physical slot or explicit unavailable state |
| 15 | `0x200` | SourceBytes | 1 | call selectors, file paths, and lexical names |

The prefix and directory occupy 544 bytes, so Metadata begins at `0x220` and
ends at `0x260`. Let `C`, `P`, `K`, `E`, `Q`, `F`, `S`, `B`, `V`, `U`, `L`, and
`X` denote the counts of EntryCaptureSlots, PCs, CallSites, Edges, CopyPairs,
SourceFiles, SourceSpans, LexicalBindings, LogicalValues, LogicalUses,
PCValueLocations, and SourceBytes. Let:

```text
W = ceil(slot_count / 64)
after_captures = align_up(0x260 + 4 * C, 8)
```

Starting at `after_captures`, the remaining sections occupy, in order:

```text
32 * P
8  * P * W
48 * K
32 * E
8  * Q
8  * E * W
48 * F
24 * S
32 * B
32 * V
16 * U
16 * L
X
```

Every term and prefix sum is checked. All terms before `X` are multiples of
eight, so the only possible padding is four zero bytes after EntryCaptureSlots
when `C` is odd. SourceBytes is final and has no trailing alignment padding.
The fixed prefix, directory, and Metadata floor is 608 bytes. A structurally
valid descriptor additionally has at least one slot, one PC, one root-map word,
and the source records required below.

Metadata is exactly 64 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 8 | portable implementation identity | §6's exact derived little-endian `u64` |
| `0x08` | 8 | source-map identity | domain-separated diagnostic identity defined below |
| `0x10` | 4 | lowering version | nonzero `u32` used by §6's identity derivation |
| `0x14` | 4 | implementation kind | `u32`: 1 NamedAdapter, 2 BlockBody |
| `0x18` | 4 | slot count | nonzero `u32`; every v2 slot is an Oop slot |
| `0x1c` | 4 | PC count | nonzero `u32`, equal to `P` |
| `0x20` | 4 | driver argument count | `u32`: 0 or 1 |
| `0x24` | 4 | entry receiver slot | in-range Oop slot |
| `0x28` | 4 | entry argument start | start of contiguous entry language arguments |
| `0x2c` | 4 | entry argument count | selector-determined entry language arity |
| `0x30` | 4 | implementation slot | named adapter Oop slot or `u32::MAX` |
| `0x34` | 4 | call-window start | receiver slot of the reusable call window or `u32::MAX` |
| `0x38` | 4 | call-window argument capacity | maximum source-send arity, or zero when absent |
| `0x3c` | 4 | counter representation | 0: language Integer; 1: immediate Oop (driver argument count 1 and lowering version >= 2) |

Oop lowering version 2 requests an immediate counter for generated native steps.
The general descriptor constructor keeps the original Integer counter contract.
Generated slot and control-site indices also use immediate Oops; Machine's index
operations continue to accept both immediate and language Integer inputs. This
keeps the internal execution machinery independent of replaceable Integer
conversion Methods. Language numeric literals still call their current owners.
The flag is Execution-owned descriptor data, not an Id calling-convention change.

Entry arguments occupy the checked half-open range
`entry_argument_start .. entry_argument_start + entry_argument_count`.
`NamedAdapter` requires an in-range implementation slot and an empty
EntryCaptureSlots section. `BlockBody` requires an absent implementation slot;
its capture slots occur in stable lexical capture order. Receiver, live entry
arguments, implementation, and captures are pairwise distinct because they are
simultaneously established at PC zero. Every capture index is in range and no
index repeats.

When CallSites is empty, `call_window_start` is absent and capacity is zero.
Otherwise it is present, capacity equals the maximum CallSites argument count,
and the checked window is:

```text
receiver = call_window_start
selector = call_window_start + 1
arguments = call_window_start + 2 .. + 2 + capacity
```

Every call site uses the matching prefix. These locations are ordinary colored
Oop slots and may be reused only when the liveness/interference proof permits.

The source-map identity is reproducible diagnostic metadata, not lookup
authority. Given the complete stable key used to admit this descriptor, it is:

```text
source_map_digest = SHA256(
    "birnam.source-map-identity.v1\0"
    || stable_key_record[0..48]
    || each lexical path ordinal as little-endian u32
    || lowering_version as little-endian u32
)

source_map_identity =
    little-endian u64 decoded from source_map_digest[0..8]
```

Its domain differs from the portable implementation identity. Collisions are
valid diagnostic collisions; the descriptor SHA-256 commits the complete source
map and remains the exact physical identity.

Each PCRecords record is exactly 32 bytes. Record index is the dense PC:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 4 | terminator kind | `u32`: 1 Call, 2 TailCall, 3 Return, 4 Continue, 5 Abort |
| `0x04` | 4 | flags | zero |
| `0x08` | 4 | payload index | kind-specific CallSite, Edge, or Oop-slot index |
| `0x0c` | 4 | source-span index | in-range SourceSpans record |
| `0x10` | 4 | first PC-value-location | canonical slice start in PCValueLocations |
| `0x14` | 4 | PC-value-location count | canonical slice length |
| `0x18` | 4 | reserved 0 | zero |
| `0x1c` | 4 | reserved 1 | zero |

Call and TailCall payloads index CallSites. A Continue payload indexes the first
Edges record in that PC's nonempty contiguous successor group. Return and Abort
payloads name an in-range Oop result or reason slot. PC zero is the only entry
PC. A descriptor with more than one PC requires driver argument count one; a
zero-argument StepMethod descriptor has exactly one PC.

RootBitmapWords contains exactly `P * W` little-endian `u64` words. PC `p` owns
the slice `p * W .. (p + 1) * W`. Bit numbering follows §5, unused high bits in
the last word are zero, and every PC map is the complete step-safepoint union
required by 60F §7.2. Because layout v2 has only Oop slots, it carries no static
slot-kind array or raw-slot metadata. A mixed-kind frame requires a successor
descriptor version rather than a required flag under v2.

Each CallSites record is exactly 48 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 4 | kind | `u32`: 1 Call, 2 TailCall |
| `0x04` | 4 | flags | zero |
| `0x08` | 4 | source PC | exact owning PC |
| `0x0c` | 4 | receiver slot | `call_window_start` |
| `0x10` | 4 | selector slot | `call_window_start + 1` |
| `0x14` | 4 | argument start | `call_window_start + 2` |
| `0x18` | 4 | argument count | selector-derived arity within call-window capacity |
| `0x1c` | 4 | result slot | Call Oop slot; absent for TailCall |
| `0x20` | 4 | continuation PC | Call successor; absent for TailCall |
| `0x24` | 4 | edge index | Call edge; absent for TailCall |
| `0x28` | 4 | source-span index | source span of the complete send |
| `0x2c` | 4 | selector length | nonzero SourceBytes slice length |

CallSites are strictly ordered by source PC and there is at most one per PC.
Their kind agrees with the owning PC terminator. For Call, result, continuation,
and edge are present, the result slot does not overlap the active argument
prefix, and the edge target equals the continuation. For TailCall all three
fields are `u32::MAX`. Receiver, selector, arguments, and any present result are
in-range Oop slots. The selector slice is valid under Birnam selector rules and
recomputes exactly `argument_count`; it is not required to be UTF-8.

Each Edges record is exactly 32 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 4 | transition kind | `u32`: 1 Continue, 2 Call |
| `0x04` | 4 | flags | zero |
| `0x08` | 4 | source PC | exact owning PC |
| `0x0c` | 4 | target PC | in-range successor root-state PC |
| `0x10` | 4 | call-site index | Call owner or `u32::MAX` for Continue |
| `0x14` | 4 | first copy pair | canonical slice start in CopyPairs |
| `0x18` | 4 | copy-pair count | canonical slice length |
| `0x1c` | 4 | result-initialization slot | Call result slot or `u32::MAX` for Continue |

Edges are strictly ordered by `(source PC, target PC)`. Targets within one
source group are therefore unique. Every Continue PC owns one or more matching
contiguous edges, and its PCRecord payload indexes the group's first edge.
`continueAt:` selects the exact edge whose target equals the requested next PC;
a target absent from the source group rejects without changing the Frame. Every
non-tail Call PC owns exactly one matching edge. No TailCall, Return, or Abort
PC owns an edge. A Call edge agrees with its CallSite source, continuation, edge
index, and result slot. Edge copy slices occur once, in edge order, without
gaps, overlap, or unreferenced suffix records.

Each CopyPairs record is exactly 8 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 4 | source slot | in-range Oop slot |
| `0x04` | 4 | destination slot | distinct in-range Oop slot |

Within one edge, pairs sort strictly by destination then source. Destinations do
not repeat; a source may feed several destinations. The order has no execution
semantics: the driver first copies every source into rooted scratch, then writes
all destinations, so arbitrary cycles remain valid. The maximum edge copy count
is derived while admitting the descriptor and sizes reusable transfer scratch;
it is not redundantly stored in Metadata.

ClearBitmapWords contains exactly `E * W` little-endian `u64` words. Edge `e`
owns `e * W .. (e + 1) * W`. Padding bits are zero. A clear bit means the driver
writes canonical nil after reading copy sources and before publishing the target
root state. A copy destination or Call result-initialization slot may not also
be in the clear map. The edge validator proves that every target-rooted slot is
established by preservation, one parallel copy, canonical-nil clearing, or the
Call result protocol, and that every slot whose logical ownership ends or
changes is copied, cleared, or reinitialized before target inspection.

Each SourceFiles record is exactly 48 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 32 | SourceRevision digest | raw durable revision identity, equal to the admitted stable key |
| `0x20` | 8 | source byte length | little-endian `u64` source extent |
| `0x28` | 4 | path length | SourceBytes diagnostic path length |
| `0x2c` | 4 | flags | zero |

Every SourceFiles record names the admitted stable key's SourceRevision; layout
v2 performs no cross-revision inlining. Records sort by raw path bytes and
duplicate records fail. Paths are diagnostic raw bytes and may be empty; they
are not module authority or required to be UTF-8. A future cross-revision source
map requires a successor descriptor version.

Each SourceSpans record is exactly 24 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 4 | source-file index | in-range SourceFiles record |
| `0x04` | 4 | flags | zero |
| `0x08` | 8 | start byte | inclusive little-endian `u64` offset |
| `0x10` | 8 | end byte | exclusive little-endian `u64` offset |

`start <= end <= source_byte_length`. Spans sort by source-file index, start,
and end. Exact duplicate spans are serialized once and all references use their
canonical index.

Each LexicalBindings record is exactly 32 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 8 | lexical identity | deterministic little-endian `u64`; zero is not a sentinel |
| `0x08` | 4 | binding kind | `u32`: 1 Receiver, 2 Argument, 3 Local, 4 Capture, 5 Implementation |
| `0x0c` | 4 | name length | SourceBytes source-name length |
| `0x10` | 4 | declaration-span index | in-range SourceSpans record |
| `0x14` | 4 | scope-span index | in-range SourceSpans record |
| `0x18` | 4 | parent-binding index | lexical parent or `u32::MAX` |
| `0x1c` | 4 | reserved | zero |

Bindings occur in normalized lexical preorder, with source order breaking ties;
their identities and complete tuples are unique. A parent therefore precedes
its child, contains the child's scope, and cannot form a cycle. Equal name bytes
remain separate records when their lexical identities differ, preserving
shadowing. Receiver and Implementation names may be empty when no authored
spelling exists; other names satisfy Birnam's binding-name rules.

Each LogicalValues record is exactly 32 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 4 | logical value ID | deterministic implementation-local `u32`; zero is valid |
| `0x04` | 4 | value kind | `u32`: 1 Receiver, 2 Argument, 3 Capture, 4 Implementation, 5 Local, 6 Temporary, 7 SendResult, 8 Block, 9 Literal |
| `0x08` | 4 | binding index | LexicalBindings record or `u32::MAX` |
| `0x0c` | 4 | definition-span index | SourceSpans record or `u32::MAX` for generated values |
| `0x10` | 4 | first use | canonical slice start in LogicalUses |
| `0x14` | 4 | use count | canonical slice length |
| `0x18` | 4 | flags | bit 0 means source-inspectable; all other bits zero |
| `0x1c` | 4 | reserved | zero |

LogicalValues sort strictly by logical value ID. Use slices occur once in that
order without gaps or suffixes. Receiver, Argument, Capture, Implementation, and
Local values require the corresponding binding kind; compiler Temporary and
SendResult values have no binding unless the normalized IR explicitly associates
them with a source binding version.

Each LogicalUses record is exactly 16 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 4 | logical value ID | exact owning LogicalValues ID |
| `0x04` | 4 | PC | in-range PC |
| `0x08` | 4 | source-span index | in-range SourceSpans record |
| `0x0c` | 4 | use kind | `u32`: 1 Read, 2 CallOperand, 3 ReturnOperand, 4 CaptureOperand, 5 AbortReason |

Uses within one logical value sort by PC, span, and use kind. Exact duplicates
fail. This table is diagnostic and verifier input; generated execution never
interprets it to choose dispatch or control flow.

Each PCValueLocations record is exactly 16 bytes:

| Offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 4 | PC | exact owning PCRecords index |
| `0x04` | 4 | logical value ID | existing LogicalValues ID |
| `0x08` | 4 | location kind | `u32`: 1 Slot, 2 Unavailable |
| `0x0c` | 4 | location | Oop-slot index for Slot; `u32::MAX` for Unavailable |

PC records own contiguous location slices in PC order. Within a PC, records sort
by logical value ID and contain no duplicate. Slot locations are in range,
rooted by that PC's bitmap, and unique among simultaneously visible logical
values. Every live source-inspectable value has a Slot record; an inspectable
binding that is source-visible but has no reconstructable live value has an
explicit Unavailable record. Omission means the value is not source-visible at
that PC. Layout v2 defines no source-map constant kind: a reconstructable value
may simply remain in a rooted slot, and a later format may add constants without
reinterpreting these enum values.

SourceBytes is populated by the common §5 length-only rule in this exact
reference-visitation order:

1. every CallSites selector in record order;
2. every SourceFiles path in record order; and
3. every LexicalBindings name in record order.

Values are never interned or aliased, equal bytes are repeated, and the final
prefix sum consumes exactly `X` bytes. No other record refers to SourceBytes.

Descriptor admission follows §8 and then validates these cross-record rules:

| Check | Required result |
|---|---|
| Prefix | exact FrameDescriptor domain, schema version 2, required flags 0, and section count 15 |
| Directory | exact kinds, strides, counts, canonical offsets, zero padding, and final length described above |
| Limits | descriptor bytes, slots, PCs, calls, edges, copies, bitmap words, source records, arena bytes, payload, and scratch satisfy §9 before proportional allocation |
| Identity | stable key and lowering version reproduce portable implementation and source-map identities; manifest row agrees with implementation kind and portable identity |
| Entry | receiver, argument range, optional implementation, captures, driver arguments, PC zero, and root map satisfy the kind-specific entry rules |
| PCs | dense records, valid terminators and payloads, one source span each, complete location slices, and no unreferenced CallSite or Edge |
| Calls | canonical source-PC order, exact staging-window prefixes, selector arity, tag-specific optional fields, and source-span agreement |
| Edges | exact contiguous Continue successor groups and one-edge Call ownership, unique targets, target/root-map agreement, complete copy slices, parallel-copy validity, result initialization, and clear-map safety |
| Roots | exact `P * W` and `E * W` bitmap counts, zero padding bits, and coverage of every compiler-proved safepoint-live Oop |
| Logical ownership | interference proof, entry ownership, edge preservation/copy/clear behavior, and PC-value locations agree; no simultaneously live logical values share a slot |
| Source map | files, spans, bindings, values, uses, locations, and SourceBytes are canonical, complete, and agree with normalized source and colored IR |
| Reproducibility | live and AOT serializers produce byte-identical complete descriptor bytes and SHA-256 for identical accepted artifact input |
| Registration | descriptor SHA-256, implementation kind, portable identity, entry language arity, and driver argument count agree with the exact manifest row and native Prepare envelope |

The Runtime decodes execution-critical sections 1–8 into owned immutable
structures before registration succeeds. It may retain source-map sections
9–15 as an admitted immutable byte view plus derived indices and decode records
on demand; it need not inflate all debug records into per-entry Rust objects.
No Frame copies any descriptor section. Each live Frame retains one descriptor
reference and its own private PCs, pending transition, links, pins, and
`8 * slot_count` bytes of Oop payload.

Committed proof:

- [`abi/fixtures/stackless-s2`](../../tests/fixtures/stackless-s2/README.md)
  freezes and independently checks the complete Descriptor-v2 matrix: minimal
  Return, source-mapped block, Call, TailCall, branch/join with a contiguous
  multi-successor Continue group, cyclic parallel copy, and colored-slot reuse.
  These provide real descriptor hashes for non-circular manifest fixtures and
  cover every execution shape required by §10.

### 7.6 Image-v5 stackless projection

> **Physical layout accepted 2026-08-22.** Image v5 is a clean format cut. It
> retains the v4 28-byte image header and 12-byte record envelope but writes
> image format version 5, adds the record and field tags below, and changes the
> heap-object vtable locator to a tagged value. Pre-S2 image versions rebuild
> from source and are never interpreted using this grammar.

The unchanged outer record envelope is:

| Offset | Width | Field | v5 requirement |
|---:|---:|---|---|
| `0x00` | 4 | record kind | little-endian `u32` |
| `0x04` | 8 | payload length | little-endian `u64`, exactly the following payload bytes |

Image-v5 assigns record kind `6` to `Resumables`; v4 kinds retain their values:

| Record kind | Value |
|---|---:|
| Facts | 2 |
| Heap | 3 |
| Code | 4 |
| Authority | 5 |
| Resumables | 6 |

The `Resumables` payload has a fixed 104-byte prefix followed immediately by
the complete canonical ResumableManifest v1:

| Payload offset | Width | Field | Canonical requirement |
|---:|---:|---|---|
| `0x00` | 64 | authority spelling | exact lowercase hexadecimal bytes from the image Authority record |
| `0x40` | 32 | manifest SHA-256 | raw SHA-256 of the complete manifest bytes |
| `0x60` | 8 | manifest byte length | little-endian `u64`, denoted `M` |
| `0x68` | `M` | manifest bytes | exact complete canonical ResumableManifest v1 |

The record payload length is exactly `104 + M`, using checked addition, and no
padding or trailing bytes are permitted. `M` is at least the 224-byte canonical
empty-manifest length and at most §9's 64 MiB image-manifest ceiling. The
manifest prefix's `total_bytes` is exactly `M`. Its complete SHA-256 equals the
raw field at `0x40`; its one raw 32-byte Authority record equals the strict
lowercase-hex decoding of the 64 bytes at `0x00`. Those 64 bytes in turn equal
both the image Authority payload and the generation digest in Code. The
Resumables prefix has no independent schema version: image version 5 fixes this
envelope, while the embedded manifest retains its own domain and version.

Record order is canonical and validated over the complete stream before any
mapping or Store mutation:

```text
code-bearing image:
    Authority, Resumables, Heap*, Facts*, Code

image without Code:
    Authority, Heap*, Facts*
```

Authority is always the first and only Authority record. In a code-bearing
image Resumables is the second and only Resumables record and Code is the final
and only Code record, including when the embedded manifest has zero rows. All
Heap records precede all Facts records. An image without Code contains no
Resumables record. Therefore a missing, duplicate, late, early, or orphaned
Resumables record, Heap after Facts, or any record after Code fails preflight.

Image-v5 assigns field tag `10` to a generated block-vtable reference. Existing
v4 field tags retain values `0` through `9`:

```text
ResumableBlockVtableRef := 0x0a || manifest_entry_index:u32-le
```

The encoding is exactly five bytes. It is accepted anywhere the existing v4
tagged `Field` grammar accepts an Oop value, including heap Oop slots and any of
a fact's entity, attribute, or value positions. The index addresses the exact
ManifestEntries array embedded in this image. It must be in range and the row's
stable key must say `BlockBody`; a `NamedAdapter` row is invalid. Equal indices
may occur repeatedly and resolve to one unit-local Runtime-catalog vtable Oop.
No durable address, generated export name, or complete entry key accompanies
the index.

The v5 Heap payload keeps its initial object count and each object's word count,
but replaces the v4 untagged `u32 name_length || name_bytes` header component
with one restricted tagged vtable locator:

```text
HeapObject := VtableLocator || word_count:u32-le || encoded_slots

VtableLocator :=
    0x09 || export_name_length:u32-le || export_name_bytes
  | 0x0a || manifest_entry_index:u32-le
```

Tag `9` is the existing `ExportRef` field representation. In locator position
its bytes must be a nonempty valid exported-vtable name and it follows the
existing v4 layout-driven slot validation. No other existing Field tag may
appear there. Tag `10` uses the same five-byte
`ResumableBlockVtableRef` representation as an Oop field. It must select a
`BlockBody` row, and every one of that object's `word_count` encoded slots is a
tagged Oop field because generated block payloads use the accepted all-Oop
layout. This lets preflight validate and skip the complete object before the
artifact is mapped. After mapping but before heap allocation, `word_count` must
also equal the capture count in the byte-identical artifact descriptor selected
by that row.

The image writer must ask the Runtime's read-only image catalog about every
vtable Oop it encounters. An ordinary exported vtable is emitted with locator
tag `9`. A catalogued generated block vtable is emitted with tag `10` only after
its owning authority, canonical manifest index, durable entry key, descriptor
hash, and binding fingerprint agree with the image manifest. A generated,
private, foreign-generation, missing, retired, or drifted vtable may not fall
back to an exported name. The same rule applies when tag `10` is emitted as an
ordinary Oop field or fact value.

Image preflight performs the following checks without mapping Code, resolving a
generated vtable, allocating a Birnam heap object, or mutating Store:

| Check | Required result |
|---|---|
| Header | exact image magic, version 5, endian tag, architecture, and Runtime ABI hash |
| Framing | every 12-byte record header and checked payload extent consumes the image exactly |
| Order | one of the two complete record sequences above; exact Authority, Resumables, and Code cardinality |
| Resumables length | fixed prefix available; checked `104 + M` equals payload length; `M` satisfies §9 |
| Authority | one canonical 64-byte lowercase hexadecimal spelling agrees byte-for-byte across Authority, Resumables, and Code and decodes to the manifest's raw authority |
| Manifest | complete v1 structural and semantic admission succeeds and its `total_bytes` equals `M` |
| Manifest hash | SHA-256 of all `M` manifest bytes equals the raw 32-byte payload field |
| Heap locators | tag is exactly 9 or 10; payload framing is complete; every tag-10 index is an in-range BlockBody row |
| Tagged fields | all Facts and layout-identifiable Heap fields are structurally complete; every tag-10 index is an in-range BlockBody row |
| Reference limits | tag-10 occurrences across heap locators, heap fields, and facts total at most 1,048,576; distinct referenced indices do not exceed manifest rows |
| Code | existing code checksum succeeds and its generation digest equals Authority |
| No-Code exclusion | an image without Code has neither Resumables nor any tag-10 locator or field |

For an ordinary exported-vtable heap object, the existing read-only layout
lookup completes the layout-driven field check without mutation. For a tag-10
heap locator, the all-Oop rule above makes that check independent of the not-yet
mapped private vtable. Any structural, order, authority, manifest, hash, tag,
index, or ceiling failure rejects the entire image at this stage.

Mapped recovery then copies and validates the artifact-exported manifest,
requires byte identity with these `M` image bytes rather than accepting only a
digest match, runs registration inside the generation transaction, and checks
the descriptor-derived capture counts before materialization. Only after those
checks may it resolve tag-10 indices, allocate heap objects, or replay facts.
The resolver is called once logically per distinct index and the Runtime catalog
verifies every repeated returned Oop identity.

Accepted semantics therefore remain:

- an image with Code contains exactly one `Resumables` record after Authority
  and before Heap, Facts, and Code;
- the record contains the exact manifest bytes and their SHA-256 under the same
  authority;
- heap and fact values may carry
  `ResumableBlockVtableRef(u32 manifest_entry_index)`;
- the index addresses the exact manifest carried by the image and must name a
  `BlockBody` row;
- repeated references resolve to one Runtime-catalog vtable identity; and
- preflight rejects structural or authority disagreement before mapping or
  Store mutation.

Committed proof:

- [`abi/fixtures/stackless-s2`](../../tests/fixtures/stackless-s2/README.md)
  freezes a code-bearing empty manifest, a complete mapped construction trace
  with four references to one block row, and a no-Code rebuild form. Four
  committed malformed images cover out-of-range and named-row indices plus
  authority and manifest-hash disagreement. The independent reader additionally
  derives every required header, record-order, locator-tag, duplicate, missing,
  late, Heap-after-Facts, record-after-Code, and orphan-Resumables mutation and
  rejects truncation at every byte boundary.
- production image-code capture reads the durable GenerationSelection without a
  policy override, projects its complete rows into the image ArtifactSelection,
  and emits the exact authority-bound manifest through the paired
  `compileImageCode:manifest:` capture seam. A live selected-generation proof
  checks a nonempty exported manifest and the corresponding resumable symbols;
  the production image lifecycle smoke then publishes those bytes, admits both
  mapped and rebuilt recovery, mints the same checksum-derived composed
  identity, retires saved origins as `#historical`, removes rollback capability,
  resaves the composed image, and proves a successor candidate can activate and
  roll back over the reconstructed stable-owned-target ledger. Fact and heap
  preflight remain unchanged and fail closed.

## 8. Structural validation order

Every durable parser must validate in this order unless a format-specific table
proves an earlier check is required to read the next field safely:

1. minimum prefix availability;
2. domain, schema version, and declared prefix size;
3. total length and checked conversion to host sizes;
4. flags, reserved fields, and zero canonical padding;
5. section-directory structure;
6. section alignment, monotonicity, non-overlap, and bounds;
7. record strides, counts, and checked count-by-stride products;
8. enum values and absent-index sentinels;
9. variable-byte references and field-specific byte validity;
10. canonical row ordering and uniqueness;
11. internal indices, ranges, bitmap padding, and cross-section references;
12. semantic completeness against the normalized inventory or owning artifact;
13. cross-format identities and hashes; and
14. ownership transfer, registration, mapping, or publication.

No parser allocates Birnam heap objects, installs a binding, publishes a
generation, or retains a process-local capability before structural validation.
Borrowed zero-copy parsing may create checked views; Runtime admission copies or
decodes into owned immutable storage before the artifact can execute.

## 9. Limits and deterministic failure

The byte grammar must distinguish:

- a **representational maximum** implied by a field width, which is not a
  promise that a compiler or Runtime accepts values that large;
- a **Runtime admission ceiling**, shared by every Runtime implementing the
  ABI, above which input is rejected before allocation proportional to the
  declared quantity; and
- a **compiler ceiling** for resumable lowering, no greater than the admission
  ceiling, above which the compiler deterministically refuses to emit the
  format.

S2 uses one ceiling for both compiler emission and Runtime admission. There is
no accepted-but-unproducible range. `MiB` below means `2^20` bytes.

The common and provenance ceilings are:

| Quantity code | Representational maximum | S2 compiler and admission ceiling | Aggregate relationship |
|---|---:|---:|---|
| CanonicalValueBytes | `u64::MAX` | 64 MiB | complete common-framed value; DescriptorBytes overrides |
| SectionCount | `u32::MAX` | 32 | each schema also requires its exact smaller count |
| FixedRecords | checked sum of `u32` section counts | 1,048,576 | all non-arena fixed records in one value |
| CoveredRevisions | `u32::MAX` | 65,536 | unique revisions in one selection |
| ImplementationRows | `u32::MAX` | 262,144 | ArtifactSelection, GenerationSelection, or ResumableManifest rows |
| PathDepth | `u32::MAX` | 4,096 | ordinals in one stable key |
| PathOrdinals | `u32::MAX` | 4,194,304 | complete PathOrdinals section |
| SelectorBytes | `u32::MAX` | 1 MiB | one selector |
| SelectorArenaBytes | `u32::MAX` | 16 MiB | complete manifest SelectorBytes section |
| LanguageArity | `u32::MAX` | 4,096 | one selector and entry |

The descriptor and per-frame ceilings are:

| Quantity code | Representational maximum | S2 compiler and admission ceiling | Aggregate relationship |
|---|---:|---:|---|
| DescriptorBytes | `u64::MAX` | 16 MiB | one complete FrameDescriptor, overriding CanonicalValueBytes |
| FrameSlots | `u32::MAX` | 65,536 | one descriptor and decoded frame |
| ProgramCounters | `u32::MAX` | 65,536 | one descriptor |
| CallSites | `u32::MAX` | 65,536 | one descriptor |
| Edges | `u32::MAX` | 262,144 | one descriptor |
| CopyPairs | `u32::MAX` | 262,144 | all edges in one descriptor |
| CopyPairsPerEdge | `u32::MAX` | 65,536 | one parallel transfer |
| LogicalValues | `u32::MAX` | 262,144 | one descriptor |
| SourceRecords | `u32::MAX` | 262,144 | one descriptor |
| BitmapWords | checked sum of `u32` section counts | 1,048,576 | root and clear bitmap words combined |
| DescriptorArenaBytes | `u32::MAX` | 4 MiB | complete descriptor byte arena |
| FramePayloadBytes | checked `FrameSlots * 8` | 512 KiB | decoded slot words for one frame |
| TransferScratchBytes | checked `CopyPairsPerEdge * 8` | 512 KiB | one transition's preallocated source scratch |
| ArtifactDescriptorBytes | checked `u64` sum | 256 MiB | all descriptors owned by one artifact |

Image-v5 additionally limits the embedded manifest to 64 MiB, the total number
of `ResumableBlockVtableRef` occurrences to 1,048,576, and the number of
distinct referenced entries to ImplementationRows. These are image-preflight
ceilings even when repeated references name the same manifest row.

Every addition, multiplication, alignment, prefix sum, aggregate, and host-size
conversion remains checked even below a published ceiling. Independent maxima
are not jointly promised when an aggregate or total-byte ceiling rejects their
combination. In particular, ProgramCounters and FrameSlots cannot both force
root maps beyond BitmapWords or DescriptorBytes.

These values may not come from ambient environment variables or host collection
capacity. Live and AOT compilation of the same accepted input must make the same
decision. `ResumableRequired` exceeding a frozen ceiling is a kinded selection
failure before registration, mapping, or publication; the compiler may not
silently substitute Synchronous because doing so would contradict the accepted
selection. The same source remains legal when selected for the independent
synchronous compiler path, so a resumable metadata ceiling is not a Birnam
language limit.

These ceilings constrain the finite metadata and bounded Runtime storage of the
S2 lowering strategy. They do not bound IdApi identity, selector or object
semantics, late binding, relational policy, source-language expressiveness, or
the size of synchronous programs. Relational selection may request
ResumableRequired but cannot override a hard compiler or admission ceiling.

The table deliberately does not bound active Machines, active frame-stack
depth, continuation depth, send or loop counts, Runtime objects, selector
population, or relational facts. Those are dynamic language execution
quantities; imposing a fixed S2 frame-depth limit would change observable
recursion behavior.

Compiler and Runtime failures use one stable diagnostic family:

```text
StacklessLimitExceeded {
    quantity,
    observed,
    limit,
    stage,
}
```

`quantity` is the table's stable quantity code and `stage` distinguishes
selection, lowering, artifact admission, and image preflight. Compiler checks
run before emission and publication; parser and image checks run before
allocation proportional to the declared quantity, registration, mapping, Store
mutation, or publication. A limit raise belongs to a successor schema/domain
and, where Runtime storage or native carriers change, a successor Runtime ABI
hash. Boundary fixtures cover the ceiling, ceiling plus one,
checked-arithmetic overflow, and aggregate overflow.

## 10. Golden fixtures and independent conformance

Each completed grammar must commit:

- a human-auditable field-value description;
- exact binary bytes;
- an annotated hexadecimal rendering;
- exact byte length;
- SHA-256 where the format is hashed;
- expected canonical row/section order; and
- a table of single-fault mutations and required rejection stages.

The minimum fixture matrix is:

| Format | Required fixtures |
|---|---|
| ArtifactSelection | empty scope, all-synchronous, mixed dispositions, nested block path, malformed scope |
| GenerationSelection | one slice, multiple slices, identical rows under different code-unit partitioning, overlap, missing revision |
| ResumableManifest | empty, named-only, block-only, mixed, injected portable-ID collision policy, forged-ID rejection, selector-byte edge case |
| Native envelope | Prepare named, Prepare block, Abort, bad size/version/index/token/nullability |
| Descriptor v2 | minimal return, Call, TailCall, branch/join, cyclic parallel copy, colored-slot reuse, source map |
| Image v5 | empty manifest, repeated block-vtable reference, out-of-range/named-row index, authority/hash mismatch |

Production may initially share a codec implementation, but shared code is not
proof that the format is independently implementable. At least one test-only
reader or fixture checker that does not call the production decoder must verify
the golden bytes. Fuzzing must cover truncation at every byte boundary, integer
overflow, offset overlap, noncanonical padding and order, invalid enums and
indices, duplicate rows, and cross-format hash disagreement.

## 11. Versioning and rebuild policy

Each durable format has an independent schema version and domain. A parser
accepts only explicitly supported versions and rejects unknown required flags.
Adding a field by reinterpreting reserved bytes is forbidden unless the original
version defined that interpretation. A semantic or structural extension that
changes canonical bytes receives a new version/domain and updates the Runtime
ABI hash where applicable.

Pre-S2 images and artifacts do not contain a compatible partial form of these
schemas. They rebuild from source rather than entering an in-place upgrader.
Once an S2 format ships, later compatibility policy belongs to Future 60H's
acceptance and rollout record; 60I continues to define the bytes of each
supported version.

## 12. Work sequence

The physical-format **decision** work proceeds in this dependency order:

1. common framing, directory, variable-byte, hash, and limit rules;
2. shared stable implementation key;
3. ArtifactSelection v1;
4. GenerationSelection v1;
5. ResumableManifest v1 and canonical index assignment;
6. native registration envelope v1;
7. FrameDescriptor layout v2;
8. image-v5 stackless projection;
9. golden fixtures, independent checker, and malformed-input matrix; and
10. one cross-format construction and recovery trace.

The first four items establish S2.0's provenance kernel. Descriptor-v2 closed
in the same gate so a partial Rust codec cannot become the de facto
specification.

Golden-fixture construction follows §3's acyclic digest order rather than the
layout-decision order above. In particular, exact FrameDescriptor fixtures must
exist before a semantically admitted ResumableManifest can commit their
SHA-256 values; manifest fixtures then precede native-envelope and image
fixtures. A synthetic descriptor hash that names no exact descriptor bytes is
not valid temporary proof and may not be repaired by choosing later bytes to
match it.

## 13. Pre-implementation closure gate

The 60I gate closed 2026-08-22 after all of the following became present and
internally consistent:

- [x] common prefix/framing offset table;
- [x] section-directory offset table and section-kind assignments;
- [x] canonical variable-byte and padding rules;
- [x] acyclic digest placement with no self-digest fields;
- [x] complete limit table;
- [x] one exact shared stable-key grammar;
- [x] ArtifactSelection v1 offset and validation tables;
- [x] GenerationSelection v1 offset and validation tables;
- [x] ResumableManifest v1 offset and validation tables;
- [x] native registration-envelope declaration and ABI assertions;
- [x] every descriptor-v2 header and record layout;
- [x] image-v5 stackless record and reference layout;
- [x] annotated golden bytes and hashes for the §10 matrix;
- [x] independent fixture-reader agreement;
- [x] malformed-input and cross-format disagreement matrix; and
- [x] a complete non-circular candidate-to-image construction trace.

This checklist is closed. 60F may mark the byte-grammar gate closed and the
stackless overview may describe the physical S2 design as ready to implement.
