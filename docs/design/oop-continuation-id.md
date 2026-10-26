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
`evalPersisting:` and `evalPersisting:from:` now retain loader diagnostics and
hand the compiled Block the caller's answer destination. The authored SourceLoader
runs through real file snapshots, source registration, and current owner methods.
The backend's existing filename/persistence/synchronous request is restored;
filenames reach nested descriptors. The retained embedding traversal uses Oop
byte access, preserves lexical shadowing/path limits, and copies sibling assets
before execution. Source-file tests cover errors versus nil, duplicate-source
policy, live owner replacement with GC, backend publication modes, embedded bytes
after file deletion, and a loaded body suspending/resuming after its file is gone
([receipt](../measurements/source-file-loading-20261025.json)).
Ordinary resumable module artifacts now compile to ELF object bytes and load
through the existing descriptor and compiled-source formats. The Oop initializer
returns the retained four-field record with an empty arity-adapter list; reflected
Methods use the current Codegen constructor. Admission checks the frozen snapshot,
ABI/kind/capability, source and metadata digests, identity, requirements, and prior
source before running the body. Registration follows successful non-nil activation.
The retained synchronous activation boundary remains; task-owned Machines created
by the body continue independently. Parser-disabled loading, cold Error-owner
absence, malformed artifacts, unavailable requirements, snapshot replacement and
suspended-task code lifetime are covered
([receipt](../measurements/artifact-admission-20261025.json)).
Synchronous artifact bodies now use the retained per-body selection request: the
initializer remains resumable while selected bodies use synchronous declarations.
Root-directory compilation and adjacent/embedded library provisioning are restored,
including name/size limits and the hex carrier that avoids nested descriptor magic.
Provisioning uses the current Alien owner; the platform Alien implementation is a
separate migration dependency. Both body modes use ordinary reflected Methods;
prior-ABI compiled arity-adapter records are incompatible, not a parallel runtime.
The displaced pending artifact implementation is removed
([receipt](../measurements/artifact-variants-20261025.json)).
Recovery, complete bootstrap/Host and installed qualification remain pending.
At that checkpoint the unconnected Generation projection remained in `pending_source.rs` (replaced by the Oop projection below). Bootstrap still
imports removed value carriers; these checkpoints do not claim a repaired cold
bootstrap.

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

At the pre-rebuild checkpoint, `cargo check --workspace --offline --locked` exited 101. Its first
reported failures are Alien (39 errors), Image (73), and Store (93), including
imports of removed native carriers and obsolete initializer signatures. This is
a recorded baseline failure, not a result of the reference, and not an exhaustive
list of upper modules that still require migration.

| Area | Existing evidence/source | Required disposition |
| --- | --- | --- |
| Oop native owners and explicit source compilation | `scripts/oop-id-slice-test.sh`; explicit synchronous/resumable compiler entries | Preserve working algorithms and tests; migrate the execution contract |
| Execution | `synchronous.rs`, `resumable_runtime.rs`, retained frame engine; uncompiled `pending_message_integration.rs` and older control consumers | Restore features in gated slices; delete displaced plumbing only with replacement coverage |
| Default compiler and native execution consumers | Remaining old-carrier references in Codegen Runtime, Generation, NativeBackend and Portable Code | Reconnect remaining artifact variants, recovery and compiler consumers; standard eval acceptance is not complete integration |
| Functional owners and startup | Alien/Image/Store build failures; Bootstrap still imports removed carriers; Host/CLI/Isolate depend on the migration | Restore the complete cold-build/load closure before installed or worker claims |
| Historical work | Oop worktree is the active development branch; pre-worktree probes are checkpointed separately at `f503c183` | Keep evidence; avoid merging obsolete experiment implementations into the new basis |

Lexical searches also find old carrier names in tests and inactive retained
source. Those matches are not all live dependencies, and are not authorization
to delete the features they used to cover. The full workspace and original
source-tail gates remain required completion criteria.

## Oop Bootstrap checkpoint

Bootstrap now uses Oop initializer arguments, Id loading and Roots-owned result
lifetimes. NativeLoader consumes ordinary status/value records, replacing the
removed value-kind packet. The build-only compiler seed boots from a fresh Id
process, constructs the foundational artifacts and hands parsing to compiled
Birnam Lang with the seed parser disabled. The default Bootstrap builds and
loads those artifacts without the grammar seed, but still fails at its required
PortableCode provider. This is not full published startup qualification.
[Receipt](../measurements/bootstrap-oop-20261025.json).

PortableCode's retained object and canonical-code operations now use Oop messages,
with owner-defined tracing and protected result cleanup. Thirteen canonical
program fixtures, construction and signed values pass in the cold compiler
process. Published Bootstrap advances past PortableCode and still requires the
remaining source/platform closure. [Receipt](../measurements/portable-code-oop-20261025.json).

