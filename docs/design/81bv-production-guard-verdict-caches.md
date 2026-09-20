# 81BV — Guard-verdict cache ownership

> **Status:** Audited next extraction after completed 81BU (`3d009cbf`).
> Production migration pending. Preserve behavior; debug validation only.

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
