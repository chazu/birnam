# 81BW — Vtable mutation-epoch ownership

> **Status:** Completed production extraction after 81BV (`1856504b`).
> Debug validation only; mutation policy and public formats are unchanged.

Move the single `Vtables::mutation_epoch` storage cell into the existing Object
Model module. Keep `Vtables::note_mutation`, its saturating increment, every
mutation call site and every reader in Id. This follows the inherited lookup
and guard-verdict extractions without moving dictionaries, method ownership or
optimizer policy. The epoch belongs to one Vtables registry, not a global
counter or any individual cache.

## Existing mutation boundary

`Vtables::new` initializes the epoch to zero. `mutation_epoch()` reads it;
`note_mutation()` performs exactly one `saturating_add(1)` and assignment.
The existing test-only exhaustion helper assigns `u64::MAX`. Saturation never
wraps, resets or authorizes cache reuse. Preserve Id's saturation operation and
its exact position relative to dictionary changes and checks.

| Site | Existing placement and count |
|---|---|
| `add_method` | One increment before mutable record lookup, including overwrite |
| `add_weak_opaque_binding` | One increment before mutable record lookup, including replacement |
| `remove_method` | One increment before mutable record lookup and key search, including an absent key |
| `set_parent` | Validate parent membership, replace parent, synchronize raw view, then increment; assigning the same parent still counts |
| `remove_dead_collectable` | Remove records and compact order; increment once only if the dead set is nonempty |
| `remove_dead_weak_bindings` | Compact all affected dictionaries and raw mirrors; increment once only if at least one binding was removed |
| `Runtime::reassociate_behavior` | Owner/allocation admission, behavior validation and successful Heap header reassociation, then increment |

Creating a Vtable alone does not call `note_mutation`. Owner assignment,
payload-layout changes and trace-hook replacement do not advance this epoch.
Keep those absences as well as the positive call sites. Do not infer new bumps
from the broader wording of the existing registry-mutation comment. Failed
operations retain their current ordering, results and invariant diagnostics.

Id's inherited lookup table, canonical lookup/slot guards and native-tier
readers continue calling the existing accessor at the same sites. Keep live
receiver behavior, registration checks, cache admission and fallback untouched.
The test-only deoptimization/read-reuse experiments retain their current access
paths. No epoch may be cached in an adapter or shared between Runtime owners.

## Native storage owner

Add an immutable 48-byte factory to Object Model: version, byte size, then four
operations. Use a separate opaque owner for every Vtables registry.

| Operation | Contract |
|---|---|
| `create() -> context` | Allocate one cell initialized to zero |
| `destroy(context)` | Free that cell exactly once |
| `read(context) -> uint64_t` | Return the stored epoch without changing it |
| `write(context, uint64_t)` | Assign the supplied epoch unconditionally |

The factory owns storage only. Saturating arithmetic and which actions count
as mutations remain in Id. All bit patterns, repeated writes and arbitrary
assignment order are supported by the cell. Id's production writer continues
to supply only its saturating successor. The private exhaustion helper remains
`cfg(test)` and uses the same ordinary storage write; add no new runtime
forcing export, reset operation or alternate counter policy.

The Id adapter preserves zero initialization and translates the current field
read/assignment into `get`/`set`. Its construction stays in the existing field
initialization order, before layout and trace-hook capabilities. Moving a
Vtables value preserves its owner; dropping it frees only native integer
storage. No Heap, Vtable, Runtime, Method, callback or image pointer is retained.
Read/write allocate nothing and invoke no callbacks. Creation/destruction are
the only allocation/free operations. Existing owner-thread and mutable-registry
serialization remain the access authority.

Export `birnam_mutation_epoch_factory_v1` from Object Model and
`birnam_mutation_epoch_dependency_v1` from Id. Prepare and pin the factory before
Id initialization, immediately after the existing Vtable Enumeration factory
from the same library. Validate version, exact size and all operations inertly;
accept repeat attachment of the same descriptor and reject retargeting.
Preserve all other preparation and ordinary Siphon initializer order. Update
both C/Rust fixture loaders and the export inventory. Before this extraction,
Object Model exported `birnam_object_model_rust_identity`, `birnam_symbol_borrow_v1`,
`birnam_vtable_enumeration_factory_v2` and `module_init`; freeze those four in
its new native export manifest and add only the epoch factory. The pre-Id
factory must work before Object Model's ordinary `ACCESS` initialization and
must not initialize or call its semantic behavior helpers early.

There is no new library. Keep 81 installed libraries, IdApi v20,
twelve-argument ModuleInit, Native Loader v12 and image v8. The epoch owner is
not serialized; introduce no explicit image epoch restore/reset. Existing
Vtables construction and dictionary replay remain authoritative.

## Adversarial review

- A missing-key removal or same-parent assignment must still advance the epoch.
  Optimizing these writes away would change cache reuse and violate this slice.
- Empty weak/registry sweeps must not advance it. Preserve one increment for a
  nonempty sweep, even if many entries disappear.
- Payload layout, owner and trace-hook replacement are separate metadata paths.
  Do not broaden this epoch to cover them, or route them through new callbacks.
- Reaching `u64::MAX` must permanently preserve cache bypass while ordinary
  mutation continues. Keep the existing guard/counter separation; never reuse
  a guard-verdict cell as the registry's epoch owner.
- Pointer identities, weak entries, raw dictionary mirrors and executable
  acquisition/retirement stay outside the cell. Its destruction must work
  after Heap teardown without reading any language storage.
- Object Model already provides a factory before Id startup. Missing its shared
  library still fails at that earlier Vtable Enumeration boundary; invalid new
  descriptors must reject before any owning cell is constructed.

