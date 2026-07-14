# 49a — Adversarial Review: Live Module Evolution

> **Status:** Review, 2026-07-13. Reviews
> [49 — Live Module Evolution](49-live-module-evolution.md) against the current
> module loader, candidate lifecycle, source recording, and image implementation.
> This document records findings; it does not revise design 49 by itself.

## Verdict

The direction is good, but the proposed first slice crosses three unresolved
atomicity seams: initialization, source-revision publication, and image
composition. Design 49 should not be implemented as written until those seams are
made explicit.

## Critical findings

### 1. Initializer ordering contradicts the proposed interface and cannot guarantee “runs once”

Design 49 §7.1 runs the initializer before compiling and installing the first
method generation. Its example initializer calls `[Ledger establishInitialState]`,
which may be a method defined by that same source. The method does not exist at the
proposed point of execution.

More fundamentally, arbitrary effects cannot be made exactly-once with a durable
marker:

- Running the effect before recording completion permits a crash between them. A
  retry repeats the effect.
- Recording completion before the effect permits a crash that leaves the module
  marked initialized without its state.

Design 49 acknowledges partially failed initialization but also requires a test
that initialization “runs once.” Those claims are incompatible without a narrower
definition of initialization or a transactional effect substrate.

Current candidates cannot parse the illustrative declaration either.
`scanCandidateSource` accepts only the exact `Module named:requires:` declaration
and `Reflect` method operations, requires at least one method operation, and
candidate compilation discards everything outside that subset. See
`src/id/codegen.zig` around `scanCandidateSource`, candidate validation, and
candidate compilation.

#### Failure scenario

1. The initializer creates `Ledger` and initial facts.
2. Candidate compilation or validation fails.
3. A retry either creates another object or skips initialization despite there
   being no active method generation.

Both outcomes violate the proposed lifecycle.

#### Recommended correction

- Compile and inspect the candidate before any initializer effect.
- Define initialization as **at-least-once**, with durable states such as
  `#uninitialized`, `#initializing`, `#initialized`, and `#failed`, plus an attempt
  identity.
- Require retryable hooks to be idempotent or explicitly resumable.
- Restrict a pre-activation hook to establishing stable targets through protocol
  that already exists, or add a distinct post-activation hook for code that needs
  the new generation.
- Treat support for `Module named:requires:initialize:` as an explicit parser and
  Codegen slice rather than assuming it is pure orchestration.

### 2. Dispatch activation and active-source publication are not atomic

Design 49 says that the active source revision authoritatively rebuilds live
behavior and that project policy associates source documents with compiled
generations.

The current native activation publication plan has a fixed lifecycle shape.
`applyPublication` publishes compiled-unit state and events. Commit changes
dispatch and applies that publication, but it does not include a source-revision
pointer supplied by project policy.

A Birnam-level evolution module can therefore only write the active source
revision after activation. An error, allocation failure, or interruption between
those steps leaves new dispatch paired with the predecessor source revision. The
method-operation log may still rebuild behavior, but the stronger source-authority
claim and `fileOut` result are false.

#### Failure scenario

1. Candidate activation succeeds and publishes new dispatch.
2. Recording the revision-to-generation association fails.
3. `fileOut` exports the predecessor source while the image executes the
   candidate.

#### Recommended correction

Either:

- extend the prepared native publication plan with a prevalidated active
  source-revision pointer/current-revision cell; or
- weaken the invariant and introduce an explicit `#activatedUnassociated` state
  that reconciliation can repair.

Do not describe source and dispatch as one accepted revision until they share a
commit seam.

## High-severity findings

### 3. “Module identity” currently means three different names

Design 49 assumes one stable module name. Today:

- `Module named:` records the declared symbol;
- source loading records `__modsrc__` under the file basename;
- artifact compilation derives recorded module identity from the output path; and
- candidate compilation checks the caller-supplied name only against an optional
  declaration.

#### Failure scenario

`accounts.birnam` declares `#ledger` but compiles to
`libbirnam_finance.dylib`. Module facts, source document, candidate generations,
and active revision land under three identities.

