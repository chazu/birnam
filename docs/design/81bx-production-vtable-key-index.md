# 81BX — Derived Vtable key-index storage

> **Status:** Completed production extraction after 81BW (`d9ad8a66`).
> Debug validation only; no new language functionality or lookup policy.

Move only `VtableRecord::key_indices`, the derived address-to-position map,
into the existing Object Model module. Id keeps ordered keys, values, weak
flags, raw dictionaries, registry membership, mutation epochs and every lookup
and rebuild decision. An index position selects the current value in Id; the
module never owns or caches a Method.

## Existing authority

`key_index` scans ordered keys when the map is empty; otherwise it reads the
stored position. `KEY_INDEX_THRESHOLD` remains 16. Appending the seventeenth
key rebuilds the complete map; later appends insert only the last position.
Overwrite changes the current value without updating the index; weak opaque
overwrite also marks the entry weak.
Removal compacts all three ordered vectors, clears/rebuilds the map and
synchronizes the raw view. Missing removal still advances the mutation epoch.
Weak sweeps rebuild only changed dictionaries and advance the epoch once for a
nonempty whole sweep. Preserve all those decisions and their order in Id.

The threshold was introduced to avoid hash allocation for small dictionaries.
A boxed empty map per Vtable would undo that property. Default native context
is therefore null and denotes an empty, unallocated map. The first insert
allocates its owner; clear preserves the owner and HashMap capacity, just as
Id's current map retains capacity. An empty retained map again selects linear
lookup. Regrowth beyond 16 reuses that allocation. Read, empty and clear never
allocate. Keep the exact `AddressHasher` implementation, including its byte
fallback and address shift/multiply. Heap and Vtable registry address maps
remain unchanged in Id.

## Private storage interface

An immutable 56-byte factory contains version, byte size and five operations:

| Operation | Contract |
|---|---|
| `destroy(context)` | Null is a no-op; otherwise free the owned map exactly once |
| `is_empty(context) -> int` | Null returns 1; otherwise return normalized map emptiness |
| `get(context, key, out_index) -> int` | Hit writes the position and returns 1; miss returns 0 and leaves output untouched |
| `insert(context_slot, key, index)` | Allocate if the caller's context is null, update that slot, then insert or replace the position |
| `clear(context)` | Null is a no-op; otherwise clear entries while retaining capacity |

Keys and positions are inert `uintptr_t` values. Every bit pattern is valid;
there is no sentinel position. The synchronous writable context-slot loan and
output loan are never retained. Id owns its context field and serializes all
access as before. Moving records during registry growth must not invalidate an
index: the module owns the map, never the address of Id's field. Destruction
must work after Heap teardown. No root, Oop dereference, callback, semantic
helper, image pointer, weak policy or registration authority enters the module.
Only insert allocates/grows and destroy frees native ownership.

Export `birnam_key_index_factory_v1` from Object Model and
`birnam_key_index_dependency_v1` from Id. Prepare/pin the dependency immediately
after Mutation Epoch, before Method Observation and Id initialization. Admission
checks version, exact size and all five operations without calling them; repeat
attachment accepts the same descriptor and rejects retargeting. The factory
works before Object Model's ordinary semantic initialization. Missing the shared
library still fails at the earlier Vtable Enumeration preparation. Invalid
new-factory fixtures must supply inert valid Enumeration and Epoch descriptors
so they reach the intended gate.

Update C and Rust fixture loaders and later host-admission fixtures. Preserve
ordinary Siphon initialization order, 81 installed libraries (53 native and 28
compiled), 38 native manifests, Object Model's existing Cargo output-path install
name, IdApi v20, twelve-argument ModuleInit, Native Loader v12 and image v8.
Index ownership is not serialized; ordinary dictionary replay rebuilds it.

## Adversarial review

- A tiny Vtable must not acquire a boxed empty owner. At 17 entries, the map
  must contain every ordered key before indexed lookup resumes.
- Clear must keep existing allocation; a retained but empty map must select
  linear lookup. Removal can change every surviving position.
- Equal pointed-to bytes are not equal key identities. Address reuse must not
  encounter stale positions after removal/reinsertion. No key becomes a root.
- Weak compaction above the threshold must cover middle survivors and several
  removals, not only the existing nine-key linear-scan witness.
- Index hits must read current Id values after method replacement. Metabehavior,
  custom lookup, registration and cache guards retain fresh dispatch authority.
- Opaque ownership adds a native carrier for large dictionaries. This slice
  claims preserved behavior and allocation cutoff, not a measured speedup.
- Null and retained-empty ownership are different lifetime states with the same
  lookup result. Never cache a context-slot address across a record move.

## Independent proof and completion gates

Preserve 81BW source and installed prefix before implementation. Run the same
new witness on original and extracted storage, with a linear ordered oracle:
empty, 1..16, 17, append, overwrite, missing/front/middle/end removal, shrink,
regrowth, weak multi-removal, empty sweep, raw mirrors, selector order, capacity,
mutation epoch, moved/independent registries and destruction after Heap. Compare
exact transcripts. Keep existing randomized indexed dictionary tests, inherited
lookup, guard/Frame tests and native-tier regressions. Compare all Id decision
bodies and the original hasher directly with preserved source.

Module tests cover null ownership without allocation, all-bit keys/positions,
hit/miss output behavior, overwrite, clear/capacity reuse, independent moved
owners and exact hash results. An independent C descriptor pins the layout.
Observe the actual dynamically loaded map used by a real dictionary: no owner
before 17 keys, complete first build, later append, rebuilds, stable owner across
registry moves, and current values after replacement. Admission fixtures must
reject every invalid operation before Id initialization or any operation call.

