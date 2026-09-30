# 74Z — Preserve deferred allocations across floor-owned String cleanup

Status: isolated experiment. No default promotion or main integration.

Subsequent boundary: [81X](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81x-production-string-storage.md) moves the production
String byte owner and this exact cleanup witness into the loaded String module.
The experiment below records the original floor-owned implementation.

The 74Y normal-floor profile for the 16-operation boxed earlier-result case
shows deferred-allocation settlement only with counting enabled. The GC currently
settles every deferred identity if any dead finalizer can observe the sweep.
The exact floor `public_api::string_finalizer` only calls `release_string`, which
removes the object's owned byte buffer from the runtime's private Values map.
It reads no Id allocation observations, invokes no language method or foreign
callback, and allocates no Id object. This supports a narrower classification
for that exact implementation and a currently owned String allocation.

## Implementation and semantic boundary

Recognize only the exact floor cleanup function, the exact process-published
runtime to which that function routes, and an entry for this object in that
runtime's owned String storage. Do not infer authority from a
prototype name, vtable family, selector, producer annotation or a wrapper around
the same function. Read this state after mark callbacks, alongside the existing
inert Machine check. A missing storage owner, replacement finalizer or any other
foreign finalizer retains the ordinary settlement requirement. Dead dictionary
bindings continue to require settlement before retirement.

The String finalizer still runs at its ordinary sweep position and releases its
storage exactly once. No cleanup is skipped or delayed. Logical allocation
counts/bytes, collection thresholds/results, identity materialization for
observers, roots, Frame V2, active native ownership and public ABI remain
unchanged. The optimization avoids realizing already-dead identities solely for
a cleanup function whose implementation cannot observe them. Mark callbacks
and foreign finalizers keep their existing language-visible observations.

## Experiment and acceptance

First add a differential runtime test with physical versus deferred clone,
numeric and counted-index allocations, plus real owned String cleanup. Retain
mark observations, charges, reclaimed counts, final live values/bytes and String
storage release. A test-only settlement counter distinguishes avoiding physical
realization from simply ending with an empty heap. Confirm the old code fails
that assertion before adding the classification.

Require both GC modes, a wrapper finalizer that observes before cleaning up,
late finalizer replacement, and dead dictionary retirement. The latter cases
must still realize every deferred allocation before observing the sweep. Reject
the exact cleanup function on objects lacking the owned String entry.

Run focused and complete runtime tests in both profiles before installed-source
composition tests and normal-floor comparison. The main-based 74Y production
and acceptance jobs keep their frozen source; this experiment uses a separate
worktree and does not qualify their binaries. Profile evidence alone does not
establish the cause or magnitude of the GC-inclusive regression, and any speed
or broader promotion decision requires actual before/after measurements.

The first valid subprocess test initializes the real public runtime. It reproduces
10 unnecessary realizations in the exact-cleanup case before the implementation
change (`74z-string-cleanup-red.json`). An earlier exact-name invocation ran zero
tests, and a private-runtime fixture aborted because the public cleanup requires
`id_init`; neither is counted as target-regression evidence. The latter finding
adds an explicit process-runtime identity guard to the proposed implementation.

## Implemented classification and runtime verification

`string_cleanup_matches_runtime` requires exact callback identity and the exact
process-published Runtime. GC additionally requires the current object's owned
String entry. It preserves the existing inert Machine classification and the
unconditional dead-binding settlement condition. Only callback-free recognition
was added; String cleanup still executes through the original finalizer.

The public-runtime differential test now passes all 16 configurations: both GC
modes, four cleanup/observer situations and physical/deferred allocation modes.
It checks ten pending clone/numeric/index identities per deferred case. Exact
cleanup causes zero physical realizations; observing wrappers, late replacement
and dead dictionary retirement each retain all ten. Original charge, mark,
collection and live-count/byte observations match, untraced numeric pointer bits
do not keep their former child alive, and String storage is released. Admission
also rejects the exact function on a non-String owner and on a private Runtime.

