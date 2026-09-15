# 77 — Bounded validation and evidence maintenance

Status: implementation complete; final local acceptance recorded below. Scope: all
recommendations in the September 14 validation architecture report, in the order
approved by the user.

## Delivery checklist

1. Remove duplicate CI workspace and cold-bootstrap gates; acquire locked Cargo
   dependencies before offline checks. Fix installer temporary-directory ownership
   and repair the advertised dylib audit against an explicit normal-floor export
   inventory. Preserve two independent cold source builds.
2. Separate everyday, exhaustive, release and performance plans. Keep an explicit
   coverage map for mutation, identity/aliases, both GC modes, allocation/finalizer
   boundaries, V2 recovery/control, native lifetime and independent ABI/image
   oracles. Scope selection must not depend on receipt-output environment variables.
   Reuse development builds by configuration; keep cold source builds isolated.
3. Consolidate build/process/receipt mechanics, including persisted progress,
   per-command timeouts, total deadlines and reuse of terminal passed evidence only
   for unchanged relevant inputs. Family-specific assertions remain independent.
4. Store future raw results outside docs; archive existing measurement bytes
   losslessly with compact manifests and working readers. Maintain one current
   workflow/status entrypoint; clearly mark superseded instructions as history.
   Do not delete still-wired experiments or rewrite Git history.

## Acceptance and stopping rule

Verify each changed tool with focused failure-path tests, then run the everyday
plan and one cold-source release check to prove the build/publication changes.
Exercise exhaustive selection and the preserved coverage inventories; changing
orchestration does not require fresh profitability research. Use a bounded sample
of real matrix execution to check compact versus exhaustive behavior. Performance
smoke uses practical regression measurements; AC/quiet admission applies only to
qualified performance claims. Do not interrupt other applications for this work.

Keep interrupted work distinct from passes. A deadline is a failed/incomplete
check, never reduced acceptance. Add required follow-up evidence here instead of
silently expanding the scope into unrelated compiler optimization work.

## Commands and coverage

`mise run verify-plan` previews the everyday plan. `mise run verify` executes it.
`mise run verify-exhaustive` runs complete matrices; `mise run verify-release`
adds the normal dylib audit and the two independent source/relocation builds.
The legacy `rust-cutover-verify.sh` entrypoint invokes that same release plan.
`validate.py --only NAME` selects a step and its prerequisites. Default deadlines
are 15 minutes for everyday work and 30 minutes for exhaustive/release work;
these are ceilings, not runtime promises. A timeout fails and retains its logs.

| Contract | Everyday integration | Exhaustive/release additions |
| --- | --- | --- |
| Recovery maps and alias identity | Workspace tests; projection and small-callee installed fixtures | Every forced exit at every sequence size, debug/release |
| GC modes and escaping identity | Both GC modes, escaping/non-escaping sequences, sizes 1/2/3/32/33 | All forced PCs in both profiles |
| Allocation budgets | Baseline-derived exact boundary +/-8 bytes, half-body and small gap; no/partial/full completion required | Full gap sweep in both profiles |
| Mutation and native dispatch | Sequence mutation, input changes, all eight live API operations, C API fixtures | Complete additional unboxed numeric/source matrices |
| V2 capture/suspend | Literal and numeric capture/suspend with forced recovery in both GC modes | All diagnostic profiles and unboxed control shapes |
| Finalizers and native ownership | String cleanup, journals, evaluator roots and TCC storage lifecycle | Both diagnostic profiles and resource ownership tests |
| Byte/ABI contracts | Independent portable and stackless readers; installed image/control fixtures | Normal-floor export/dependency audit; two cold source snapshots |
| Performance | Separate explicit task; correctness never waits for host admission | Qualified measurements only when requested |

Routine closure matrices use one diagnostic release build and a normal release
installation. Sequence integration tests sample first/middle/last exits while
retaining every size and both GC/escape modes. The existing exhaustive oracle
continues to demand the full matrix; compact evidence is not accepted as full
qualification. The old `--quick` options are not used by the plan.

CI fetches locked dependencies before offline work. Runtime/compiler, language,
ABI, fixtures or validation changes select exhaustive correctness; bootstrap and
workflow changes select release checks; other edits select routine. Weekly and
manually requested CI runs use release qualification. Missing diff information
selects exhaustive. One core job builds the normal prefix and diagnostic floors;
four disjoint matrix workers reuse those exact artifacts and prerequisite
receipts. A final job checks every dependency and the full matrix oracle. Artifact
transfer uses tar to preserve executable permissions. Raw receipts/logs are
uploaded even on failure. Shards balance estimated work; coverage comes from the
complete plan and exact coordinate assertions, never those estimates.

## Delivery and validation

All four delivery passes are implemented. The normal-floor export inventory
passes debug/release audits. Installer success/failure cleanup is tested, and
explicit development caching separates configurations and holds its lock through
artifact copy. Release acceptance still requires two independent cold snapshots.

The routine plan has 36 steps; exhaustive has 77, release 79 and performance 11.
A completed routine run took about seven minutes of executed checks on this host;
this is an observation, not a runtime guarantee. The initial debug integration
attempt was interrupted for being too slow; routine integration now uses release
diagnostics while workspace unit tests retain debug coverage. Interrupted runs
remain recorded as incomplete and do not count toward acceptance.

Focused execution covered a full 320-case release sequence, debug V2 literal
control, 48 diagnostic counter cases, and all recovery families in both profiles.
The shared timing driver completed its eight-case smoke run using only the
workspace, installation and counter proof it needed, then removed its temporary
installation. The practical performance screen passed 24 alternating application
runs across four scenarios with no consistent slowdown. Both timing exercises
are unqualified and establish no speedup claim.

Seven orchestration tests cover failed-result reuse, input/artifact invalidation,
atomic receipt replacement, descendant timeout cleanup, explicit coverage and
prerequisites, disjoint CI assignment, seed transfer and installer ownership.
Python compilation and workflow YAML parsing pass. All 376 archived receipts
were checked byte-for-byte against their original Git contents as well as against
the archive manifests; 124,603,358 bytes compress to 14,619,943 archive bytes.
Original paths remain usable through the archive-aware readers and exact extraction.

The actual CI core stage passed locally and produced the shared artifact seed.
Hosted CI and the complete distributed exhaustive matrix have not been run on
this uncommitted change. The preserved full oracle, shard coverage and artifact
transfer were checked locally; hosted acceptance remains a distinct result.

Final cold-source execution passed with explicit receipt assertions for cold mode,
release profile and absence of diagnostic features. Both independent installations
ran after relocation. The final routine rerun also exercises replacement of an
existing String-cleanup output, fixing the old preserve-existing-file guard that
conflicted with resumable execution. Shared fixture generators are included in
that step's and the evaluator-lifetime step's cache inputs.

Final local acceptance: all 36 routine steps passed, including the closing
input/artifact/dependency audit. The receipt is
`.cargo-target/validation/routine-final-verified.json`; new raw evidence remains
outside the documentation tree. Workspace and unchanged installed checks reused
matching passed evidence; changed matrices executed again. The cold-source check,
both dylib audit profiles, focused exhaustive cases, orchestration/archive tests
and practical slowdown screen also passed. No complete hosted/exhaustive run or
qualified speedup is claimed by this delivery.
