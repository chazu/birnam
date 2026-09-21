# Deferred Index v1

Storage owns a lazy native vector of `(deferred identity, dead-journal index)`
pairs. Runtime retains forward lookup, numeric fallback, swap-removal ordering,
displaced-index repair and materialization. No payload journal or identity
sequence moves with this storage.

Id loads Storage through `birnam_native_module_v1` with the Bytes request
`deferredIndex`. The initializer returns the immutable 80-byte descriptor as a
Word. Bootstrap validates version 1, exact size and all eight operations,
then supplies its copy as an explicit predecessor to Runtime construction.
Storage is usable before Runtime exists; no dependency setter or later
initialization is needed. Id pins the native library after entering its initializer.
See [native Runtime support factories](runtime-support-v1.md).

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
native row reference escapes. Runtime reads copied rows to choose lookup and repair;
Storage neither searches for a handle nor chooses which displaced index to fix.

Only push allocates/grows and destroy frees ownership. Empty Heaps and
numeric-only work acquire no index allocation. Clear preserves a previously
allocated owner and capacity, including after settlement/reclaim. Runtime's existing
`capacity * 16` native-storage accounting term stays unchanged; the native carrier
does not become a payload charge. No performance improvement is claimed.

The module retains no Oop, Heap, Rc payload, callback, Method, native lease or
image pointer. Runtime keeps all roots, journal limits, charges, sequencing, GC and
recovery. There is no language method or serialized index field for this storage.
The row ABI and image v8 format are unchanged.

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

The fixtures use the actual native entry for Storage and explicit Runtime
services for their constructors. Saved native methods are typed entries selected
through Id; they are never recovered by casting a language Method to a function.
The shared image host requires exact mapped/rebuilt outcomes and checks the idle
storage view before execution and its guarded result afterward. Storage forwarding
fixtures replace only their named factory result; all other requests reach the
installed native initializer unchanged.
