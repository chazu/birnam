# Basic live image loading without automatic rollback

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

2026-10-16. User-selected simplification of
[optional Store](../../design/optional-store-and-relational.md). This supersedes the live failure
policy in [coordination cleanup](coordination-cleanup.md), not its ordinary-edit or
fresh-startup boundaries. Implementation and qualification are complete; results are recorded below.

## Contract

`System loadImage:` and its live variants apply an image to the current world.
Users save and check their own recovery image first if they want one. A failure
once application begins returns the original native failure or ordinary Error and
reports `#partialLoad` through `System imageSaveReport`. Applied and callback writes
remain at the failure boundary; the loader neither compensates nor replays them.
This promises no failure-time undo, not immunity from later ordinary writes that
supersede an earlier value during loading. A failed world can be
partially loaded. No automatic snapshot, retry, restart or invocation of main is
introduced. A caller can discard it and start fresh from a saved image; another
live load is an explicit attempt, not a guarantee of recovery.

Fresh `startImage:` variants retain discard-on-failure, reporting `#startupFailed`
for application failures. Earlier admission failures retain ordinary rejection
reporting. Replaceable validation/staging callbacks are not promised side-effect-free.
External I/O and unsaved work are outside snapshot recovery.

Success retains the existing mapped/rebuilt result convention, image format,
source-free mapped startup, owner rooting and entered/captured Method lifetimes.
The failure boundary clears composition identity and abandons old preparation and
adoption capabilities. Staging cleanup releases temporary roots. Basic loading
must not revive stale capabilities merely because an inverse is no longer attempted.
Explicit Generation activation/rollback retains its separate, requested contract.

## Removed machinery

The Image path no longer retains predecessor Methods or foreign bindings for
rollback, takes a Globals checkpoint, captures Store recovery revisions, checks
dictionary epochs around each installation, records per-binding inverses, or
conditionally restores dictionaries, Store or source registry after failure.
Validation, preparation, normal owner mutation and successful Generation adoption
remain. Explicit transaction owner primitives are not removed opportunistically;
remaining unused APIs can be reviewed with their owning modules.

This is a simplification of the failure contract, not a claim that callbacks cannot
mutate definitions. All sends remain replaceable. There is no new native owner,
registry or coordinator. Partial application is explicit rather than hidden behind
an unsuccessful claim of rollback.

## Future robustness

A stronger loader can later be an explicit operation with clearly bounded
checkpoint or owner-receipt guarantees. No such framework is needed now. It must
not impose transactions on ordinary edits or become a prerequisite for Store-free
core operation. Resource lifetime, validation and code safety remain obligations
of both basic and enhanced loading.

## Qualification

The isolated release build at `/tmp/birnam-partial/prefix` includes the pending
ordinary-object field migration as well as this loader change. Its Image production
source matches the qualified implementation. Qualification performed no user
installation, release or push; commit status is tracked in the Store plan. No CPU or memory speedup is claimed.

- 826 language assertions and all postchecks pass, including ordinary-record
  semantics and retention.
- The failure matrix passes in mapped and rebuilt modes: native failure, ordinary
  Error identity, bound nil, startup, authored tools image and imported Globals.
  Callback method/Global/fact/source edits survive, and composition identity is
  cleared after failed application.
- Generation ledger modes and explicit transaction rollback pass. Image snapshot
  consistency and portable materialization/code lifetime tests pass.
- The twenty fresh-process mapped/rebuilt resaves pass their retained-fact, Unit
  count and image-size checks.
- The final full image roundtrip passes, including source-free mapped/rebuilt
  children, explicit restoration after partial failure, failed-save file preservation,
  sealed-registry rejection, and malformed-image rejection against current state.
- The formal gate checks 321 Lean theorems, rejects 86 mutations, and obtains all
  203 expected TLC outcomes. New laws model failure-boundary value preservation,
  diagnostic identity and revocation of authority. New temporal cases catch undo,
  stale authority and callback replay. These are abstract contracts, not proofs of
  the native implementation.

The original image test expected automatic restoration of a later source entry,
its sealed registry and old method/global identities. It now verifies retained
application after failure, explicitly reloads the saved snapshot for subsequent
save-admission tests, and re-establishes the seal before testing sealed-registry
rejection. The malformed-image check compares against the actual current object
identity. These oracle changes do not weaken save-file preservation, malformed
admission, lifecycle or retention checks; no runtime correction was needed.

The [receipt](../../measurements/basic-live-image-loading-20261016.json) records source
hashes, isolated-build provenance, commands, final results and corrected test-oracle
failures. Validation applies to this development build, not the installed release.
