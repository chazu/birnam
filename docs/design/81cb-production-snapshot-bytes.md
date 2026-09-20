# 81CB — Large deferred-snapshot byte ownership

> **Status:** Deferred at the production extraction closeout. No implementation
> is shipped. 81CA (`ad9557fb`) remains the final runtime extraction in this phase.

The uncommitted prototype was removed on 2026-09-20. This boundary would add
three ABI operations and another startup dependency while leaving snapshot
preparation, sharing, journals and recovery in Id. That does not justify an
independent extraction round. Reconsider the buffer only as part of a coherent
allocation/materialization ownership change with a concrete Birnam consumer.
The audit below is retained as design evidence, not a queued next task. See the
[phase closeout](81-production-extraction-status.md).

Move only the backing allocation of `Payload::Heap(Vec<u8>)` in deferred clone
snapshots into Storage. Keep `Payload::Inline`, `DeadAllocation`, Rc sharing,
vtable bits, total size, thresholds, preparation, journals and materialization in
Id. Do not move numeric Batch storage or counted Frame-index multiplicities.

## Existing authority and ordering

`DeadAllocation::zeroed` derives padded size from total size. At eight bytes or
less, bytes live directly inside the untraced Rc allocation. Above eight, it
creates `vec![0; padded]` before allocating the Rc header. `prepare_dead_clone`
checks journal room, exact Heap membership, payload limit and current vtable,
then either copies into inline storage or creates `bytes.to_vec()` before the
Rc allocation. Preserve all checks, copying order and allocation order.

Other preparation paths write fresh all-Oop words or test bytes through
`Rc::get_mut` and the uniquely borrowed payload slice. The buffer stores exact
bits; it neither recognizes nor roots pointer-shaped contents. A retained
snapshot can be shared across many logical allocations without copying its
buffer. Materialization reads the original bytes and preserves charge timing,
header assignment, single-use identities and numeric fallback.

`INLINE_PAYLOAD_BYTES` stays one word (8 on the supported target), and
`MAX_DEAD_PAYLOAD_BYTES` stays 1024. Inline snapshots must acquire no separate
buffer or boxed native owner. Separate payload capacity remains zero for inline
snapshots and the actual buffer capacity for large snapshots. Native storage
accounting keeps the same snapshot header size and Rc bookkeeping term.

## Ownership interface

A per-snapshot boxed service would enlarge the header and add allocations. Use
an owned raw-buffer descriptor instead: pointer, length and capacity, 24 bytes
on the supported target. The C descriptor contains `uint8_t *data`, `size_t len`
and `size_t capacity`. It transfers ownership of a Rust Vec allocation created
inside Storage; it does not expose a Rust Vec object or retain an Id pointer.

Id's private owner stores a non-null data pointer and the two sizes, with no
per-owner factory copy. Its pointer niche preserves the existing Payload enum
size. A standalone layout probe with the proposed owner confirms 24-byte Payload
and 40-byte DeadAllocation geometry, equal to the current Vec representation.
Verify the actual implementation's size/alignment and inline byte location again
before considering this gate met.

Use an immutable 40-byte factory with version, byte size and three operations:

| Operation | Contract |
|---|---|
| `zeroed(length, out_owner)` | Execute the original zeroed Vec construction and transfer pointer/length/capacity |
| `copy(bytes, length, out_owner)` | Copy a synchronous byte loan with the original slice-to-Vec construction and transfer ownership |
| `destroy(owner)` | Reconstruct/drop exactly the returned Vec allocation in Storage |

The module adds no 1024-byte threshold, vtable check, byte charge or journal
policy. Only Id selects the large-payload branch. Module-level empty buffers are
valid and retain Vec's non-null empty-pointer convention. Id accepts an owner
only after checking its shape; metadata must remain unchanged until destruction.
Do not introduce a second allocation for a native owner header.

Id's slice accessors borrow the owned data using its exact length. Mutable loans
remain exclusive under the existing `Rc::get_mut` condition; they never resize
or change ownership metadata. Storage retains no input pointer or output-slot
address. No callback, Heap, Oop, Method, Rc header or image pointer enters the
module. Moving a snapshot and dropping one shared Rc alias preserve the same
buffer lifetime. The last Rc drop invokes Storage destruction exactly once.

Factory operations are resolved from the already attached process descriptor
when creating/destroying a buffer. Id must not add a factory copy to each owner
or silently change snapshot geometry. Same-descriptor repeat attachment remains
valid; retargeting stays forbidden and the host pins code for all owners.

Export `birnam_snapshot_bytes_factory_v1` from Storage and
`birnam_snapshot_bytes_dependency_v1` from Id. Prepare after Deferred Index and
before Id initialization, with inert exact-version/size/operation admission.
Update C/Rust fixture loaders, later admission fixtures and prior forwarding
spies so earlier checks still reach their intended gates.

Keep ordinary Siphon initializer order, 81 installed libraries, 38 native
manifests, Storage install name, IdApi v20, twelve-argument ModuleInit, Native
Loader v12 and image v8. Snapshot buffers remain native process state; image
recovery adds no buffer pointer or owner field.

## Adversarial review and completion gates

- Preserve 81CA source and installation. Run identical old/new witnesses over
  every size near 0/8/16 and the 1024-byte limit, all padding, pointer-shaped bits,
  zeroed and copied payloads, Rc aliases, drop order, out-of-order recovery,
  settlement/reclaim and failed preparation. Compare exact bytes, identities,
  charges, capacities, snapshot geometry and native-accounting transcripts.
- Verify no separate allocation below the inline cutoff. Verify the original
  large branch still creates one data allocation before its Rc header, with no
  hidden boxed owner, duplicate copy or eager materialization.
- Use an independent C descriptor to exercise all three operations, including
  empty buffers, zero initialization, nonzero copied data, mutable loans, moved
  descriptors and exact final destruction. Ownership copies are aliases only;
  destroy exactly once and never use data after release.
- Observe actual loaded large snapshot preparation and lifetime during closure
  and aggregate work. Track the real returned buffer and forwarded destruction;
  isolated buffer tests do not prove runtime integration. Retain unchanged
  inline/numeric behavior and mixed-journal capacity/exhaustion witnesses.
- Preserve Id decision bodies and every caller. Keep 81CA index repair and both
  identity sequences unchanged. Test that byte ownership does not create roots,
  hide finalizers, prolong dead snapshot sharing or change collection admission.
- Verify dynamic startup, source-free compiled callers and mapped/rebuilt/baked
  images executing actual deferred construction and recovery, with aliasing,
  shared Cells, live method changes, finalization, precise/conservative GC,
  eviction and reload. Construct transient portable Code programs after image
  load and preserve existing BUSY storage observation boundaries.
- Compare real image-backed workers and full export/linkage/install inventories.
  Verify source/artifact receipts, add ABI reference and focused task, update
  README/changelog, and commit before taking another slice. Release validation
  is waived.
