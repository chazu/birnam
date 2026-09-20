# 81CA — Deferred-clone index storage

> **Status:** Completed production extraction after 81BZ (`29880f1a`).
> Debug validation only; handle lookup, journal ordering and recovery stay in Id.

Move only `Heap::deferred_allocations`, the vector of deferred-handle/dead-journal
index pairs, into Storage. Leave the clone payload journal, numeric journal,
counted Frame indices, both identity sequences and every recovery decision in Id.
This is native ownership of two-word records; it neither identifies an Oop nor
changes which recorded allocation can be materialized.

## Current authority and ordering

`record_virtual_clone` checks the deferred successor, records and charges the
snapshot, captures the current handle, stores the successor, then appends its
handle/current dead-journal index. Preserve that order. Numeric results share
81BZ's counter but do not enter this index.

`materialize_recorded_clone` scans pairs from the front for the requested handle.
On a miss it calls the existing numeric reverse search. On a hit it removes the
matching pair with `swap_remove`, captures the last dead-journal position,
swap-removes the snapshot, scans all surviving pairs and repairs every index
matching that displaced position, then materializes the snapshot. Keep search,
fallback, mutation order, repair and physical publication in Id. Do not replace
the sequence with a hash map, cache a match, sort it or combine the journals.

Settlement takes the snapshot vector, clears this index, realizes each snapshot,
then handles numeric entries and counted Frame indices. Reclaim clears all
journals and this index. Neither clears the deferred sequence. `clear` retains
vector capacity. Existing native-storage accounting includes
`capacity * size_of::<(DeferredAllocation, usize)>()` even when empty. Preserve
that term and its place in the sum; add no payload charges for the native owner.

## Private interface

Use a C row containing `uint64_t identity` and `uintptr_t index` (16 bytes,
alignment 8 on the supported target). Neither field is a pointer or sentinel;
every bit pattern, duplicate identity and arbitrary row order is native data.
Id alone ensures issued-handle uniqueness and valid journal references.

A null context denotes an empty, unallocated vector. The first push allocates
its owner. Clear preserves ownership/capacity; moving a Heap must never retain
an address of its context slot. An immutable 80-byte descriptor has version,
byte size and eight operations:

| Operation | Contract |
|---|---|
| `destroy(context)` | Null no-op; otherwise free vector ownership exactly once |
| `len(context) -> size_t` | Null returns zero |
| `capacity(context) -> size_t` | Null returns zero; report actual vector capacity |
| `push(context_slot, row)` | Lazily allocate owner, then perform original Vec push |
| `read(context, position, out_row) -> int` | Copy present row; miss leaves output unchanged |
| `write(context, position, row) -> int` | Replace an existing row without interpreting it; miss changes nothing |
| `swap_remove(context, position, out_row) -> int` | Original Vec swap removal; miss leaves output/state unchanged |
| `clear(context)` | Null no-op; preserve retained vector capacity |

Only push grows native storage; destroy frees it. No callback, Oop, Rc snapshot,
Method, Heap, native lease or image pointer crosses the interface. Context-slot
and fixed-size output loans are synchronous and never retained. Use the same
Rust Vec element geometry/growth behavior. Empty reads must not allocate a boxed
owner. The extra carrier after first use is outside Heap payload accounting;
make no performance improvement claim.

Id's adapter may provide a copied-row iterator. The index-repair loop stays in
Id as an ordered read/conditional write loop; do not export an operation that
chooses which displaced journal index to repair. No native references or mutable
slices may escape across the ABI. Private tests can inspect copied rows instead
of reaching into Vec storage.

Export `birnam_deferred_index_factory_v1` from Storage and
`birnam_deferred_index_dependency_v1` from Id. Prepare after Deferred Identity,
before Id initialization. Preserve ordinary Siphon module initialization order.
Validate exact version/size and all eight operations inertly; accept the same
descriptor on repeat attachment, reject retargeting. Missing Storage still fails
at the earlier observation preparation. Later gate fixtures must provide all
earlier valid inert factory descriptors before testing this one.

