# Architecture cleanup before completing the Oop migration

2026-10-28. Follow-up to a read-only inspection of the migration, with source at
`466ee9c6`. This records two confirmed contract defects and two areas requiring
focused review. It does not establish another fundamental Id defect, prescribe a
new Id API, or authorize changes to the language contract.

The authority remains [Current Invariants](../misc/DESIGN.md#current-invariants)
and explicit user decisions. The [migration record](oop-continuation-id.md) holds
implementation and validation receipts. A passing Host run does not resolve the
independent contract failures below.

## User decisions confirmed on 2026-10-28

The user confirmed that Method construction purity is a project invariant and
that the current per-Method metadata-vtable implementation violates it. Preserve
purity and redesign metadata ownership above Id together with admission; do not
accept epoch advances as the fix. This requirement is now explicit in
[Current Invariants](../misc/DESIGN.md#current-invariants). The representation
remains design work; no new Id API or substitute weak-table design is approved.

For explicit binding rollback, the user chose the existing value-checked policy:
preserve a differing current binding, but permit rollback when the expected value
is present again. The old Id atomic restoration/global-epoch ABA guarantee is
retired. This choice does not weaken construction purity or ordinary invalidation.

## When to do this work

Address these items **before declaring the migration complete or doing final
release qualification**. Do not defer them to a general cleanup after migration.

1. Resolve Method metadata construction and admission together as a design
   question, then implement and validate them in small commits. They already
   violate retained contracts; do not extend migration paths that depend on the
   defective mechanisms.
2. Review continuation wrapping and the remaining error-policy scopes next,
   before extending those mechanisms. Start with focused behavioral checks;
   neither concern yet justifies a runtime redesign.
3. Independent harness ports and checks may continue where they do not build on
   an unresolved violation. Finish the full native gate and qualify one corrected
   cold-built revision after these items are resolved.

Automatic optimization work is a separate decision. It should not delay these
correctness repairs or become a reason to add dispatch machinery.

Before any core-path repair, state its contract, show the actual before/after
operations, and inventory any new messages, lookups, allocations, registries and
lifetime rules. Prefer existing owner mechanisms. If no solution preserves all
invariants, stop that affected change and present the specific conflict; this
document does not approve weakening purity, admission or malleability.

## 1. Per-Method metadata behavior changes the global binding epoch

**Status: confirmed, unresolved contract violation.**

[Codegen Method construction](../../rust/birnam-codegen-runtime/src/lib.rs)
(`State::implementation`) currently performs:

```text
create context and Method
  -> create a private delegated vtable
  -> install four metadata Methods
  -> reassociate the Method's behavior
```

The binding epoch advances five times per constructed Method. The existing purity
assertion observes 664 → 674 for two Methods. This couples private construction to
global invalidation and adds per-instance behavior machinery.

Contract: [pure Method binding construction](minimal-mutation-and-coordination.md)
does not change the dictionary epoch. Real binding edits still invalidate lookup
and explicit prepared work according to their existing contracts.

**Remediation:** redesign metadata ownership above Id so construction can create
an unpublished Method with its required metadata without performing global binding
mutations. Determine the representation jointly with item 2. Do not merely replace
private vtables with another weak table unless its necessity, ownership, tracing
and retirement are demonstrated. Do not hide epoch changes, add blanket exemptions,
or expose foreign payloads to make the test pass. The concrete representation is
an open design task, not settled by this document.

**Acceptance:** construction leaves the epoch unchanged; genuine edits retain
invalidation behavior; reflection, captured Method meaning, replacement and code
lifetime remain correct. Retain the negative direct-dispatch tests: poisoning
lookup on the selected Method must not prevent ordinary entry. Review the operation
sequence separately from the test result.

Evidence: [construction-purity failure](../measurements/minimal-mutation-epoch-oop-20261027.json).

## 2. Admission splits related facts and omits required validation

**Status: confirmed, unresolved implementation defects; ownership needs review.**

[Execution admission](../../rust/birnam-execution/src/oop_reflection.rs) (`admit`)
currently queries parameter count, installs a `codeObject` getter on another
private vtable, reassociates behavior and records arity in a separate Execution
table. Capture count is not admitted. Repeated admission can replace the code
association and re-query shape. Direct Block construction omits required capture
validation.

Contract: [Method code admission](method-code-admission.md) establishes immutable
CodeObject identity and checked parameter/capture counts, with descriptor agreement
where applicable. Same-object admission is idempotent; different-object rebinding
is rejected. Failed or reentrant admission cannot leave inconsistent facts.

**Remediation:** give the admitted association and checked facts one coherent
authority owned by Execution, coordinated with item 1. Query current owner methods
only when admission requires it, validate before publishing, and recheck competing
admission after callbacks. Direct construction must validate capture count and
reject raw absent references while accepting supported immediate Oops. Preserve
the explicit external CodeObject pairing route and legitimate native/adaptor
construction routes; do not impose portable-code admission on every callable.

Representation-specific validation, tracing and retirement remain with their
owner. No Id payload catalog, private access to another owner's state or repeated
shape queries on ordinary closure creation are authorized.

**Acceptance:** all six recorded failures pass without relaxed assertions; the
external-pairing control continues to pass. Cover callback failure and reentry,
idempotence without another query, conflicting identities, synchronous/resumable
capture validation, Method aliases, GC lifetime and image restoration. Public
reflection and construction must agree with the admitted association.

Evidence: [admission failures](../measurements/closure-admission-regressions-oop-20261027.json).

## 3. Continuation wrapping depends on Execution identity bookkeeping

**Status: architectural concern to investigate, not a demonstrated defect.**

[Execution's resumable driver](../../rust/birnam-execution/src/resumable_runtime.rs)
uses `return_methods` to associate its answer Methods with pending calls. A known
answer lets the callee join the caller's Machine. Incoming native failure observers
wrap that answer and call `alias_return` to preserve its identity in this table.

This is local Execution bookkeeping, not shared private state with Id. Existing
tests cover branch suspension with and without an observer. Nevertheless, special
recognition of a wrapper can make execution placement depend on more than the
public continuation operation. This deserves an explicit equivalence check.

**Remediation:** compare direct answers with semantically equivalent forwarding
continuations, including nested wrappers and an independently declared wrapper
that does not call `alias_return`. Compare observable results, failure identity,
effect order, suspension/resumption, cancellation, cleanup and supported frame
inspection. Different internal Machine placement alone is not proof of a bug.

If recognition only improves execution efficiency, demonstrate that the general
path preserves semantics. If observable correctness depends on special alias
registration, repair the general Execution path using the existing continuation
contract rather than adding registrations for each new wrapper. Any proposed new
public contract requires a separate decision. Preserve weak-table cleanup before
address reuse and do not teach Id about Execution's frame or continuation layout.

**Acceptance:** equivalent forwarding does not lose the supported control behavior
or replay effects; GC and stale/consumed answer handling remain correct. Record
either the counterexample and repair, or evidence explaining why the table is a
valid owner-local implementation detail. Do not report the concern as a confirmed
defect without that evidence.

## 4. Ambient native-failure policy needs a bounded-scope audit

**Status: one demonstrated regression repaired; remaining explicit uses need review.**

Ordinary typed native invocation previously set Execution's shared `preserve`
flag around body execution. A callback's local language error handling then behaved
differently from the equivalent direct call. Commit `13e430e8` removed that implicit
flag change. Explicit preservation and native-request evaluation still use the
flag in [synchronous Execution](../../rust/birnam-execution/src/synchronous.rs) and
[native adapters](../../rust/birnam-execution/src/oop_native.rs).

**Remediation:** inventory the remaining writers and readers, their intended
dynamic extent, and every outgoing callback within it. Verify restoration on
success and failure, nested/reentrant calls and any allowed suspension. A parked
computation must not leave policy active for unrelated work. Prefer existing
explicit result handling if an ambient dependency is unnecessary; do not invent
another global mode or duplicate language error convention. Retain supported
explicit preservation behavior unless the user changes its contract.

**Acceptance:** keep the publication regression test and exercise nested explicit
scopes, callback-local handling, exact native failure identity, Error objects
returned as ordinary data, and subsequent calls after failure. Distinguish native
barriers that prohibit suspension from cases that actually suspend and resume.
If existing checks establish these properties, close the review with evidence
without adding another mechanism.

Evidence: [callback-policy correction](../measurements/publication-callback-policy-oop-20261027.json).

## Completion record

For each item, add the decision, implementation commit if needed, behavioral and
negative-test evidence, and remaining limits here. This document creates follow-up
work; it does not mark any unresolved item repaired. Final migration acceptance
must identify one source revision containing all required corrections and qualify
its native tests, cold installation, relocated archive and Host behavior.
