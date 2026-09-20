# Deferred Identity v1

Storage owns an opaque `u64` cell for each Heap. Id owns the shared sequence of
deferred clone and numeric-result handles, all admission and charging, and
materialization, settlement and reclaim. This sequence remains distinct from
physical allocation identities.

`birnam_deferred_identity_factory_v1` exports an immutable 48-byte descriptor.
`birnam_deferred_identity_dependency_v1` attaches it to Id after Allocation
Identity preparation and before `id_init`. The host pins Storage's library.
Admission checks exact version 1, size and all four operations without invoking
them. Repeating the same descriptor succeeds; retargeting fails.

| Field | Offset | Contract |
|---|---:|---|
| `version: uint32_t` | 0 | 1 |
| `byte_size: size_t` | 8 | 48 |
| `create() -> void *` | 16 | Independent zero-initialized native cell |
| `destroy(void *)` | 24 | Destroy that owner once |
| `read(const void *) -> uint64_t` | 32 | Read current bits |
| `write(void *, uint64_t)` | 40 | Assign supplied bits unconditionally |

Read/write allocate nothing and invoke no callbacks. The factory retains no
Heap, journal, Oop, payload, Method, native lease or image pointer. Moving a Heap
preserves its cell. Other Heaps have independent cells. All bit patterns and
repeated/reordered assignments are valid native storage; the module supplies no
issuance, saturation, limit or reset policy.

Id still issues the current value after checking its successor and completing
the existing charge/record steps. First handle: zero. Last handle: `u64::MAX - 1`.
At `u64::MAX`, another handle request returns `None`, while zero-count admission
and unhandled dead allocations retain their existing separate capacity rules.
Invalid indices and capacity failures consume neither handles nor charges.
Materialization is single use and advances the separate physical sequence.
Settlement, reclaim and budget reset do not reset deferred issuance. No global
or cross-Heap uniqueness policy is added.

There is no new language method, image field or public-format version. Heap
journals remain native process state. IdApi v20, twelve-argument ModuleInit,
Native Loader v12, image v8 and the installed library inventory are unchanged.

Run `mise run deferred-identity` for storage, Heap and host admission coverage.
With a debug installation, run:

```sh
scripts/deferred-identity-loaded-smoke.sh PREFIX
scripts/deferred-identity-image-smoke.sh PREFIX
```

The loaded smoke uses a harness-only forwarding Storage library to count
actual Heap writes while every operation reaches the installed Storage owner.
It checks source and compiled closure/numeric work, ordinary fallback and forced
recovery. The image smoke uses the ordinary installed libraries for startup,
source-free compiled callers and mapped/rebuilt/baked images. Existing closure
reports prove that the deferred path actually executes. Additional forwarding
runs count both journals in mapped/rebuilt/baked execution and forced recovery.
Host checks assert loader outcomes and exact physical identities before the
workload, then preserve the existing `BUSY` observation result after eviction.
