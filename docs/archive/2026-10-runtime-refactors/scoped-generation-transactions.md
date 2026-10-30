# Scoped generation transactions

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

**2026-10-18 follow-up:** [Candidate policy is now authored in Birnam](candidate-policy-migration.md).
Its selected contract retains partial effects for explicit conditional undo, removes
automatic compensation/reservations and setter postchecks, and treats lifecycle
fields as descriptions. The implementation and qualification below describe the
earlier slice; those stronger failure promises are superseded.

2026-10-16. Implementation of the next [generation simplification](generation-lifecycle-review.md)
slice, following [prepared value admission](prepared-operation-admission.md).
Implemented and qualified; committed in `e4931deb`. This does not complete the optional-certification redesign.

## Contract

Activation, rollback and failure compensation compare the operation's observed
state: selected target/behavior identities and direct Method entries, projected
Globals with presence distinguished from nil, lifecycle fields, current generation
and source-registry revision. Their postcondition substitutes the intended writes
into those observations. It does not take a fresh snapshot after callbacks and
call whatever they left behind success.

The complete managed binding ledger no longer gates these operations. A changed
method or Global in another managed module can coexist with a prepared operation,
its commit, failed-commit compensation and its later rollback. Side effects of
replaceable setters remain outside the inverse unless they overlap the selected
writes. This is the same rule for tracked and untracked names. Replacing a Global
used as one of this operation's method targets is still rejected.

A relevant conflict returns an error without marking all units drifted or
revoking their inverses. The one-use prepared token is consumed as before. Restore
the affected binding and prepare a new token to retry. Selected targets, fields,
source revision, no-op setters and returned Errors remain checked. Ordinary sends
have no new guards. Native reentry exclusion, Method roots, code lifetime,
lookup invalidation and deoptimization are unchanged. No new native service,
public API, artifact/image format or bootstrap module is introduced.

Whole-program source certification does not become true merely because a scoped
edit succeeded. The existing ledger retains its outside observations; explicit
`reconcileCandidate:` and strict image validation still compare them. Image
snapshots still identify drift and restore live code as descriptive state.

## Remaining coupling

This slice still requires initial adoption and a matching source/selection lineage:
current generation identity, source-map identity, digest, selection and registry
revision. Selected names with present bindings must still belong to the adopted baseline.
Compilation envelopes, predecessor projection, source-registry sealing, image
composition/adoption, explicit reconciliation's sticky drift policy and Host intent
fulfillment remain. The full ledger is still retained for certification and image
recovery; this slice removes its use as a blanket transaction conflict check.

Next, decouple source-free image reconstruction from automatic adoption, then make
baseline certification opt-in. Remove sticky whole-world drift side effects as
part of that change. Do not migrate the remaining policy unchanged into Birnam.
Host-only activation and discard/file-cleanup separation remain on the same
simplification checklist. No further Store migration precedes those decisions.

## Verification

The transaction regression introduces a second, separately registered managed
module. Its method and projected Global are edited before preparation and during
commit/rollback; a replacement setter edits another managed selector while
returning an Error. The original runtime masks that Error with whole-ledger
compensation failure. The revised runtime must preserve it, preserve outside edits,
restore selected state exactly and keep a captured side-effect Method callable
after rollback and GC. Both synchronous and resumable candidates run this path.

A selected-binding conflict must still reject without overwriting the foreign
change, retain the inverse, and permit a newly prepared retry after restoration.
Explicit whole-ledger validation must still reject independent managed drift.
Existing coverage checks target identity, absent-versus-nil Globals, metadata,
source-registry ABA, one-use tokens and callback failure compensation. The Host
image regression also edits a second managed module before activating the first,
snapshots that divergent world, and reloads it through both mapped and rebuilt
paths. The restored live edits must survive without gaining managed authority.

Lean proves outside-state preservation, admission despite outside drift, rejection
of selected conflicts, conditional undo and separation from certification. TLC
explores outside edits before activation/undo, selected conflicts and certification;
negative variants reintroduce whole-ledger admission, ignore selected conflicts,
erase outside edits or falsely certify them. These abstract models do not prove
arbitrary owner callbacks coherent; observation stamps still bound each native
validation pass. No performance improvement or completed lifecycle redesign is
claimed.

## Qualification result

The isolated production build `/tmp/birnam-scoped-admission/prefix` passes:

- Expanded synchronous/resumable transaction regression, including managed
  outside edits, returned Errors, exact compensation/rollback and conflict retry.
- Candidate inspection/mapping, native retirement and captured-code lifetime,
  source projection, explicit ledger certification and stale image-save rejection.
- Real Host intent fulfillment and mapped/rebuilt/resaved image recovery, including
  live-load failure handling and successor activation/rollback. Both new divergent
  snapshot modes preserve the independent method edit as live code without
  reactivating managed authority.
- All 826 language assertions, with zero failures.
- 349 Lean theorems, 98 rejected mutations and all 224 expected TLC outcomes.

The [receipt](../../measurements/scoped-generation-transactions-20261016.json)
records commands, logs and source hashes. The initial build rejected a changing
source tree; the completed build used the stable production sources. Subsequent
edits changed test fixtures and documentation only. The old runtime failed the
expanded managed-side-effect regression before the implementation changed.
The receipt records the pre-commit qualification snapshot. This slice was subsequently
committed in `e4931deb`; no push, release or user installation accompanied it.
