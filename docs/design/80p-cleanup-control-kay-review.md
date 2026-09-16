# 80P — Cleanup-control review through a Kay-inspired lens

> Independent adversarial review, 2026-09-16. Fixed reviewed revision:
> `9973f4d785acfa7ec28bbb00b225b7bb1ec8e92e`.
> Inputs: 80N, the accepted policy in 80, and dispositions in 80M; previous
> reviews 80J/80C supply the lens. All numbered anchors refer to that revision.

## Lens and verdict

This applies the comprehensible live-system, messaging, and object-composition
lens established in [80C](80c-concurrency-kay-review.md#baseline-and-lens), using
its previously cited sources. It is an interpretation, not impersonation or an
endorsement by Alan Kay. No new historical comparison is needed.

**Retain the adapter-plus-delimited-exit direction.** It now explains how a
cancelled wait reaches ordinary Birnam cleanup without treating cancellation as
an application value or silently invoking fatal Abort. The explicit normal-return
handoff closes an important gap. The accepted compatibility constraints are
credible design obligations; this review finds no concrete need to break IdApi,
the Method/StepMethod ABI, descriptor V2, or image format.

One supplied-protocol handoff still needs a precise ordering rule before its
implementation is accepted. This is a narrow clarification, not a demand for a
new framework or native enforcement of user-defined policy.

## 1. Medium — Protect the outgoing failure handoff as well as cleanup entry

**Classification:** specify before implementing shield restoration and failure
propagation; no architectural or ABI change demonstrated.

**Anchors:** [80N §6](80n-cancellation-to-cleanup-transition.md#6-handoffs-and-nested-cleanup),
lines 200–220; [80N §7](80n-cancellation-to-cleanup-transition.md#7-other-entry-paths-and-limits),
lines 243–250; [80 §7](80-concurrency-domains-and-structured-tasks.md#7-structured-task-lifecycle),
lines 369–376 and 402–405. The exact section-7 policy is the failure-before-
cancellation publication rule and preservation of genuine cleanup failures.

80N carefully protects the route *into* E: handler delivery defers cancellation
until E owns the cleanup shield. On the way out, E aggregates failure evidence,
"restores the appropriate enclosing state, and propagates outward." The text
does not yet say when exit-in-progress ends relative to mask restoration and
transfer of the accumulated evidence to the next handler. The original exit and
cleanup failure may still be retained only by E at this point.

**Counterexample to an insufficient implementation:**

1. The body returns normally. Its cleanup explicitly fails with F.
2. The cleanup's fresh boundary returns F to E, which builds the aggregate.
3. E restores an unshielded enclosing cancellation state before propagating F.
4. The executor stops at an eligible boundary and delivers newly pending parent
   cancellation to the next active outer body boundary.
5. That transfer abandons E. If E was the only owner of F's semantic evidence,
   the outer handler receives cancellation without F. Final publication can
   report Cancelled despite a genuine cleanup failure.

This is not an unavoidable consequence of the proposal: keeping exit-in-progress
active through propagation would prevent it. The missing point is the explicit
protection/ownership transition that makes that interpretation mandatory for the
supplied executor. Merely rooting F prevents collection; it does not ensure the
next handler knows that F must participate in the outcome.

**Smallest remedy:** specify that the supplied protocol retains its propagation
protection until the next handler owns the aggregate, including while restoring
the enclosing mask. Alternatively, first publish the evidence into a traced
task-owned accumulator that every subsequent exit consults. Choose one rule and
show the stop between restoration and outward propagation: cancellation remains
recorded, outer cleanup still runs, and F remains in the eventual Failed outcome.
The existing normal-success return path should also state when this protection
ends so pending cancellation can subsequently be delivered.

This can be a Birnam protocol rule over the proposed execution facts. Arbitrary
replacement methods remain the user's responsibility; the remedy does not
require a runtime that certifies cleanup policy or knows what an Error means.

## What is now resolved or narrowed

| Concern | Assessment at this baseline |
| --- | --- |
| Cancelled wait resumes application code with a fabricated value | **Resolved in the proposed trace.** Only the adapter's park Call receives the wake record; checked exit abandons the application's receive continuation. |
| Body returns immediately before cancellation and skips cleanup | **Resolved in direction.** 80N lines 206–212 requires handler delivery to become visible in the same transfer that commits H's Call. Its concrete phase representation remains engineering work. |
| Cleanup fails and prevents outer cleanup | **Narrowed.** A fresh boundary retains E and its original evidence. The outgoing handoff in finding 1 is the remaining ordering detail. |
| Ordinary Error or envelope-shaped return is interpreted as task failure | **Resolved in the proposed library contract.** 80N lines 127–132 wraps ordinary returns as Normal, and lines 222–224 explicitly distinguish Error values from nonlocal failures. |
| Live replacement changes an already entered handler | **Resolved within existing invariants.** Entered H/E Frames retain their implementations; subsequent cleanup sends use fresh lookup. No new replacement certifier is needed. |
| Installed legacy ensure already promises cancellation cleanup | **Resolved as an explicit compatibility limit.** 80N lines 257–262 applies the guarantee to registered task-aware handlers and declines retroactive recognition of old Frames. The implementation must actually install that library protocol before advertising the new behavior. |
| Request cancellation destroys the serial worker loop | **Resolved at the accepted contract level.** 80M's dispositions give each request its own task/scope and explicitly isolate request failure from service lifetime. |

## Execution and compatibility check

The proposal is consistent in direction with [60A §3](60a-stackless-execution-semantics.md#3-semantic-objects):
actions remain inspectable Oops, native barriers remain real, and entered code
identity is distinct from subsequent fresh dispatch. [60B](60b-stackless-machine-and-frames.md)
freezes existing action layouts and checked Frame identity; 80N preserves those
and proposes a distinct extension rather than changing Abort. [60P](60p-v2-control-completion.md)
already describes committing a staged result-bearing Call with its copies,
clears, and root transition. That supports the adapter/handler result approach;
it does not itself implement multi-frame exit.

The installed [ensure:](../../src/id/stackless-control.birnam#L205) evaluates the
body, then cleanup, then returns the body's value. 80N correctly identifies that
as insufficient for the new nonlocal path. No compiler recognition of the
selector is justified or requested.

This review supplies no runtime proof, performance result, or implemented
capability claim. It changes only this review document. The new publisher,
action schema, boundary establishment, and preflighted transfer still require
the bounded evidence already requested by 80N.
