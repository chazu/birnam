# 80N — From a cancelled wait to Birnam cleanup

> **Worked design proposal, 2026-09-16.** The defaults were accepted and committed
> in `aa58aa4342df1980458e9f1e19225aaa8f459f86` before this exercise. This document
> works through the remaining cancellation/control mechanism against that source
> revision. It proposes a concrete transition; it does not claim an implemented
> mechanism or completed invariant proof. The user subsequently accepted the
> compatibility constraints below and requested another adversarial review.
> The user subsequently accepted the four findings in [80S](80s-cleanup-control-review-synthesis.md)
> and authorized implementation to begin. ABI/format breaks remain outside scope.
> Sections 9–10 record the accepted follow-up and supersede the earlier open
> encoding choices. Implementation evidence is tracked separately in [80T](80t-cleanup-control-implementation-evidence.md).

## 1. Result of the exercise

Use two distinct operations:

1. **Resume the wait adapter normally with an internal wake record.** Reuse V2's
   existing result-bearing resume. The adapter's continuation interprets the wake;
   the application's continuation after `receive` has not resumed.
2. **On cancellation, perform a checked delimited exit to a retained library
   handler.** Discard only the abandoned body segment, satisfy the handler's exact
   pending Call with an internal exit record, and let Birnam execute cleanup.

The first operation exists in mechanical form today. The second needs a new
general control operation and boundary protocol. It must not reuse current fatal
`Abort`/`Machine.fail` semantics. It is not compiler recognition of `receive`,
`await`, `fail:`, or `ensure:`. The native part validates and transfers execution;
Birnam chooses cleanup, shielding, failure aggregation, and propagation.

This is the smallest concrete direction identified here. Final action encoding,
publication selectors, and canonical protocol amendments require mechanism
review. Compatibility is an obligation to prove, not a result of this paper trace.

### Accepted implementation constraints

| Boundary | Required constraint for this mechanism |
| --- | --- |
| Public IdApi C table | Preserve its version, layout, entries, signatures, and existing contracts. Expose new operations through ordinary Machine messages rather than new C table slots. |
| Method and StepMethod calling ABI | Preserve receiver/argument/result conventions. StepMethods continue to return StepAction Oops, not a Rust-only enum or opaque action sink. |
| Frame/descriptor V2 | Preserve its byte grammar, Call-edge layouts, and root-map rules. Use existing result-bearing Call edges; retain valid old Frames and their entered implementations. |
| Image format | Preserve the current image format/version and existing serialization contracts. Live boundaries, waits, running/suspended execution, and control bookkeeping remain process-local. |
| Existing Machine/StepAction meanings | Preserve current action layouts, reflective accessors, publishers, validation, and behavior. In particular, do not reinterpret Abort as language cleanup. |

The permitted change is an explicitly specified extension of the Machine/object
execution protocol, with private runtime state as needed. That is distinct from
changing the C ABI or serialized formats. Keep the independent source bootstrap
and existing artifact/native-module compatibility obligations. New facilities
may require the updated runtime/library; unchanged format does not promise that
an older runtime can execute programs that use new operations.

If a concrete mechanism cannot meet these constraints, present the conflicting
operation and a bounded alternative before proposing a break. Do not silently
broaden this task to revise an ABI, descriptor grammar, or image format. These
constraints concern cancellation-to-cleanup; thread-domain routing still needs
its own review under Design 80.

### Suggested extension approach for review

Expose boundary establishment and checked exit through the existing ordinary
Machine-message surface, backed initially by runtime primitives. Register those
method implementations through the existing binding machinery. Keep library
policy, adapter interpretation, and cleanup handlers in Birnam. No compiler
recognition of their selector names or bypass of ordinary fresh lookup is needed.

If a new driver action is needed, specify a distinct canonical action kind/vtable
and a checked Machine publisher. Preserve every existing v1 action object and
payload; the extension must explicitly define its own schema and reflection.
The existing common header/validated slot-index discipline is the starting point,
not permission to reuse Abort or change an old action's size. The new action must
remain an Oop inspectable through ordinary protocols. Validate boundary identity
against owner-local runtime records instead of embedding borrowed Frame pointers.

Keep boundary identity, handler-delivery phase, and lifetime metadata in transient
owner-local runtime state. Satisfy H's already-described Call edge with a rooted
internal result; do not add cancellation-specific PCs or serialized edge kinds.
Ordinary library code/outcome objects use existing serialization where admitted;
the live native-backed execution records remain non-imageable. Check canonical
object registration, native binding identities, and image admission explicitly:
unchanged file bytes alone are not sufficient compatibility evidence.

