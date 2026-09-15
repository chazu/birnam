# 74AG — Compose the four consumers with complete Unit ownership

Status: integrated by [74AJ](74aj-landing-the-combined-consumers.md), 2026-09-14.
The focused release landing checks passed with exact source and artifact audits.
No optimization default is promoted; broader family qualification remains open.

The implementation originates on `codex/74ag-complete-consumer-composition`: source
`89805312`, source lifetime acceptance `6a5a11e5`, and restored artifact acceptance
`a364ae0a`. Its implementation and committed evidence are included in this merge.
74AJ supersedes the exhaustive replay requirement below for landing; the original
full verification was interrupted and is not recorded as passed.

Start from the 74AD interaction candidate `eb006eb7`: numeric closure bodies,
counted Frame indices, scoped control reads and exact String cleanup. Add the
shared numeric prediction implementation `b758b0ae`, compiler heap accounting
`42d976d9`, actual consumer teardown `e244324f`, collected Block republication
`976acf64`, and Unit-cache/synchronous escaped-owner repair `e340e1da`.

## Required behavior

Preserve all existing Id operations, mutable lookup, shared Cells, object and
Code identities, exact numeric/error behavior, logical allocations and collection
boundaries, Frame V2 recovery/control, and native ownership. Keep the frozen IdApi,
Method ABI, canonical descriptors and image formats. Shared prediction storage
never grants dispatch authority. Cached resumable vtables retain method edits;
synchronous nested vtables and Code objects remain fresh per invocation.

## Composition and evidence

Apply each implementation's source/test changes to the 74AD base with explicit
three-way conflict resolution. Preserve every pre-existing joint sequence,
budget, V2, changed-input and String-finalizer test. Record the source changes and
resolve overlaps by retaining both behaviors, not choosing one whole file.
Do not import old measurement receipts as proof for this new source combination.

Run compiler and complete runtime tests in both profiles, isolated complete
consumer teardown, and source-level lifetime tests for the combined numeric
closure body. The latter must actually exercise shared numeric predictions and
counted recovery before releasing its generation; a direct-capture-only closure
probe does not establish the combined ownership contract. Exercise observing
String finalizers during that work, shared capture changes, forced exits and
capture/suspend at original boundaries. Preserve sibling and escaped identities.

Run the full production source/build/install/image/relocation gate on the exact
combined inputs, publish a fresh normal prefix and repeat installed source and
ordinary application acceptance. Audit actual activity before attributing a
performance result. Costs remain unqualified under the quiet-host waiver.
Independent predecessor gates and costs provide regression expectations, not
acceptance by transfer. Keep source inputs frozen while their tests run.

Complete this composition before integrating it into main. Family completion
still requires the full 74T contract, including application behavior, retained
resources and profitability; this document does not replace those obligations.

## Focused composition checks

The initial merged source passed 92 compiler and 225 runtime tests in debug
(16 runtime tests intentionally ignored). The subsequent diagnostic-only probe
extension is covered by a new frozen-source queue that reruns the complete suites
and module tests in both profiles.

The source-level lifetime matrix retains the original direct-capture case and adds
an arithmetic body with three private invocations. Its two arithmetic operations
return the original captured Cell afterward, so the probe checks both exact Cell
identity and real numeric execution. Eight measured factory calls must complete
24 substitutions using exactly eight entry prediction kernels. A live numeric
consumer must retain nonzero native Unit, relocation and compiler heap storage;
ordinary root release and collection must return all private totals to baseline.
Weak references observe descriptor, plan, constructor and numeric-chain ownership
without rooting the language objects. Shared predictions remain invocation-local.

The same source runs with counted Frame indices off and on under conservative and
precise GC, with escaping and nonescaping results, and with closure optimization
off/on. Logical allocation bytes must match for every paired phase. Counting-on
acceptance additionally requires actual counted-index activity. Each process runs
three full prepare/warm/evict/hold/release cycles. Separate normal-floor cases
retain synchronous fresh identities and resumable cached identities and method
edits through eviction and escaped calls. No counter reset, manual retirement or
root clearing substitutes for ordinary lifetime release.

[Source composition audit](../measurements/74ag-source-composition-audit.json)
checks exact gate-helper ASTs against their originating implementations and exact
production ownership/numeric files against their source checkpoints. Seven ABI,
descriptor and image source authorities remain byte-identical to the composition
base. The [debug export audit](../measurements/74ag-normal-export-audit.json)
checks the normal floor, TCC and Codegen public exports against matching-feature
ownership-checkpoint builds. Neither audit establishes runtime acceptance.

The [debug completion audit](../measurements/74ag-debug-completion-audit.json)
passes 336 tests (92 compiler, 225 runtime, 18 Codegen module and one TCC module),
36 source processes, and 72 complete diagnostic lifetime cycles. Twelve of the
source processes use the normal floor for identity/mutation checks. All three
capture/numeric/numeric-counted matrices are complete under both GC modes, with
escaping/non-escaping results and closure optimization disabled/enabled. Their
exact coordinates, raw output, source hashes and current artifact hashes were
independently revalidated.

Enabled arithmetic consumers retain two private Units, 81,920 relocated bytes and
42 compiler heap blocks/91,377 requested bytes while live; all four totals return
to zero on each release. Each measured batch completes 24 substitutions through
eight shared predictions. Counting-on cases also exercise 168 counted indices;
all paired phases retain the baseline's 13,632 logical allocation bytes. These
are exact observations for this fixture, not timing or whole-process memory
claims. Ordinary eviction preserves live ownership and disables admission while
generation retirement is pending, as required by the existing control guard.

