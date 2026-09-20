# 81BU — Inherited-lookup cache ownership

> **Status:** Completed production extraction after 81BT (`d0e6ec30`).
> Debug validation only; no lookup, acquisition or image-format change.

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
lookup/insertion, direct/phase bypass and GC clearing in the existing order,
not merely exercise a standalone table. Saturated-epoch bypass is exercised
through the Runtime witness using the existing test-only epoch-exhaustion
setter; do not add a production export for forcing saturation. Test inert host
admission and missing/invalid dependencies before Id initialization.

Verify the new library during real Siphon startup and source-free compiled
callers, mapped/rebuilt saved native Code images and baked execution. These
consumers must exercise inherited lookup, warm mutation, aliases, GC and module
reload through ordinary sends; arithmetic alone does not demonstrate this
boundary. Compare actual image-backed worker transcripts, inspect all old/new
exports and linkage, and verify final source/artifact receipts. Add the ABI
reference, focused task, README links and changelog. Commit completed 81BU
before selecting another extraction. Do not run release validation.

## Completed implementation and evidence

`birnam-dispatch-cache` owns the 64-entry table and its matching, replacement
and clearing operations. Id's adapter retains Method pointers as pointers and
keeps dictionary reads, phase/epoch bypass and hit/miss accounting in place.
The default table cannot produce a present empty entry: Id inserts only a
`Some(MethodAddress)`, and saturated epochs still bypass before probing.

The host pins the factory before Id initialization and loads the ordinary
module after Allocation Budget. The workspace, installer, native manifest,
C/Rust fixture preparation and 38 focused build prerequisites include it.
The new library uses the standard `@rpath` install name. The existing Siphon
initializer order is otherwise byte-for-byte unchanged. Runtime guard logic,
the tracing/invalidation site, guarded canonical lookup and Method pointer
conversion implementations are unchanged.

Debug evidence:

- ABI, Dispatch Cache, host and Id library tests passed: 386 tests, 16 ignored.
  Workspace build and all-target workspace check passed. Existing enabled/
  disabled dispatch, guarded lookup and guarded Machine-slot tests remain in
  the Id suite, including changed registration without dictionary mutation.
- The new ownership witness passed against archived 81BT source augmented only
  with the same test. All twelve old/new transcript lines match: direct bypass,
  warm inherited entries, colliding repeated misses without negative caching,
  positive collision replacement, both phase bypasses, method/parent mutation,
  GC repopulation, saturated epochs, independent/moved owners and destruction.
  Neither run retained Frame pins or acquired entries.
- Native table tests fill and clear all 64 slots, distinguish epoch/shape/key
  mismatches, replace collisions, cover sentinel values and prove independent
  owner lifetime. The Runtime GC test checks the warmed entry at its original
  pre-collection epoch, so epoch invalidation alone cannot satisfy clearing.
- The loaded-factory observer confirms no context at inert attachment and one
  actual Runtime owner after Id initialization. It observes target-specific
  miss/insert/hit calls, direct bypass, Marking/Finalizing bypass, warm mutation
  and collection clearing after native admission but before providers. A root
  callback performs ordinary dispatch without probing the cache, and the next
  post-collection send repopulates it.
- All thirteen host admission cases pass, including the installed final prefix:
  missing library/provider, null descriptor, version/size/five-operation
  rejection, missing/rejected floor attachment and successful initialization.
- The CLI image smoke passes dynamic Siphon startup, source-free compiled
  callers, mapped/rebuilt saved native Code images and baked execution. Its
  native consumer exercises inherited sends, warm method/parent mutation,
  shadow removal, custom lookup effects, automatic collection, native roots,
  aliases and reload. An independent reader requires a v8 native Code record;
  the saver source and dylib are absent during image execution.
- A real-host consumer additionally requires `Mapped` for normal and baked
  loading and `SourceRecovery` for explicit rebuilding, then runs image main
  and compares its complete transcript. These checks passed against the final
  installed artifacts; identical output alone is not used to infer mapping.
- Actual old/new image-backed workers pass twelve exact wire-v1 round trips and
  orderly stop with byte-for-byte equal complete transcripts. The external
  debug harness leaves production handshake deadlines unchanged.
- Independent storage-observation, storage-tracing and live-tracing C fixtures
  produce equal old/new transcripts, preserving extents, identity, description
  validation, coverage, reentry, native roots, LIFO admission and failure before
  sweep. Storage Observation Scope, Collection Phase and Allocation Budget
  regression gates pass, including their 14/12/14 host admission cases.
- Installation contains 53 native libraries and the same 28 compiled modules.
  All 80 earlier libraries retain their exports except Id's declared dependency;
  the new module's three exports and all 37 native manifests match. Id and
  Dispatch Cache link only system libraries. Old/new installed admission
  contracts match, including IdApi v20, runtime ABI and image v8. Native
  initialization remains twelve arguments and Native Loader remains v12.
- All 499 source inputs and 538 installed artifacts match their receipts.
  Source digest:
  `8bbc5941659800904004f0faa5a576f7d18401e7d332b1bbcdb9b3ae6243d017`.
  Artifact digest:
  `ef646caede66c54ba432f9c39da650a0e27b40188d87bd148b5d2a8768d4d831`.

The [v1 reference](../../abi/dispatch-cache-v1.md), independent C declaration,
`mise run dispatch-cache`, image smoke, README links and changelog accompany
the extraction. Local evidence and prefix markers are `.cargo-target/81bu-*`.
No release or dispatch performance qualification was run.
