# Allocation Identity v1

> **Superseded by Design 82/83:** allocation identity is again internal to Id.
> The Storage factory and pre-Id attachment described below have been removed.
> This document preserves the historical extraction contract.

Design [81BY](../docs/design/81by-production-allocation-identity.md) moves the
per-Heap next-allocation identity cell into `libbirnam_storage`. Id retains raw
allocation, checked increment, the 59-bit limit, packed block metadata, registry
insertion and byte charging. Existing block identities remain in Id's Blocks.

`birnam_allocation_identity_factory_v1` returns an immutable 48-byte descriptor:
`version: u32`, `byte_size: size_t`, followed by four function pointers.

| Operation | Contract |
|---|---|
| `create() -> context` | Allocate a zero-initialized `u64` cell |
| `destroy(context)` | Free the cell exactly once |
| `read(context) -> uint64_t` | Return stored bits without mutation |
| `write(context, uint64_t)` | Assign supplied bits unconditionally |

The module owns storage only. It neither issues nor validates an identity.
Every bit pattern, repeated writes and arbitrary assignment order are supported;
Id's production writer still checks the successor against `u64::MAX >> 5`.
Issuance starts at 1. Exhaustion retains the original fatal diagnostic and its
position after raw allocation but before registry insertion and byte charging.
No production reset or exhaustion-forcing API is added.

The owner-thread caller supplies a live context and serializes access as before.
Only create/destroy allocate/free memory; read/write invoke no callbacks and
allocate nothing. The cell retains no Heap, Oop, raw block, Method, finalizer,
callback or image pointer. Moving a Heap preserves its owner; destruction needs
no live language storage. Different Heaps never share their cells. Native cell
memory is outside Heap payload-byte accounting, like other native service and
registry carriers.

The host pins and attaches the descriptor through
`birnam_allocation_identity_dependency_v1`, after Storage Observation and before
Id initialization. Validation checks version, exact size and all operations
without calling them. Repeated attachment of the same descriptor succeeds;
retargeting fails. Cell creation works before Storage's ordinary initializer.
A missing Storage library still fails at the earlier observation factory.

The 81-library inventory, existing Siphon initializer order, Storage install
name, IdApi v20, twelve-argument ModuleInit, Native Loader v12 and image v8 are
unchanged. There is no serialized counter field or explicit image reset;
ordinary Heap construction and allocation replay keep issuing identities.

The independent C declaration is
[allocation_identity_v1.h](fixtures/allocation-identity-v1/allocation_identity_v1.h).
Run `mise run allocation-identity` for the current allocation regression checks:
Id identity/geometry tests and installed object-storage observations. Factory
admission tests and the loaded external-counter forcing fixture are removed;
Id unit tests retain exhaustion coverage. The image smoke checks source,
compiled and mapped/rebuilt/baked paths. Validation is debug only.