#### Recommended correction

Make the declared module name canonical and required for project-managed units.
File and artifact paths are origins only. Source and artifact loaders must parse or
embed the canonical name before recording source, and mismatches must be rejected.

### 4. Image-native editing omits `adoptLive` and has no definition-ownership rule

Design 49 describes reflective edit → candidate revision → activate. An ordinary
reflective edit has already changed dispatch and marks a managed generation
drifted. Current policy requires `adoptLive`, which records the already-live
behavior as a new baseline without a second dispatch mutation.

Recorded reflective operations currently carry target, selector, and operation
source, but no owning module document. Two modules may intentionally patch the
same target/selector pair, so the system cannot infer which authored document to
patch.

#### Recommended correction

- Use reflective edit → identify or choose owning revision → patch and reparse the
  document → `adoptLive` → associate the adopted baseline with the revision.
- Record canonical definition ownership at installation: module revision,
  document identity, span/node identity, target, and selector.
- Require an explicit owner when provenance is absent or ambiguous.

### 5. Image composition destroys a direct one-generation-per-module association

After boot, `reconcileImageComposition:` marks saved active origins historical and
points their names at one active `#imageComposition` generation. A source revision
cannot continue to identify its saved origin as the active compiled-unit generation
without becoming false.

#### Recommended correction

Keep source-revision activity independent from process-local compiled-unit
activity. Model that an active revision is materialized by the current executable
composition, or introduce per-module facets of that composition. Do not leave a
revision pointing at a historical origin while calling that origin active.

### 6. Image boot does not guarantee the initializer dependency order the proposal needs

Whole-module sources are gathered through Store attribute enumeration and
concatenated before method sources; they are not topologically sorted by module
dependencies. Existing image behavior runs module top-level programs on boot.
Adding a new initialization declaration makes source order load-bearing: the
module manager protocol and dependencies must exist before each declaration runs.

Facts load before compiled code in a full image, so consulting a durable marker is
possible. The missing guarantee is executable module order.

#### Recommended correction

Compile and replay image module programs in dependency order using canonical
module metadata, with the module manager first. Add mapped, rebuilt, and appended
image tests where an initializer depends on another module and prove the hook is
skipped after restoration.

### 7. Migration must close application rollback before its body begins

Design 49 correctly observes that migrated state may invalidate old methods, but
its sequence is activate → migrate → verify → checkpoint. If the process fails in
the middle of migration, native `rollbackAvailable` may still be true even though
application rollback is unsafe.

#### Recommended correction

Before invoking a migration, durably enter `#migrationInProgress` and mark
application-level rollback unsafe. Keep the native dispatch capability separately
named and observable. Only a proven reverse migration may reopen application
rollback.

Tests must cover interruption:

- before the migration body;
- during a partially applied body;
- after the body but before recording completion; and
- after completion but before image persistence.

### 8. The proposed first vertical slice is at least three slices

Design 49 §11 combines initialization semantics, source round-tripping,
divergence, staged and immediate activation, rollback, mapped and rebuilt image
recovery, and image-native source patching.

Current source records include whole-document and individual-operation text, but
not persistent ownership and definition spans. Source loading can also record
`__modsrc__` before evaluation succeeds. Lossless reflective source patching is a
separate provenance/editor feature.

#### Recommended decomposition

- **Slice A:** use an already-initialized module. Add canonical identity, a
  revision entity, explicit file import/export, divergence detection, staged and
  immediate activation, atomic revision association, rollback, and mapped/rebuilt
  survival.
- **Slice B:** add the initializer state machine and dependency-ordered boot.
- **Slice C:** add reflective editing, definition ownership, spans, and lossless
  patch/file-out behavior.
- Add migration only after those slices.

## Medium findings

### 9. `prepareDependencies` discards dependency-load errors

Its current fold evaluates `[Module load: dep]; me`, returning the unit rather than
the result of a failed load. The unresolved dependency later appears as a generic
`#dependenciesMissing` error, contrary to the proposed detailed immediate-path
errors.

