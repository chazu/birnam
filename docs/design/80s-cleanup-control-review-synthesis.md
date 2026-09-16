# 80S — Cleanup-control compatibility review synthesis

> **Third adversarial review, 2026-09-16.** The review examines the additive
> cancellation-to-cleanup mechanism and its accepted compatibility constraints.
> Historical lenses are interpretations, not participation or endorsement by the
> named authors. Findings are proposals, not implementation authorization.

## Fixed input

The updated constraints and suggested extension approach were committed before
review in **`9973f4d785acfa7ec28bbb00b225b7bb1ec8e92e`**.

Primary input: [80N](80n-cancellation-to-cleanup-transition.md). Accepted policy
and earlier dispositions: [80](80-concurrency-domains-and-structured-tasks.md)
and [80M](80m-concurrency-second-review-synthesis.md). Reviewers inspected relevant
source and 60A/60B/60P contracts at that same commit. Their line references always
refer to this fixed input, which remains recoverable through Git.

## Compatibility assessment

**All four reviewers support retaining the additive approach.** The examined
source supports an additive Machine/object-protocol extension.
No reviewed counterexample establishes that the public IdApi C table,
Method/StepMethod calling convention, descriptor V2, or current image format must
change. This is a design assessment, not demonstrated binary compatibility or
image round-trip evidence. Existing action meanings, ordinary dispatch, entered
Frame lifetime, and independent bootstrap remain constraints.

Do not equate a new native primitive with a new C ABI slot, or new image admission
logic with a new serialized record format. Conversely, preserving byte layouts
alone does not prove correct execution or admission.

| Review | Remaining focus |
| --- | --- |
| [80O — Piumarta](80o-cleanup-control-piumarta-review.md) | Concrete atomic establishment/invocation and capture/restore of boundary metadata. |
| [80P — Kay](80p-cleanup-control-kay-review.md) | Preserve failure evidence while leaving cleanup and restoring cancellation state. |
| [80Q — Ingalls](80q-cleanup-control-ingalls-review.md) | Boundary/delivery metadata must follow detached and restored execution. |
| [80R — Cox](80r-cleanup-control-cox-review.md) | Explicit admission classification for new live capabilities, without changing image format. |

## Material findings

### 1. Make boundary establishment an actual atomic invocation

**Source: [Piumarta lens](80o-cleanup-control-piumarta-review.md). Before mechanism implementation.**

An ordinary register-then-call sequence can register the wrong pending Call;
a synchronous native wrapper invoking the body would leave a native barrier in
the path of its first wait. The proposal requires atomic establishment, but must
show which resumable publisher returns the action and which exact Call belongs
to retained handler H.

Smallest follow-up: a generic establishment/invocation trace, normal-return path,
and handler-delivery acknowledgement. A distinct combined action is a candidate,
not an approved schema. Preserve old Call/Abort behavior and avoid compiler
recognition of cleanup selector names.

### 2. Cleanup boundaries must follow captured execution

**Sources: [Piumarta](80o-cleanup-control-piumarta-review.md) and [Ingalls](80q-cleanup-control-ingalls-review.md). Before continuation compatibility is claimed.**

Current capture retires Frame handles; restore installs their contents with new
live generations. Keeping a boundary record tied to H's old handle would make
later cancellation stale or lose its handler. Code pins alone cannot fix this.

Smallest follow-up: transfer transient boundary/delivery metadata with the
captured segment, remap internal references on one-shot restore, and specify
retained-handler/detached-body and cancellation-while-detached cases. Preserve
stale external handles and native barriers. Retire metadata on abandonment
without running user cleanup from GC finalizers. Do not silently forbid existing
capture or change which delimiter it selects to avoid this work.

### 3. Explicitly classify new live capabilities as non-imageable

**Source: [Cox lens](80r-cleanup-control-cox-review.md). Implementation acceptance obligation.**

The current image saver enumerates eight action families; a ninth sibling does
not automatically inherit their rejection. The same issue applies to a new
opaque live boundary-token family.

Smallest follow-up: extend existing classification for the new live families,
preserving canonical prototype/reference handling and ordinary inert outcome
serialization. Verify the intended rejection before durable publication. This
is image admission work, not evidence for a new image format or version.

### 4. Protect outgoing failure evidence through propagation

**Source: [Kay lens](80p-cleanup-control-kay-review.md). Before shield restoration is implemented.**

The proposal protects entry into cleanup. It must also define protection while
E restores enclosing cancellation state and propagates accumulated failure. If
late cancellation abandons E before the outer handler receives that evidence,
the final outcome could incorrectly be Cancelled instead of Failed.

Smallest follow-up: keep propagation protected until the next handler owns the
aggregate, or first record it in task-owned state consulted by later exits.
Choose one supplied-protocol rule and show the intervening scheduling stop.
Rooting the evidence alone does not establish its participation in the outcome.
This is Birnam policy ordering, not a demand for native knowledge of Error values
or protection against intentionally broken replacement methods.

## Scope of this review

The prior choices of parallel owned domains, user responsibility for replacement
policy, request-isolating serial workers, separate lifecycle ingress, and strong
scope ownership remain settled. This review neither adds native-library
certification nor reopens worker hot reload or the general concurrency model.

No runtime implementation, ABI fixture change, image round-trip, or performance
qualification is part of this documentation review.

The four reviews and this synthesis were recorded after review, with README and
the second-review index pointing here. The reviewed 80N mechanism remains
unchanged at the fixed baseline. Document validation checks links, whitespace,
code fences, review provenance, and preservation of the semantic input. Runtime
compatibility tests remain future implementation evidence.
