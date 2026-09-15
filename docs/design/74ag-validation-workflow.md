# 74AG validation workflow consolidation

Current landing decision: [74AJ](74aj-landing-the-combined-consumers.md) supersedes
the queue plan below following the user's request to shorten the path to landing.
The exhaustive AG replay and optional AI allocator experiment are stopped.
All 24 fresh release/compatibility/focused checks passed; the terminal receipt
is `docs/measurements/74aj-landing-resumed.json`. Completed source evidence was
reused after exact input checks.
The broader qualification objective remains open.

The user explicitly requested this consolidation in the side conversation on
2026-09-14: slim the workflow and speed up the remaining validation.

The active candidate is `codex/74ag-complete-consumer-composition`. AC installed
matrices and AD, AE and AF production/install queues were deliberately cancelled
as superseded. Their partial and completed receipts remain intact. A cancelled
queue is neither failed implementation evidence nor successful acceptance. Do not
restart these predecessor queues merely because their last receipt says running
or their subprocess terminated. The cancellation inventory is
`/tmp/birnam-validation-workflow-current.json` and the preserved copy is in AG's
`docs/measurements/74ag-workflow-consolidation.json`.

Keep the restored-artifact focused verification already running. Its replacements
have new binary hashes, which provides a concrete reason for its source checks.
Whole-Runtime teardown has already passed and must not be repeated on the same
preserved test binaries.

The sole production/install controller is now
`/tmp/birnam-74ag-production-acceptance-slim.py`, with receipt
`74ag-production-acceptance-slim.json`. It supersedes the stopped restored
controller. It waits only for AG restored focused verification; the former AF
build-slot dependency is removed. It retains source/artifact, ABI export and
teardown assertions and runs one complete AG production gate, followed by a
fresh normal release install and changed-input counting-off/on checks.

After that full gate, the String/finalizer installation replay uses the existing
`--quick` mode in release only (120 cases instead of 480). It still covers both GC
modes, counting off/on, private/escaping results, inert/observing/late-replaced
finalizers, and forced recovery. Both-profile full and over-limit cases remain in
the production gate. Runtime source and frozen test drivers are unchanged.

Do not launch additional broad predecessor acceptance or noisy timing matrices.
Reuse successful evidence for identical inputs and artifacts. A new check should
answer a concrete uncovered behavior, investigate a failure, or validate a source
or artifact change; it must not repeat the complete qualification stack by habit.
Broader application or semantic gaps still require targeted work when identified.
This consolidation does not declare the optimization families complete or promote
any default.

The restored focused verification has now passed. The slim controller and gate
later disappeared before recording the last verification command's result.
`/tmp/birnam-74ag-production-acceptance-resumed.py` supersedes that stopped
controller. It preserves 74 completed command results after checking the source
and recorded test-binary hashes, then executes the unfinished production
verification and the same installation checks. See `74ag-slim-interruption.json`;
the old running receipts are preserved partial evidence, not live jobs or passes.