The complete runtime suites pass 219 tests in each profile, with 16 diagnostic
or subprocess-only tests ignored by the general runner. The subprocess-only
cleanup test is executed by its passing parent test. Evidence is retained in
`74z-string-cleanup-runtime-correctness.json`. Exact-source installed production,
source/control composition and before/after performance acceptance remain open.
The counted main-composition proof applies to its unchanged predecessor and
cannot be reused as acceptance for this runtime change.

## Production and focused acceptance pipeline

The full production gate is running against the 295 frozen inputs at `cadb791b`.
The follow-on queue retains the original gate's process identity and requires
its complete successful proof before publishing a fresh release installation.
The queue then runs 1,848 counted numeric, Id-driven, aggregate, numeric V2, loop
and closure V2 cases across the exact verified debug/release floors, followed
by 240 ordinary Id driver cases on test and normal floors. Every existing
identity, allocation/GC, effect, native-lifetime and recovery assertion remains
required. It stops for independent audit and commit before timing or integration.

`74z-string-cleanup-acceptance-start.json` preserves the initial observation and
complete queue program; it is not a completed acceptance receipt. The live
receipt is `74z-string-cleanup-acceptance.json`. Inconclusive process observations
are retried against the same identity; no gate is restarted merely because a
poll fails. The predecessor's verification continues independently.

## Isolated normal-floor cost comparison

`scripts/string-cleanup-checkpoint-timing.py` compares the verified counted
predecessor with the cleanup candidate. Both already contain counted indices
and reverse numeric lookup; production source differences must be exactly the
cleanup implementation and its supporting tests. The driver requires both
complete production proofs, matching 240-case normal/test-floor driver receipts,
and the completed cleanup focused acceptance queue. It binds source, installed
artifacts, generators and receipts before every process.

The matrix covers three lengths, five shapes, both GC modes, unboxing off/on and
counting off/on. Four alternating pairs produce 960 normal processes at 100
batches each. A two-batch/one-pair smoke run checks actual execution before that
comparison. Execution, GC-inclusive, cold, preparation, wall time and peak RSS
remain separate metrics. No diagnostic floor or native probe runs in timing
processes; post-pair host observations do not qualify intervals.

`74z-cleanup-timing-admission.json` confirms that the incomplete real cleanup
production proof is rejected before timing, and that adding this independent
driver left all 295 running production inputs unchanged. Actual runner execution
and measurements remain pending acceptance; no performance claim follows from
this admission check.

## Completed production gate

The independent cleanup production gate passed all 75 commands, 3,164
optimization source cases and 24 evaluator-lifetime/baseline-storage cases.
`74z-string-cleanup-production-proof-audit.json` rechecks all required matrix
counts/statuses, command exits, 295 current source inputs, both current test-floor
hashes and exact lifetime/storage coordinates. This proof belongs to the cleanup
candidate and does not substitute for either counted-predecessor acceptance or
cleanup timing.

The follow-on queue has published its fresh release prefix and begun the focused
matrices. Full focused and normal-driver acceptance remains pending. The counted
predecessor's release aggregate matrix has an independently reproduced pre-work
heap mismatch; its remaining control and normal-driver checks passed. The
cleanup before/after comparison remains held until both checkpoints satisfy its
admission requirements.

The fresh-prefix debug aggregate matrix now passes all 240 processes and 120
matched counting-off/on pairs without retries. The independent
`74z-cleanup-aggregate-debug-audit.json` replays the original result, identity,
effect and allocation assertions, checks all four collection phases and exact
starting-heap comparisons, and binds generated sources, installed artifacts and
the diagnostic floor to this production proof. This follows the audited 60
debug numeric and 120 debug Id-call cases. This debug result
does not resolve the predecessor's release fixture mismatch or establish a
cleanup speedup.

## Release aggregate admission stopped the queue