Keep 81 installed libraries, 38 native manifests, Storage install name, IdApi
v20, twelve-argument ModuleInit, Native Loader v12 and image v8. The index is not
serialized. Existing image recovery reconstructs ordinary runtime state.

## Adversarial review and completion gates

- Preserve 81BZ source and installation. Run identical old/new witnesses with an
  independent ordered vector oracle, comparing exact pairs, journal lengths,
  payloads, both identity sequences, charges and retained native capacities.
- Cover empty/unallocated owners, first push, repeated growth, moved independent
  Heaps, middle/front/end recovery, displaced non-deferred snapshots, repair of
  every surviving row, numeric fallback, stale/duplicate materialization,
  settlement, reclaim, clear/regrowth and owner destruction after payload release.
- Reuse the mixed-journal and exhaustion witnesses. A failed admission must not
  create or grow the index. Unhandled clones and numeric-only work must leave an
  unused index unallocated; numeric batches still share only the handle sequence.
- Module tests cover all-bit rows, duplicates, arbitrary order, misses with
  untouched outputs, exact swap-removal order, unconditional row replacement,
  capacity retention and context-slot movement. Freeze the C descriptor/row ABI.
- Compare Id decision bodies against preserved source, explicitly accounting for
  the copied iteration and repair syntax. Keep all policy, Rc lifetimes, GC/root
  retention, observation exclusions, recovery maps and native-tier controls.
- Observe the actual loaded index during real closure construction and escaping
  recovery; a standalone container test does not prove production ownership.
  Show that numeric-only work does not append clone-index rows. Count operations
  only through harness forwarding of the real installed implementation.
- Verify ordinary Siphon startup, source-free compiled callers and mapped,
  rebuilt and baked images executing actual deferred construction/recovery.
  Retain fresh escaping identities, shared Cells, live method changes, aliases,
  precise/conservative GC, finalization, eviction and reload. Use portable Code
  constructors after image load; do not serialize temporary runtime closures.
- Observe physical storage only where the existing admission guard permits it.
  An evicted generation still retained by image main can keep retirement pending;
  that remains BUSY. Do not weaken this guard to make a fixture pass.
- Compare actual image-backed worker wire values; keep Heap/native-tier/guard
  and storage observation regressions. Check all exports, linkage, install names,
  public formats and final source/artifact receipts. Add ABI reference, focused
  task, README and changelog, then commit. Release validation is waived.

## Implementation and evidence

Storage owns the lazy Vec of 16-byte rows. Id's adapter starts with a null context
and returns copied rows; only push allocates the owner. Id retains forward
lookup, numeric fallback, swap-removal order and the displaced-index repair loop.
Clear retains ownership/capacity. The existing native-storage accounting term
and both identity services remain unchanged.

Reversing the field/constructor and copied iteration/repair syntax reproduces
the preserved Heap and clone-journal source exactly. The private test row
snapshot is the only other seam. Another 87 Runtime, native-tier, Vtable,
numeric/counted-journal and identity-service files match the baseline byte for
byte. No production test hook, language method or serialized index is added.

The host and C/Rust fixture loaders prepare the dependency after Deferred
Identity, before Id initialization. Later admission fixtures and the previous
identity forwarding probe acknowledge the new factory. Ordinary Siphon
initialization order is unchanged.

Debug evidence:

- Core ABI, Storage, Object Model, Dispatch Cache, host and Id suites: 428 passed,
  16 ignored. The workspace all-targets check and focused task pass.
- Three identical new witnesses pass against the original vector and extracted
  owner, with all 15 transcript lines matching exactly. They compare ordered
  rows, unique snapshot tags, payload bytes, journal indices, both identity
  sequences, charges and retained capacities. The witness grows through 52
  indexed clones mixed with unhandled snapshots and numeric entries, then
  recovers front/middle/end rows out of order. It checks actual displaced-index
  repair, numeric fallback, stale handles, repeated recovery, settlement,
  reclaim, regrowth, moved independent Heaps and final Rc release.
