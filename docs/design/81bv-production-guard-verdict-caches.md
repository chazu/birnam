# 81BV — Guard-verdict cache ownership

> **Status:** Completed production extraction after 81BU (`3d009cbf`) and
> implementation checkpoint `0718e168`. Debug validation only; no behavior
> or public format change.

Move the two `(mutation_epoch, verified)` cells from Runtime into the existing
Dispatch Cache module. They memoize different proofs: canonical lookup and
canonical Machine-slot access. Give each cell its own opaque native owner;
do not combine their verdicts or attach them to an inherited-lookup table slot.
Id retains all eligibility checks, their order, mutation authority, execution
classification and fallback behavior.

This is the next small cache-state boundary after 81BU. Moving canonical guard
evaluation itself would require exposing Heap, Vtables and execution facts.
Keep that larger optional-optimizer separation outside this slice.

## Existing guards and ordering

Both `slot_guard_cache` and `lookup_guard_cache` start at `(u64::MAX, false)`.
At each existing read, a stored verdict is reusable only when the current
vtable epoch is not saturated and matches the stored epoch. Otherwise Id
recomputes its existing conditions and writes the current epoch and verdict.
Both positive and negative verdicts are cached. Even after a write at
`u64::MAX`, the next call recomputes; the sentinel never authorizes reuse.

| Guard | Checks outside the cached verdict that remain in place |
|---|---|
| `guarded_default_lookup_eligible` | Test-only disable; dispatch-reader presence and receiver metabehavior before reading the cell; current canonical lookup registration after the cached/recomputed verdict |
| `guarded_slot_access_eligible` | Owner admission; execution objects and the receiver's actual Machine behavior; current registration of canonical read, write and lookup Methods, all before reading the cell |

Canonical lookup's recomputed verdict checks the canonical Vtable object's
metabehavior and its direct lookup binding. The slot verdict checks the direct
canonical Machine read/write bindings, Machine and Vtable metabehaviors and
the floor lookup binding. Leave those exact expressions and short-circuit
order in Id. Do not replace direct dictionary checks with ordinary sends or
move any receiver/registration check inside the epoch cache.

Registration can change invocation classification without a dictionary epoch
change. A warm true verdict must not bypass a resumable carrier or unpublished
registration. Conversely, an early rejection must not overwrite a previously
cached verdict or create a new cache write. Cached false still falls back to
ordinary behavior; it is not a cached language result or exception.

The current guards return eligibility only. Checked slot operands, nil on
canonical failure, current Frame selection, ordinary fallback exactly once,
lookup effects, DNU and target acquisition remain in their original callers.
The cells hold no Machine, Frame, receiver, Method or executable owner.

## Independent native owners

Add a separate immutable 48-byte factory to `birnam-dispatch-cache`: version,
byte size and four operations. Its 16-byte state record contains `epoch: u64`
and `verified: u32`, with ordinary C layout padding. Only those fields carry
meaning; do not inspect or compare padding bytes.

| Operation | Contract |
|---|---|
| `create() -> context` | Allocate a cell initialized to epoch `u64::MAX` and false |
| `destroy(context)` | Free that cell exactly once |
| `read(context, out)` | Copy the epoch and verdict, with the verdict encoded as zero or one |
| `write(context, epoch, verified)` | Unconditionally assign the epoch and a verdict where any nonzero integer means true |

Every epoch value and repeated assignment is supported. This is storage only:
there is no epoch advancement, validity predicate, transition graph or
invalidation policy in the module. Only create/destroy allocate/free memory.
Reads and writes allocate nothing and call neither Id nor foreign code. No
Runtime, Heap, language object, Method, descriptor or output pointer is retained.
Callers provide live contexts and existing owner-thread serialization.

The Id wrapper keeps the existing tuple-shaped `get`/`set` behavior at each
call site. Replace the two fields and their initialization in the same order:
slot verdict, lookup verdict, then the existing inherited-lookup cache. A moved
Runtime retains all three independent owners. One guard's writes must not affect
the other guard, another Runtime, or the 64-entry table. Clearing that table
during GC must not introduce clearing of either verdict cell.

