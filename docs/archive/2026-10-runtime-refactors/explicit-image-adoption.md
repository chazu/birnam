# Image restoration and explicit generation adoption

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

2026-10-16. Follows scoped admission commit `e4931deb` and the
[generation simplification review](generation-lifecycle-review.md).
Implemented and qualified; uncommitted.

## Contract

Image restores executable Methods and their captures independently of Generation
certification. Successful load discards prior process-local candidate/preparation
and rollback capabilities, then returns without invoking authored adoption policy.
Retained CompiledUnits and generations remain the same ordinary objects, with
aliases, fields and explicit annotations preserved. No replacement component
units or generation are minted automatically. Historical state/rollback fields do
not authorize a new process to execute an old inverse.

`DispatchGeneration adoptCurrent` is an explicit operation after load. It derives
a predecessor-free baseline from the retained source map, digest and selection,
with nil component-unit references. The existing native adoption validates that
baseline against live code and registers a fresh ledger. A mismatch restores the
prior current root, without rewriting history. A process that already has an
adopted ledger cannot silently replace it using this operation. Initial ordinary
startup still uses `initializeCurrent`; removing that default is the next slice.

Saving/resaving does not reconcile or poison generation records. Snapshot admission
checks current entries at registered authored selectors for imageability, including
opaque native replacement after restoration without an adopted ledger. The source
registry's retained metadata identifies those selectors; no source is re-evaluated
and no equality with old Method bodies is required. The existing registry metadata
format and native decoding requirements remain. A general ability to serialize
arbitrary foreign native pointers is not introduced.

Removed: authored image-composition policy, composed-component admission rules,
`adoptComposedGeneration:`, `imageRecoveryIdentity`, their dynamic recovery
permission flag, and unused compositionIdentity/composedInto slots and accessors. The code inventory, image identity admission, ownership handoff,
GC roots, candidate reentry exclusion and native retirement checkpoints remain.
Basic live loading still reports partial application on failure. It does not
restore old authority over a partially loaded world.

This changes the lifecycle API/default, not the image wire format or Id ABI.
Rebuild old lifecycle facades/images to obtain the new explicit API. No compatibility
adapter is introduced. Store/Relational descriptions and ordinary late-bound sends
are unchanged. Full Store-free bootstrap and optional source certification are
not claimed.

## Acceptance

- Load matching and divergent images without adopting; preserve generation aliases,
  ancestry, captured Methods, explicit facts and ordinary editing.
- Resave an unadopted image and load it again through mapped and rebuilt code paths.
- Explicitly adopt a matching baseline, then activate a successor and roll it back.
  Reject mismatching source/code without losing the current descriptive root.
- Reject old rollback capabilities after reload, including a live load in a process
  with retained inverse state. Keep independently captured code callable after GC.
- Preserve native and returned-Error live-load failure handling at the recovery
  boundary; verify partial-load reports and absence of resurrected authority.
- Preserve opaque-native rejection without relying on an adopted ledger, snapshot
  consistency checks, parser/compiler-disabled startup and the language suite.

Lean now states that all restored images lack managed authority, even when their
code matches the recorded revision; separate laws require explicit request and
matching code for baseline adoption. TLC explores restoration without automatic
adoption alongside live-code, intervening-edit and callback preservation controls.
These are abstract contracts, not a proof of the entire decoder or native loader.

## Remaining simplifications

Startup adoption, sealing and sticky validation were subsequently removed in
[opt-in baselines](opt-in-generation-baselines.md). The following records this
slice's earlier boundary.

Make ordinary startup/compilation certification opt-in; remove sticky explicit
reconciliation drift and initial sealing policy; qualify activation inside entered
code before removing Host-only intent fulfillment; separate logical discard from
file cleanup and diagnostics. Finish those simplifications before more Store
migration. The current native source registry and image owner coordination remain.

## Qualification notes

The fresh production build uses 38 native libraries, four source bootstrap modules
and 23 authored tools. It is isolated under `/tmp/birnam-image-adoption/prefix`;
the user installation is untouched. The matching/divergent mapped, rebuilt and
resaved Host cases pass, as do explicit baseline adoption, successor rollback,
opaque-native rejection without a ledger, retained-code GC and live-load failures.
The reflection suite passes with the reduced 13-slot CompiledUnit layout.

The old runtime failed the new no-automatic-adoption assertion. The final language
suite passes 826 assertions; the CLI gate passes 61 policy checks. Formal checks
pass 353 Lean theorems, reject 100 mutations and obtain all 224 expected TLC outcomes.
Parser/compiler-disabled installed startup and the native transaction, ledger,
retirement and snapshot-consistency tests pass.

Qualification corrected stale test expectations for automatic event facts, the
number of help entries, and the intentional no-adoption diagnostic. An attempted
combined Host test run could not initialize the process-global Id runtime twice;
each Host test was rerun in a fresh process. A build could not publish an image
when the filesystem filled; disposable artifacts from these qualification runs
were removed and the complete build passed. Logs and source hashes are retained
in the measurement receipt; these intermediate failures are not counted as passes.

The standalone ordinary-edit test passes both adopted and unadopted source-free
worlds through mapped, rebuilt and resaved images after deleting source files.
The lifecycle-record image test also preserves field aliases, cloning, numbering
and explicit annotations. See the [qualification receipt](../../measurements/explicit-image-adoption-20261016.json)
for exact commands and source hashes. Temporary test images were removed after
verification to reclaim disk space; logs, receipts and the current build remain.
No push, release or user installation accompanies this slice.