All 924 debug focused cases now pass, including 448 numeric V2, 32 loop and
24 closure V2 cases; the independent controls audit checks their source,
identity/alias, recovery, pin and loop-reuse assertions. Release numeric 60 and
ordinary Id-call 120 cases also pass their audits.

The original queue then failed release aggregate admission, preserving 78
accepted cases across 100 actual processes and 11 incomparable starting-heap
pairs. Every individual process exits successfully and passes its original
assertions. The terminal coordinate is one clone, shape 4, precise GC, with
both aggregate consumers disabled. Three consecutive attempts differ only in
the restored-phase starting heap: 3,495 versus 3,496 objects, a 24-byte
difference. Which counting flag has the extra object changes between attempts.
`74z-cleanup-aggregate-release-failure-audit.json` replays every retained attempt
and confirms the original failed result. It does not convert it to acceptance.

`74z-quiet-repl-diagnosis.json` tests the ordinary Id presentation override at
this exact coordinate, across both GC modes and four alternating pairs. It uses
the unchanged String-cleanup floor and original assertions. The earlier counted
fixture diagnosis suggests this comparison, but does not identify the retained
object in this new failure. All 16 processes now pass and all eight pairs have
matching starting heaps and outcomes; the independent diagnostic audit replays
the original checks. This supports carrying the same presentation-only fixture
repair into a separately verified cleanup composition. The original failed
receipt remains authoritative for the unchanged fixture.

An independent queue runs the unreached release numeric V2, loop and closure V2
checks, followed by the 240 normal/test-floor driver cases. Its source-bound
initial state and complete program are retained in
`74z-cleanup-remaining-checks-start.json`. Completion does not resolve aggregate
admission or permit timing/integration. The original failed acceptance receipt
remains unchanged; no retry limit, pre-work comparison or GC root was weakened.

The independent remaining-check queue has completed: release numeric V2 448,
loop 32 and closure V2 24 cases pass, followed by all 240 ordinary Id driver
cases across normal and test floors. Their audits revalidate exact coordinates,
source/install/proof hashes, original identity/recovery assertions and driver
outputs. Its terminal status is `passed_remaining_checks_aggregate_unresolved`.
Only release aggregate admission remains unfinished on this unchanged fixture;
the source-bound presentation repair is the next required acceptance step.

## Cleanup composition with the repaired fixture

`codex/74z-cleanup-quiet-repl` starts from the completed cleanup evidence at
`04f45bcb` and carries the aggregate generator already checked on
`codex/74y-composition-quiet-repl`. Only `scripts/aggregate-source-smoke.py`
differs from the cleanup production proof's 295 inputs. The runtime and all
original GC, allocation, identity and comparison assertions remain unchanged.
The old failed cleanup receipt is retained as evidence for the old fixture.

Run a fresh full production proof as `74z-quiet-repl-production.json`, then
publish a matching normal release prefix and repeat the full focused and
normal-driver acceptance. The resulting exact-source proof must precede the
cleanup checkpoint comparison. The scoped-read and numeric-closure-body
experiments are absent from this composition.

### Quiet composition terminal verification

The original quiet composition at source `3b1266c6` reached its 75th gate
command and failed the debug `nested-sync` evaluator lifetime assertion. The
source fixture globally replaces `Repl printHostResult:`, while the lifetime test
requires the printed answer between REPL prompts. The process exited zero and
reported answer 42; the missing host-output assertion still prevents acceptance.
The terminal production receipt and follow-on watcher failure are retained in
`74z-quiet-repl-production.json` and `74z-quiet-repl-acceptance.json`. No focused
acceptance or timing is admitted by this failed gate. The next composition must
use the explicit quiet-caller fixture repair already isolated in `48a1d54d`,
then run its own source-bound verification. This result does not establish a
String-cleanup runtime failure or a performance benefit.

### Explicit quiet-caller repair

