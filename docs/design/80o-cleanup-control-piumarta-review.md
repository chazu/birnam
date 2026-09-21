# 80O — Cleanup-control review: Piumarta-inspired lens

> **Third adversarial review, 2026-09-16.** Fixed input commit:
> `9973f4d785acfa7ec28bbb00b225b7bb1ec8e92e`. Primary input: 80N; accepted
> policy and dispositions: 80 and 80M. Source and contract anchors below refer
> to that commit. This is an interpretive lens, not Ian Piumarta's participation
> or endorsement. It reuses the open-implementation/metacircularity lens sourced
> in [80B](80b-concurrency-piumarta-review.md); no new historical claim is made.

## Verdict

**Keep the adapter plus generic delimited-exit direction. No reviewed evidence
requires an IdApi, Method/StepMethod ABI, descriptor V2, or image-format break.**
The ordinary result edge can carry the internal wake and handler result without
changing the meaning of application return values. Preserving old action objects
while adding a separately specified action is consistent with 60B's extension
rule. Direct driver decoding remains the small nonrecursive implementation floor;
library cleanup and cancellation choices remain ordinary replaceable behavior.

The earlier control concern is **narrowed, not yet closed**: 80N demonstrates
where a cancelled wait's answer goes and what must remain rooted. It still needs
one concrete boundary-establishment entry, plus composition with existing
continuation capture. These are mechanism questions, not reasons to certify user
policy or make supplied cleanup behavior immutable.

## 1. High — Atomic boundary establishment needs an actual publisher/driver trace

**Scope:** before accepting the new control protocol for implementation.

**Baseline anchors:** [80N](80n-cancellation-to-cleanup-transition.md), lines
57–69 and 114–125, proposes ordinary Machine messages but requires registration
and invocation to be one transition. Lines 205–212 also require normal return
to establish handler-delivery state atomically. [60B](60b-stackless-machine-and-frames.md),
lines 56–64, stages one exact pending edge; lines 384–399 distinguish ordinary
publication from nonrecursive driver decoding.
[runtime.rs](../../rust/birnam-runtime/src/runtime.rs), lines 3824–3843, invokes a
synchronous target before committing its caller's pending result edge.

**Counterexample:** implement the suggested surface naively as an ordinary
`registerBoundary` send followed by a separate body invocation. During the first
send, H's pending edge belongs to registration, not the future body call. After
registration returns, that edge has been consumed; treating it as the body's
exit target is invalid. Conversely, a native `invokeWithinBoundary:` method that
registers H and synchronously invokes the body keeps a native activation live:
the body's first cancellable wait then encounters the forbidden native barrier.
Both follow the high-level instruction to expose ordinary messages, but neither
implements the documented H/B arrangement.

**Smallest remedy:** add one concrete, generic establishment trace alongside the
exit trace. Identify the resumable entry/StepMethod that returns the proposed
action, which descriptor-admitted Call is H's retained invocation, and how the
driver establishes the boundary and enters the wrapped body after native return.
Show normal return through the same pending edge and the point where delivery
state becomes visible to Birnam policy. Include the matching generic
acknowledgement/retirement of that delivery state when E assumes its shield, so
a later cleanup deadline is not permanently suppressed by stale delivery state.

A distinct combined establishment/invocation action is a plausible candidate;
this review does not select its schema. Neither compiler recognition of an
`ensure:` selector nor a reinterpretation of `CallAction` is needed or justified.
The missing trace is the evidence that the proposed ordinary object interface
really terminates at a usable primitive, rather than recursing across a native
activation. Replacements that deliberately fail to acknowledge delivery may
stall or alter cancellation; that remains the user's responsibility.

## 2. Medium — New boundary records need a capture/restore ownership rule

**Scope:** before claiming compatibility with existing continuation operations.

**Baseline anchors:** [80N](80n-cancellation-to-cleanup-transition.md), lines
71–77 and 121–125, keeps records owner-local and ties them to H's checked Frame
handle. Lines 168–184 require boundary invalidation and account for independently
retained continuations. [60B](60b-stackless-machine-and-frames.md), section 8,
defines detached segment and delimiter ownership.
[execution.rs](../../rust/birnam-runtime/src/execution.rs), lines 1138–1175,
detaches Frames and advances their generations; lines 1195–1213 reinstall their
execution state without restoring those old generations. Lines 3697–3705
describe the existing captured-segment record.

**Counterexample:** an eligible existing capture detaches a segment containing
E, H, and B to a host delimiter. If the proposed library boundary remains in an
active Machine side table, its H handle now denotes a retired generation. If
the boundary is simply invalidated as a removed Frame, restoring the continuation
brings back the cleanup wrapper but not its cancellation boundary. A subsequent
cancellation either rejects the stale target or skips the intended handler.
The retained Frame roots and pins alone do not repair the control identity.

**Smallest remedy:** specify that capture transfers the new boundary metadata
associated with a detached segment, including any delivery phase, into transient
continuation ownership. On one-shot restore, remap the internal Frame references
to the installed live generations without making old public Frame handles valid
again. Cover a retained H with a detached body as a separate case. State how
continuation destruction retires these records under existing lifetime rules.
This can use private process-local metadata; it does not require extending the
serialized descriptor or image grammar. Do not silently turn every new cleanup
boundary into a prohibition on existing capture as a shortcut.

## Disposition and limits

| Earlier concern | Third-review disposition |
| --- | --- |
| Cancelled pending Call has no path to cleanup | **Narrowed.** The adapter consumes cancellation before returning to application code; the retained H edge supplies a credible destination. Finding 1 concerns construction of that destination. |
| Roots, entered code, and live lookup | **Addressed in the worked exit trace; capture composition remains finding 2.** Preflight, rooted exit evidence, retained E/H, and fresh cleanup sends are explicit requirements. |
| Policy versus execution authority | **Resolved at the design level.** No VM scope engine, immutable cleanup policy, or protection against intentionally broken replacements is requested. |
| Native callback legality | **Retained.** No new escape across a synchronous native activation is promised. |
| ABI/image compatibility | **A credible constraint, not yet implementation evidence.** No unavoidable break was found; action schema, publication, binding registration, and non-imageability still need the stated proof. |

This review inspected documents and targeted source at the fixed revision. It
did not implement or test the new mechanism. The two findings do not reopen the
accepted concurrency policies or authorize runtime changes.
