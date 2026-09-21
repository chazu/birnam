# 80I — Second concurrency review: Piumarta-inspired lens

> **Independent adversarial review, 2026-09-16.** Reviewed commit
> `f6bbe471ec396a0916c7f8011ff80ae112c3053b`, specifically Designs 80, 80A, 80G,
> and 80H, with 80F for accepted decisions and 80B for the previous review.
> This lens is an interpretation, not participation or endorsement by Ian
> Piumarta. It reuses the open-implementation and small-metacircular-foundation
> lens explained and sourced in [80B](80b-concurrency-piumarta-review.md).
> Findings are review judgments, not implementation authorization.

## Verdict

**The revised architecture fits the open-implementation direction. One concrete
control transition still needs a design before implementing cancellable waits.**
The proposal now places scheduling and lifecycle behavior in ordinary Birnam
objects, accepts the consequences of replacing them, and names an honest native
boundary. It does not need another supervisory layer to defend the supplied
protocols against user edits. I found no demonstrated unavoidable invariant
violation and no reason to reopen the parallel-domain choice.

The remaining gate is narrower than selecting all primitive names or designing
every cleanup representation: demonstrate how cancellation leaves an already
suspended V2 invocation and reaches Birnam cleanup without treating cancellation
as its ordinary return value or losing the invocation's roots.

## 1. High — Specify the cancelled pending-Call transition before implementing waits

**Severity:** pre-implementation blocker for the cancellation/cleanup substrate,
not a blocker for accepting the overall architecture or for independent transport
work. The design acknowledges the missing mechanism; this finding identifies
the exact existing transition that the mechanism must extend or compose with.

**Anchors at the reviewed commit:**

- [80G](80g-concurrency-task-lifecycle-exercise.md), lines 22–26: cancellation
  withdraws the receive wait and enters cleanup without resuming the task body.
- [80H](80h-concurrency-review-edge-cases.md), lines 81–86: explicit failure
  exits task control, runs cleanup, and settles descendants.
- [80](80-concurrency-domains-and-structured-tasks.md), lines 621–627: the
  proposed substrate supports cleanup control transfer without requiring a
  native scope engine.
- [60P](60p-v2-control-completion.md), lines 13–29: suspension stages an exact
  result-bearing Call edge; predecessor roots remain active until an answer
  commits copies, clears, and successor roots.
- [execution.rs](../../rust/birnam-runtime/src/execution.rs), lines 2281–2300:
  `resume_suspended` commits that pending Call with a supplied Oop before
  returning the Machine to Running. Lines 2304–2320 show that `fail` changes
  lifecycle and clears control bookkeeping; it does not invoke cleanup.

**Concrete trace:** A owns a resource with registered cleanup and suspends in
`receive`. Its current Frame has a staged V2 Call edge and predecessor root map.
Cancellation wins the wait. The language contract requires A to skip the normal
continuation after `receive`, run cleanup, and potentially suspend again within
cleanup. Neither of the two existing operations above directly expresses that
sequence:

1. Resuming with `nil` or an ordinary Error commits a normal result edge. Unless
   an explicitly designed adapter intercepts that result before returning to
   application code, execution continues down the successful return path.
2. Marking the Machine failed removes the existing control context without
   driving the promised cleanup code.

There is a root-state issue as well as a return-value issue. The cancelled
invocation cannot simply jump to another PC or apply arbitrary successor clears:
cleanup still needs its live resource references and entered-code pins. A new
cleanup wait also must not reuse the identity of the cancelled receive wait.

**Smallest remedy:** add one lower-level trace for this case. Identify the
active Frame/pending edge, where the cancellation signal is consumed, which
ordinary Birnam activation runs cleanup, what retains its values and code, and
how its later wait gets a new identity. Two approaches are worth evaluating:

- A low-level wait returns an internal tagged wake result to an ordinary Birnam
  adapter, which initiates supported cleanup control before returning to the
  application caller.
- A dedicated checked control transition leaves the pending invocation without
  fabricating a successful answer and transfers to the cleanup protocol.

These are alternatives to assess, not prescriptions for new syntax or an ABI
change. Whichever approach is chosen must explain the same native-barrier case
already specified in 80H. Preserve ordinary Machine messages and the StepAction
object boundary; do not resolve this by compiler recognition of selectors such
as `await`, `fail:`, or `ensure:`. Keep ordering, cancellation policy, and cleanup
bodies in Birnam. The mechanism only needs to make the selected execution
transition valid.

## Disposition of the first review

| Earlier concern in 80B | Second-review disposition |
| --- | --- |
| Settlement authority versus replaceable behavior | **Resolved at the architectural level.** 80 lines 78–91 and 613–627, and 80F lines 134–140, explicitly reject policing replacement policies and permit Birnam scope accounting. The original demand that an overridden `outcome` never mislead its caller is not retained as a requirement. Machine ownership, roots, and valid resumption still require enforcement. |
| Scheduler execution and metacircular recursion | **Narrowed to implementation detail.** 80 lines 629–635 and 80G lines 153–160 choose synchronous select-and-return, release incompatible runtime borrows before policy calls, and accept a nonreturning replacement stalling its domain. The concrete entry and cold-start sequence must be described when implementing the driver; another policy supervisor is unnecessary. |
| Ready versus pending wait under native entry | **Resolved as a reviewable API choice.** 80H lines 88–104 rejects both through an ineligible synchronous entry and preserves today's fatal contract where that entry cannot report an out-of-band error. This honestly exposes the existing limitation rather than inventing recoverability. |
| Ordinary Error value versus explicit task failure | **Resolved for the public distinction; control implementation remains the gate above.** 80H lines 52–86 supplies both paths and keeps conversion an explicit application decision. Its cross-domain envelope also avoids reusing the legacy `isError` heuristic. |

## Small documentation cleanup, not an additional design gate

[80A](80a-concurrency-api-sketches.md), lines 147–153, still says the treatment
of deliberately cancelled children at normal scope exit needs a decision.
[80](80-concurrency-domains-and-structured-tasks.md), lines 327–337, and
[80G](80g-concurrency-task-lifecycle-exercise.md), lines 23–29, now settle it.
Update that explanatory paragraph so the small runnable-looking example does
not reopen an accepted question. Exact selector spelling can remain tentative.

## Review limits

This was a document and targeted source review of the fixed commit. It did not
implement a scheduler, execute runtime tests, or qualify thread hosting. Existing
thread affinity, fresh dispatch, Frame/code-pin lifetime, public ABI constraints,
and independent bootstrap remain required. No worker hot-reload, native-library
certification, or protection against intentionally broken replacement policy is
requested by this review.
