# 60N — Portable Resumable Native Carrier

> **Status:** Implementation contract derived from the accepted 60K and frozen
> 60L designs. This record supplies the private native-carrier declaration
> deliberately left open by 60L and identified as delivery gate H6 in 60LA. It
> does not reopen CodeObject meaning, materialization policy, Frame execution,
> ordinary lookup, or image-v7 authority. The live K3/K4 carrier and lifetime
> slice and the mapped/rebuilt K5 image-v7 carrier were implemented and
> verified 2026-08-26.

## 1. Decision

A portable resumable native unit is admitted against the exact 60L
`PortableProgramInventory`, `CodeBindingInventory`, and
`CodeMaterializationSelection` that selected it. Its private manifest contains
one descriptor-owning entry for every and only `Resumable` selection row.
Generated registration continues to use the append-only registration-v2
envelope and the existing Runtime `resolve-and-pin` substrate.

No portable carrier fabricates a `SourceRevision`, lexical source path, or
source-map identity. Legacy source artifacts retain the frozen 60I manifest
and descriptor admission path unchanged.

## 2. Authority and identity

The complete lookup and admission authority for one portable native entry is:

```text
portable image-code authority
complete CodeMaterializationSelection bytes
selection row index
complete semantic subject bytes through the admitted inventory
exact FrameDescriptor bytes and SHA-256
compiler/lowering contract version
target ABI and Runtime ABI
```

The `u64` portable implementation identity remains reproducible diagnostic
metadata and never becomes lookup authority. It is the low eight bytes,
interpreted little-endian, of:

```text
SHA-256(
    "birnam.portable-materialization-identity.v1\0" ||
    implementation_kind:u32-le ||
    semantic_sha256[32] ||
    lowering_version:u32-le
)
```

For a BlockBody, `semantic_sha256` is the complete PortableProgram SHA-256 in
its selection row. For a NamedAdapter, it is the provenance-free CodeBinding
semantic digest in that row. Full subject bytes are compared through the
admitted inventories; equal or colliding digests never substitute for them.

## 3. PortableResumableManifest v1

The private manifest is canonical pointer-free data. It uses 60I's 64-byte
common prefix and 32-byte directory entries.

```text
domain = "birnam.portable-resumable.v1\0" padded with zeroes to 32 bytes
format_version = 1
required_flags = 0
section_count = 2
directory_stride = 32
```

It has two sections:

| Kind | Section | Stride | Count |
|---:|---|---:|---:|
| 1 | Metadata | 64 | exactly 1 |
| 2 | Entries | 64 | exactly the number of Resumable selection rows |

Metadata is:

| Offset | Width | Field |
|---:|---:|---|
| `0x00` | 32 | portable image-code authority raw bytes |
| `0x20` | 32 | SHA-256 of complete CodeMaterializationSelection bytes |

Each Entry is:

| Offset | Width | Field | Requirement |
|---:|---:|---|---|
| `0x00` | 4 | selection row index | exact row in the complete selection |
| `0x04` | 4 | driver argument count | zero or one |
| `0x08` | 4 | lowering version | nonzero |
| `0x0c` | 4 | required flags | zero |
| `0x10` | 8 | portable implementation identity | exact §2 derivation |
| `0x18` | 8 | reserved | zero |
| `0x20` | 32 | FrameDescriptor SHA-256 | nonzero |

Entries sort by selection row index. Every selected Resumable row occurs once;
no Synchronous row occurs. Admission checks the row's kind, semantic digest,
selector-derived arity, capture count, and disposition through the exact three
60L inputs.

## 4. FrameDescriptor-v2 portable profile

Portable resumable lowering retains FrameDescriptor layout v2 and all existing
execution sections, root maps, edge plans, capture locations, arity checks, and
descriptor hashing.

Its identity admission profile differs from the legacy source profile:

- `portable_implementation_identity` is §2's source-independent value;
- `source_map_identity` is zero when no separate source-map artifact is
  supplied;
- the source-location tables use the canonical unavailable-source spelling:
  one anonymous zero-length file with a zero 32-byte provenance digest, one
  zero-length span, and all logical locations referring to that span; and
- source provenance, when present under a later debug policy, is admitted as a
  separately hashed optional artifact and never changes portable meaning or
  materialization selection.