The action encoding and boundary publisher are proposals for reviewers to test,
not already accepted modifications to the frozen execution protocol. The purpose
of this review is to identify a concrete counterexample to this additive approach
or the smallest missing transition, not to assume an ABI break is necessary.

## 2. What the source actually provides

| Existing mechanism | Consequence for this proposal |
| --- | --- |
| [60P](60p-v2-control-completion.md) stages a Call edge with predecessor roots and commits it when supplied an answer. | The internal park call can receive a tagged wake through ordinary resume. |
| [resume_suspended](../../rust/birnam-id/src/execution.rs#L2281) validates and commits the top pending Call before driving its successor. | The top pending Call must belong to a library adapter, not the application's `receive` call. |
| [commit_v2_transition](../../rust/birnam-id/src/execution.rs#L1371) applies parallel copies, result insertion, clears, and the successor root state. | The handler's exit result must use a real descriptor-admitted Call edge. No arbitrary PC assignment. |
| [finish_stackless_return](../../rust/birnam-id/src/runtime.rs#L3296) pops a frame, commits its caller's Call, and releases its entry pin without invoking user code while borrowed. | Delimited exit can follow the same transfer discipline, but must preflight the entire multi-frame transition. This function is not already a delimited unwinder. |
| [DelimiterKind](../../rust/birnam-id/src/execution.rs#L1609) currently contains Host and EntryBarrier; capture/restore is same-Machine and one-shot. | There is no supplied library-handler delimiter today; ordinary capture/restore does not by itself escape back into a retained cleanup handler. |
| [Abort handling](../../rust/birnam-id/src/runtime.rs#L4101) calls `fail`, which clears execution control state. | It cannot stand in for language cleanup. |
| [Current stackless ensure:](../../src/id/stackless-control.birnam#L205) evaluates its body, evaluates cleanup, then returns the body's value. | It supports sequential/Error-value behavior, not nonlocal cancellation. A task-aware implementation must establish a real handler before the body runs. |

These are current-source observations. The rest of this document is the proposed
extension, including its obligations rather than an assertion they already hold.

## 3. The frame arrangement

Suppose application code calls `receive` inside a protected body. Conceptual
frame names below are not public selectors or a new syntax:

```text
older
  T   task runner / outer task-control boundary
  E   Birnam ensure implementation, retaining cleanup and outer control state
  H   library boundary wrapper, awaiting a wrapped protected-body invocation
  B   application body, awaiting receive's result
  A   Birnam receive/wait adapter, awaiting an internal park result
newer
```

Additional ordinary call frames can occur between these roles. The invariant is
that E and H remain below the body segment to be abandoned. H is a resumable
wrapper whose own Call to the body has a real result slot and continuation.
It must remain a live frame while the boundary exists; an invocation needing
that frame cannot tail-replace it. This is established by the actual wrapper
code/Call edge and checked boundary setup, not selector-based compiler treatment.

The boundary registration identifies the owning Machine, H's generation-checked
Frame handle, its exact pending Call, and the stack segment above it. Registering
the boundary and starting that invocation must be one valid control transition:
the body cannot run before its handler is established. No foreign activation
may lie between an exit source and H.

The wrapper's body returns `Normal(value)` on ordinary completion, even when
`value` is an Error or resembles an internal envelope. Explicit control exit
supplies `Exit(reason, evidence)` through the boundary protocol. These are ordinary
internal objects with distinct wrapping, not magic treatment of application
return values. The supplied library interprets them; replacement methods remain
the user's responsibility.

E retains its cleanup closure and state across its call to H. Descriptor root
maps must therefore retain those ordinary live values. A reference needed by
cleanup must be captured/retained there, not recoverable only from a body frame
we intend to discard. The owner also strongly traces the unfinished task and its
Machine, as settled in Design 80.

## 4. Cancelled receive: exact transition sequence

| Step | Execution | Root/control state |
| --- | --- | --- |
| 1. Park | A requests a low-level park; the Machine suspends. | A's park Call is pending under its predecessor root map. B's receive Call and H's body Call remain pending in older Frames. Wait W1 identifies this suspension. |
| 2. Claim cancellation | Domain ingress records cancellation; cancellation wins resolution of W1. | Invalidate W1 before making A runnable. The owner retains the task/Machine throughout queue changes. No message value is consumed by this cancelled receive. |
| 3. Resume A | Supply a rooted internal `Cancelled(reason)` wake record to the existing resume operation. | Commit only A's park Call: copies, result, clears, successor roots. B and H still have pending Calls. |
| 4. Interpret wake | A sees cancellation and invokes the Birnam task-control protocol. | Before nonlocal transfer, that protocol records exit-in-progress and defers repeated enclosing cancellation. No ordinary return reaches B. |
| 5. Prepare exit | Select the nearest active library body boundary H, retaining its exit record and E's cleanup state. | Validate the full target/segment/native-boundary transition before discarding anything. No language call runs under the mutable execution borrow. |
| 6. Commit exit | Abandon the body segment above H, including B and A, and complete H's pending invocation with `Exit(cancelled, evidence)`. | Pop abandoned Frames and retire their handles/pins once; do not execute or commit their normal continuations. Commit H's exact Call edge with the exit record. Retain E and H throughout. |
| 7. Run handler | H returns its internal outcome to E through ordinary Return. E runs registered cleanup under shielding. | E's ordinary Call completion preserves its live cleanup captures. The body boundary is consumed; E's cleanup is claimed once and remains owned while in progress. |
| 8. Cleanup waits | E's cleanup makes an ordinary supported wait through a fresh adapter. | Use W2 and its own pending Call/root state. A late W1 notification cannot resume W2. Repeated enclosing cancellation is recorded but does not interrupt cleanup. |
| 9. Cleanup settles | E combines genuine cleanup failure with the original exit, then propagates to the next enclosing boundary if needed. | Outer cleanup handlers remain live below E. Each uses its own boundary; the supplied protocol visits them from inner to outer. |
| 10. Task settles | The task runner accounts for children and remaining cleanup, then publishes under the agreed failure/cancellation/success rule. | Release execution-specific ownership only after settlement; retain outcomes and explicit diagnostic references as required. |

The cancellation wake is a normal answer to **park**, not a successful answer to
the application's **receive**. Likewise, the exit record answers the library's
delimited invocation; it never masquerades as the application's job value.

After step 3, A has no pending park edge to erase specially. In step 6, pending
edges belonging to abandoned Frames die with those Frames; we do not apply their
successor copies/clears. H's pending edge alone gets the exit result. This is how
the adapter approach avoids inventing a cancelled form of every compiled Call.

## 5. What the checked exit must guarantee

The operation needs narrowly defined execution facts:

- Owner and Machine identities agree; H is live with the registered generation
  and exact still-pending Call. Boundary identities are invalidated on completion,
  exit, or removal; a reused Frame slot cannot revive one.
- The segment contains no live synchronous native/entry barrier, and no active
  nested host drive would be removed. Host-only or stale targets reject before
  mutation. Valid same-domain object identity is not authority to cross C frames.
- All pop counts, handle-generation increments, delimiter retirements, root
  transfers, result placement, and pin-release bookkeeping are preflighted. A
  failed preflight leaves the old execution intact. An allocation-free commit
  invokes no user code or collecting callback and cannot fail halfway through.
- The exit object and cleanup captures are rooted before removing any source
  owner. Abandoned frames' entry pins are released exactly once; H/E and any
  independently retained continuations keep their own pins. Keep necessary
  physical-code leases alive until the whole transfer is complete.
- Only H's validated edge installs a new root state. Frames still awaiting other
  calls retain their existing predecessor roots. Cleanup creates new frames using
  ordinary fresh lookup; entered H/E Frames keep their old implementations.

An existing `arena.pop()` clears a frame's descriptor/entry metadata and can
reject generation exhaustion. Repeatedly calling it without preflight and pin
accounting would not meet this contract. Reuse its mechanics where possible, but
specify a full transfer rather than claiming a loop around `pop()` is sufficient.

The native mechanism does not know which object is an `ensure:` method, what a
request failure means, or how siblings should be treated. Supplied Birnam code
chooses the next boundary and what to do with its result. Users can replace that
policy; checks protect valid transfer and lifetime, not faithful user semantics.

## 6. Handoffs and nested cleanup

### Entering cleanup must not expose a cancellation gap

Marking exit-in-progress before step 6 prevents another delivery of the same
enclosing cancellation from skipping the cleanup handler. Shielding must cover
the transfer through H into E, not start only at the first statement of the
cleanup closure. Invalid control entry cannot leave a task falsely marked settled;
preflight/entry failure follows the established host error contract.

Normal body completion has the same issue: cancellation can arrive after the
body returns but before E starts cleanup. The boundary must expose a result-
delivery phase to the supplied executor, which defers cancellation delivery
through that handoff until E owns the cleanup shield. Record the completion of
the boundary at the same transfer that commits H's Call; do not leave this to a
later allocating user callback. The phase is an execution fact; the supplied
Birnam executor's delivery decision remains policy. Timeslicing can continue.

### A cleanup operation fails

E runs the cleanup invocation under a fresh boundary, retaining the original
exit, remaining cleanup, and saved mask state below it. Explicit cleanup failure
exits to that boundary and returns evidence to E. E aggregates it, restores the
appropriate enclosing state, and propagates outward. A failed cleanup must not
skip E's aggregation/outer cleanup, nor cause the same cleanup to run again.

Ordinary Error values remain ordinary returns; they are not caught nonlocal
failures. Required child settlement follows the resource's structured ordering,
including shutdown signals needed before joins. New work remains owned.

### Cleanup's own deadline expires

Enclosing cancellation is shielded, but the cleanup attempt can have an explicit
inner cancellation/deadline boundary. Its expiry resolves that attempt's wait
and follows the same adapter/exit protocol, running that attempt's nested cleanup.
E observes timeout/failure evidence or an explicitly handled fallback. This does
not reuse the expired parent deadline or force-return through blocked native code.

## 7. Other entry paths and limits

- **Value wins the wait race:** A receives a value wake. A later recorded
  cancellation can still initiate exit before A returns, but committed channel
  transfer is not undone. If the task reaches final publication first, later
  cancellation cannot change it.
- **Explicit failure:** the Birnam task-control method initiates the same exit
  protocol with failure evidence; no wait adapter is needed. Existing fatal
  Machine failure is still a different path.
- **CPU-running task:** a bounded drive returns at an eligible committed boundary
  with a distinct runnable stop, not a fabricated result-bearing suspension.
  The owning Birnam executor can select control exit to the registered library
  boundary before driving the body's next step. This needs an owner-side entry
  to the same checked transfer for a stopped Machine; it is not supplied today.
  Do not inject an arbitrary ordinary Call into a Frame with no matching edge.
  If cancellation is shielded or the boundary is in handler delivery, continue
  the protected path and keep the request pending.
- **Debugger pause:** the explicit pause wins eligibility; ordinary cancellation
  remains recorded until an explicit supported resume. No frames/locks disappear.
- **Synchronous native callback:** no escape/suspend across that activation,
  even if a result is ready. Ineligible entry retains the documented existing
  error boundary. The transition runs only after the relevant StepMethod returns
  to the driver; it is not C/Rust stack unwinding.
- **Existing code and live edits:** the supplied task-aware `ensure:` must create
  these boundaries before invoking its body. Already entered older implementations
  are not retroactively rewritten or discovered by recognizing a selector on the
  stack. Ordinary sequential/Error-value behavior stays the same; concurrency
  cleanup guarantees apply to registered task-aware handlers. User replacements
  can change them, as agreed. Preserve independent bootstrap and old-frame pinning.

## 8. Narrow proof obligations before implementation acceptance

The mechanism needs an explicit action/protocol extension review against 60A/60B;
their existing action set and frozen publisher contracts cannot be silently
reinterpreted. Demonstrate the new control boundary and escape using existing
descriptor-v2 Call edges and unchanged public ABI/image formats, under section 1's
accepted constraints. If this cannot be demonstrated, return the concrete conflict
for a design decision rather than changing those contracts during implementation.
No existing `Abort` behavior changes under the same name.

Use a few discriminating examples when implementing, rather than a large rollout
plan:

1. Compiler-produced adapter park Call resumes with cancellation, application code
   after `receive` does not run, and the intended cleanup runs once.
2. Force GC at wake resolution, handler delivery, and cleanup's second wait; retain
   resource captures and entered-code pins, then release execution after settlement.
3. Nested cleanup fails; outer cleanup still runs and preserves the original cause.
4. Race cancellation with normal body completion and with cleanup entry; no handler
   is skipped, no old wait resolves the cleanup wait, no committed value is replayed.
5. Reject a stale boundary, wrong owner, crossed native barrier, and invalid edge
   before mutation. Exercise generation/pin preflight failures too.
6. Replace a method while stopped: retained Frames use entered code; subsequent
   cleanup sends use fresh lookup. Drive a CPU loop into the same exit path at a
   real scheduling stop without introducing fake call results.

These are future evidence requirements, not tests run for this document. The
worked trace supports the adapter-plus-delimited-exit direction; the remaining
engineering work is defining and validating that small control primitive and its
publisher/driver entry, including the protected handler-delivery phase.

## 9. Accepted third-review resolutions and first implementation

The four findings in 80S are accepted. The review itself remains a historical
assessment of commit `9973f4d7`, not evidence that the new mechanism works.

- **Atomic establishment:** use a distinct `BoundaryCallAction`, published by
  `Machine boundaryCallSite:` against an existing descriptor V2 Call site. The
  driver establishes the boundary on the waiting caller before resolving and
  entering the body. There is no native wrapper activation spanning the body.
  `ExitBoundaryAction`, published by `Machine exitBoundaryAt:`, delivers a rooted
  slot value to the nearest active boundary. It never means Abort. Propagation to
  successive enclosing handlers is ordinary library policy.
- **Captured execution:** keep the pending boundary and delivery bookkeeping with
  its owning frame. Detach/restore transfers that bookkeeping with the frame; it
  does not retain the retired external Frame handle. Existing capture delimiter
  selection stays unchanged. Detached metadata is unavailable to active execution;
  abandonment releases it without invoking user cleanup. Task ownership must retain
  detached execution until supported restoration or explicit abandonment; an empty
  active arena is not evidence of settlement.
- **Failure propagation:** the supplied Birnam protocol records accumulated actual
  failure in task-owned, traced state **before** restoring the enclosing cancellation
  mask. Every later exit and final publication consults that state. A late cancel
  therefore cannot erase the failure even if it interrupts further propagation.
  Error-valued ordinary returns do not enter this accumulator.
- **Image admission:** explicitly reject instances of both new live action families,
  including derived families, using existing image preflight. Canonical prototypes
  remain exported identities; inert outcomes retain ordinary serialization rules.
  No serialized boundary record, new image version, or live boundary token is needed.

The additive action schemas use the existing four-word v1 header (schema, Machine
index, Machine generation, drive epoch). BoundaryCall has the same five operand
slots as Call, under a distinct vtable; ExitBoundary adds one value-slot index.
Both support ordinary action reflection. Old actions and their publishers retain
their schemas and meanings. The protected Call must match the actual pending
descriptor edge, including its operand layout, before boundary installation.

Completing the protected Call, normally or by exit, records handler delivery in
the same allocation-free transition. `Machine boundaryDeliveryPending` exposes
this fact; `Machine acknowledgeBoundaryDelivery` acknowledges one handoff after
the library has installed its cleanup shield. Unacknowledged deliveries travel
with capture/restore and through ordinary returns/tail replacement until their
owner acknowledges them. This is bookkeeping for the supplied executor, not a
native enforcement of cancellation policy. Failure aggregation remains Birnam
policy; the runtime does not inspect the delivered Oop for cancellation or Error.

The first slice exercises establishment → park → cancelled wake → checked exit →
cleanup that parks again → completion. It supplies the control mechanism before
the task/channel/domain API. CPU timeslicing and its owner-side exit entry remain
separate work; this slice must not claim the entire concurrency design implemented.

## 10. Obtaining compatibility evidence

Keep this within the existing validation workflow, with four evidence groups:

1. Run the committed frozen IdApi client and module against the candidate runtime
   **without rebuilding fixtures**. Preserve checksums and the existing table,
   Method/StepMethod ABI, old action behavior, and reflection. The archived v4
   image must still reject; it is not a supported-image loading promise.
2. Reuse descriptor V2/compiler-produced execution tests. Add discriminating
   control traces for normal/cancel handoffs, nested failure, capture/restore,
   precise GC, code pin retirement, invalid transfer before mutation, and live
   method replacement. Baseline descriptor bytes must remain unchanged; generating
   all inputs with a changed producer is not independent compatibility evidence.
3. Load supported baseline images, exercise existing mapped/rebuilt paths, and
   round-trip ordinary data/code under the current format. Check new live-action
   rejection before durable publication, plus canonical references and inert data.
   A new-runtime self-round-trip alone does not prove old-image compatibility.
4. Use focused checks during development and `mise run verify-exhaustive` for the
   runtime change. Release acceptance uses the existing `mise run verify-release`
   export/dependency audit and two independent cold-source installs. No separate
   release procedure is introduced.

Record baseline/candidate source identities, unchanged fixture hashes, exact
commands/results, and outstanding checks in a short implementation evidence note.
Design review is plausibility evidence; unrun tests and incomplete runs are not
passes. Full concurrency, production installation, performance qualification, and
old-runtime execution of new operations are not established by this narrow slice.
