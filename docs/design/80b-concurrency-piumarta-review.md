# 80B — Adversarial concurrency review: Piumarta-inspired lens

> **Independent review, 2026-09-16.** This is an interpretation of published
> ideas, not impersonation, participation, or endorsement by Ian Piumarta.
> Findings are reviewer judgments, not accepted project decisions. Reviewed
> Design 80 at SHA-256
> `0d7f0a217e3adf6c42dc1952327a126b901441e9cb14f942c7fd06ed9e242055`
> and Design 80A at SHA-256
> `06e79906abfc92e4cd3345e36993db63cd63a785fc08e2cb4108640fbe0f079e`.

## Lens and verdict

Piumarta and Warth's *Open, Extensible Object Models* argues that implementation
decisions become unnecessarily restrictive when programmers cannot reach them
through their own object language. Its small bootstrap exposes message lookup
through objects and messages rather than prescribing one permanent object
organization. It nevertheless identifies a small mechanism that terminates
metacircular lookup recursion. Those are the sourced principles used here;
the paper does not establish a preferred concurrency model for Birnam.
See [the authored paper, sections 1–3.2](https://gwern.net/doc/cs/lisp/2008-piumarta.pdf).

**Verdict: retain the direction, but do not treat the invariant list as a proof
that the proposed lifecycle can be implemented without reducing malleability.**
Independent domains, explicit messages, and library-authored structured tasks
fit this lens. The difficult remaining question is precisely which operations
constitute the native floor and which remain ordinary replaceable behavior.

I found no demonstrated invariant violation in these proposal documents.
Findings below identify underspecified boundaries with concrete failure cases.
“High” means resolve before implementing the relevant public behavior, not that
the existing runtime is newly broken.

## 1. High — Settlement authority and mutable behavior are not separated

**Affected:** Design 80 sections 5, 7, and 10; 80A sections 1 and 3.

One-shot terminal publication, scope ownership, mandatory cleanup, and persistent
cancellation are strong contracts. The proposal also requires ordinary lookup
and live method replacement and says lifecycle policy should live in Birnam.
It does not identify which facts remain trustworthy when lifecycle methods are
replaced.

**Counterexample:** a scope automatically joins children by sending each task
`outcome`. A user replaces `outcome` with a method that returns a diagnostic
snapshot immediately. If the scope treats that return as settlement, it can
release resources while the task remains runnable. If Rust silently bypasses
the method and implements scope joining itself, behavior that appears to be an
ordinary protocol has a hidden second implementation. The same issue arises if
an overridden `cancel` acknowledges a request without recording it.

Neither unrestricted forgery of lifecycle facts nor hidden canonical dispatch
is required by malleability. Existing Machines already expose ordinary objects
over checked private execution state; [60A sections 1–3](60a-stackless-execution-semantics.md)
provide a useful precedent.

**Bounded remedy:** add a small authority table. Separate checked execution
facts—owner, runnable eligibility, cancellation request, root retention, terminal
publication—from replaceable policy such as child selection, supervision,
failure aggregation, and result presentation. State whether a scope waits on a
checked completion capability or invokes a documented task protocol, and what
an invalid protocol response means. Exercise one custom lookup and one replaced
task method before choosing an internal fast path.

This is an unclosed mechanism boundary, not evidence that scopes must be native.

## 2. High — A scheduler written in Birnam needs its own execution contract

**Affected:** Design 80 sections 5, 6, and 10.

The document correctly rules out suspending with live host borrows and warns
that regaining driver control is insufficient. However, “policy in Birnam”
introduces ordinary sends while deciding which task runs next.

**Counterexample:** the scheduler sends a queue-selection message whose custom
lookup logs through an object that waits for another task. Alternatively, the
selection method exhausts its timeslice while moving a task between queues.
Does the scheduler recursively schedule itself, suspend halfway through queue
ownership transfer, or run every selection method without a budget? Each answer
has different safety and fairness consequences. Treating scheduler methods as
secretly canonical would contradict the stated open implementation goal.

There is also a bootstrap version: installing scheduler objects must not require
an already operational scheduler to complete their initialization.

**Bounded remedy:** specify an explicit scheduler invocation context and the
checked handoff that makes a selected task runnable. Define whether policy calls
can wait, how a forbidden wait is reported, and how failed or nonreturning policy
calls leave ownership inspectable. Keep queue membership changes protected only
for the necessary state transition; avoid assuming arbitrary message sends are
atomic. State how cold bootstrap reaches the first scheduling turn.

[60D section 6](60d-stackless-concurrency-fit.md#6-reentrant-machinestate-access)
already requires releasing host borrows before callbacks and defines execution
entry scopes. Extend that discipline instead of constructing another implicit
current-Machine mechanism. A tiny recursion boundary is compatible with this
lens; an undocumented privileged scheduler object is not.

## 3. High — Native-call compatibility needs a public failure decision

**Affected:** Design 80 sections 5, 7, and 12; 80A sections 1 and 5.

The proposal explicitly recognizes native barriers. The missing decision is what
the programmer sees when an ordinary live edit introduces a wait underneath one.

**Counterexample:** a native sorting routine calls a Birnam comparator. The
comparator is replaced with a method that consults another task using `await`.
If the result is already available, no suspension may be needed; if it is pending,
the native caller still needs a synchronous answer. A simple live edit can thus
expose a previously dormant control boundary.

[Design 74 section 5.4](74-malleable-multicore-execution-and-optimization.md#54-practical-consequences-of-the-native-boundary)
documents the current consequence: a `ContinuationBarrier` escaping through
ordinary synchronous `IdApi.send` reaches `fatal_invariant` and aborts the
process. This is an existing constraint, not a newly discovered concurrency
regression. Nevertheless, it is materially stronger than a task failing or
entering the debugger. Thread-hosted domains share that process fate.

**Bounded remedy:** make this a named API design gate. Define an honest rejected
wait/control-transfer outcome compatible with the calling ABI, or explicitly
classify that callback context as requiring an adapter with the existing fatal
behavior still exposed. Show both ready and pending results in one callback
example. Do not return an ordinary Error as though suspension succeeded, unwind
foreign frames, or introduce a supposedly recoverable debugger stop after an
unrecoverable abort.

This is a known open question whose consequence deserves elevation, rather than
a contradiction in the proposal's qualified suspension promises.

## 4. Medium — The sketches do not exercise failure as an explicit operation

**Affected:** Design 80 section 7; 80A sections 1 and 4.

Preserving ordinary Error values is correct. It also means structured failure
does not arise automatically from libraries already returning those values.

**Counterexample:** `/profile` returns an Error value and `/activity` starts an
expensive request. Under the documented rules, the profile task succeeds with
that Error, the sibling keeps running, and the map is a successful scope result.
Changing the task driver to recognize Error objects would silently alter the
language. Requiring every caller to remember an undocumented conversion would
make the advertised fail-fast scope misleading.

**Bounded remedy:** add one failure-path example showing the explicit library
decision that converts a returned Error into declared task failure, alongside
one example that deliberately returns it as data. Locate that decision in a
replaceable adapter or application policy. Define what happens if that operation
is invoked outside a task or underneath a native barrier. No new exception
taxonomy is needed to demonstrate the distinction.

## Questions to settle next

1. Which checked lifecycle facts cannot be forged, and which policies can a
   programmer replace using ordinary messages?
2. In what execution context does scheduler policy run, and how is its own
   suspension, failure, and cold initialization handled?
3. What does an ineligible wait do before any irreversible unwind or effect?
4. What is the smallest working example of explicitly declaring task failure
   while leaving ordinary Error values untouched?

These answers would do more to establish vision alignment than adding another
concurrency abstraction. Copied messages and pinned domains are reasonable
initial constraints; they should remain explicit boundaries rather than reasons
to hide local behavior from the object language.