The follow-on `codex/74z-cleanup-scoped-repl` composition applies exactly the two
fixture-script changes from `48a1d54d`. Runtime and compiler sources remain those
of the failed quiet composition. The shared source generator now defaults to
ordinary REPL presentation; only the standalone aggregate and counted-aggregate
measurement callers request quiet presentation. Fifty source coordinates are
byte-identical to the original ordinary fixtures and, when opted in, to the
previously measured quiet fixtures.

All 12 evaluator lifetime cases pass using the prior composition's diagnostic
floors and codegen modules, including the three nested evaluator cases in both
profiles. The green receipt binds those exact binaries and the repaired fixture
sources. This establishes the fixture correction without substituting a rebuilt
runtime. It does not replace the new composition's full production gate or its
installed acceptance, which remain required before timing or promotion.


The repaired explicit-quiet-caller composition at `aca122e8` has completed its
own full production gate. `74z-scoped-repl-production-audit.json` independently
checks 295 current inputs, all 75 successful commands, current test binaries,
both diagnostic floor hashes and all 3,164 optimization plus 24 lifetime/storage
rows. Exact lifetime/storage coordinates and evaluator floor identity also match.
The prior broad-quiet fixture failure remains preserved; it is not the result of
this repaired composition. Fresh installed acceptance and costs remain required
before integrating this cleanup behavior.


The installed acceptance runner is now executing against this frozen composition.
`74z-scoped-repl-installed-start.json` records its live process identity, all 295
source inputs, production/audit hashes and complete runner. It publishes the new
normal release prefix `/tmp/birnam-74z-scoped-repl-prefix`, checks 1,848 focused
cases across debug/release diagnostic floors, then checks 240 normal/test-floor
driver cases. The runner preserves existing evidence and stops on failure; a
completed result requires independent audit before integration or cost comparison.
Its live receipt is `74z-scoped-repl-installed-acceptance.json`.

## Complete repaired installed acceptance

`7c7a26dc` records the repaired composition's complete installed acceptance:
1,848 focused debug/release cases and all 240 normal/test-floor driver cases.
`74z-scoped-installed-acceptance-audit.json` independently checks current source,
production proof and published artifact bindings, exact matrix coordinates,
every retained aggregate attempt and all collection snapshots, numeric/Id
outputs and V2/loop/closure identity and native ownership assertions. Both
aggregate profiles pass the original comparisons; no root or retry rule changed.

The direct normal-floor cost comparison is now running against the accepted
counted predecessor. `74z-scoped-cost-start.json` records its process identity
and frozen source inventory. The two-batch/one-pair smoke precedes the 100-batch,
four-pair comparison; both retain `qualified:false`. Installed correctness does
not establish a String-cleanup speedup or promote the implementation.

The cost smoke has passed all 240 ordinary processes. Its independent audit
rechecks current bound sources/artifacts/evidence, exact case and checkpoint
order, generated fixtures, parsed result/identity assertions and RSS. These
short samples validate execution only. The 960-process, 100-batch comparison is
now running; its results remain unqualified and require separate review.


### Completed direct cost comparison

The 240-process smoke and full 960-process comparison have completed. The full
run uses 100 batches and four alternating pairs per case on the two accepted
normal release installations. `74z-scoped-cost-completion-audit.json` rechecks
every bound source/artifact, exact process coordinates, raw result and RSS
parsing, paired calculations and queue exit/log evidence.

| Counting | Execution reduction | Reduction including GC | Positive execution cases | Positive cases including GC |
| --- | --- | --- | --- | --- |
| Off | 1.75% | 1.59% | 35/60 | 34/60 |
| On | 1.72% | 9.59% | 34/60 | 45/60 |

Each percentage is the median of the 60 per-case medians of paired reductions.
It is a descriptive summary, not a causal estimate or an application speedup.
All 480 host observations rejected admission, individual pairs vary widely, and
the execution results remain mixed. The GC-inclusive result with counting on is
a useful follow-up signal, but it does not justify default promotion. Complete
receipts are `74z-scoped-cost-timing.json` and `74z-scoped-cost-queue.json`; no
runtime integration or optimization default changes accompany these results.