Export `birnam_guard_verdict_factory_v1` from Dispatch Cache and
`birnam_guard_verdict_dependency_v1` from Id. Prepare and pin this descriptor
before Id initialization, after the inherited-lookup factory, without calling
any operations during validation. Check version, exact size and all four
operations; accept the same descriptor repeatedly and reject retargeting.
Both factories remain independently described even though their code resides
in one library. Preserve Dispatch Cache's ordinary Siphon initializer position.
Update C/Rust fixture preparation and native export manifests.

There is no new library: the installed count remains 81, comprising 53 native
libraries and 28 compiled modules. IdApi v20, twelve-argument native
initialization, Native Loader v12 and image v8 remain unchanged. Saved images
contain no guard-cell records or native pointers; each new Runtime starts with
fresh cells and recomputes its current facts.

## Adversarial review

- Malleability: preserve method/parent replacement, private receiver behavior,
  custom lookup and metabehavior effects. Never promote a stored true verdict
  into authority for a different receiver or a changed registration.
- Lookup versus slots: identical tuple representations do not mean identical
  proofs. They must have separate owners and separate writes at the existing
  sites, including when they carry opposite verdicts at one epoch.
- Saturation: a stored saturated epoch must never become a hit. Recompute and
  preserve both true-to-false and false-to-true changes without epoch advance
  when the existing test fixture exhausts the mutation counter.
- Lifetimes: the cells retain only integers, not object roots or code leases.
  Destruction may occur after Runtime's Heap has been dropped, so it must never
  consult heap or execution state. Keep owner checks at their original sites.
- GC and recovery: preserve the current absence of blanket verdict clearing.
  Do not borrow the inherited table's `clear` operation or serialize either
  cache into an image. Fresh Runtime creation and existing mutation checks
  remain the authorities for recovering eligibility.

## Independent proof and completion gates

Preserve completed 81BU source and the installed debug prefix before migration.
Run the same new Runtime witness against the original Cells and extracted
owners. Compare guard answers, tuple contents, ordinary results and effects for
independent/moved owners, opposite guard verdicts, warm true/false hits, receiver
reassociation, method/parent changes, private metabehaviors, missing execution
objects/read authority, changed registration, saturated epochs, GC and surviving
owner destruction. Keep the guards' original early-return/read/write order.

Retain the enabled/disabled oracles in `runtime/guarded_lookup/tests.rs` and
`runtime/guarded_dispatch/tests.rs`, including each canonical Method registered
as resumable without dictionary mutation. Preserve invalid operand results,
fallback exactly once, effect ordering, active Frame behavior and pin counts.
Run inherited-lookup and collection-phase regressions so the new factory cannot
silently merge, clear or otherwise change the table's independent state.

Module tests cover initial/sentinel values, all boolean encodings, repeated and
reordered writes, two distinct owners and destruction. A loaded-factory observer
must identify the actual Runtime's two guard contexts and observe reads/writes
through canonical lookup and guarded Machine-slot execution. Do not substitute
standalone cell tests or arithmetic output for reaching both guards. Use the
existing test-only epoch setter for saturation; add no production forcing API.
Test inert attachment and actual host missing/invalid descriptor admission.

Verify dynamic Siphon startup, source-free compiled callers, mapped/rebuilt
native Code images and baked execution, with explicit loader outcomes. These
consumers must exercise canonical and overridden lookup and slot behavior,
aliases, GC and module reload. Compare actual image-backed worker transcripts;
verify unchanged guard expressions, descriptor/source bodies, existing exports,
linkage and final source/artifact receipts. Add the ABI reference, focused task,
README links and changelog. Commit the completed extraction before selecting
another boundary. No release validation is required.

## Implementation and evidence

Dispatch Cache owns two separate native cells through its second factory.
Runtime retains the original tuple-shaped adapter calls. The lookup guard,
Machine-slot eligibility and condition expressions, guarded send dispatch and
inherited-cache adapter match the 81BU source byte for byte. GC still clears
only the inherited table. No new library, language method, Image record or
production forcing API was introduced.

