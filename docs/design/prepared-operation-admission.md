# Prepared edits observe values, not a global mutation count

**2026-10-18 follow-up:** [Candidate policy is now authored in Birnam](candidate-policy-migration.md).
Its selected contract retains partial effects for explicit conditional undo, removes
automatic compensation/reservations and setter postchecks, and treats lifecycle
fields as descriptions. The implementation and qualification below describe the
earlier slice; those stronger failure promises are superseded.

2026-10-16. Follows lifecycle ownership commit `2671e533` and the
[generation review](generation-lifecycle-review.md). Implemented and qualified; committed in `e4931deb`. This is a bounded admission simplification, not
the full optional-certification migration.

## Change

Prepared Generation tokens no longer retain the global Id dictionary epoch or
Globals binding basis. Those counters previously made an otherwise valid token
expire after an unrelated binding change or private Block construction.

Commit still compares the observed target object and behavior identities, exact
direct entries, projected Globals (including absent versus present nil), lifecycle
fields, current generation and source-registry content revision. Binding/Global
values restored to the same identities between checks do not conflict merely
because a global counter advanced. Source-registry ABA remains a conflict because
the source revision is part of this operation's declared observation.

The existing global stamps now bound a single validation pass across callbacks;
they are not a lease covering the interval between preparation and commitment.
They remain a conservative way to reject a mixed dictionary/Globals observation.
No per-binding watcher, new native registry, or method-identity guard was added.

Transactional dictionary writes send to the current owner method. They no longer
require exactly one global dictionary mutation or insist that a non-error return
value equal the requested Method. The caller checks the actual installed/removed
binding. A setter that does nothing while the requested binding differs still
fails. Native failures and returned Errors propagate; selective compensation
restores still-owned writes and preserves other callback effects.

This also removes the special epoch reset after candidate factory construction.
Private adapter registration is ordinary work, followed by the same value checks.
Factories still run once; failed publication does not replay their construction.

## Boundary retained at this slice

The subsequent [scoped transaction slice](scoped-generation-transactions.md)
removes the complete binding ledger from transaction admission and compensation.
The following records the earlier qualification boundary.

The complete managed ledger, full-source predecessor digest and frozen registry
remain. An edit outside the prepared write set can still conflict if it changes
another binding described by that ledger. This slice permits *untracked* edits;
it does not claim arbitrary independent edits to managed modules are admitted.
Making certification opt-in requires a coherent source/image contract change.

Host safe-point admission, one-use tokens, code/Method roots, current owner lookup,
conditional field/Globals inverses and native reclamation checkpoints are retained.
Ordinary sends and edits were already independent of Generation; no checks are
added to their path. Optimizer invalidation and deoptimization are unchanged.

There is no artifact/image format or public plan-shape change. Only process-local
prepared state loses two counters. Existing six-field plans remain valid.

## Evidence required

- An actual replaced setter performs extra dictionary writes, returns the receiver
  rather than the Method, and still succeeds when the requested binding is current.
- A no-op setter fails; an Error after mutation survives conditional compensation.
  Side effects outside the selected bindings remain, including Method/code owners.
- Private Block construction, unrelated Globals/dictionary changes, and restored
  identical bindings/Globals between prepare/commit do not expire the token.
- A changed selected binding, projected Global, relevant lifecycle field, target
  or source revision still rejects the operation. Present nil is distinct from an
  absent projected Global. Rejection does not overwrite the intervening edit.
- Exact activation/rollback, source-registry ABA rejection, candidate retirement,
  captured-code lifetime, Host intents and mapped/rebuilt images still work.

Lean models the observed values and setter postcondition. TLA+ explores changed
bindings, targets, Global presence, metadata and source revisions; it rejects
variants that omit those checks, expire on unrelated epochs, accept no-ops or
ignore Errors, or erase other effects. Its observation step is atomic: the model
does not prove arbitrary accessor callbacks coherent, nor model the remaining
whole-program ledger as an optional facility. Older revision-conditioned models
still describe owners that explicitly use revisions, including the source registry.

The new native regression fails against the preceding runtime: its exactly-one
rule masks the setter's returned Error after additional writes. The old retirement
fixture also expected rejection after a remove/reinstall of the same Method; it
now checks successful activation, exact rollback and eventual code reclamation.

No CPU improvement is claimed. The result is a smaller admission contract with
fewer false conflicts and less prescribed behavior for replaceable setters.

## Qualification

The isolated build `/tmp/birnam-prepared-admission/prefix` passes the expanded
transaction test in synchronous and resumable modes. The test covers a changed
newly introduced binding, present-nil versus absent projected Globals, relevant
field edits, private Block construction, unrelated names, selected Method/global
remove-and-restore, extra setter writes, alternate success returns, no-op rejection,
returned Errors, exact compensation/rollback and retained side-effect code after GC.
It also preserves the existing Store/db-absent commit test.

Candidate inspection/mapping and retirement pass, including replacement of the
setter after token creation, same-Method remove/reinstall inside that setter,
source-registry ABA rejection, captured Methods after rollback and eventual TCC
reclamation. Real Host intent activation/rollback passes. Mapped/rebuilt/resaved
managed images pass successor activation/rollback and live-load failure handling.
The language suite passes 826 assertions.

Lean checks 344 theorems and rejects 95 mutations. TLC obtains all 219 expected
outcomes, including the ten new prepared-admission cases. The safe model and
negative controls were checked along with the existing formal suite.

The regression first failed against the preceding runtime, which masked the
setter's Error under its extra-mutation rule. Qualification then updated the old
retirement expectation that same-Method ABA must poison a generation. A test-only
helper initially expected the low-level removeMethod: primitive to return an
object; it now uses the raw result protocol. No product fix was needed for that
harness error.

The [receipt](../measurements/prepared-operation-admission-20261016.json) records
commands, source hashes, logs and verification scope. No push, release, user
installation, ABI/image format change or performance claim accompanies this slice.