- Numeric-only/unhandled work fills the journal without allocating index
  capacity. Failed capacity admission, invalid numeric indices and exhausted
  handles leave the unused index unallocated. The previous mixed-journal and
  exhaustion witnesses also pass old/new: four tests and 12 exact transcript
  lines match.
- Storage tests cover null/no-allocation behavior, every bit position, duplicate
  identities, arbitrary replacement, miss outputs, all remaining rows after each
  swap removal, capacity retention and moved-owner destruction. The independent
  C caller exercises all eight operations against workspace and installed Storage,
  including writes, misses, 128 all-bit rows and exact removal order.
- All 16 new host-admission cases pass against workspace and installed libraries.
  Installed Deferred Identity, Allocation Identity, Dispatch Cache, Guard Verdict
  and Storage Observation admission checks pass 12, 12, 13, 12 and 14 cases.
  The updated C fixture loader preserves actual physical allocations, GC and
  last-valid identity. The Relational native-output contract passes through the
  updated Rust fixture loader and installed floor.
- A harness-only forwarding library observes the real Heap index and delegates
  every operation to installed Storage. It compares rows with an independent
  ordered C model after every mutation, checks reads and retained capacity, and
  observes lazy first allocation plus stable ownership. Source and compiled
  callers match: enabled closure work performs 386 pushes and 193 removals;
  forced recovery performs 386 pushes and 386 removals. Numeric-only work changes
  no clone-index rows; disabled paths perform no pushes/removals/writes. The
  source-free compiled recovery caller matches too.
- The previous actual identity-counter probe still passes: closure/numeric
  issuance remains 386/150 normally and 386/75 under forced recovery. The original
  closure-elision suite passes fallback, enabled and forced modes, including
  precise/conservative GC, collecting observation, escaping aliases, shared Cells,
  replacement and eviction.
- Actual old/new image-backed workers match all 12 wire values and orderly
  shutdown byte for byte. Only the external harness waits for debug boot.

Dynamic Siphon startup and source-free compiled callers pass. Mapped/rebuilt
native Code images and baked execution reproduce the same 31-line output,
including 24 saved method values and their alias. Authored source and the saver
library are deleted before recovery. Existing portable Code constructors create
the transient workload after loading, without serializing native journals.

The actual host asserts `Mapped` for ordinary/baked loads and `SourceRecovery`
for rebuild. It verifies idle physical identities, sizes, rooted aliases and
one-time finalization before the workload, and the existing `BUSY` observation
result after eviction. Active Frame reads/writes, custom lookup, large-dictionary
mutation, parent/behavior replacement, GC and reload retain their previous checks.

Six additional forwarding runs observe real index operations in every image
mode: normal closure execution performs 386 pushes and 193 removals, while
forced recovery performs 386 pushes and 386 removals. Numeric-only work performs
no clone-index pushes, removals or writes. All three loader modes match exactly
within each recovery setting; all twelve image-carrier outputs match byte for
byte. The forwarding harness compares real rows and capacity against its own
ordered model while leaving every actual allocation and mutation in Storage.

## Inventory and receipts

The inventory remains 81 libraries: 53 native and 28 compiled modules. All 38
native manifests match; only Storage's index factory and Id's attachment export
are added. Id and Storage link only system libraries; Storage retains
`@rpath/libbirnam_storage.dylib`. IdApi v20, twelve-argument ModuleInit, Native
Loader v12 and image v8 match the baseline exactly.

Preserved 81BZ baseline: 520 source inputs and 559 installed artifacts.

- Source: `e18010a0327ba3dc35707c3540f171963a8a5c9233b081eaa7ca4d97aca75fad`
- Artifacts: `16a9731d11206ea463fa5abe9203f056fc62c1ca08493cec6a7ba27b3e5cf50f`

Final debug installation: 524 source inputs and 563 installed artifacts, verified
against current source and installed contents.

- Source: `49dd52d2a257f131c119bd0ddf2f07cb839876cd0e52dc5dbfd53e364883c601`
- Artifacts: `c838d01f5048689391755766266457d10118b2707ffe7e59e8f7dd8f5cfb5d9c`

No release validation was run.
