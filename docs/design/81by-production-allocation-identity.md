# 81BY — Heap allocation-identity storage

> **Superseded by Design 82/83:** allocation identity is again internal to Id.
> The Storage factory and pre-Id attachment described below have been removed.
> This document preserves the historical extraction contract.

> **Status:** Completed production extraction after 81BX (`d20a499a`).
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
a dead address. Clearing marks, changing headers, detaching/reclaiming,
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

## Implementation and evidence

Storage owns one opaque cell per Heap. Id changes only the field type,
constructor, checked successor's read/store and metadata read. Reversing those
four production substitutions reproduces the preserved Heap source exactly;
the private test setup seam is the only other adaptation. Block layout and
metadata, all raw allocation/registry/charging expressions, deferred journals,
Runtime storage observation, Vtables and native-tier/guard callers are unchanged.
Storage's existing implementation is unchanged apart from the module declaration
and factory re-export.

The host and both fixture loaders prepare/pin the factory immediately after
Storage Observation. Later cache-admission fixtures now acknowledge it while
retaining their original failure gates. The ABI reference, independent C header,
focused task, README and changelog describe native storage ownership without
adding language methods.

Debug evidence:

- Core ABI, Storage, Object Model, Dispatch Cache, host and Id suites: 415 passed,
  16 ignored. The workspace all-targets check passes.
- Four identical new Heap witnesses pass on original and extracted storage;
  all 12 exact transcript lines match. They cover zero/first/last identities,
  all eight padding possibilities, permanent/collectable blocks, four-word
  Block geometry, zeroed payloads, header changes, marking, finalizer attachment,
  detachment, callback/reclaim, unchanged charging, budget reset and independent
  moved owners. Across 256 allocate/reclaim cycles, the allocator actually
  reuses addresses while identities remain strictly new.
- The original last permitted identity is `576460752303423487`; the witness
  checks its packed metadata. Isolated child processes test both the 59-bit
  limit and `checked_add` overflow, each producing SIGABRT and the exact
  `allocation identity exhausted` diagnostic. No production forcing export is
  added. Module tests independently cover all-bit/reordered/repeated stores,
  zero initialization and independent owner destruction.
- The complete focused task passes after fixture corrections. The actual loaded
  Heap probe passes for workspace and installed libraries: attachment invokes
  no operations; Id creates one zeroed owner; every bootstrap/later write is
  the exact successor. Sixteen explicit allocations each advance once and
  storage reports preserve their exact identities and extents through GC.
  An unrooted finalizer runs exactly once; GC/observation do not reset issuance.
  Last-valid and both fatal exhaustion cases also pass with installed libraries.
- All 12 new host-admission cases pass in workspace and installation. A missing
  Storage library fails at the earlier observation factory; invalid new
  descriptors and rejected/missing Id dependencies fail before initialization.
  Installed Dispatch Cache, Guard Verdict and Storage Observation regressions
  pass their 13, 12 and 14 cases. The Relational native-output fixture passes
  through the updated Rust fixture loader and real installed floor.
- Dynamic Siphon startup and source-free compiled callers pass. Mapped/rebuilt
  native Code images and baked execution reproduce the exact 30-line output,
  including all 24 distinct saved method values and their alias. The consumer
  exercises fresh allocations, finalization, large-dictionary mutation, live
  parent/behavior replacement, active Frame reads/writes, custom lookup, GC and
  reload. Authored sources and the saver module are deleted before recovery.
- The real host asserts `Mapped` for ordinary/baked loads and `SourceRecovery`
  for rebuild. After main returns, its native idle check observes exact block
  identities/extents, preserves a rooted alias through collection/finalization,
  and confirms that the next allocation receives a greater identity. During
  active dispatch the consumer instead asserts the existing `STORAGE_BUSY`
  result; it does not relax the observation boundary. The standalone collection
  probe supplies a live stack anchor, preserving the existing zero-anchor
  suppression rule.
- Old/new actual image-backed host workers match all 12 wire values and orderly
  shutdown byte for byte. The external debug wait changes no production deadline.

## Inventory and receipts

The inventory remains 81 libraries: 53 native and 28 compiled modules. All 38
native export manifests match, with only Storage's allocation-identity factory
and Id's dependency added. Both libraries link only system libraries; Storage
retains `@rpath/libbirnam_storage.dylib`. Public admission contracts match exactly:
IdApi v20, twelve-argument ModuleInit, Native Loader v12 and image v8.

Preserved 81BX baseline: 512 source inputs and 551 installed artifacts.

- Source: `e70241bb25681871f210af47206ffd705d80848e7abc96f2abfa354794285c28`
- Artifacts: `50393b8141a15d1d1108510b54329ca27dddfbbb798c0d8863fd4ef96e1a4308`

Final debug installation: 516 source inputs and 555 installed artifacts,
verified against current source and installed contents.

- Source: `5d03e1080fe395f6fe2b44ff98485f050e56e852ccefebd1fec578ed7a498a06`
- Artifacts: `5af4ea2ccc1902fa18b3a1080d51d655544645a20a217e950ad90aa6dc07ccd5`

No release validation was run.