The legacy `frame_descriptor::inspect(bytes, SelectionSpec)` contract remains
unchanged. Portable staging uses a separately named admission function and
cannot reinterpret a legacy source manifest as portable.

## 5. Private staging ABI

The host-to-floor entry point is versioned and outside frozen `IdApi`:

```c
uint32_t birnam_id_begin_portable_resumable_unit_v1(
    const uint8_t *programs, size_t programs_length,
    const uint8_t *bindings, size_t bindings_length,
    const uint8_t *selection, size_t selection_length,
    const uint8_t *manifest, size_t manifest_length,
    const uint8_t authority[32],
    uint64_t code_generation_identity,
    uint64_t code_generation_lease);
```

The call synchronously copies and admits all inputs, recomputes 60L authority,
and creates one rollback-capable staged unit. Existing
`birnam_id_finish_resumable_unit_v1` and
`birnam_id_cancel_resumable_unit_v1` close either legacy or portable staging.
Only one staged unit of either profile may exist in one Runtime.

Generated code continues to call `birnam_register_resumable_v2`. Its
`manifest_entry_index` indexes the portable manifest; the staged entry names
the corresponding complete selection row. The Runtime selects the portable
FrameDescriptor-v2 admission profile from its staged context, never from
untrusted envelope flags.

## 6. Unit exports

A mapped v7 portable native unit exports exact byte arrays and `uint64_t` sizes
for:

```text
__birnam_portable_program_inventory_v1
__birnam_code_binding_inventory_v1
__birnam_code_materialization_selection_v1
__birnam_portable_resumable_manifest_v1
```

It also exports the existing registration context bracket and the existing
bounded block-vtable resolver when BlockBody rows are present. Boot compares
every exported byte sequence with the image before calling registration or a
resolver. A live K3 TCC materialization may supply the same bytes directly to
the staging ABI without exporting them; this is a transport difference only.

## 7. Repository and lifetime

Repository reuse compares the complete 60K materialization key. The complete
program bytes and disposition are always compared after digest indexing.

A portable TCC unit records its committed `CodeGeneration` identity before it
becomes disposable. Its finalizer requests Runtime retirement before releasing
native code. GC removes unreachable vtable bindings and their registration
references before running unit finalizers; therefore a unit reachable only
from an unreachable Block/vtable graph retires synchronously. A rejection at
that point is an invariant failure: native code must never be freed while the
Runtime still recognizes its addresses.

Evicting a weak repository entry removes only the cache root. A live Block's
private `[CodeObject, materializationOwner]` carrier continues to retain the
unit. Active Frames and protected dispatch readers retain the registered entry
and physical generation through existing 60E pins.

## 8. Acceptance

Implementation must prove:

1. source-less PortableProgram bytes produce a portable manifest and admitted
   FrameDescriptor-v2 without a SourceRevision;
2. malformed inventories, selection, authority, manifest coverage, identities,
   descriptor hashes, arities, captures, or dispositions reject before a
   registration claim;
3. the same source-less CodeObject executes identically through synchronous and
   resumable materializations;
4. ordinary sends enter the existing shared resolve-and-pin path and never
   compile;
5. byte-equal CodeObjects reuse compatible physical materializations while
   retaining distinct Oop identity and captures;
6. eviction leaves live Blocks callable and final ownership quiescence retires
   the Runtime generation before TCC releases native code;
7. legacy 60I source manifests and v5/v6 admission remain byte- and
   behavior-compatible; and
8. mapped and rebuilt v7 units project the same 60L selection and authority.

Implementation checkpoint (2026-08-26): one monolithic portable image unit
exports the exact programs, bindings, selection, and portable manifest bytes.
Ordinary boot rejects unsafe Mach-O carriers before loading, compares every
export, stages block vtables and NamedAdapters against the Runtime, and retains
one generation owner across all recovered Blocks and bindings. Forced rebuild
uses the same inventories through an in-memory TCC unit. The v7 lifecycle smoke
proved a nested source-less closure and a current CodeBinding through save,
process exit, mapped boot, forced rebuild, resave, and mapped boot of the
successor image. The K3/K4 eviction, replacement, self-removal, and quiescent
retirement smokes remained green.