The focused queue is `/tmp/birnam-74ag-focused-verification.py`, with immutable
runner text and source hashes embedded in its receipt. The
[both-profile completion audit](../measurements/74ag-source-lifetime-completion-audit.json)
now passes 672 compiler/runtime/module tests, 72 source processes and 144 complete
diagnostic lifecycle cycles. All six source matrices pass, with exact coordinates,
raw-output validation, paired logical allocation comparisons, and current
source/artifact hashes. Twenty-four normal-floor processes cover identity and
mutation independently of the diagnostic probe. Isolated whole-Runtime teardown
is running. Production and installed acceptance are not yet established. The repository-wide formatting check reports
pre-existing differences only in unchanged `dev_slot_access.rs`; changed Python
sources parse and the edited shell driver passes `bash -n`.

The production/installation queue is
`/tmp/birnam-74ag-production-acceptance.py`. It was started with both dependency
identities verified live: the focused queue PID 53946 (18:42:30 local on September
14) and AF production PID 64635 (18:27:14). AF completion only schedules the build;
its evidence does not accept AG. The queue rechecks the complete focused receipt,
all source/artifact hashes and each lifetime matrix's exact case coordinates
before running AG's own full production gate. It then publishes the fresh normal
prefix `/tmp/birnam-74ag-production-prefix` and repeats changed-input and String
cleanup source acceptance. Broader installed application/resource/cost acceptance
still follows that stage; it is not supplied by the queue's passing status alone.


## Completed teardown and artifact-cache loss

All eight isolated whole-Runtime teardown tests subsequently passed, repeating
numeric, loop, discarded-clone and virtual-field semantic matrices three times
per profile. [The teardown audit](../measurements/74ag-consumer-teardown-completion-audit.json)
checks all 24 zero-resource observations, source hashes, exact test coordinates,
raw logs and byte-identical saved copies of the executed diagnostic binaries.
The original focused queue completed all 13 steps successfully.

The subsequent full focused audit failed because the normal debug/release build
artifacts had disappeared. [The artifact-loss record](../measurements/74ag-artifact-loss.json)
records 39 other emptied Cargo targets in the same sequential 18:57–18:58 local
window. The actor is unknown; the pattern is consistent with a wider cache
cleanup. `/tmp` and `/private/tmp` resolve to the same target inode. The consumer
fixture's only directory removal is restricted to its own fresh
`birnam-fields-PID-counter` directory. No source, receipt, or executed semantic
result was changed by this observation.

Rebuilding the same frozen source produced different normal-library hashes.
Do not silently substitute these files for the originally tested artifacts.
The failed audit and original completed matrices remain intact. Copies of the
rebuilt normal libraries and unchanged diagnostic libraries/test executables now
live outside Cargo caches in `/tmp/birnam-74ag-proof-artifacts`; their exact hashes
and source inputs are recorded in `74ag-restored-artifacts.json`.

A fresh acceptance queue, `/tmp/birnam-74ag-restored-verification.py`, repeats all
six source matrices and the compiler/runtime/module suites against the saved
normal/diagnostic modules. The original whole-Runtime tests use unchanged,
byte-identically preserved executables and do not need another repetition.
Only the owned AG production queue was interrupted, while it was still waiting
and had executed no production command. The replacement
`/tmp/birnam-74ag-production-acceptance-restored.py` requires the fresh source
receipts, exact saved-artifact hashes, completed teardown proof and captured
normal export contract before beginning the full production gate. Its immutable
startup evidence is `74ag-restored-verification-start.json`.

Until fresh artifact acceptance finishes, the earlier 672-test/72-process proof
is historical evidence for its recorded binaries, not acceptance of the rebuilt
normal files. Production, installed application acceptance and profitability
remain outstanding. No runtime implementation or optimization default changed
while recovering these verification artifacts.


## User-directed validation consolidation

The [slim validation workflow](74ag-validation-workflow.md) supersedes the earlier
queue scheduling and post-install replay plan. AC/AD/AE/AF predecessor queues were
intentionally cancelled; do not restart them from stale running/failed receipts.
Keep AG's restored focused verification, then use only
`/tmp/birnam-74ag-production-acceptance-slim.py`. The AF scheduling dependency is
removed. One full combined gate and fresh release installation remain required;
the post-install String replay uses the existing release quick matrix. See the
workflow document and cancellation inventory for exact scope and process IDs.

## Restored acceptance and production resumption

Restored artifact acceptance is complete: six source matrices passed all 72
processes, and compiler/runtime/module suites passed 672 tests. The
[completion audit](../measurements/74ag-restored-completion-audit.json) rechecked
source and saved binary hashes, raw results, allocation accounting and normal
exports. With unchanged preserved whole-Runtime tests, this covers 168 lifecycle
cycles. This supersedes the pending restored-acceptance status above.

The slim controller and its child process subsequently disappeared. Its partial
receipt contains 74 successful commands, but no completed final production
verification. The cause is unknown. The
[interruption record](../measurements/74ag-slim-interruption.json) preserves this
observation, source/test-binary checks and the resumption runner.

`/tmp/birnam-74ag-production-acceptance-resumed.py` is now the sole controller. It
retains those 74 historical command results and executes the unfinished production
verification with the original validation logic. Its proof is
`74ag-production-resumed.json`; fresh installation and the agreed focused
installed checks follow. No predecessor queue was restarted. Production,
installed acceptance, broader application acceptance and profitability remain
open. Optimization defaults remain unchanged.
