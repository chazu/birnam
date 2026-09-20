# 81CA — Deferred-clone index storage

> **Status:** Audited next extraction after 81BZ (`29880f1a`).
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
