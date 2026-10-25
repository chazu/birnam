# Continuation-aware Id: design and proof checkpoint

2026-10-24. Id, the explicit compiler paths, TCC and Execution now use the
continuation contract. Their native/source gate passes, including the original
custom-lookup tail regression. This is not completion of the broader runtime,
control/recovery, default compiler, image, worker or installed qualification.

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
are visible to the ordinary allocation inventory and collectible. The following
compiler/Execution checkpoint connects their accounting and collection safe points.

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
   handoff, native boundary checks and an independent C caller.
3. **Completed for the explicit source paths:** compiler/TCC v3 declarations,
   Execution return continuations, allocation accounting and safe points. The
   preliminary lookup classification, native fallback guessing and captured-alias
   map are removed; the original source gate passes.
4. Restore retained control, handlers, cancellation, cleanup, recovery and
   optimized/named/portable compiler consumers, keeping explicit features intact.
5. Reconnect remaining functional owners, bootstrap, Host/CLI, images and workers;
   qualify clean source builds and installed behavior before release.

## Compiler/Execution continuation checkpoint

The suspension/capture slice reconnects the retained V2 control operations:
publish the existing result-bearing Call edge, suspend or detach owned frames,
then resume/restore with a supplied Oop and commit that edge once. Execution owns
the continuation representation, tracing and retirement. The explicit Machine
driver is the suspension boundary; a native callback cannot capture its waiting
caller. This requires no Id operation, payload catalog or compiler format change.
The source gate checks supplied values without effect replay, live replacement,
budgeted resume, wrong-Machine and repeated restore, GC with only the Continuation
retaining its Machine, abandoned code release, and both direct/native-callback
barriers ([receipt](../measurements/execution-control-continuations-20261025.json)).
Delimited calls now establish the retained frame boundary before sending through
Id. An exit unwinds only to that boundary and commits its saved Call once;
normal return and exit retain the existing delivery acknowledgement. External
exit accepts a committed budget stop, refusing a pending Id send and a second
exit before delivery. Native entry barriers still prevent unwinding a waiting
native caller. No new frame algorithm or Id operation is introduced. The source
gate covers normal return, early exit, budget interleaving with GC, native refusal,
external exit and code release
([receipt](../measurements/execution-boundaries-20261025.json)).

The retained `protectedBlock:`, `leaveBoundary:`, `park` and `controlEligible`
adapters now enter as ordinary registered Methods. Execution owns their frames
and uses the same boundary/suspension algorithms as compiled source; the compiler
recognizes no special selectors. Their obsolete native value plumbing is removed,
and registration and the four-slot driver descriptor are shared. Source checks
cover nested exits, budgeted resume, replacement, GC/code release and native
refusal ([receipt](../measurements/execution-control-adapters-20261025.json)).
The explicit source gate now loads the retained foundations and unchanged
`stackless-control.birnam`, and passes all 177 checks in `task-control-smoke.birnam`
with its native observer callbacks. Additional checks suspend inside `on:do:`
handlers and ordinary `ensure:` cleanup. Task policy remains authored Birnam.
`Reflect defineMethod:on:as:` installs an ordinary continuation Method whose
Sequence context retains the implementation Block; its body sends `value:…`
with the receiver and arguments, preserving the answer destination. Id reads
none of that context. No selector-specific compiler recognition is introduced.
Boolean selection, Globals hashing, printing and System GC now use Oop messages
instead of the removed native value-carrier adapters. Execution publishes Block's
existing vtable ownership and the retained error primitive; reflected arities up
to 16 use the existing Block convention. GC retirement scans the weak return
continuation table once per dead-object set rather than once per dead object.
[Acceptance receipt](../measurements/authored-task-control-20261025.json).
Standard `Codegen compile:`, `emit:` and `eval:` now use the same resumable
pipeline. `emit:` returns C without mapping code; `compile:` returns a Method;
`eval:` sends the current `compile:` and transfers its answer destination to the
compiled Block. Compile errors remain data and native failures retain their mark.
The authored control gate now uses `eval:` throughout. Separate checks cover
compiler replacement, suspended evaluation, changed source without replay, and
code retention/release with GC
([receipt](../measurements/default-compiler-entrypoints-20261025.json)).
The retained source-record format and Codegen's connection to the authored
`SourceRegistry` are now restored. Inspection produces ordinary compilation
Vectors without publishing or executing source. Saved metadata is decoded and
checked against its source digest without parser fallback. Registry publication,
replacement and lookup use current sends; publication hands off the original
answer destination so replacement policy may suspend. No native source table is
introduced. The source gate covers duplicate/name/digest rejection, retained
snapshots after replacement, method-source inventory with the parser disabled,
64 KiB automatic collection, and suspension across registry rebinding
([receipt](../measurements/source-registration-20261025.json)).
Filename-aware evaluation, source/file loading, artifact admission, recovery,
complete bootstrap/Host and installed qualification remain pending. Unconnected
source evaluation and Generation projection remain in `pending_source.rs`.
Bootstrap still imports removed value carriers; these source-path checkpoints do
not claim a repaired cold bootstrap.