The host and C/Rust fixture loaders prepare the dependency before Id. The
independent C header pins the 48-byte factory and 16-byte state layout. The
ABI reference, `mise run guard-verdict`, installed-prefix smoke script and
README links describe the boundary. The Relational native-output fixture now
also prepares its missing real String binding; this is fixture setup only.

Debug validation:

- Core ABI, Dispatch Cache, host and Id suites: 390 passed, 16 ignored.
- The same new Runtime witness runs against preserved 81BU source and the
  extracted owners. All 17 transcript lines match byte for byte, including
  independent/moved owners, opposite verdicts, warm true/false, early refusals,
  registration changes without an epoch bump, private receivers, parent
  mutation, GC, saturation in both directions and surviving owner destruction.
  Bootstrap already warms lookup; native factory tests separately prove both
  owners start at `(UINT64_MAX, false)` before bootstrap runs.
- The original five enabled/disabled lookup and guarded-send tests pass in
  both source trees. They retain active Frame results, invalid operands,
  override and lookup effects, registration classification and GC mutation.
- The loaded C observer captures the actual Runtime's two contexts. Attachment
  creates neither owner; Id initialization creates exactly two. It observes
  separate cold writes, warm reads, cached false and opposite verdicts, ordinary
  override calls exactly once, and no guard writes during collection. The same
  probe passes against the final installed libraries.
- All 12 host guard-verdict admission cases pass with workspace and installed
  libraries. Invalid descriptors reject before Id initialization; the missing
  shared library fails at the earlier inherited-table factory as designed.
- Dynamic startup, source-free compiled callers, mapped/rebuilt native Code
  images and baked execution pass. The consumer runs a resumable send with real
  active Frame reads and writes, canonical and overridden slot methods and
  custom lookup, plus restored canonical behavior, aliases, GC and module
  reload. Sources and the saver module are removed before recovery. The actual
  host asserts `Mapped` for ordinary and baked loading and `SourceRecovery`
  for explicit rebuild before running image main.
- Actual old/new image-backed host workers match all 12 wire values and orderly
  shutdown byte for byte. The external debug harness waits for startup without
  changing the production handshake deadline.
- Inherited-cache, collection-phase and storage-observation scope regressions
  pass, including their 13, 12 and 14 host admission cases. The updated Rust
  namespace loader is exercised by the Relational native-output fixture; that
  explicit ignored fixture passes, and its ordinary suite has 22 passing tests
  and two ignored fixtures.

The image consumer deliberately uses a resumable send. A simple argument-return
probe produced no slot reads and was rejected by the observer assertions; it
was replaced before claiming image coverage. GC may remove a private Vtable
and advance the mutation epoch, so the independent witness checks retained
cell contents without assuming collection leaves the dictionary epoch fixed.

## Final inventory and receipts

There are still 81 installed libraries: 53 native and 28 compiled modules.
All 37 native export manifests match. Existing exports are unchanged except
for the declared guard-verdict factory and Id dependency. Id and Dispatch Cache
link only system libraries; Dispatch Cache retains its ordinary install name
and initializer position. The public admission contract matches 81BU exactly:
IdApi v20, twelve-argument ModuleInit, Native Loader v12 and image v8.

The preserved baseline has 499 source inputs and 538 installed artifacts:

- Source: `8bbc5941659800904004f0faa5a576f7d18401e7d332b1bbcdb9b3ae6243d017`
- Artifacts: `ef646caede66c54ba432f9c39da650a0e27b40188d87bd148b5d2a8768d4d831`

The final debug installation has 503 source inputs and 542 installed artifacts,
all checked against the current worktree or their installed file contents:

- Source: `083463fdb66a21ce8c7079ebc5e1bdcc877b3e534e65152dc68e0969606424fd`
- Artifacts: `1827409f949d7224cde88d88902ec02ef54d8140b3b5daab610f6f9e688f3f4d`

The fixture-only String preparation fix required an install receipt refresh.
Id, Dispatch Cache and the host remained byte-identical; the installer regenerated
the 28 compiled modules. Image and worker comparisons were rerun against those
final artifacts. No release validation was run.
