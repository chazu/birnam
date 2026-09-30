# 74AA — Scope counted-index control reads to a fused numeric invocation

Status: isolated implementation experiment; no integration or promotion.

The retained 74Y normal-floor profiles include environment lookup under
`fused_numeric::execute -> regions::Compiled::invoke`. Each admitted PC reads
`BIRNAM_DEV_COUNTED_FRAME_INDICES` even though the enclosing numeric fusion has
already proved that the entire bounded invocation has no observing method,
collection or retirement boundary. The profiles identify repeated work; they
do not establish its isolated timing cost.

Read the switch lazily at the first admitted region and reuse that value only
inside this invocation. Discard it when the invocation returns. Ordinary region
execution continues reading after each PC's admission. There is no process,
Runtime, Machine or candidate cache, and no stored ABI or image field. A native
publication override makes fusion ineligible and retains live reads around that
observer. Test-floor control stops retain their existing fusion refusal.

Keep all per-PC checks, whole-body budget checks, allocation charges, physical
fallback, Frame copies, ordinary publication and numeric identity unchanged.
The environment switch remains a live development control between invocations;
concurrent environment changes need not be sampled between internal PCs of a
single admitted observation-free invocation. No Id operation is made immutable.

The differential runtime test must prove one read per fused invocation, fresh
off/on/off decisions on the same hot Runtime, ordinary per-PC reads, exact
logical allocation charges, physical savings, results and post-GC values under
both GC modes and both return forms. An observing publication method changes
the switch during execution and must affect subsequent PCs through fallback.
First retain the failing repeated-read result on the predecessor implementation.

Run the complete debug/release runtime suites after the change. Production,
installed V2/control and direct predecessor/candidate performance acceptance
remain required before integration or a timing claim. Preserve the frozen 74Y
and 74Z verification jobs. Any eventual timing remains `qualified: false` under
the user's quiet-host waiver.

## Runtime evidence

The corrected 16-operation fixture reproduces 15 reads per fused invocation on
the unchanged implementation. The scoped implementation reduces this to one,
including when counting is disabled. The same hot Runtime honors off/on/off
changes; unfused execution keeps one read per admitted region. Both return forms
and both GC modes preserve allocation charges, physical savings and results.
The observer changes the switch after the second Call publication, placing
admitted native regions on both sides of the change. PC 0 performs entry/global
setup and is not itself an admitted copy region.

The initial short fixture admitted only one region and was unsuitable for the
repeated-read assertion. The first observer test changed the switch before any
admitted region, so it could not prove behavior on both sides; both attempts are
retained alongside the corrected tests. These were fixture corrections, not
changes to runtime admission or GC roots.

`74aa-scoped-control-runtime.json` records 219 passing runtime tests and 15
ignored tests in each profile, including the new differential test and existing
late-binding, collection-budget, fault and V2 coverage. The production gate,
installed acceptance and isolated before/after timing remain pending. Removed
environment queries are a measured work reduction, not a speedup estimate.

The full `recovery-graphs.py --unboxed --verify` production gate is now running
against the frozen `7ba56db1` implementation. Its initial live process identity
and source bindings are retained in `74aa-scoped-control-production-start.json`;
the production receipt is `74aa-scoped-control-production.json`. A live gate is
not completed acceptance. Installed comparison and timing still follow a
successful, audited production proof.

The production gate has completed successfully: all 75 commands, 3,164
optimization source cases and 24 evaluator-lifetime/baseline-storage cases pass.
`74aa-scoped-control-production-proof-audit.json` independently verifies all 295
current inputs, required matrix counts/statuses, both diagnostic floor hashes
and exact lifetime/storage coordinates. Installed counting-on/control acceptance
and the isolated cost comparison remain required. The proof does not promote
the scoped-read candidate or include the later quiet-REPL fixture repair.


## Explicit quiet aggregate callers

The `codex/74aa-scoped-control-quiet` composition applies the two fixture-file
changes from `48a1d54d` to the accepted original AA implementation. Only aggregate
measurement callers request quiet REPL presentation. The shared source generator
keeps ordinary host printing by default, so evaluator-lifetime tests still
exercise the real nested evaluation and host-output boundaries.

