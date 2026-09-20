# Key Index v1

> **Superseded by Design 82/83:** dictionary indexing and mutation counts are
> internal to Id again. The factory and pre-Id attachment described below have
> been removed. This document preserves the historical extraction contract.

Design [81BX](../docs/design/81bx-production-vtable-key-index.md) moves the
derived Vtable address-to-position map into `libbirnam_object_model`. Id keeps
ordered keys/values/weak flags, threshold 16, linear scanning, complete rebuilds,
raw mirrors, mutation accounting and all current-value lookup decisions.

`birnam_key_index_factory_v1` returns an immutable 56-byte descriptor:
`version: u32`, `byte_size: size_t`, then five function pointers.

| Operation | Contract |
|---|---|
| `destroy(context)` | Null is a no-op; otherwise free the map exactly once |
| `is_empty(context) -> int` | Return 1 for null or an empty map, otherwise 0 |
| `get(context, uintptr_t key, uintptr_t *out) -> int` | Hit writes the position and returns 1; miss returns 0 and leaves output unchanged |
| `insert(void **context_slot, uintptr_t key, uintptr_t position)` | Allocate on the first insert, update the context slot and insert/replace the position |
| `clear(context)` | Null is a no-op; otherwise clear entries and retain allocation/capacity |

Null is the initial unallocated state. Thus constructing a small dictionary
never allocates a boxed empty index. Id inserts all keys only when its ordered
dictionary first exceeds 16 entries. A retained empty map selects linear
lookup again; later regrowth reuses its capacity. Keys and positions accept
all bit patterns. They are integer data, never dereferenced language pointers,
roots, Methods or executable leases. The exact previous address hasher is
preserved; there is no new hash or lookup policy.

The owner-thread caller serializes access. Non-null contexts must be live and
belong to this factory. The context-slot pointer is a valid exclusive writable
loan for the duration of insert. On a get hit, output points to writable
`uintptr_t` storage; callers may supply a valid output for either result.
Neither loan is retained. Moving a Vtable record does not move its owned map.
Destroy works after Heap teardown and cannot affect another index. Reads,
empty checks and clear allocate nothing; insert may allocate/grow and destroy
frees the owner. No operation invokes a callback or Id semantic helper.

The host pins the descriptor through `birnam_key_index_dependency_v1`, after
Mutation Epoch and before Method Observation/Id initialization. Validation
checks version, exact size and every operation without invoking them. Repeat
attachment accepts the same descriptor; retargeting is rejected. This factory
works before Object Model's ordinary initializer. Missing Object Model still
fails at the earlier Vtable Enumeration dependency.

The 81-library inventory, ordinary Siphon initializer order, existing Object
Model Cargo output-path install name, IdApi v20, twelve-argument ModuleInit,
Native Loader v12 and image v8 remain unchanged. The index is not serialized;
ordinary dictionary replay reconstructs it during recovery.

The independent C declaration is [key_index_v1.h](fixtures/key-index-v1/key_index_v1.h).
Run `mise run key-index` for the current dictionary/runtime regression tests and
Object Model export audit. Factory admission and factory-spy fixtures are removed.
`scripts/key-index-image-smoke.sh PREFIX` retains the production source, compiled
and mapped/rebuilt/baked image workload. Validation is debug only.
