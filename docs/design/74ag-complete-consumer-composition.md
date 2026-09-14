# 74AG — Compose the four consumers with complete Unit ownership

Status: implementation in progress in an isolated worktree. No family is complete
and no optimization default is promoted by this composition.

Implementation and evidence are committed on `codex/74ag-complete-consumer-composition`
in `/tmp/birnam-74ag-complete-consumer-composition`: source `89805312`, both-profile
source lifetime acceptance `6a5a11e5`. The measurement links below refer to that
branch's evidence directory. This main-branch copy records the design and status;
main has not integrated the implementation.

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