## Full published Bootstrap startup

The published Bootstrap closure now starts in a fresh Id process. The cold
qualification builds its native owners, uses the build-only Lang seed to compile
all 22 required authored artifacts, then replaces Bootstrap with its default
build and removes the seed dylib. The published process runs from the staged
library directory with installed-module fallbacks disabled. Removing a required
artifact still fails startup; it does not fall back to source or the seed.

The existing message contract is unchanged. Before this migration, Alien, Image
and Isolate sent the removed Operand/Outcome structs and installed native entry
records. They now declare Methods in their native module metadata and send Oops.
Ordinary dispatch remains current lookup followed by direct Method entry. These
owners keep their native resource tables and tracing responsibilities; scoped
references are enumerated by their existing Roots providers. No Id operation or
payload-type catalog was added.

Execution's explicit native-facing adapters normalize Word and Bytes through
those owners. AlienCompiler emits the same Oop calling convention, and generated
wrappers retain their TCC Unit and use Roots to protect results during cleanup.
AlienRecipes inspects a vtable's own bindings through `bindingsDo:` when checking
registration identities. That inspection neither changes ordinary lookup nor
adds a Method application protocol.

ObjectModel adds `imageSlotKindAt:in:` to report its inherited slot layout. Image
must distinguish a raw word from an object-reference slot holding an immediate
Oop; inspecting the returned value's encoding cannot make that decision. The
layout and its interpretation remain with ObjectModel. Image retains its own
format policy and codecs above Id.

Acceptance includes published arithmetic and collection, all startup owner
initializers, copied Isolate values and non-transferable rejection, Alien memory
access, generated integer/string/float/full-width unsigned calls, and a C callback
that collects before returning to its caller. Callback release clears the foreign
address. Oversized embedded libraries are rejected before reading their bytes.
The broad Oop gate separately retains the direct-dispatch counterexample tests
and authored task-control checks. [Receipt](../measurements/published-startup-oop-20261025.json).

This completes published **Bootstrap** startup, not Host/CLI installation or full
image/worker acceptance. Image graph recovery still needs the pending compiler,
Execution and reflection connections; Generation, relational integration and
Host/CLI remain outside this gate. Isolate's copied-value conversion and owner
activation are qualified here; no restored-image worker process is claimed.

The authored MethodInvocation policy now installs on that published runtime.
Its cache obtains neutral templates from Codegen's existing declared Method entry,
then binds separate implementation contexts through `withImplementation:`; it no
longer emits and compiles another C adapter for each arity. Contexts remain
owner-defined Oop Sequences, with no new Id operation or native registry.
Execution's explicit `invokeOn:` / `invokeOn:arguments:` messages snapshot the
argument owner's `sizeWord` / `atWord:` view and enter the captured Method through
the existing continuation primitive. Ordinary dispatch does not use those messages.
The bootstrap gate covers 19 construction/replacement/GC/suspension checks, arities
0–16, and the 177 task-control checks after installing the authored policy. Native
Host migration and installed CLI acceptance remain separate work.

HostEntry, HostInteraction, HostProject, ProjectBuild and the executable facade now
use declared Oop Methods. Host keeps paths and copied projections; it does not read
foreign payloads or pass function pointers through Id. Its send helper protects
receiver/arguments in an existing Sequence while constructing a selector, and
protects the result through Roots cleanup. Returning scopes transfer their result
with `releaseTokensProtecting`; REPL/client scopes still release independently.

`scripts/oop-host-test.sh` qualifies a relocated CLI with explicit source startup,
minimal startup, diagnostics, module compilation without execution, source-free
artifact activation, and a project-query result surviving collecting cleanup.
It also runs the published Bootstrap/task-control gate and 23 CLI/support unit
tests. Default startup still requires its compiled bootstrap image; it does not
silently use source. Image/worker acceptance and the old native integration test
fixtures remain pending. The workspace build now stops in Generation, Store and
Relational, rather than Host support. [Receipt](../measurements/host-oop-20261025.json).

## Generation and portable reflection

Generation now uses Oop declarations and owner messages throughout admission,
inspection, mapping, certification and retirement. CandidatePolicy still owns
publication and explicit undo in Birnam. Direct binding observations use
Execution's existing `directMethodFor:in:` protocol over `bindingsDo:`; its fixed
visitor behavior is installed once, so observation does not itself change the
binding epoch. The language Vtable remains its metatable prototype, with the same
ObjectModel owner queries exposed there and on the Id API receiver.

Generated Methods retain CodeObjects through ordinary `codeObject` getters.
Codegen exposes its reflected implementations in the same way; captured Methods
preserve that behavior. No Method payload is read by another owner and no metadata
registry or Id operation was added. PortableCode loads before the first compiled
artifact, including in the cold build. Compiled artifact admission accepts the
inherited Method behavior used by these getters.

