# Continuation-aware Id: design and proof checkpoint

2026-10-24. Committed development direction. Production Id now implements the
continuation mechanism below. The old compiler/Execution custom-lookup tail gate
remains red until those callers migrate; full runtime qualification remains held.

The user rejected the shared-private-implementation packaging proposed by the
[entry experiments](oop-entry-seam-experiments.md), and explicitly allowed
considering a small addition to Id itself. The design priority is malleability,
with a public object contract that does not make Execution depend on private Id
state or require both to use one implementation image.

The proposed addition makes the answer destination of a send explicit as an Oop,
and gives the primitive calling convention a bounded-stack transfer mechanism.
Id performs current lookup and direct entry without interpreting continuation
payloads. Execution owns saved computation representations and their tracing;
the scheduler owns ready/waiting policy. Lookup must itself be able to suspend
and resume without replaying effects or losing the pending send's destination.

This is a semantic proposal, not a chosen C signature. In particular, simply
adding a continuation parameter to the existing synchronous send is insufficient.
The metacircular lookup base case and native callback contracts now have
explicit formal models. Native adapters and transfer encoding still need a
concrete implementation and testing. There is no new implicit application lookup
on the selected Method, no fixed payload-type catalog and no approval to relax
those invariants.

The [formal package](../../formal/continuations/README.md) supplies executable
TLA+ specifications, Lean proofs, required negative variants and one reproducible
checker. Its README states the model bounds, fairness assumptions, exact proof
scope and missing implementation correspondence. The checked frame representation
independence and tail-transfer laws support exploring this boundary; they do not
prove all of Id, the compiler, GC, native code or arbitrary scheduler policy.

The [initial receipt](../measurements/oop-continuation-model-20261024.json) records
1,628,246 distinct states across three passing TLC configurations, 19 checked Lean
theorems, nine required TLA+ counterexamples, four reachable-scenario witnesses,
and four rejected Lean implementation mutations. Concrete `decide` witnesses
also reject those mutations, establishing changed behavior rather than merely
proof-script breakage. Kernel audits admit only standard Lean foundational axioms.

The [boundary extension](../../formal/continuations/BOUNDARIES.md) models recursive
lookup, bootstrap publication, source re-entry during lookup, synchronous and
asynchronous native callbacks, ancestor barriers and completion ownership.

The [full boundary receipt](../measurements/oop-continuation-boundaries-20261024.json)
records all 47 TLA+ checks and 41 Lean theorems passing their expected outcomes.
The seven valid configurations cover 1,647,954 distinct states, including 19,708
in the four new boundary configurations. There are 28 required bad TLA+ variants,
12 reachable-scenario witnesses, and 10 rejected Lean mutations. The new models
preserve the full original request across lookup-source re-entry and retain
native ancestors when an asynchronous inner activation returns.

The base case terminates recursion along the seeded chain to the root while
reading a replaceable lookup binding. It is not a termination guarantee for
arbitrary user-defined lookup or cyclic vtable graphs. Likewise, a completion
ticket's native holder must keep it and its required code alive for as long as
foreign code can still access it, including late or duplicate completion.
Rejecting a used ticket is not proof that an expired native pointer is safe.
Address reuse and actual foreign callback lifetime remain implementation obligations.

## Executable reference and rebuild order

### Real Id integration

Before: Id returns a synchronous native answer; explicit resumable callers cannot
keep a custom lookup's selected Method through a tail transfer at the public
boundary. After: pending work and lookup returns are captured Method Oops whose
contexts are owned Sequences. A drive enters each work Method directly, retaining
its context/code owner; a continuation declaration receives its destination as
the first Oop argument and returns the next captured Method (nil ends the drive).
Language answers are delivered as arguments to their captured continuation, never
inferred from the returned object's payload or behavior.

The existing v2 Protocol and synchronous declarations remain valid. A public v3
module descriptor declares the convention per Method, allowing synchronous GC
reporting and continuation bodies in one owner. Initialization is synchronous and
receives `ContinuationProtocol`: the existing fields plus one fixed `control`
entry. The C Method function shape is unchanged. Convention flags belong to
immutable executable declarations, not allocation records or object type tags.

The control entry takes an interned operation Oop and an Oop argument array:

| Operation | Arguments | Result |
| --- | --- | --- |
| `prepareSend` | receiver, selector, destination, arguments… | Captured work Method |
| `prepareEntry` | selected Method, receiver, destination, arguments… | Captured direct-entry work |
| `answerTo:with:` | destination, answer | Work delivering the answer as data |
| `drive:budget:` | work, size-immediate budget | Pending work or nil |
| `pause:` | pending work | Nil; the current continuation entry must then return nil |

Existing marked Oop failure returns report invalid requests, consumed internal
continuations, overlapping drives and native barriers. These are primitive
operations, not an additional family of root-object methods. Constructing a
lookup continuation must not require another lookup merely to return a lookup
result. The duplicate message wrappers considered during implementation were
removed in the scope audit.

