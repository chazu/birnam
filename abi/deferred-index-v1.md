# Deferred Index v1

Storage owns a lazy native vector of `(deferred identity, dead-journal index)`
pairs. Id retains forward lookup, numeric fallback, swap-removal ordering,
displaced-index repair and materialization. No payload journal or identity
sequence moves with this storage.

`birnam_deferred_index_factory_v1` exports an immutable 80-byte descriptor.
`birnam_deferred_index_dependency_v1` attaches it after Deferred Identity and
before Id initialization. The host pins the descriptor and library. Admission
checks version 1, exact size and all eight operations without invoking them;
repeat attachment of the same descriptor succeeds, retargeting fails.

A row is 16 bytes, aligned to 8: `uint64_t identity` at offset 0 and
`uintptr_t index` at offset 8. Both are inert bits. There is no sentinel,
uniqueness check, pointer interpretation or journal validation in Storage.

| Field/operation | Offset | Contract |
|---|---:|---|
| `version: uint32_t` | 0 | 1 |
| `byte_size: size_t` | 8 | 80 |
| `destroy(context)` | 16 | Null no-op; otherwise destroy once |
| `len(context)` | 24 | Current row count; null returns zero |
| `capacity(context)` | 32 | Actual Vec capacity; null returns zero |
| `push(context_slot, row)` | 40 | Lazily allocate owner and append |
| `read(context, position, out_row)` | 48 | Copy a row; miss leaves output unchanged |
| `write(context, position, row)` | 56 | Unconditional replacement; miss changes nothing |
| `swap_remove(context, position, out_row)` | 64 | Copy removed row and move last row into its place |
| `clear(context)` | 72 | Clear rows, retaining capacity; null no-op |

Read/write/swap-remove return normalized 1 for a present position and 0 for a
miss. Loans of context slots and fixed-size outputs end on return. No borrowed
native row reference escapes. Id reads copied rows to choose lookup and repair;
Storage neither searches for a handle nor chooses which displaced index to fix.

Only push allocates/grows and destroy frees ownership. Empty Heaps and
numeric-only work acquire no index allocation. Clear preserves a previously
allocated owner and capacity, including after settlement/reclaim. Id's existing
`capacity * 16` native-storage accounting term stays unchanged; the native carrier
does not become a payload charge. No performance improvement is claimed.

The module retains no Oop, Heap, Rc payload, callback, Method, native lease or
image pointer. Id keeps all roots, journal limits, charges, sequencing, GC and
recovery. There is no new language method or serialized index field. IdApi v20,
twelve-argument ModuleInit, Native Loader v12 and image v8 are unchanged.

Run `mise run deferred-index` for module, Heap and host-admission checks. With
a debug installation:

```sh
scripts/deferred-index-loaded-smoke.sh PREFIX
scripts/deferred-index-image-smoke.sh PREFIX
```

The loaded harness forwards every operation to actual installed Storage while
comparing rows against an independent ordered model. It checks source and
compiled closure recovery, lazy ownership and numeric-only work with no clone
row changes. The image smoke checks ordinary startup, source-free compiled
callers and mapped/rebuilt/baked execution, then repeats image modes with actual
index observation and forced recovery. Existing storage observation exclusions
remain enforced before/after eviction.