## Independent proof and completion gates

Preserve completed 81BV source and its installed prefix before editing. Run the
same new witness against the original `u64` and extracted owner, comparing exact
epoch values, ordinary results, raw mirrors and lookup effects for all table
rows above, including absent removal, identical parent, empty/nonempty and
multi-entry sweeps, unchanged metadata paths, moved/independent registries,
saturation and destruction. Module tests exercise initial/all-bit-pattern state,
repeated writes, distinct owners and lifetime independence.

Retain Vtable mutation tests, the 81BV tuple-state witness and enabled/disabled
lookup and Machine-slot tests, including registration changes without a
mutation epoch. Run inherited-cache and native-tier regressions; compare the
original arithmetic and every mutation/reader expression at its call site.
Observe reads and writes on the actual loaded Vtables owner through dictionary,
parent and behavior changes, verifying initial creation and inert attachment.
Test host missing/invalid descriptors, including the shared-library distinction.

Verify dynamic Siphon startup, source-free compiled callers, mapped/rebuilt
native Code images and baked execution with explicit host loader outcomes.
Use warm lookup and active Frame slot guards before/after live mutation, custom
lookup, aliases, GC and reload. Compare actual image-backed worker transcripts.
Check the complete library/export/linkage inventory and final source/artifact
receipts. Add the ABI reference, focused task, README links and changelog;
commit the completed extraction before selecting another boundary. No release
validation is required.

## Implementation and evidence

Object Model now owns one opaque epoch cell per Vtables registry. The Id
adapter changes only the field type, initialization, accessor, private test
assignment and the read/store expression surrounding the same saturating
increment. Reversing those five substitutions reproduces the baseline Vtables
source exactly. Every mutation/reader site, Runtime reassociation, guarded lookup
and slot path, inherited cache adapter, native-tier module and existing Object
Model body remain unchanged (apart from test module declarations).

The host and both fixture loaders prepare the new factory after Vtable
Enumeration and before Id. Earlier host-admission fixtures now acknowledge that
dependency so their original failure gates remain reachable. The independent C
header pins the 48-byte layout. The ABI reference, native export manifest,
`mise run mutation-epoch`, installed-image smoke script and README links describe
the owner without adding any language methods.

Debug validation:

- Core ABI, Object Model, Dispatch Cache, host and Id suites: 399 passed,
  16 ignored. The workspace also passes the all-targets debug check.
- The same new two-test witness runs against the original `u64` and extracted
  owner. All 20 exact epoch/raw-view transcript lines match byte for byte.
  Coverage includes moved and independent registries, creation without an
  epoch bump, overwrite, missing/present removal, same-parent
  assignment, owner/layout/hook non-mutations, empty and multi-entry sweeps,
  saturation with continued live mutation, reassociation results, and cell
  destruction after Heap teardown.
- Existing Vtable, 81BV tuple-state, enabled/disabled lookup and Machine-slot,
  inherited-cache and native-tier tests pass. The original six Runtime guard
  tests also pass in the preserved baseline source tree.
- The C observer captures the actual loaded registry's owner. Preparation
  creates no owner; Id initialization creates exactly one cell at zero. Every
  observed production write is the exact saturating successor. Method changes,
  absent removal, identical/replaced parent and identical/replaced behavior each
  advance once; warm inherited lookup reads the same owner without writing.
  This probe passes for workspace and installed libraries.
- All 12 new host admission cases pass for workspace and installed libraries.
  A missing shared library still fails at Vtable Enumeration. Invalid epoch
  descriptors and missing/rejected Id dependencies fail before initialization.
- Dynamic Siphon startup, source-free compiled callers, mapped/rebuilt native
  Code images and baked execution pass. The consumer checks canonical and
  overridden lookup, active Frame reads/writes, parent and behavior replacement,
  absent removal, aliases, GC and reload. Source files and the saver module are
  removed before recovery. The actual host asserts `Mapped` for ordinary and
  baked loads and `SourceRecovery` for explicit rebuild before running main.
- The actual old/new image-backed host workers match all 12 wire values and
  orderly shutdown byte for byte. The external debug harness leaves the
  production handshake deadline unchanged.
- Guard-verdict, inherited-cache, allocation-budget, collection-phase and
  storage-observation regressions pass, including their 12, 13, 14, 12 and 14
  host admission cases. The Relational native-output fixture passes through the
  updated Rust namespace loader and real installed floor.

## Final inventory and receipts

The inventory remains 81 libraries: 53 native and 28 compiled modules. All 38
native export manifests match, including the new Object Model inventory. The
only export additions are its epoch factory and Id's dependency. Both libraries
link only system libraries. Object Model's existing Cargo output-path install
name is byte-for-byte unchanged from the baseline; this extraction does not
convert it to `@rpath`. The public admission contract matches exactly: IdApi
v20, twelve-argument ModuleInit, Native Loader v12 and image v8.

Preserved 81BV baseline: 503 source inputs and 542 installed artifacts.

- Source: `083463fdb66a21ce8c7079ebc5e1bdcc877b3e534e65152dc68e0969606424fd`
- Artifacts: `1827409f949d7224cde88d88902ec02ef54d8140b3b5daab610f6f9e688f3f4d`

Final debug installation: 508 source inputs and 547 installed artifacts,
all checked against current source or installed file contents.

- Source: `b8a6643e7ff150c63d6797c31535c2012944569f3b52670db4139bd973c6df66`
- Artifacts: `cf1573d39419f32b053518bf0153df0f24cba1330a5549517bea9af6c275c68e`

No release validation was run.