`74aa-quiet-source-parity.json` proves all 50 ordinary generated fixtures match
original AA and all 50 explicitly quiet fixtures match the previously tested
quiet presentation. Rust, compiler, modules, runtime admission, allocations and
roots are unchanged. `74aa-quiet-lifetime.json` records all 12 debug/release
lifetime cases passing with the original production-proved AA diagnostic floors
and diagnostic codegen modules over an existing host layout. This is focused
fixture evidence, not a fresh installation of the changed composition.

The exact composition still needs its own full 75-command production gate,
fresh normal release installation, 1,848 focused numeric/Id/aggregate/V2/loop/
closure cases and 240 normal/test-floor driver cases before cost comparison.
The acceptance runner waits for the existing String production build to end to
limit simultaneous heavy builds. That is scheduling only; a failure pauses for
inspection of possibly shared fixture defects. Existing jobs and their evidence
remain intact. No integration, default promotion or timing claim follows from
these fixture checks.

## Direct scoped-read cost comparison

The repaired composition has now passed all 75 production commands. Its fresh
release installation is published and all 1,848 focused installed cases plus
240 normal/test-floor driver cases pass their independent audit. The next
comparison uses that exact installation and the accepted counted 74Y
installation. Their 295-input production inventories differ in only
`fused_numeric.rs`, `regions.rs`, and the corresponding fused numeric tests.
This isolates the scoped control-read change from the earlier counted-index and
reverse numeric-journal changes.

`counted-control-checkpoint-timing.py` runs the same 60 ordinary Id workload,
GC-mode and unboxing coordinates on both installations with counting off and on.
A 240-process, two-batch smoke validates execution before the 960-process,
100-batch, four-round measurement. Every state occupies every execution-order
position. Comparisons report scoped versus unscoped behavior separately for
counting off/on, plus the counting on/off difference within each build. They
retain cold execution, steady execution, execution including collection,
materialization, process wall time and peak RSS.

The shared `counted_checkpoint_evidence.py` auditor requires both complete
production proofs, current source and published artifact bindings, and all
1,848 focused plus 240 normal/test-floor driver coordinates before measurement.
It reparses retained numeric/Id output and every aggregate attempt, checks
allocation conservation and all GC snapshots, and rejects an outcome mismatch
from comparable starting heaps. It retains the original V2 alias, identity,
recovery and native ownership assertions. The accepted predecessor passes this
reusable audit. Twelve admission checks reject missing/duplicate matrices,
stale sources, wrong publications, mismatched receipts, a correctly rebound but
corrupted GC snapshot, and nonisolated source changes; they also check complete
four-position rotation. These are checker tests, not new candidate performance
evidence.

Both measured runs remain `qualified:false` under the quiet-host waiver. The
queue waits for installed correctness acceptance, not for host quiescence, and
stops on failure. Results require review before integration or default promotion.
The previous noisy counted comparison is not repeated without this changed
implementation hypothesis.


### Complete installed acceptance and receipt-shape repair

`74aa-quiet-installed-audit.json` validates all 2,088 installed cases against the
current 295 production inputs, published normal prefix and diagnostic floors.
It reparses the retained numeric/Id and aggregate observations, preserves the
original V2/loop/closure controls, and separately revalidates the predecessor's
2,088 cases. This completes installed correctness for scoped controls; cost
selection and default promotion remain separate.

The first cost smoke stopped before producing any measurement receipt. Its
reusable auditor expected exactly 14 acceptance steps, while the scoped queue
also records the production step, for 15. The repaired auditor permits that one
explicit extra step only when its path/digest identify the already verified
production proof, its log matches, and it succeeded. Every focused/driver step
remains mandatory. Nine checks verify both actual receipt shapes and reject
duplicate or altered production records, missing focused cases and unknown
extra steps. The failed queue/logs remain bound in the audit.

No runtime, production-source inventory, installation, fixture or timing schedule
changed. `74aa-scoped-cost-repaired-start.json` records the new source/helper-bound
watcher. Its separate output paths preserve the failed attempt. The 240-process
smoke is running; the 960-process comparison starts only after it passes. Neither
run can establish a qualified speedup under the current host waiver.
