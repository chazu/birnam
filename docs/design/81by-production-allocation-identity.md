# 81BY — Heap allocation-identity storage

> **Status:** Audited next extraction after 81BX (`d20a499a`).
> Debug validation only; identity issuance and allocation behavior stay in Id.

Move only `Heap::next_allocation_identity`, the per-Heap `u64` storage cell,
into the existing Storage module. Id retains the raw allocation, checked
successor, 59-bit exhaustion bound, metadata encoding, exact-address registry,
byte charging and all readers of identities stored in individual blocks.
This follows the Vtable storage extractions without moving Heap membership or
changing what constitutes an allocation.

## Existing authority and order

`Heap::new` initializes the cell to zero, after `next_deferred_id` and before
the Finalizers service. `Heap::allocate` computes checked aligned size, creates
and zeroes the raw allocation, derives the payload address, then computes the
checked successor and rejects values above `u64::MAX >> 5`. Only then does it
insert the Block, packing the identity in bits 5..63, padding in bits 2..4 and
permanence in bit 1. Mark bit 0 begins clear. The duplicate-address invariant
and allocation-budget charge follow insertion. Preserve this exact ordering,
including the `allocation identity exhausted` fatal diagnostic after raw
allocation. The first issued identity is 1; the last is `(1 << 59) - 1`.

An identity is never reused within a Heap, including when the allocator reuses
a dead address. Clearing marks, changing permanence, detaching/reclaiming,
Vtable mutation, finalizer work and storage observation do not reset this cell.
The counter is distinct from deferred-allocation IDs, Runtime observation IDs,
Vtable mutation epochs and code-generation identities. Do not consolidate them.

`Block::identity()` continues shifting its own metadata. Storage descriptions
continue reading those block values, not the current next-identity cell. Keep
four-word Block layout, raw allocation/header layout, all padding rules, address
provenance, GC admission and journal retention unchanged.

## Private storage interface

Storage exports an immutable 48-byte factory: version, byte size, then four
operations. Each Heap owns a separate opaque cell:

| Operation | Contract |
|---|---|
| `create() -> context` | Allocate a zero-initialized `u64` cell |
| `destroy(context)` | Free the cell exactly once |
| `read(context) -> uint64_t` | Read without mutation |
| `write(context, uint64_t)` | Assign supplied bits unconditionally |

The module does not issue, increment, saturate, clamp or validate identities.
The checked addition and limit remain in Id. All bit patterns and repeated or
reordered assignments are native cell data; production Id supplies only its
existing successful successor. Use no new production exhaustion/reset export.
A private test seam may drive the existing failure path in an isolated process.

Read/write allocate nothing and invoke no callbacks. Construction/destruction
allocate/free only the native cell; the Heap's accounted payload bytes exclude
this carrier just as they exclude its other native registry/service carriers.
No Heap pointer, Oop, raw block, image pointer, finalizer, callback or Method is
retained. Moving or dropping a Heap preserves independent ownership and requires
no live language storage. Preserve construction and field-drop order.

Use `birnam_allocation_identity_factory_v1` in Storage and
`birnam_allocation_identity_dependency_v1` in Id. Pin/attach immediately after
Storage Observation and before Id initialization. Validate exact size, version
and all operations without invoking them; accept repeat attachment of the same
descriptor and reject retargeting. The factory must work before Storage's
ordinary initializer, without initializing semantic helpers.

Missing the shared Storage library still fails at its earlier observation
factory. Invalid new-factory fixtures must provide a valid inert observation
descriptor to reach the new gate. Update C/Rust fixture preparation and later
fake-Id admission fixtures, keeping all existing gates reachable. Preserve
ordinary Siphon initializer order and the same 81 libraries, 38 manifests,
IdApi v20, twelve-argument ModuleInit, Native Loader v12 and image v8. The cell
is not a new image field; ordinary Heap construction and allocation replay
continue issuing identities.

## Adversarial review and completion gates

- Preserve completed 81BX source and installed prefix before editing. Compare
  identical witnesses against the original scalar and extracted owner.
- Check exact identities, total/payload sizes, all eight padding possibilities,
  permanence, mark state, roots/finalizers, live byte charging, detachment and
  observed address reuse. Sorting snapshots by identity avoids treating hash
  iteration or allocator addresses as observable order.
- Verify first/last valid issuance and the next allocation's exact fatal result
  in isolated test processes. Include checked-add failure as well as the 59-bit
  bound; never turn exhaustion into wraparound or saturation. Compare placement
  after raw allocation and before registry insertion/charging directly in source.
- Distinct/moved Heaps start at zero independently and never borrow another
  owner's counter. Module tests cover zero, all bits, unconditional assignment,
  independent owners and native destruction without Heap access.
- Loaded probes must observe the actual Id Heap cell during bootstrap and later
  allocations: inert attachment, one owner, first write 1 and exact successors,
  with unchanged block identities in storage reports. Do not substitute an
  isolated cell-only probe for real allocation evidence.
- Keep existing Heap, weak-GC, allocation-budget, observation-scope, guard and
  native-tier regressions, including deferred/retained allocation journals and
  allocation identity consumers. All policy and metadata expressions must match
  preserved source except the storage adapter substitutions.
- Verify dynamic Siphon startup, source-free compiled callers, mapped/rebuilt
  native Code images and baked execution with explicit loader outcomes. Exercise
  allocation, aliases, precise GC, finalization, live behavior, large dictionaries
  and reload; compare actual image-backed worker transcripts.
- Check all library exports/linkage/install names and final source/artifact
  receipts. Add the ABI reference, focused task, README and changelog, update
  Storage's manifest scope, then commit before the next extraction. No release
  validation is required.
