# 80E — Adversarial concurrency review through a Brad Cox-inspired lens

> Review of the 2026-09-16 proposal, not implementation approval. This applies
> ideas from Cox's published work; it does not impersonate him or claim his
> endorsement. Findings below are the reviewer's engineering judgments.
>
> Frozen inputs: Design 80 SHA-256
> `0d7f0a217e3adf6c42dc1952327a126b901441e9cb14f942c7fd06ed9e242055`;
> Design 80A SHA-256
> `06e79906abfc92e4cd3345e36993db63cd63a785fc08e2cb4108640fbe0f079e`.

## Lens and verdict

In [Planning the Software Industrial Revolution (1990), author-uploaded text](https://www.researchgate.net/publication/3246772_Planning_the_software_industrial_revolution),
Cox argues for reusable, interchangeable components; describes compiled components
meeting previously unknown objects through dynamic binding; and values several
binding technologies for different integration levels. He also distinguishes
tools for constructing components from tools for checking their conformance.

My application of those principles: a concurrency component should be usable
through a stable contract without requiring its consumer to understand its
scheduler, heap, native implementation, or deployment layout. Isolation helps,
but it does not itself provide that contract.

**Verdict: retain the architecture, but do not freeze the component-facing API
yet.** The proposal carefully preserves native barriers and malleability. I found
no demonstrated violation of existing invariants in the proposed architecture.
The strongest risks are unresolved boundary contracts that could produce such
violations if filled in casually. The small programs exercise runtime mechanisms
well; they do not yet demonstrate independently replaceable components.

## 1. High — Returning an Error changes meaning across the legacy boundary

**Affected:** Design 80 sections 7, 8, 10; 80A sections 1–2.
**Classification:** concrete compatibility seam requiring a design decision.

The new task contract says an ordinary returned Error remains a successful value.
The [shipped isolate contract](../guide/06-isolates.md) instead converts a handler's
Error return into an application failure. The actual worker loop sends `isError`
to its result and chooses an application-error reply in
[birnam-isolate](../../rust/birnam-isolate/src/lib.rs), around lines 2100–2135.
The admitted copied-value set also excludes arbitrary Error objects.

**Counterexample:** a reusable parser returns an Error object for malformed user
input. Locally, `fork:` followed by `await` successfully returns that object.
Placed behind a worker endpoint by a naive asynchronous wrapper around today's
protocol, the same parser produces task failure and cancels its caller's siblings.
Making the new worker return the Error object directly instead produces a
serialization problem. Neither behavior follows from the parser's message
contract alone.

**Bounded remedy:** define separate envelopes for application data, explicit task
failure, and transport failure. Preserve legacy behavior in an explicitly named
compatibility adapter. Require a parser's remote adapter to translate its local
Error into an admitted application-data representation, or reject the result as
nontransferable without inferring application failure from `isError`. Do not
silently widen the copied-value model. One local/remote parser example would make
this distinction concrete.

## 2. High — A settled request can still leave an ownerless remote operation

**Affected:** Design 80 section 7 cancellation, settlement and scope-exit rules;
section 8; 80A section 2.
**Classification:** acknowledged open decision, essential before asynchronous
remote scopes have a public lifecycle guarantee.

The document correctly distinguishes local request settlement from remote
completion after disconnect. It also promises that scopes account for children.
Those statements need a concrete connecting protocol.

**Counterexample:** a worker accepts a file-conversion request, writes half its
output, and loses its reply channel. The local request publishes a failure with
unknown remote outcome. The scope has no remaining unsettled local request, but
the conversion and its cleanup may still be running. Scope return, scope
retention, process reaping, and transfer to a service owner are different choices.
Retaining a request ID alone does not establish who remains responsible.

**Bounded remedy:** specify one disconnect sequence for an owned process domain.
Name the owner until death/reaping or acknowledged settlement; expose unresolved
cleanup explicitly. Specify a separate handoff for borrowed service domains
before offering that API. Keep evidence about external effects distinct from
evidence that remote execution ended: even a successfully reaped process does not
prove rollback. Local request terminal state must never silently discharge
remote ownership.

## 3. High for thread hosting — Binary compatibility needs an admission contract

**Affected:** Design 80 sections 1, 5 and 10.
**Classification:** acknowledged implementation constraint; missing component
qualification rule, not proof that an ABI revision is necessary.

The proposal promises ordinary native participation while moving toward multiple
thread-hosted runtimes. Its source audit is justified:
[public_api.rs](../../rust/birnam-runtime/src/public_api.rs) uses `PUBLIC_RUNTIME`, and
`initialize_once` rejects a second process-lifetime initialization. Replacing that
pointer with thread-local routing would address only one part of the problem.

**Counterexample:** a precompiled extension caches an Oop or an IdApi pointer in a
C static variable at initialization. Two domains load the same library. The
second initialization overwrites state later used by the first domain, or a
callback enters after its original domain shuts down. Copied language messages
and unchanged function signatures do not prevent either failure.

**Bounded remedy:** define explicit native-module admission categories: qualified
for multiple domains, confined to one process-domain instance, or rejected for
thread hosting. Specify initialization, retained references, callback ownership,
and unloading obligations for the qualified category. Keep old unqualified
binaries usable through a documented process-hosted path rather than asserting
that unchanged ABI layout proves concurrency compatibility. The host must report
the placement decision; it should not silently claim equivalent containment.

## 4. Medium — Selector names are not component compatibility contracts

**Affected:** Design 80 section 9; 80A sections 2 and 5.
**Classification:** explicitly acknowledged open policy; current sketches hide
its practical consequence.

**Counterexample:** `ImageWorker render:` originally answers an encoded image;
an edit changes it to answer a filesystem path. Both results are valid copied
strings, and the selector is unchanged. An already queued request runs against
the new method and succeeds at the transport level while violating its caller's
expectation. A startup-time capability check cannot prevent a later edit from
changing that expectation.

**Bounded remedy:** define compatibility at an endpoint's application contract,
including admitted input and output meanings. Demonstrate one incompatible edit:
either maintain an adapter for the old contract, reject incompatible requests
before effects, or fence admission while changing the endpoint contract. Continue
ordinary live lookup within that contract; do not pin every queued request to an
old method implementation. This preserves malleability without confusing dynamic
binding with arbitrary interface compatibility.

## 5. Medium — Outcome observation is not a recovery boundary for components

**Affected:** Design 80 section 7 failure policy; 80A section 4.
**Classification:** documented behavior with a composition consequence, not a
contradiction.

**Counterexample:** an image component runs two alternative decoders, intending
to inspect both outcomes and use whichever succeeds. A terminal failure in one
child cancels the ordinary scope body and its other decoder. Calling `outcome`
does not make this recovery strategy work, even if the caller immediately handles
the failure. The proposal states this honestly, but supplies no recovery example.

**Bounded remedy:** add one fallback-component sketch that catches expected
application problems inside each child before terminal failure, and explicitly
state when a supervised scope is required. Keep raw infrastructure failures
distinct. This would demonstrate whether callers can compose a component using
its documented result contract without inheriting its internal failure topology.

## Questions to settle next

1. What application-data representation carries an expected remote error while
   preserving legacy isolate behavior and the existing copied-value boundary?
2. Who owns a remote operation after local settlement with unknown outcome?
3. What must a native binary establish before it may inhabit two thread domains?
4. Can the same component contract survive one implementation replacement and
   one expected failure without changes to its consumer?

Answer these with a parser or renderer, a single disconnect timeline, and a small
native extension. No new scheduler mechanism or large validation matrix is
needed to decide the contracts.