Candidates now carry one native factory in `BIRNAMC2`; named arity/disposition
specifications construct Codegen templates through Id instead of compiling
redundant adapter units. Synchronous named templates retain their native control
boundary. Generation hashes the Codegen-owned shared-entry description; it does
not pretend that this entry has a Machine frame descriptor. Old `BIRNAMC1` bundles
are rejected. The displaced `pending_source.rs` projection is removed.

`scripts/oop-generation-test.sh` runs the core, Bootstrap, Host and 28 lifecycle
checks, including adoption, drift/restoration, inert compile/map, activation,
captured calls across GC, exact rollback, idempotent repeated rollback, file
cleanup and discarded-handle refusal. [Receipt](../measurements/generation-oop-20261025.json).
This is not image or full-language completion: image compiler connections,
Store/Relational, remaining native fixtures, installed tooling and restored workers
still need integration and acceptance.

## Image and portable materialization reconnection

The retained image operations now use Oop messages. Previously, image compilation
and reflection depended on the removed carrier and native-entry interfaces. The
new sequence is: snapshot allocated objects and vtable bindings through Id;
query each owner's portable representation; construct Codegen Methods; restore
Execution-owned Blocks and captures; install current bindings. Id's ordinary
lookup and direct Method entry are unchanged. No Id operation, payload catalog,
or shared native runtime state was added.

Execution owns Block capture restoration and rejects repeated initialization.
Codegen owns its image stage and bounded materialization cache; reflection obtains
implementations through Method messages. NativeBackend records whether each
portable body is synchronous. `BIRNAMO2` stores two native bodies per portable
program; named templates use Codegen's existing constructors. Mapped recovery
loads those objects without invoking the compiler; explicit rebuild uses portable
programs without parsing recorded source. Image owns the immediate-reference
locator described in Design 60O. Word exports its existing immediate vtable so
methods installed there can be restored.
Temporary byte-conversion Sequences now leave their scoped roots after each
conversion instead of remaining alive for the complete inventory or save.

Id's missing-message packet remains the selector followed by original arguments.
When `addMethod:as:` installs `doesNotUnderstand:`, it asks the Method for its
handler form. Native Methods return themselves. Codegen's source Methods construct
an independent context that accepts their declared argument prefix. Installation
still acknowledges the submitted Method, which retains its original arity;
lookup returns the installed handler. No additional Method lookup occurs on an
ordinary send, and neither a captured Method nor its implementation is rewritten.

`scripts/oop-image-test.sh` runs the core, published Bootstrap, Host and Generation
gates, the existing 107 Portable Code checks, handler/restoration checks, immediate
locator admission, and fresh-process mapped/rebuilt recovery. The image probe
preserves aliases, a cycle, immediate identity and behavior, native signatures,
missing-message errors, and task suspension across GC. The default CLI is also
checked against the saved bootstrap image. Store/Relational, restored workers,
remaining native fixtures, installed tooling and performance qualification remain
outside this milestone.

The fresh combined gate passed on 2026-10-26. Its 124,711,349-byte image saved in
211 seconds, loaded mapped in 47 seconds, rebuilt in 34 seconds, and reached the
CLI input result in 51 seconds. Each is one startup-inclusive acceptance run, not
a comparative benchmark; image/startup performance remains unqualified.
[Receipt](../measurements/image-oop-20261026.json).


## Primitive replacement without recursive numeric conversion

The generated step used to receive a boxed program counter, call `Integer wordOf:`
to decode it, and construct more Integers for frame and control indices. A source
replacement of Integer conversion therefore recursively needed itself before its
body could execute. Generated steps now declare an immediate-counter flag in the
existing frame descriptor and send immediate Oop indices to Machine. Execution
interprets that descriptor data; Id still only sees Oops and enters the selected
Method directly. Ordinary numeric literals and explicit conversions remain live
owner sends. General descriptors retain the Integer counter contract for authored
steps, and Machine continues to accept both supported index forms.

Lowering version 2 identifies this generated convention. The descriptor's last
metadata word is 0 for an Integer counter or 1 for an immediate Oop counter;
unknown flags and immediate counters without one driver argument are rejected.
Image's runtime ABI hash advances to `Id-oop-image-v4`, requiring older bootstrap
images to be rebuilt. No new Id message, Method entry protocol, or payload catalog
was added. The regression exercises source replacements of both conversions,
collection during calls, captured originals and ordinary arithmetic. It captures
its dependencies before replacement: global lookup and Vector literals retain
their normal authored dependencies, including Integer conversion.

The core, published Bootstrap, Host, Generation and fresh mapped/rebuilt image
gates pass with this convention. The same self-contained source replacement
probe overflows on the previous runtime. [Receipt](../measurements/primitive-replacement-oop-20261026.json).