Verify dynamic Siphon startup and source-free compiled callers with an actual
more-than-16-key target dictionary. Verify mapped/rebuilt native Code images and
baked execution with explicit host outcomes, exercising large dictionary
mutation, live parent/behavior changes, active Frame reads/writes, aliases, GC
and reload. Compare actual image-backed workers. Check full library/export/
linkage inventories and source/artifact receipts, update ABI reference, focused
task, README and changelog, then commit this extraction before selecting another.
Release validation is waived.

## Implementation and evidence

Object Model owns the nullable map context; no owner is allocated until Id's
first insert at 17 keys. The adapter changes only the Vtable field, constructor
and copied map-read expression. Reversing those three substitutions reproduces
the preserved Vtables source exactly, including all cutoff, rebuild and raw-view
logic. The original address hasher is copied byte for byte. Heap/registry maps,
Runtime, inherited lookup, lookup/slot guards and native-tier code are unchanged.

The host pins the new factory after Mutation Epoch. C and Rust fixture loaders
follow the same order. The independent C header freezes its 56-byte layout;
earlier admission fixtures acknowledge the dependency without changing their
original failure gates. The ABI reference and `mise run key-index` describe the
storage owner without adding language methods.

Debug evidence:

- Core ABI, Object Model, Dispatch Cache, host and Id suites: 405 passed,
  16 ignored. The workspace all-targets check passes.
- Two identical new witnesses pass against both the original map and extracted
  owner; all 38 exact transcript lines match. They compare every key against a
  linear oracle, ordered selectors/current values, weak flags, strong edges,
  raw mirrors, capacities and mutation epochs. Cases include every initial
  size through 20, overwrite, missing/front/middle/end removal, shrinking to
  16, regrowth, address reinsertion, registry growth/movement, independent
  teardown and 32-entry weak dictionaries compacted both above and below the
  threshold, through all-dead and repeated-empty sweeps.
- Module tests cover unallocated null ownership, every bit position in keys and
  values, unchanged miss outputs, overwrite, retained capacity after clear,
  moved slots, independent ownership and frozen address/byte hash results. ABI
  attachment tests reject null, wrong version/size, each missing operation and
  retargeting without calling an operation.
- The real loaded dictionary probe passes for workspace and installed modules.
  Attachment invokes no operation. Empty and 1..16-entry dictionaries allocate
  no owner; 17 entries cause exactly 17 inserts and one allocation, and the
  eighteenth adds once. Overwrite reads the current Method without rebuilding;
  absent removal does not rebuild. Present removal repairs positions, shrinking
  selects linear scans, and regrowth reuses the owner. Growing the actual
  registry through 4096 Vtables moves the context field while preserving its
  owned map and correct positions.
- All 13 new host-admission cases pass for workspace and installed libraries.
  Missing Object Model fails at Vtable Enumeration; each invalid new descriptor
  fails before initialization. Installed Mutation Epoch, Guard Verdict,
  Dispatch Cache, Allocation Budget, Collection Phase and Storage Observation
  host regressions pass their 12, 12, 13, 14, 12 and 14 cases. The Relational
  native-output fixture passes through the updated Rust loader.
- Dynamic startup, source-free compiled callers and mapped/rebuilt/baked images
  exercise 24-key dictionaries with overwrite, removal through the threshold,
  regrowth, live parent/behavior replacement, active Frame reads/writes, custom
  lookup, aliases, precise GC and reload. The saved dictionary has 24 distinct
  portable methods; recovery checks every result, 0 through 23, and its alias.
  Authored sources and the saver module are deleted before image recovery.
  The real host asserts `Mapped` for ordinary/baked loads and `SourceRecovery`
  for explicit rebuild; all three then match the complete 30-line transcript.
- Old/new actual image-backed workers match all 12 wire values and orderly
  shutdown byte for byte. The external debug wait leaves production deadlines
  unchanged.

The saved-dictionary fixture deliberately sends through a local `Reflect`
receiver to install ordinary portable CodeBindings. Compiled-module top-level
literal `Reflect` definitions take the separate artifact installation path;
that initial probe lost its methods at recovery in the preserved 81BW build as
well, both with and without an explicit global root. The ordinary dynamic send
preserves all 24 results on both builds. This extraction changes neither
compiler nor image installation semantics.

## Inventory and receipts

The installed inventory remains 81 libraries: 53 native and 28 compiled. All
38 native manifests match. The only export additions are Object Model's key
index factory and Id's dependency. Both libraries remain system-linked only;
Object Model's Cargo output-path install name is unchanged. Public admission
contracts match exactly, including IdApi v20 and image v8; ModuleInit remains
twelve arguments and Native Loader remains v12.

Preserved 81BW baseline: 508 source inputs and 547 installed artifacts.

- Source: `b8a6643e7ff150c63d6797c31535c2012944569f3b52670db4139bd973c6df66`
- Artifacts: `cf1573d39419f32b053518bf0153df0f24cba1330a5549517bea9af6c275c68e`

Final debug installation: 512 source inputs and 551 installed artifacts,
verified against current source and installed contents.

- Source: `e70241bb25681871f210af47206ffd705d80848e7abc96f2abfa354794285c28`
- Artifacts: `50393b8141a15d1d1108510b54329ca27dddfbbb798c0d8863fd4ef96e1a4308`

No release validation was run.
