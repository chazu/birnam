# Deferred Identity v1

Storage owns an opaque `u64` cell for each Heap. Runtime owns the shared sequence of
deferred clone and numeric-result handles, all admission and charging, and
materialization, settlement and reclaim. This sequence remains distinct from
physical allocation identities.

Id loads Storage through `birnam_native_module_v1` with the Bytes request
`deferredIdentity`. The initializer returns the immutable 48-byte descriptor as a
Word. Bootstrap validates version 1, exact size and all four operations,
then supplies its copy as an explicit predecessor to Runtime construction.
Storage is usable before Runtime exists; no dependency setter or later
initialization is needed. Id pins the native library after entering its initializer.
See [native Runtime support factories](runtime-support-v1.md).

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

Runtime issues the current value after checking its successor and completing
the existing charge/record steps. First handle: zero. Last handle: `u64::MAX - 1`.
At `u64::MAX`, another handle request returns `None`, while zero-count admission
and unhandled dead allocations retain their existing separate capacity rules.
Invalid indices and capacity failures consume neither handles nor charges.
Materialization is single use and advances the separate physical sequence.
Settlement, reclaim and budget reset do not reset deferred issuance. No global
or cross-Heap uniqueness policy is added.

There is no new language method, image field or public-format version. Heap
journals remain native process state. The cell ABI and image v8 format are unchanged.

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

The fixtures use the actual native entry for Storage and explicit Runtime
services for their constructors. Saved native methods are typed entries selected
through Id; they are never recovered by casting a language Method to a function.
The shared image host requires exact mapped/rebuilt outcomes and checks the idle
storage view before execution and its guarded result afterward. Storage forwarding
fixtures replace only their named factory result; all other requests reach the
installed native initializer unchanged.
