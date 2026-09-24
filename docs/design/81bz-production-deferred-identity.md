# 81BZ — Deferred-allocation identity storage

> **Allocation policy superseded (2026-09-24):**
> [Design 90](90-functional-modules.md#optimization-preserves-language-behavior-not-allocation-history)
> retires baseline allocation/count/GC-timing equivalence, dead-payload replay and
> prepaid deferred identities. Preserve language effects, observable identity,
> recovery and actual resource lifetimes. Older algorithms, receipts and commands
> below record history; they do not require restoring that machinery. See the
> [Self implementation review](90-self-optimization-review.md).

> **Status:** Completed production extraction after 81BY (`45bdd38f`).
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

## Implementation and evidence

Storage owns the zero-initialized per-Heap cell. Id retains both checked
successors, the shared handle sequence, capacity/index admission, all charging,
entry construction, searches, materialization, settlement and reclaim.
Reversing only storage-adapter syntax and private test setup reproduces the
preserved Heap, clone-journal and numeric-journal sources exactly. Another 85
Runtime, native-tier, Vtable, counted-frame and physical-identity files match the
baseline byte for byte. No new language method or production test hook is added.

The host and fixture loaders prepare/pin the descriptor after Allocation
Identity. Storage's semantic initializer remains stateless. Later cache-admission
fixtures acknowledge the dependency without hiding their original failure gates.
The independent C header, ABI reference, focused task and README document the
boundary.

Debug evidence:

- Core ABI, Storage, Object Model, Dispatch Cache, host and Id suites: 422 passed,
  16 ignored. The workspace all-targets check passes. The focused task passes,
  including Heap/journal, physical-identity and native-tier regressions.
- Four identical new witnesses pass against original and extracted storage;
  all 12 transcript lines match exactly. They cover interleaved clone/numeric
  handles beginning at zero, equal values/repeated batch indices, out-of-order
  single-use recovery, exact payload/padding bits, repaired journal indices,
  Rc release, charges, physical identities, settlement and reclaim. Capacity
  refusal, invalid indices, stale handles across batches and budget reset do not
  consume or reset the sequence. Moved/independent Heaps retain separate owners.
- Last handle `u64::MAX - 1` and exhaustion are tested through both journals.
  At `u64::MAX`, further handles return `None` without charging, while zero-count
  admission and unhandled dead allocations retain their existing behavior.
  Native module tests cover every bit position, arbitrary/repeated writes and
  independent destruction. ABI admission invokes no operation and rejects every
  invalid field and retargeting.
- All 12 new host-admission cases pass against workspace and installed libraries.
  Installed Allocation Identity, Dispatch Cache, Guard Verdict and Storage
  Observation regressions pass 12, 13, 12 and 14 cases. The Relational native-output
  contract passes through the updated Rust fixture loader and installed floor.
  The updated C loader also passes the actual physical-allocation/GC probe,
  last valid physical identity and both original fatal exhaustion paths.
- A harness-only forwarding Storage library observes the actual loaded Heap
  cell; every create/read/write reaches installed Storage. Attachment is inert,
  the one Heap starts at zero and every observed write is exactly the successor.
  Source and compiled callers match: disabled paths write zero; enabled closure
  work writes 386 times and numeric work 150 times; forced numeric recovery
  writes 75 times, with closure issuance still 386. Source-free compiled recovery
  matches. The fixture crosses the unchanged 64-entry hot threshold instead of
  lowering that production threshold.
- The original installed closure-elision suite passes ordinary fallback,
  enabled and forced recovery, including precise/conservative collection,
  observer entry, escaping identities, shared Cells, replacement and eviction.
- Actual old/new image-backed host workers match all 12 wire values and orderly
  shutdown byte for byte. Only the external harness waits for debug boot; no
  production deadline changes.

The image fixture constructs source-less portable Code programs using existing
ordinary Code APIs after each load. It does not try to persist temporary runtime
closures. An earlier fixture that constructed those closures before saving was
rejected by the same image preflight on both the baseline and extracted runtime.
The saved payload now contains the existing recoverable main/CodeBindings and
24 distinct methods. The saver asserts `#saved` before inspecting the image.

Dynamic Siphon startup and source-free compiled callers pass. Mapped/rebuilt
native Code images and baked executables reproduce the same exact 31-line
output, including all 24 saved method values and their alias. Authored sources
and the saver library are removed before recovery. The workload covers real
closure and numeric construction, fresh escaping identities, shared Cells,
method replacement, eviction, precise GC, finalization and reload, alongside
large-dictionary and active Frame/lookup regressions.

The actual host asserts `Mapped` for ordinary/baked loads and `SourceRecovery`
for rebuild. Additional forwarding-library runs observe the real Heap sequence
in all three modes: normal execution reports 386 clone writes and 150 numeric
writes; forced recovery reports 386 and 75. All six match their source/compiled
counterparts and all twelve image-carrier transcripts match byte for byte.
No wrapper policy or counter emulation replaces installed Storage operations.

Physical storage checks run before the workload, when the host is idle: exact
identities/extents survive rooted alias retention, GC and one-time finalization.
After the workload evicts materializations, the existing storage admission guard
returns `BUSY`; the fixture asserts that result and leaves its output untouched.
It does not force pending code retirement or weaken observation admission.

## Inventory and receipts

The inventory remains 81 libraries: 53 native and 28 compiled modules. All 38
native manifests match. Only Storage's deferred-identity factory and Id's
attachment export are added; both libraries link only system libraries and
Storage retains `@rpath/libbirnam_storage.dylib`. IdApi v20, twelve-argument
ModuleInit, Native Loader v12 and image v8 match the baseline exactly.

Preserved 81BY baseline: 516 source inputs and 555 installed artifacts.

- Source: `5d03e1080fe395f6fe2b44ff98485f050e56e852ccefebd1fec578ed7a498a06`
- Artifacts: `5af4ea2ccc1902fa18b3a1080d51d655544645a20a217e950ad90aa6dc07ccd5`

Final debug installation: 520 source inputs and 559 installed artifacts, verified
against current source and installed contents.

- Source: `e18010a0327ba3dc35707c3540f171963a8a5c9233b081eaa7ca4d97aca75fad`
- Artifacts: `16a9731d11206ea463fa5abe9203f056fc62c1ca08493cec6a7ba27b3e5cf50f`

No release validation was run.