Before, Execution classified default lookup in advance and used synchronous send
for custom lookup, losing source tail-call context. Now it issues one Id send
with an owned return continuation. A selected registered source entry recognizes
that continuation and transfers directly into the retained frame engine; native
answers return through it. Lookup helpers receive their own continuations, so
receiver/arity similarity cannot cause an accidental tail replacement. Id never
reads Execution's return-site or frame representations.

Generated modules use v3 metadata. Resumable steps answer their action destination;
synchronous functions retain their synchronous convention. TCC validates either
native descriptor version and selects the corresponding declared entry wrapper.
Entered Unit roots and mapping retirement remain TCC's responsibility. Execution
keeps reporting and frame operations synchronous and uses continuation entries
for source entry, Block value and driver callbacks.

Machine budget stops retain the existing frame algorithm. An Id-paused lookup
also retains its pending work, frame edge and nested delimiters. Only the owner
can resume that checkpoint. Native callbacks during an owned Machine drive stay
on that Machine; recursive drive is rejected. Nested explicit Machine drives use
the existing synchronous native boundary rather than overlapping Id drivers.

A generic `allocationBytesTotal` fact counts requested Id allocation extents,
including headers/padding, without inspecting payloads. Collect keeps the current
construction-charge hooks and applies an Id-allocation pressure floor, including
reported external charges, at `prepareAllocation`. Execution calls that policy
at rooted step boundaries. This covers transfer allocations without adding a
continuation category to the allocator or collector, or double-charging ordinary
managed allocations. Collection resets the measurement origin, not Id's lifetime
counter. The existing refusal of allocating GC reporting remains unchanged.

Continuation answers preserve the existing native failure mark. Execution keeps
ordinary Error values distinct from failed native returns and retains
`evaluatePreservingFailures:` behavior. Missing/invalid lookup results reach the
call continuation instead of silently abandoning a running Machine.

The native gate retains all old assertions and adds compiled lookup-helper
re-entry on the same receiver/arity, suspension after selection, binding
replacement plus GC before resumption, two computations alternating budgets with
exact effect counts, automatic collection without manual collection during the
run, same-Machine native callbacks, nested Machine drives and resumable error
checks. It also retains code-owner release and captured-alias acceptance.

This checkpoint changes no scheduling policy and introduces no private Id/Execution
interface. `pure_lookup`, its retained query witnesses, callee classification at
the send site, and the captured-method alias registry are removed. Authored control
and standard compiler entry points were still pending at this checkpoint; their
later acceptance is recorded above.

[Acceptance receipt](../measurements/compiler-execution-continuations-20261024.json):
188 unit tests pass; all native/source gate modes pass. The original tail assertion
reports 2 frames for both default and custom lookup at 8 and 24 calls. The added
source checks cover 134 interleaved owner turns, automatic collection, suspended
lookup replacement, native callback frame identity, nested Machine drives and
failure conversion. No assertion was weakened or skipped to obtain these results.

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
| Default compiler and native execution consumers | Remaining old-carrier references in Codegen Runtime, Generation, NativeBackend and Portable Code | Reconnect source/file loading, artifact admission, recovery and remaining compiler consumers; standard eval acceptance is not complete integration |
| Functional owners and startup | Alien/Image/Store build failures; Bootstrap still imports removed carriers; Host/CLI/Isolate depend on the migration | Restore the complete cold-build/load closure before installed or worker claims |
| Historical work | Oop worktree is the active development branch; pre-worktree probes are checkpointed separately at `f503c183` | Keep evidence; avoid merging obsolete experiment implementations into the new basis |

Lexical searches also find old carrier names in tests and inactive retained
source. Those matches are not all live dependencies, and are not authorization
to delete the features they used to cover. The full workspace and original
source-tail gates remain required completion criteria.
