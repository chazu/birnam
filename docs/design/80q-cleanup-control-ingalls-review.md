# 80Q — Cleanup control review through an Ingalls-inspired lens

> **Third independent adversarial review, 2026-09-16.** Historical lens
> interpretation, not participation or endorsement by Dan Ingalls. Findings are
> review proposals; they do not amend accepted policy or authorize implementation.

## Fixed input and scope

Reviewed commit **`9973f4d785acfa7ec28bbb00b225b7bb1ec8e92e`**, principally
[80N](80n-cancellation-to-cleanup-transition.md), against the accepted policy in
[80](80-concurrency-domains-and-structured-tasks.md) and
[80M](80m-concurrency-second-review-synthesis.md). All line anchors below refer
to that commit, including source references. This reuses
[80D's sourced lens](80d-concurrency-ingalls-review.md#sources-and-lens) and
[80K's previous assessment](80k-concurrency-second-ingalls-review.md); it adds
no new historical attribution.

The focus is comprehensible execution transitions, roots, entered-code lifetime,
and ordinary replaceable policy. The user's accepted responsibility for changing
policy governs this review. No finding requires watchdogs, certification of user
methods, frozen cleanup semantics, or automatic protection from custom deadlock.
The public IdApi, Method/StepMethod ABI, descriptor V2, and image format must remain
unchanged; an explicit additive Machine/StepAction protocol is permitted for review.

## Verdict

**Retain the adapter plus checked-exit approach. One continuation interaction
needs an explicit contract before implementing the affected mechanism.** No
examined path establishes that a public ABI or serialized format must change.
The document is candid about its remaining protocol proof obligations; a paper
trace is not implementation qualification.

The cancelled-wait path is now substantially concrete: the adapter receives an
ordinary park result, H alone receives the exit result, abandoned body edges do
not commit, and E retains cleanup state. Full-segment preflight, allocation-free
commit, entry-pin retirement, and a protected handler-delivery phase address the
dangerous shortcuts raised previously. The gap below concerns how that new
boundary state composes with an existing execution operation.

## 1. High, pre-implementation — Boundary metadata must follow captured execution

**Design anchors:** 80N lines 71–77 (transient owner-local metadata), 114–125
(live H and generation-checked registration), 167–181 (invalidation and retained
continuation pins), and 206–211 (handler-delivery phase).

The proposed boundary identifies H by its live Frame handle and exact pending
Call. Existing continuation capture can detach a segment containing H and E:

- [execution.rs](../../rust/birnam-id/src/execution.rs), lines 1138–1175,
  detaches Frames and increments the original arena-slot generations.
- Lines 1178–1215 restore the Frame contents without restoring those old handle
  generations. The restored H therefore does not have its old live handle.
- Lines 3655–3706 transfer Frames and code pins into a Continuation; lines
  3753–3832 validate/install its segment and transfer pins back. They do not
  transfer the proposed new handler records or delivery phase.
- [60A](60a-stackless-execution-semantics.md), lines 273–291, preserves delimited,
  same-owner, same-Machine one-shot capture/restore. [60B](60b-stackless-machine-and-frames.md),
  section 8, makes detached Frame ownership explicit.

**Concrete trace:** a body protected by the supplied task-aware handler captures
to an enclosing legal delimiter. H/E move into the continuation. The owner later
restores it once, then cancellation reaches an eligible point. If registration
still contains H's old handle, checked exit rejects a now-stale target. If capture
treated detachment as ordinary boundary removal, there is no registered handler
to receive the exit. Merely keeping the continuation's code pins alive solves
neither case. Capturing during handler delivery creates the analogous question
for the phase that prevents cancellation from skipping cleanup.

This is **missing composition semantics, not a demonstrated contradiction** in
80N's proposed extension. Choosing a new library boundary as an implicit capture
delimiter or barrier might avoid this particular trace, but that would itself
require an explicit interaction rule; it must not silently change the existing
Capture action's meaning.

**Smallest remedy:** specify that the additional transient boundary records
belong to their execution segment. For admitted capture across these records,
transfer them with the segment, make them unavailable as active exit targets
while detached, and remap their Frame identities and pending-edge associations
atomically on restore. Include handler-delivery state and any native-held Oop
roots. Preserve one-shot consumption and invalidate records on abandonment or
destruction without executing cleanup from a GC finalizer.

Also state what the supplied task protocol does if cancellation arrives while
its relevant execution is detached: it must retain outstanding cleanup ownership
and cannot publish settlement merely because the active arena is empty. It may
defer delivery until a supported restore; prompt cancellation of every custom
continuation use need not be promised. Demonstrate one capture/restore/cancel
trace with GC and verify that stale external Frame handles remain stale.

This adds process-local bookkeeping to the new mechanism, not a serialized
descriptor field, new C ABI slot, or general continuation framework.

## Disposition of earlier concerns and other examined paths

| Concern | Round-three assessment |
| --- | --- |
| Cancelled Call enters cleanup without running the application's successor | **Design bridge supplied.** 80N sections 3–5 retain H/E, answer only the adapter's park Call and H's delimited Call, and prohibit reinterpretation of Abort. Concrete publisher/action encoding remains an acknowledged implementation prerequisite. |
| Strong ownership during queue transfers | **Resolved at the supplied-policy level.** 80M's accepted tracing owner and 80N lines 134–138 retain unfinished execution. Section 5 additionally requires rooting the exit value before removing source owners. |
| Multi-frame pop, pins, and roots | **Adequate stated obligation; proof outstanding.** Section 5 preflights generations, handles, pin releases, edge placement, and delimiter retirement and requires a non-failing, callback-free commit. Implementation must demonstrate that contract; no additional policy framework is required. |
| Normal return versus cancellation at cleanup entry | **Explicitly covered.** Section 6 ties completion to H's Call commit and retains a delivery phase until E owns its shield. Keep this in the discriminating implementation examples. |
| Nested cleanup failure, waits, and private deadline | **Coherent trace.** E retains aggregation state below a fresh cleanup boundary; W2 is distinct from the cancelled W1. Outer cancellation and the cleanup attempt's own deadline remain separate policy. |
| CPU cancellation without a pending adapter Call | **Explicitly open and bounded.** Lines 243–250 require a distinct runnable stop and owner-side checked exit; they do not pretend ordinary result-bearing resume can implement it. The stop's epoch/drive transition belongs in the new protocol specification. |
| Live edits and native barriers | **Preserved constraints.** Retained Frames keep entered implementations, subsequent sends use fresh lookup, and transfer cannot cross synchronous native entry. No retrospective selector recognition or worker hot reload is required. |

## Review limits

This was targeted document and source inspection, not a runtime test. No reviewed
input, runtime code, ABI, format, or historical review was edited. The remaining
finding concerns mechanically valid execution under the supplied protocols;
arbitrary user replacements remain the user's responsibility.
