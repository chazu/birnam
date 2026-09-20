# 81BZ — Deferred-allocation identity storage

> **Status:** Audited next extraction after 81BY (`45bdd38f`).
> Debug validation only; journal admission, charging and recovery stay in Id.

Move only `Heap::next_deferred_id`, a separate per-Heap `u64` cell, into the
existing Storage module. Keep every journal, payload, handle, admission check,
materialization path and native-tier decision in Id. Never combine this sequence
with 81BY's physical allocation identities: their ranges and failure behavior
are different.

## Existing authority

The cell begins at zero. `DeferredAllocation(0)` is a valid first handle. Id
computes a checked successor, issues the current value, and stores that successor
only after successful charging/recording. The last issuable value is
`u64::MAX - 1`. At `u64::MAX`, a request for one more identity returns `None`;
it does not abort, wrap or saturate. Id's zero-count admission remains true at
that value when the separate journal-capacity condition permits it.

| Consumer | Existing ordering to preserve |
|---|---|
| `record_virtual_clone` | Check successor; record/charge dead allocation; capture current identity; store successor; append deferred identity/index pair |
| `record_numeric_result` | Validate batch index and one-entry admission; check successor; charge; capture current identity; store successor; append numeric entry |
| `admits_virtual_allocations` | Check shared journal room first, then checked addition of requested count; never mutate the cell |

Failed journal admission or invalid numeric index must not consume an identity
or charge bytes. `record_dead_allocation` without a materialization handle and
counted dead Frame indices do not consume this sequence. Repeated numeric batch
indices and equal values still receive distinct executed-call identities.

Materialization consumes the matching handle once, preserves swap-removal
index repair and numeric reverse search, and realizes already-charged storage.
`materialize_raw` temporarily clears/restores the budget around ordinary Heap
allocation: that advances the physical identity sequence, not the deferred
sequence. Settlement and reclaim clear journal entries without resetting the
counter. Stale handles remain stale across later batches. These handles are
private to the owning Heap's callers; add no global uniqueness or cross-Heap
validation policy that the existing raw handle lacks.

Keep `MAX_DEAD_ALLOCATIONS = 4096`, payload limits, inline/heap payload choice,
Rc ownership, untraced payload bits, observer/collector exclusion, root retention,
recovery publication and fresh dispatch classification unchanged. Preserve all
native-tier modes and defaults. Physical storage observation remains unavailable
inside an admitted native interval or an active dispatch loan.

## Native storage interface

Add an immutable 48-byte factory to Storage: version, byte size and four
operations. Each Heap receives an independent opaque cell.

| Operation | Contract |
|---|---|
| `create() -> context` | Allocate zero-initialized native `u64` storage |
| `destroy(context)` | Free that cell exactly once |
| `read(context) -> uint64_t` | Read without mutation |
| `write(context, uint64_t)` | Assign supplied bits unconditionally |

The module does not issue handles, charge, check limits, admit work or own a
journal. Id retains all checked arithmetic and its exact position relative to
fallible operations. The factory supports every bit pattern, repeated writes
and arbitrary assignment order. Existing private exhaustion tests adapt their
setup to the storage seam; add no production forcing/reset export.

No Heap, Oop, Rc, payload, Method, native lease, callback or image pointer is
retained. Read/write allocate nothing and invoke no callbacks. Construction and
destruction allocate/free only the native cell. Preserve Heap field order: this
cell precedes the physical allocation-identity cell and Finalizers service.
Different/moved Heaps retain independent sequences and destruct safely without
consulting their journals or language storage.

Export `birnam_deferred_identity_factory_v1` from Storage and
`birnam_deferred_identity_dependency_v1` from Id. Pin/attach after Allocation
Identity and before Id initialization; keep ordinary Siphon initializer order.
Validate exact version/size and all operations inertly. Repeated attachment of
the same descriptor succeeds; retargeting fails. Missing Storage still fails
at its earlier observation preparation; invalid new-factory fixtures supply
valid inert observation and allocation-identity descriptors first.

Update C/Rust fixture loaders and later fake-Id admission fixtures. Keep the
same 81 installed libraries, 38 native manifests, Storage install name, IdApi
v20, twelve-argument ModuleInit, Native Loader v12 and image v8. Deferred journals
are not made image values, and no counter field/reset is added to recovery.

## Adversarial review and completion gates

- Preserve completed 81BY source and installation. Run identical witnesses on
  original and extracted storage, comparing exact handles, physical identities,
  payload bytes, journal order/repair, live counts and budget charges.
- Interleave virtual clones and numeric results to prove shared issuance across
  those two journals. Cover equal values/repeated indices, one-time and
  out-of-order materialization, stale handles, settlement, reclaim, fresh batches,
  independent moved Heaps and destruction.
- Exercise zero/last valid handles, zero-count admission at exhaustion, refused
  successor, capacity refusal and invalid indices. Verify that failed operations
  leave counters, journals and charges untouched. Unhandled dead allocations
  remain admissible independently of handle exhaustion where room remains.
- Module tests cover zero/all-bit storage, unconditional writes, independent
  lifetime and inert descriptor admission. Compare Id's original arithmetic,
  checks, charging, entry construction and every caller directly with preserved
  source; only field/adapter syntax and private test setup may differ.
- Observe the actual loaded Heap owner, including zero initialization and reads/
  writes caused by real deferred work. A standalone cell exercise or an ordinary
  allocation-only smoke is insufficient for this boundary.
- Exercise the existing production closure-elision and unboxed-numeric paths,
  with their current controls, ordinary fallback and forced recovery. Existing
  closure reports can establish executed deferred construction; any additional
  instrumentation stays in the harness. Keep ordinary installed-floor proof
  distinct from optional `ffi-test-hooks` diagnostic evidence.
- Startup, source-free compiled callers and mapped/rebuilt/baked images must
  execute actual deferred construction and recovery. Include escaping fresh
  identities, shared Cells, method replacement, allocation accounting, GC,
  finalization, eviction and reload, with explicit host loader outcomes. Preserve
  the dynamic Reflect path when constructing saved portable CodeBindings.
- Compare actual old/new image-backed workers; keep Heap/journal/native-tier,
  81BY physical identity, guard and observation regressions. Verify all exports,
  linkage/install names, public formats and final source/artifact receipts.
  Add ABI reference, focused task, README and changelog, then commit the completed
  extraction. Release validation is waived.