Pause cannot cross a waiting native activation. Synchronous Id sends drive a v3
callee until it answers; an escaped answer continuation is closed before native
return and rejects late delivery. Consumed internal continuations release their
saved request/result references. Execution and the scheduler own pending-work
roots and waiting/completion policy. Id reads none of their payloads.

`python3 scripts/id-continuation-test.py` builds real Id, Roots and Collect plus
independent C declarations. It checks mixed conventions, malformed descriptors,
suspended lookup with helper re-entry, selected-code/context retention through
replacement and GC, direct entry despite poisoned Method lookup, native barriers,
late replies, interleaved work and a 10,000-call custom-lookup tail chain. The
tail chain returns to the same live allocation count after precise collection
and has zero observed C Method stack-position span.

The [integration receipt](../measurements/id-continuation-integration-20261024.json)
records the independent C gate, 186 passing existing unit tests, the retained
native compatibility checks and the unchanged failing compiler/Execution tail
assertion. The scope audit removed duplicate root-object transfer methods and
their special pause handling. No scheduler, collector or compiler implementation
changed in this milestone.

Scope is the Id mechanism. There is no scheduler, collector redesign, compiler
change or new payload catalog in this milestone. Native images retain the
existing residency policy. GC still refuses allocating reporting/lookup before
retirement; the C test checks that refusal and recovery. New transfer objects
are visible to the ordinary allocation inventory and collectible, but automatic
collection accounting/safe-point policy for these internal allocations must be
connected with Execution before default language promotion. The gate uses
explicit precise collection, not a claim that this policy is already connected.

The [executable reference](../../formal/continuations/reference/README.md) now
connects the model to a Rust Id library and independently compiled consumers.
The primitive continuation is an ordinary captured Method Oop; its owner handles
its state, and Id enters it directly. An explicit owner adapter can issue fresh
messages when resumed. This makes returning a lookup result possible without
requiring another lookup merely to resume the pending send.

The [reference receipt](../measurements/oop-continuation-reference-20261024.json)
records 12 passing executable checks and exact agreement with nine projected
Method-entry events in each of the synchronous/asynchronous TLA+ completion
witnesses. The expanded formal suite passes 49 checks and 41 Lean theorems.
A 10,000-call tail chain keeps one native Method-entry activation; memory and
performance acceptance are not claimed because this reference retains allocations.

This is a public semantic choice for the reference, not a frozen C signature.
All language arguments and answers remain Oops. The Rust transfer algebra is
control notation, not a new runtime value carrier. Actual roots, collection,
native mapping lifetime and independent native-library compatibility must be
qualified before production promotion.

The reconstruction order is:

1. **Completed:** formal boundary checks and the executable semantic reference.
2. **Completed for the bounded Id mechanism:** transfer convention, root/context
   handoff, native boundary checks and an independent C caller. Compiler and
   Execution have not yet adopted it.
3. Reconnect compiler and Execution through that contract; remove displaced
   preliminary lookup classification and native fallback guessing when their
   replacements pass the original source gate.
4. Restore retained control, handlers, cancellation, cleanup, recovery and
   optimized/named/portable compiler consumers, keeping explicit features intact.
5. Reconnect remaining functional owners, bootstrap, Host/CLI, images and workers;
   qualify clean source builds and installed behavior before completion/push.

## Loose ends verified before rebuilding

`cargo check --workspace --offline --locked` currently exits 101. Its first
reported failures are Alien (39 errors), Image (73), and Store (93), including
imports of removed native carriers and obsolete initializer signatures. This is
a recorded baseline failure, not a result of the reference, and not an exhaustive
list of upper modules that still require migration.

| Area | Existing evidence/source | Required disposition |
| --- | --- | --- |
| Oop native owners and explicit source compilation | `scripts/oop-id-slice-test.sh`; explicit synchronous/resumable compiler entries | Preserve working algorithms and tests; migrate the execution contract |
| Execution | `synchronous.rs`, `resumable_runtime.rs`, retained frame engine; uncompiled `pending_message_integration.rs` and older control consumers | Restore features in gated slices; delete displaced plumbing only with replacement coverage |
| Default compiler and native execution consumers | Remaining old-carrier references in Codegen Runtime, Generation, NativeBackend and Portable Code | Reconnect ordinary compilation, captures, recovery and admission; do not treat explicit test entrypoints as complete integration |
| Functional owners and startup | Alien/Image/Store build failures; Bootstrap still imports removed carriers; Host/CLI/Isolate depend on the migration | Restore the complete cold-build/load closure before installed or worker claims |
| Historical work | Oop worktree is the active development branch; pre-worktree probes are checkpointed separately at `f503c183` | Keep evidence; avoid merging obsolete experiment implementations into the new basis |

Lexical searches also find old carrier names in tests and inactive retained
source. Those matches are not all live dependencies, and are not authorization
to delete the features they used to cover. The full workspace and original
source-tail gates remain required completion criteria.
