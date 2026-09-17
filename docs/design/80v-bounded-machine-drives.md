# 80V — Bounded Machine drives and owner-side cleanup entry

**Follow-up:** [80W](80w-timesliced-task-executor.md) enables automatic task
timeslicing and protects the executor transitions described here.

This continues [80U](80u-birnam-task-cleanup-slice.md) with the runtime mechanisms
needed for timeslicing. The supplied task executor still runs cooperatively.
Switching that executor to budgeted drives requires protecting its multi-send
state transitions first.

## Implemented protocol

The ordinary Machine method table now includes:

| Send | Behavior |
| --- | --- |
| `runBlock: body budget: turns` | Start a new Machine with a nonnegative drive budget. |
| `continueWithBudget: turns` | Continue a runnable Machine without supplying a Call result. |
| `resumeWith: value budget: turns` | Commit a waiting Machine's actual result, then drive with a budget. |
| `isRunnable` | Observe whether a Machine is stopped for another scheduling turn. |
| `executionState` | Observe `#new`, `#running`, `#runnable`, `#suspended`, `#completed`, or `#failed` independently of the returned value. |
| `exitRunnableBoundary: payload` | Transfer a runnable Machine to its nearest protected handler without running its next body step. |

The existing unbounded `runBlock:` and `resumeWith:` retain their behavior.
All these sends retain ordinary lookup and can be replaced. There is no new
public IdApi entry, Method/StepMethod ABI, descriptor grammar, or image version.

Blocks that need scheduling must be selected for resumable execution through the
existing compiler protocol, as in 80U. Passing a synchronous native block does
not convert it into resumable code.

A budget counts outer driver turns, not time or source instructions. A turn
includes its committed action and any synchronous native invocation or optimized
work within it. Native callbacks and their nested resumable execution finish
before the outer driver can stop. Long native calls can still monopolize an
owner; this mechanism does not promise a wall-clock latency bound.

Budget zero stops before the next body step. On a budget stop the drive returns
the Machine and `isRunnable` answers true. Waiting also returns the Machine, but
`isRunnable` answers false. Completion returns its ordinary value. Consumers
must inspect lifecycle through the protocol rather than infer it from value
identity alone: an ordinary result may itself be a Machine.

## State and transfer invariants

The transient `Runnable` lifecycle is distinct from `Suspended`. Stopping is
allowed only with a committed current frame, one outer drive, no synchronous
native boundary, and no entry barrier. The stop invalidates the old scratch
action epoch and releases the drive activation; it retains frame identities,
precise roots, delimiters, and entered-code pins. Continuing changes neither a
result slot nor a program counter. Explicit internal disposal also accepts this
stopped lifecycle, releasing its retained frames and pins.

An owner-side exit uses the same checked transfer as an in-body boundary exit:
preflight validates frames, generations, the handler's pending Call, barriers,
delivery counts, and pin ownership before any mutation. Commit abandons only
the protected body and supplies the handler's actual pending result. The
Machine stays runnable; the owner chooses when to drive its handler.

External exit rejects while any handler delivery is pending. The library must
first install its shielding and acknowledge that delivery. Cancellation masking,
failure aggregation, and the choice of payload remain Birnam policy. Stopping
for a scheduling turn does not by itself deliver cancellation.

## Evidence

Six focused Rust tests cover:

- Zero budgets and alternating non-waiting Machines, including precise GC while
  only stopped frames retain their values.
- Wait-result resume versus runnable continuation, terminal lifecycle rejection,
  and successful completion without a fabricated result-bearing suspension.
- Synchronous native callbacks with nested resumable execution: the callback
  completes before a budget stop and no native activation remains parked.
- Atomic rejection for native/entry barriers, pending edges, wrong owners,
  generation/epoch exhaustion, invalid pins, and pending handler delivery.
- A real compiler-produced body stopped with its implementation pinned, method
  replacement while stopped, owner-side exit before another body step, precise
  GC across transfer, and fresh cleanup dispatch with pins released correctly.

The existing task smoke now runs 105 assertions in normal compiled startup and
forced source recovery. Its additional cases execute CPU loops without explicit
waits/yields, continue after GC, distinguish waits from budget stops and ordinary
Machine-valued results, and enter a
protected handler through owner cancellation. The existing image admission and
mapped/rebuilt outcome checks run in both startup modes. No validation phase was
added.

**Validation status:** `mise run verify-exhaustive` exits 0 with all 82 existing
checks and the complete-matrix/evidence audits passed. The normal entrypoint
accepts matching, audited receipts produced with the unchanged plan's commands,
inputs, dependencies, and timeouts; independent phases ran with two workers.
Its log is `normal-exhaustive.log` under the evidence directory, with the passed
receipt preserved as `exhaustive-run.json`. `candidate.json` binds the tested
source and release artifacts to their hashes. This is a fresh qualification of
these edits; 80U's earlier result belongs to commit `dce38f2d`.

The six focused tests also pass in a release build. The frozen IdApi client and
module pass without rebuilding and with unchanged checksums; the archived v4
image still rejects. The unchanged pre-change supported image retained from 80T
loads in mapped and rebuilt modes, and its saved method returns 42. Its producing
source commit is still unknown. Logs and provenance are retained under
`.cargo-target/scheduling-evidence/`.

Production installation and independent cold-source release acceptance have not
been performed for this revision. No performance claim is made.

## Next work

Protect executor admission, wait registration/resolution, queue mutation, outcome
publication, and join bookkeeping across multi-send interleavings, then select
bounded drives in the Birnam executor. Cleanup deadlines, asynchronous ingress,
channels, parallel domains, capture ownership, and task-record reclamation
remain separate work. This slice does not add execution across multiple CPUs.