#### Recommended correction

Propagate the exact `Module load:` error and leave the candidate inspectable with
the original failure cause. Add a focused regression test.

### 10. “Accept” and “checkpoint” lack domain semantics

Current `CompiledUnit` has activation and rollback but no acceptance transition.
`[System imageCheckpoint]` advances the append baseline; it does not write an
image.

The design must define whether acceptance:

- only marks a source revision;
- forfeits rollback;
- performs `saveImage:` or `appendImage:`;
- changes artifact retention; and
- precedes or follows persistence.

It must also define what happens when persistence fails after acceptance.

## Missing acceptance tests

Add tests for:

- interruption between dispatch commit and revision publication;
- every initializer body/marker crash window;
- filename, declared-name, and artifact-name mismatch;
- duplicate/idempotent file-in of identical content;
- reflective edit ownership ambiguity;
- multiple sequential span-changing live edits;
- composed-image reassociation of every active source revision;
- dependency-ordered mapped, rebuilt, and appended boot;
- migration interruption and application-level rollback gating;
- acceptance followed by save failure;
- stale or missing base metadata and explicit forced replacement;
- rejected import never changing `__modsrc__`; and
- changed initializer source not rerunning an initialized module.

## Sound parts to retain

- Files and image-native edits are sibling revisions rather than one medium being
  globally privileged.
- Live dispatch, reproducible source, and filesystem representation are distinct
  authorities.
- Immediate and staged activation are policies over one lifecycle.
- Authored documents are preserved instead of synthesized only from method facts.
- Exact dispatch rollback is explicitly narrower than initialization and
  migration.
- Migration is optional and orthogonal, with idempotent/resumable expectations.
- Durable initialization is distinct from per-process recovery.
- Multi-unit transactions are explicitly deferred.
- Background file watching is absent from the first implementation.

## Resolution after review

Design 49 rev. 2 incorporates the review through the following decisions.

1. **Initializer ordering:** resolved. Candidate source is compiled and inspected
   before effects. The initializer is bootstrap-only, cannot call methods from its
   own pending generation, and has an at-least-once idempotent/resumable contract.
   Initialization state is independent of whether a generation is active.
2. **Atomic source publication:** resolved without a second active-source write.
   Each prepared unit carries immutable module-indexed source revisions. Active
   source is derived through the atomically published `activeCompiledUnit`.
3. **Canonical identity:** resolved. The declared `Module named:` symbol is
   required and canonical for project-managed modules; source and artifact paths
   are origins only.
4. **Image-native editing:** resolved. Source-first editing is normal. Immediate
   reflection remains an escape hatch requiring definition ownership and
   `adoptLive`.
5. **Image composition:** resolved in the model. A composed generation carries a
   canonical module-to-source-revision map copied from its active origins.
6. **Boot ordering:** resolved as an invariant. Every execution of module-level
   programs is dependency-ordered, with the module manager first.
7. **Migration rollback:** resolved. Durable migration intent is persisted before
   effects, and application rollback becomes unsafe before the body starts. Native
   dispatch rollback remains a separately named expert mechanism.
8. **Slice size:** resolved. Work is split into file revision lifecycle, module
   initialization, image-native editing, and migration slices.
9. **Dependency errors:** accepted as an independent prerequisite fix. The exact
   `Module load:` error propagates while previously loaded dependencies remain.
10. **Acceptance/checkpoint:** resolved by deletion. There is no first-class
    acceptance state initially. Activation controls live authority; `saveImage:`
    and `appendImage:` control durability; `imageCheckpoint` remains internal.

These are design resolutions, not claims that the corresponding implementation
already exists.

## Conclusion

The design is viable after narrowing the first slice and resolving the two
critical commit seams. Initialization needs an explicit at-least-once state
machine and an ordering that does not depend on methods unavailable before first
activation. Source revision and dispatch publication must either become one
prepared commit or expose an honest intermediate state.
