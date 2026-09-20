# 81BU — Inherited-lookup cache ownership

> **Status:** Audited next extraction after completed 81BT (`d0e6ec30`).
> Production migration pending. Preserve behavior; debug validation only.

Move the bounded inherited dictionary-answer cache in
`runtime/dispatch_cache.rs` into a new `birnam-dispatch-cache` module. Keep
canonical lookup eligibility, direct dictionary lookup, mutation authority,
collection-phase admission, target classification and executable acquisition
in Id. This extracts the existing cache's storage and matching mechanism; it
does not complete the broader optional-optimizer separation in 81C/81E.

The 64-entry table is a small boundary with no callback, heap ownership or
execution-engine dependency. The neighboring canonical-lookup and Machine-slot
guard verdict cells remain in Id for a later audit. Moving execution registries
or inventing a combined optimization service would cross larger lifetime and
semantic boundaries than this slice needs.

## Existing sequence to preserve

`Runtime::cached_default_lookup` is entered only after canonical lookup has been
proved and dispatch-read authority acquired. Its existing sequence is:

1. The test-only disabled mode delegates to ordinary dictionary lookup.
2. Query the receiver shape's direct dictionary. Return its answer immediately,
   including a direct terminal miss, without probing or writing the cache.
3. Read the authoritative vtable mutation epoch. If it is `u64::MAX`, or the
   Runtime collection phase is not Idle, perform inherited lookup without a
   cache probe or write.
4. Compute `((selector_address >> 4) ^ (selector_address >> 10)) & 63`.
   A hit requires the slot's epoch, selector address and shape address all to
   match. Return its copied Method address on a hit.
5. On a miss, perform the original inherited lookup from the already-selected
   parent. Write the slot only for a present Method. A missing method does not
   clear a colliding entry or install a negative result; DNU is resolved normally.

The test-only HITS/MISSES counters keep these meanings and positions. Cache
hits are dictionary answers, not acquired call targets. Registration can change
classification without changing the vtable epoch; every send still classifies
and acquires its current target after lookup. No memoized result may bypass an
unpublished registration rejection, a resumable carrier or an executable lease.

GC invalidation stays in `runtime/tracing.rs::roots`, after native and initial
root emission and before floor/provider roots. Do not move it to collection
entry or sweep, and do not make the cache's pointer-shaped metadata into roots.
The cache must not keep unreachable objects or retired code alive.

## Module-owned table and narrow factory

Each Runtime owns one opaque native table, initialized to the existing empty
entries: epoch `u64::MAX`, zero shape and selector keys, no Method. Preserve
64 slots and direct collision replacement. There is no resizing, LRU policy,
negative caching, prewarming, global table or per-thread shared table.

Use an immutable 56-byte v1 descriptor: version, byte size and five operations:

| Operation | Contract |
|---|---|
| `create() -> context` | Allocate one empty table |
| `destroy(context)` | Free that table exactly once |
| `lookup(context, epoch, shape, selector) -> method` | Select the existing hash slot and return its Method pointer only on exact key/epoch match; null means miss |
| `insert(context, epoch, shape, selector, method)` | Replace that slot with the supplied keys and non-null Method pointer |
| `clear(context)` | Restore every slot to the original empty entry |

Shape and selector are inert `uintptr_t` keys. Methods cross as `void *`, using
Id's existing `MethodAddress::as_oop`/`from_oop` conversions. Preserve pointer
provenance; do not encode a Method as an integer or create a Rust reference to
code. The module never dereferences, invokes, classifies, pins or frees a stored
Method, shape or selector. Insert requires a non-null Method supplied by Id.
An empty entry is a miss, including when the sentinel epoch and zero keys are
queried directly by a native test. Production still bypasses saturated epochs
at its original Id call site.

Operations require a live owner and existing owner-thread serialization. Only
create/destroy allocate/free native memory. Lookup, insertion and clearing are
callback-free and allocation-free. No reference to the table or an entry crosses
the call boundary. A moved Runtime retains its owner; independent runtimes and
loaded floors do not share table contents.

The Id adapter replaces only table operations. Its existing dictionary reads,
parent selection, phase/epoch checks and hit/miss accounting stay in place.
No Runtime, Heap, Vtables, ExecutionState or source callback enters the factory.
The slot-guard and canonical-lookup verdict caches remain unchanged.

Export `birnam_dispatch_cache_factory_v1` from the new module and
`birnam_dispatch_cache_dependency_v1` from Id. Validate version, exact size and
all five operations inertly; accept repeat attachment of the same descriptor,
reject retargeting, and pin descriptor/code before Id initialization. Add the
ordinary twelve-argument Siphon initializer with no language methods or exports;
place it after Allocation Budget while preserving every existing initializer's
relative order. Update workspace, installer, export manifest and native fixture
prerequisites. The installed count becomes 81: 53 native libraries and the same
28 compiled modules. No public IdApi field, native-loader revision or image
record changes. New processes create empty tables when loading saved images.

## Adversarial review

- Fresh dispatch: changing a method, parent, receiver behavior, global lookup or
  private metabehavior must select the current ordinary behavior. Keep direct
  lookup ahead of caching and do not cache custom lookup effects.
- Identity and lifetime: address bits are keys only. Preserve invalidation at
  the exact GC root-emission site, including failed/early collection paths.
  Store neither language roots nor executable-owner references in the table.
- Registration: a warm dictionary answer must be classified again when native
  registration changes without a dictionary mutation. Keep acquisition and
  release outside the table, including nested mutation/reentry and retirement.
- State: collisions replace one slot, misses do not overwrite it, clearing
  restores all 64 entries, and one Runtime's clear/destruction does not affect
  another. Do not share the two unrelated guard verdict cells with this table.
- Recovery: saved images carry no cache entries or pointers to native owners.
  Reconstructed execution must fill a fresh process-local table through the
  same lookup path, with no source-only shortcut or forced disabled-cache mode.

## Independent proof and completion gates

Preserve completed 81BT source and installed debug artifacts before migration.
Use the same Runtime witness against old and new implementations and retain the
existing cache-enabled/cache-disabled oracle. Compare results, lookup effects,
HITS/MISSES, collection admission and entry/Frame pin counts under direct and
inherited lookup, collisions, repeated misses, method/parent changes, custom
lookup, private metabehavior, saturated epochs, GC and nested reentry.

Retain `dispatch_cache/tests.rs`, `guarded_lookup/tests.rs`, guarded slot tests
and the three-phase cache checks in `collection_phase_tests.rs`. Module tests
cover every slot, collision replacement, epoch/shape/selector mismatch, sentinel
values, no negative caching, independent/moved owners and clearing. Preserve
the unpublished and published registration cases that change classification
without requiring a dictionary mutation. Do not substitute module arithmetic
or pointer round trips for actual Runtime dispatch tests.

A loaded-factory observer must verify actual Runtime creation, target-specific
lookup/insertion and GC clearing in the existing order. It must observe direct,
phase and saturated-epoch bypass, not merely exercise a standalone table. Test
inert host admission and missing/invalid dependencies before Id initialization.

Verify the new library during real Siphon startup and source-free compiled
callers, mapped/rebuilt saved native Code images and baked execution. These
consumers must exercise inherited lookup, warm mutation, aliases, GC and module
reload through ordinary sends; arithmetic alone does not demonstrate this
boundary. Compare actual image-backed worker transcripts, inspect all old/new
exports and linkage, and verify final source/artifact receipts. Add the ABI
reference, focused task, README links and changelog. Commit completed 81BU
before selecting another extraction. Do not run release validation.
