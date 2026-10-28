# Continuation-aware Id: design and proof checkpoint

**Open contract regression (2026-10-27):** ordinary Method construction currently
advances the binding epoch while initializing private metadata behavior. The
preserved purity assertion fails (664 → 674 for two Methods). This is unresolved,
not a qualified implementation of that contract. No Id change or test relaxation
has been made; see [the evidence](../measurements/minimal-mutation-epoch-oop-20261027.json).

**Open closure-admission regressions (2026-10-27):** Oop admission omits the
established capture-count and immutable CodeObject-association checks. Repeated
admission queries shape again; direct construction accepts incorrect capture
counts and raw absent references. Six isolated contract cases fail; explicit
external pairing passes its identity/mismatch control. No production repair or
contract relaxation has been made. See [the evidence](../measurements/closure-admission-regressions-oop-20261027.json).

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


## Explicit Store and Relational activation

Both optional native owners now declare Oop Methods and use their Namespace as
Method context. Their existing fact/history and query algorithms are unchanged.
Store keeps its own records, snapshots and retirement cleanup; Relational keeps
its own query collectors and canonical LogicVars. Dependencies are reached only
through current Id messages, including full-width signed/Float conversion and
indexed bytes. Binding inspection uses the existing allocation/binding visitors.
`Reflect nameOf:` is reconnected because normal compiled module loading needs it.
No Store dependency was added to Bootstrap or ordinary object edits.

The release workspace builds. Acceptance covers 51 Store and 46 Relational checks,
ten retained authored Store checks, resource exhaustion with reserved rollback,
and fresh mapped/rebuilt images that activate their declared providers. Recovery
preserves retained fact history, queries and a fact-backed Cell with parsing
blocked; mapped recovery also blocks the compiler. The broader workspace library
test command still fails in ProjectBuild's old-carrier test harness. Native test
fixtures, additional optional image families, installed tooling and restored
workers remain outside this milestone. [Receipt](../measurements/optional-owners-oop-20261026.json).


## Project observation acceptance

ProjectBuild and HostProject's test helpers now use the same Oop bootstrap and
owner-loading protocol as Host. Collecting/failing callback fixtures declare
Methods and retain captured originals in Oop contexts; the old native entry and
stack-pointer injection is gone. All 22 pure tests and three retained runtime
observation tests pass, including source non-execution, current policy replacement,
GC, exact native failure identity and malformed metadata refusal.

The project CLI probe passes check, build/cache repair and published execution
with source hidden, then stops at the partial prefix's missing tools image.
Worker startup separately reaches the fixed 10-second ready-handshake timeout;
restored workers are not yet qualified. Host's lifecycle/image library tests still
need their old-carrier harness migrated. [Receipt](../measurements/project-observation-oop-20261026.json).


## Real installer and project workflow

Host no longer requires an Alien receiver when entered over the build-only
compiler seed. It queries Namespace and configures Alien when present; published
Bootstrap retains its required native-owner closure. This fixes the first cold
artifact compilation without adding a seed API or loading unrelated platform
owners into the compiler seed.

The real installer now produces the core and tools images, removes the parser
seed, and passes fresh console and `doc stdlib` probes. All 811 installed files
match its receipt. Default image-backed project policy/query checks pass. The full
project CLI workflow also passes with explicit source startup: human/JSON check,
build/cache repair, run/install/test, path and pinned Git dependencies, source-hidden
published execution, relocation and missing-library refusal. This used the
explicit developer build cache; release cold/archive qualification is separate.
Worker startup still exceeds its unchanged ready-handshake limit, and the remaining
Host/native test harness migration remains open. [Receipt](../measurements/published-project-oop-20261026.json).

## Byte snapshots on the image/worker path

Image input and Codegen's binary output now use Bytes' existing bounded
`fromByteValues` / `appendByteValues` operations instead of allocating a Sequence
with one managed reference per byte. The validated native-code bundle remains a
Bytes Oop across Image and Codegen. Codegen retains an owner-provided snapshot,
reads its framing through indexed messages, and requests each selected object
through `sliceFromWord:toWord:` before sending it to TCC. Artifact loading also
keeps one byte snapshot for inspection and mapping. No foreign byte payload is
read and no Id message or calling convention is added.

The existing native Word-indexed slice operation now copies only its requested
range. It previously cloned the entire source buffer before copying the range,
which made mapping many small objects from one image repeatedly copy the complete
bundle. The buffer borrow ends before allocation or any callback. Generated
binary descriptors/programs are constructed as Bytes; language String literals
continue to call their current String constructor.

A trial batch-copy API and per-owner callback receivers were discarded after the
slice-copy cause was isolated. They are not part of the implementation. The
snapshot regression changes the caller's original byte object's slicing behavior
and collects after preparation; mapped execution must still succeed with both
parser and compiler disabled. Another regression refuses binary metadata passed
through String construction while requiring ordinary String construction to run.

The final core, Bootstrap, Host, Generation and image gate passes. In its single
local run, mapped recovery took 9.1 seconds and default CLI startup/execution took
8.9 seconds; the initial mapped probe took 46.1 seconds. These are acceptance
measurements, not a release benchmark. The targeted service-image worker now
starts within the unchanged deadline; complete worker acceptance remains a
separate gate. [Receipt](../measurements/image-byte-snapshots-oop-20261026.json).

The complete original worker suite now passes on `88378faf`, including copied
values, failures, async requests, timeout/crash, orphan reaping, relocated library
configuration and authored service cancellation. The ready-handshake limit stays
at 10 seconds. [Receipt](../measurements/worker-oop-20261026.json).


## Host library acceptance over Oop callbacks

The retained Host library tests now compile against declared native Methods.
Their callback contexts, counters, captured Methods and injected failures are
ordinary rooted Oops. Test callbacks explicitly invoke a captured Method through
its existing reflection operation; ordinary Id sends retain their direct-entry
contract. No production callback registry, private cross-owner interface or Id
operation is introduced.

This qualification found a lost diagnostic in generation projection. Missing
retained metadata now returns the existing `compileError` value with rebuild
guidance, as before the Oop migration. Malformed metadata still fails, and neither
case reparses source. The callback and image checks retain their assertions for
current setters, explicit undo, code ownership, partial publication and failure
propagation.

The saved-main rooting fixture now installs its hook on a delegated vtable for
that one Block. Replacing the shared Block `value` method also intercepted Blocks
used by Globals mutation, recursively entering the fixture itself. The corrected
fixture still removes global main and collects during entry; the Host must keep
main alive for the call. This is a fixture correction, not a runtime stack-size
change.

The workspace library suite, the Host package suite and all 18 installed-runtime
Host library tests pass, including mapped/rebuilt/resaved generation images and
live-method drift. The 28-check source Generation test also passes. Results are
recorded in [the Host receipt](../measurements/host-oop-20261026.json). The three separate
Host native integration tests also pass with declared callbacks: owner initialization/reload, live NativeBackend emission, and the
authored AlienCompiler/ImageFormat byte and failure protocols. The NativeBackend
fixture now replaces `emitResumable:source:`, which is the operation used by
`Codegen compile:`; the old named-descriptor operation is no longer its interface.
Older Siphon test harnesses still require migration. This does not qualify the
complete `host-cli` runner or a cold release.


## Installed task-control harness

The installed task-control Rust test now uses the existing declared C callbacks
from the continuation fixture. Cleanup and failure reason are ordinary Oops in a
context Sequence. The test retains and releases roots explicitly and no longer
leaks a native Observer structure or passes carrier structs. Its original 177
assertions pass. All seven binaries required by `host-cli-test.sh` now build;
the full installed task group remains a separate check.
[Receipt](../measurements/task-control-oop-20261026.json).


## Baked image bytes stay with their owner

The installed interaction gate exposed a baked-startup timeout. The Host facade
constructed one large String/Sequence for the entire image, then HostEntry read
it back into native bytes and constructed another String. Baked startup now
constructs Bytes using the existing bounded `fromByteValues` / `appendByteValues`
operations. HostEntry passes that Oop directly to the current Image entry, which
owns interpretation and validation. No Id operation or private byte interface
is added; String byte providers remain usable by Image.

A regression substitutes an Image entry that accepts an opaque object with no
byte operations. It fails against the prior HostEntry and passes now, proving
that HostEntry does not decode the provider before the selected Image operation.
A 119 MiB baked image runs saved main after its input image is deleted in 16.4
seconds; the installed fixture had exceeded its unchanged 120-second limit.
The workspace library suite passes. The complete interaction retry and the
separate Concurrency ready-handshake regression remain open.
[Receipt](../measurements/baked-bytes-oop-20261026.json).


## Installed group qualification: remaining failures

**The full installed task and interaction groups are not yet qualified.** Task
images, task-field aliases, socket owners, console owners and compiler-disabled
startup pass. Init/wrap preservation, documentation and documentation generation
also pass. The task group then fails Concurrency worker startup at the original
ten-second ready handshake. Its saved 136 MiB image starts correctly in a direct
protocol probe but needs about twelve seconds; the earlier 119 MiB service image
also needs about 10.5 seconds in this run. The earlier complete worker receipt
remains evidence for its tested image/run, not proof of adequate startup margin
for Concurrency's larger image.

A seven-annotation Id inlining experiment reduced the larger image's startup from
11.6–12.2 seconds to 10.7 seconds in an interleaved comparison. It still missed
the deadline and was removed; the patch and measurements are retained locally.
No cache, contract extension, private byte interface or timeout increase was
introduced. The interaction group separately exposed the baked-startup problem
fixed in `f9d701f7`; its complete CLI retry is still running.
[Group results and reproduction evidence](../measurements/installed-oop-groups-20261026.json).


## Restore exported-global and parent reflection

The corrected interaction CLI passes its complete tooling, baked-executable,
published project tty/socket and persistent-session checks. All 26 REPL golden
checks also pass. The subsequent LSP test exposed three unported existing Reflect
queries: `globalNamed:`, `allGlobalNames` and `parentOf:`. Completion had been
reflecting the Error returned for the missing global query, rather than System.

These queries now use ordinary current Namespace, Globals and Id messages.
Reflection preserves native-export precedence, including a bound nil, then falls
back to authored Globals for an absent export. Its exported-name list uses
Namespace `count` and `nameAt:` and excludes authored-only Globals. Parent lookup
uses the current Id query, without reading vtable payloads. No Id entry, registry
or private owner interface is added.

The regression fails against the prior runtime and passes with the restored
queries, including replacement, precise collection and exact failure propagation.
The real LSP exchange passes, as do 145 stdlib checks, 75 CLI checks and the
workspace library suite. The interaction group is now covered by these component
runs on staged installed prefixes; cold release acceptance remains separate.
Worker startup remains over its unchanged deadline. Additional timing localized
about 4.5 seconds to ImageFormat validation, mostly heap admission. A generated
immediate-word experiment was also removed after insufficient measured improvement;
no global tuning or constructor behavior change is included in this milestone.
[Receipt](../measurements/reflection-oop-20261026.json).


## Worker startup policy: 15 seconds

The user explicitly raised the worker ready-handshake timeout to 15 seconds and
prioritized end-to-end operation over optimizing to the earlier ten-second limit.
The Isolate startup path now uses that limit. The previously failing Concurrency
image completes its ready handshake with the established Id implementation, and
all 13 Isolate tests pass. Earlier ten-second measurements remain historical
evidence, not the current acceptance target. The Method-layout, inlining, byte
sharing and adapter experiments are not included in this change.


## Static source-inspection harness

The retained Siphon source-inspection test now uses Oop arguments/results and
existing declared native callbacks. Caller-side rooting reuses HostSupport as a
test-only dependency; Siphon's production dependency remains just the Id ABI.
The fixture loads the optional Store wrapper explicitly over the native bootstrap,
without requiring stdlib's module-loading policy.

All original assertions pass: temporary inspections retain no objects or facts,
parser and metadata accessors remain live through collecting callbacks, equal
source text does not merge identity, retained records do not reparse, and source
preparation/registration work without Store. No old carrier compatibility API is
introduced. [Receipt](../measurements/source-inspection-oop-20261026.json).


## Ordinary slot reflection respects zero-byte handles

The full authored-language gate found that slot reflection treated every unowned
zero-byte allocation as empty ordinary storage. String's Oop representation is
now such a handle, so this incorrectly answered an empty inventory and
`unknownSlot` instead of declining foreign storage and answering
`unsupportedSlots`.

ObjectModel now uses the existing current GC `referenceOwnerOf:` query to admit
unowned empty allocations. Its own instance/layout records retain their previous
behavior. This adds no payload-family list, private owner access or Id operation.
String and Bytes handles are rejected; ordinary empty objects remain supported.
A separate check replaces the reference-owner query and verifies live dispatch.

All 870 authored assertions, 35 focused slot assertions and the core Oop gate
pass. The new source-inspection harness also passes. With the approved 15-second
startup policy, Concurrency passes all 19 service checks. The first multicore run
failed seven later checks; an instrumented rerun and an unmodified repeat both
pass all 27. Both outcomes are retained because the first failure's cause is not
established. Closure observation and all 177 task-control checks pass. These are
component acceptance results, not a clean release qualification.
[Receipt](../measurements/slot-owner-oop-20261026.json).


## Registry and ordinary-record retention checks

The two retained native measurement harnesses now use the installed Oop bootstrap
and existing caller scopes. They restore the tools image and run the original
registry/ordinary-record batches with unchanged object-count and fact-publication
assertions. Temporary arguments live for one sample; returned values are not kept
through the post-call collection. Both tests pass at every original batch size.
The deliberate Logger-retention negative control fails at one call after warmup,
confirming that the migrated harness still detects a leak.
[Receipt](../measurements/retention-harness-oop-20261026.json).


## Additional image and native-loader acceptance

Frame-call and collection-search checks pass after source deletion in both
mapped and rebuilt images, preserving identity, current replacement, collection,
protocol withdrawal and suspension. The collection-search runner's save operation
exceeded its old 180-second test limit; the runner now allows 600 seconds while
retaining every behavioral assertion. This is a test budget, not a runtime policy.

The native-loader image fixture now declares its callback through the Oop module
format. Initializers return an explicit Oop; the fixture receives its desired
return object as a second argument instead of returning the retired non-object
Unit carrier. No compatibility operation or runtime API was added. The checks pass
through source execution, a source-free compiled artifact, mapped/rebuilt images
and a baked executable, including collecting initialization, repeated loading,
missing/incompatible modules, aliases and current method replacement. Retired
initializer/dependency exports still abort if mistakenly entered. Authored Trace
also passes mapped/rebuilt image recovery with Oop native adapters and collection.
[Receipt](../measurements/image-followup-oop-20261026.json).


## Harness and local-Cell retention checks

The retained Test/Bench and local-Cell measurement harnesses now use Oop callers
and scoped roots. Their original batches pass exact post-collection object-count
and no-fact-publication assertions. The negative controls still fail when a suite
is left registered or a temporary Cell is deliberately retained. This migrates
additional native tests without adding a compatibility ABI or changing runtime
behavior. [Receipt](../measurements/harness-cell-retention-oop-20261026.json).


## Restore the vtable publication operation

`addMethod:as:` was incorrectly installed on generic Object behavior during the
Oop port. Its established receiver is a vtable, and direct capture at the vtable
behavior therefore returned nil. Execution now installs this existing operation
on the current behavior of ObjectVt, leaving `methodFor:` on Object. No Id
operation, observer mechanism or private interface is added.

The direct-capture regression fails before the correction and passes afterward.
The core Oop gate and the unchanged Alien recipe-reentry test pass, including
nested publication, collecting replacement and exact policy errors. At that
checkpoint, the full Alien publication test failed one Store-notification assertion:
the older `Vtable methodChangesTo:` subscription had not been connected to the
corrected core. Its assertion was retained until the later
[explicit retirement decision](#method-observation-contract-decision).

System image checks pass with source removed, mapped/rebuilt recovery and baked
entry: saved aliases/data, current file/environment state, binary input/output,
EOF and real read errors. StringBuilder checks also pass through compiled
artifacts and mapped/rebuilt/baked images, including copied String identity,
current method replacement and rejection of a live native buffer.
[Receipt](../measurements/vtable-publisher-oop-20261026.json).

## Foreign wrappers and optional Store image participation

The Alien image checks preserve authored replacements and foreign wrappers after
source deletion through mapped recovery, rebuilt recovery and a baked executable.
A newer wrapper installed during an image callback remains authoritative after
resave and rebuild, including collection during that callback. Hash recovery passes
with both identity-based hashing and hashing that calls a foreign function, so
foreign bindings are available before the restored hash tables are rebuilt.
The registration suite also passes successive mapped and rebuilt saves, retaining
code reuse, owner aliases, removed methods and authored replacements.

Explicit Store opt-out also passes: saving does not invoke the replaced Store
operations, recovery preserves destination facts, and a Cell belonging to an
unselected Store is rejected without replacing the saved file. A custom selection
error preserves its identity and leaves that file unchanged.

The basic Alien and Store-participation runners exceeded their former 90- and
180-second save limits. They now allow 600 seconds per command, retaining all
behavioral assertions. This changes test budgets only; worker startup still has
the user-selected 15-second deadline. No runtime implementation or Id contract
changed for these checks. The subscription gap remained open at that checkpoint;
the later user decision retires that feature.
[Receipt](../measurements/alien-store-images-oop-20261026.json).

## Standalone Siphon acceptance

`scripts/siphon-test.sh` again builds and tests the production Id without upper
runtime modules. The old test no longer compiles because it used Operand/Outcome
and expected a native protocol struct as a message result. Its replacement uses
seed Names and Oop messages, loads an independent C module, and verifies repeated
initialization with current arguments, initializer rejection, canonical symbols,
invalid library paths and process-owned Id identity across reopening. The C
fixture aborts if the retired initializer is entered. Both tests pass; no runtime
code or Id operation changed.
[Receipt](../measurements/siphon-handoff-oop-20261026.json).

## Integer operation replacement acceptance

The native Integer operation test now uses scoped Oop roots and captured Methods
instead of native entry/context carriers. It retains all five distinct operation
receivers, replaces addition with a two-argument authored Method that collects and
returns a captured object, and counts entry through the existing declared test
callback. Addition observes that replacement exactly once; subtraction remains
unchanged, and restoring the captured Method restores the original sum. The test
passes against the staged runtime without production changes.
[Receipt](../measurements/integer-operation-oop-20261026.json).

The numeric identity-fallback test is also migrated. It replaces `identityOf:` on
the Id receiver's vtable, first proves that replacement is active, then verifies
that existing Words remain readable without invoking it while new allocation
propagates its exact failure object once. Captured Methods and callback state use
scoped Oops. The first port incorrectly treated the Id receiver as a vtable;
correcting the fixture preserves the original assertions without a runtime change.
[Receipt](../measurements/numeric-identity-fallback-oop-20261026.json).

## Restore wrong-arity Block failures

The installed Host image suite found that a saved one-argument `main` accepted
`value` and exited successfully. The shared Block entry introduced by the Oop
port exposed every `value...` selector, and the synchronous generated body answered
nil on mismatched arguments. Host correctly treats nil as a successful result.

Execution now records the parameter count when it admits a Method's CodeObject
through the current `CodeObject parameterCountOf:` message. Its default Block
entry checks that admitted count before preparing entry into the selected body.
Captured Methods copy the count; nil admission removes it. The existing retirement
callback removes records before address reuse, and the records retain no objects.
Unadmitted native Methods keep their own argument checks. Id, generated code and
the ordinary send protocol are unchanged.

This preserves current `BlockVt value` replacement. Giving each Block a private
value binding would hide that supported replacement. Reading Method metadata on
every call would impose an extra dependency on native Methods; admission keeps
that responsibility with Execution's existing code-shape operation instead.

The focused regression fails before this change and passes afterward, including
wrong argument counts, cloning, collection, captured Methods and current value
replacement. A CodeObject getter that deliberately fails is not queried during
entry. The complete Oop core gate passes, as does entry into the previously saved
wrong-arity image with the expected error. The saved-main Host check still passes
with compilation disabled and collecting callbacks. Optional Store/Relational
worker recovery also passes with the unchanged 15-second startup deadline.
The complete authored suite passes all 870 checks, including suspension,
cancellation, cleanup and replacement during collection.
The unchanged Host image CLI suite now passes in full: source/asset deletion,
nil/false results, REPL entry, explicit source recovery, invalid entry diagnostics,
and rejection of corrupt or incompatible images. The full optional-provider image
suite passed before the arity fix, and its worker case was rerun successfully with
the final runtime. These are staged-prefix checks, not a new installation or
release qualification. Older native harness migrations remain open. The later
decision below resolves the Store subscription question.
[Receipt](../measurements/block-arity-oop-20261026.json).

## Method observation contract decision

The old `methodChangesTo:` subscription observed the actual Id dictionary edits,
including edits entered through a previously captured primitive Method. Wrapping
the current `addMethod` binding above Id cannot preserve that coverage: a probe
observes an ordinary edit through the wrapper, then invokes the captured original
Method and confirms both that the dictionary changed and that the wrapper was not
entered. Retargeting the captured Method would violate captured-call semantics.

Restoring that universal coverage therefore requires an opt-in hook at Id's actual
mutation point. A wrapper-only implementation would silently weaken the feature.
Neither change was authorized by the general migration instruction. The user then
explicitly chose: **keep Id unchanged; retire the universal subscription and
retain explicit Store invalidation.** This supersedes the earlier subscription-gap
notes in this document. No core hook or wrapper subscription is being introduced.
[Probe receipt](../measurements/method-observation-boundary-oop-20261026.json).

## Allocation-fact acceptance through Oops

The single-object and batch allocation-fact tests now use Oop arguments, scoped
roots and declared callbacks. They retain the assertions for changed getters,
replacement of the query itself, custom method lookup, mismatched size/vtable,
reassociation and the two-to-32-object batch bound. Changed getters are not called
when the shortcut declines. The custom lookup runs exactly once for the query.

The batch test uses the existing `representationsMatch` message instead of the
retired packed native-pointer byte buffer. Malformed Oop arguments remain rejected.
A test-only lookup callback forwards the existing argument slice through an
explicitly installed alias, avoiding an argument Sequence and recursive sends to
the Id receiver. No production runtime code changed. Both migrated tests and four
existing callback-fixture consumers pass; the C fixture also passes strict warnings.
[Receipt](../measurements/allocation-facts-oop-20261026.json).

## Retire universal method subscriptions

The approved decision is now reflected in Current Invariants, the Store contract,
Alien publication checks and the remaining legacy harness expectations. The unused
v1 Id registration hook is removed; compiled Id is unchanged. The new Oop
`method_observation` test asserts that the subscription is absent before and after
loading Store, while explicit `Store methodChanged:on:` invalidates descriptions
across its Store instances and remains replaceable. Store's explicit publication
authorizer and post-edit observer are retained.

This check exposed a Store ownership defect: an explicit assertion identical to a
mirror could be discarded as redundant, and projection updates could reuse mirror
ownership. Invalidation also consulted the current user value instead of removing
all owned descriptions for the dictionary key. Store now keeps explicit projection
writes separate from mirror rows and invalidates only its owned rows, preserving
user facts. Duplicate explicit assertions remain idempotent. The
[ownership receipt](../measurements/store-mirror-ownership-oop-20261026.json)
records failing unit and native cases before the correction and passing results
afterward.

Validation passes: 13 Store unit tests, workspace library tests, explicit
invalidation, all 20 Alien publication assertions, compiled/source optional Store
activation, image participation and all 870 authored checks. The legacy `store`,
`publication_contract`, `binding_undo` and `minimal_mutation` harnesses had their
retired-subscription expectations removed but still awaited Oop conversion at
that checkpoint; subsequent conversions are recorded below. This is
not a claim that the full native-message gate passes.
[Retirement receipt](../measurements/explicit-method-invalidation-oop-20261026.json).

## Installation admission metadata

The installer receipt and its verifier still declared Id protocol version 1 even
though the current Oop interface declares version 2. Both now record/require
version 2; continuation module declaration version 3 is a separate field of the
native module format. The receipt schema is unchanged. Tests reject both the
retired version 1 and unknown version 3 as Id protocol versions, and the installer
and release-orchestration tests pass. This corrects qualification metadata without
changing Id or the runtime.

## Independent cold installation and archive startup

A clean detached checkout of `28a2f117` builds and publishes a release-profile
prefix with fresh driver and Cargo target directories and no development cache.
The installed Birnam executable is absent from the build PATH. The receipt records
the clean source identity, Id protocol version 2 and an exact 813-file closure.
Core image preparation succeeds before optional providers are built; the Lang seed
is absent from the published prefix.

The exact signed installation is packaged without rebuilding. With the original
checkout and prefix temporarily hidden, the extracted archive passes relocation,
source/foreign calls, diagnostics, collection, image save/load and compiled startup
with parsing and compilation disabled. Archive and installation hashes remain
unchanged. Both original paths are restored afterward.

The Host smoke fixture now checks the missing Id library's path and current loader
diagnostic instead of the retired `native status=` text. No runtime change or
timeout relaxation was needed. This proves one cold source build and the bounded
archive gate, not the full legacy native/Host matrix or a published release.
[Receipt](../measurements/cold-install-oop-20261026.json).

## Lang comparison through Oop callers

The installed CLI group reached the Lang comparison gate but could not compile
its Operand/Outcome harness. That caller now uses scoped Oop roots and the existing
indexed byte messages for Symbol/String data. The original oracle is unchanged:
133 fixtures and shipped sources have identical concrete syntax trees, byte spans,
diagnostic bytes and failure positions under seed and authored Lang. Cloning and
collection pass; self-compilation produces identical artifact bytes. The production
load probe excludes the Rust grammar and still rejects missing compiled Lang even
when the seed is present. No runtime or parser implementation changed.
[Receipt](../measurements/lang-caller-oop-20261026.json).

## Alien compiler oracle across the ABI change

The CLI group next found that its frozen Rust oracle still emits Operand/Outcome
native adapters. The oracle remains unchanged. The runner now compares the foreign
marshalling bodies and diagnostics, excluding the known version-specific prelude
and native entry/registration wrappers and translating only the old failure-status
field access. All 64 comparisons and eight ABI-classification checks pass.

The Oop adapter is additionally exercised against an independent C fixture rather
than copying the production adapter into the oracle. Twenty checks pass for signed,
unsigned, float, string, pointer and void calls; structs, arrays, nested HFA and
non-HFA returns; malformed aggregate shape; and collecting scalar/aggregate callbacks
with release. These checks run without the stdlib. No runtime/compiler behavior or
Id contract changed. [Receipt](../measurements/alien-compiler-oop-20261026.json).

## Cold-candidate generation acceptance

The complete installed Host `generation` group passes on the independent cold
candidate. Coverage includes retained Methods and current setters, source-registry
image aliases and failures, explicit candidate cleanup/undo, mapped and rebuilt
lifecycle recovery with binding drift, explicit adoption, running/TCP activation
and rollback, and all 62 compiled-unit policy checks. No runtime change was needed.
Other Host groups and the legacy native integration audit remain separate work.
[Receipt](../measurements/cold-generation-oop-20261026.json).

## Explicit Store publication callbacks

The retained publication-contract harness now uses declared C callbacks with Oop
Sequence state instead of native function entries and pointers to Rust test state.
It checks authorization denial, exact native failure identities, post-publication
dictionary/mirror agreement during collection, retained callback edits after a
failure, and non-veto Error data returned by a void observer. A failed post-remove
callback does not resurrect the Method; missing removal invokes only authorization.
The test passes on the cold candidate without production changes or a universal
subscription. [Receipt](../measurements/store-publication-oop-20261026.json).

## Cold-candidate interaction acceptance

The full installed Host `interaction` group passes on the cold candidate: socket
ownership, REPL ownership, compiled startup without parsing, tooling and image
commands, persistent TCP/project sessions, all 26 REPL golden checks and the LSP
workspace/diagnostic/UTF-16/version/close checks. No runtime changes were needed.
[Receipt](../measurements/cold-interaction-oop-20261026.json).

## Explicit Store compensation through Oops

The compensation harness now uses Oop calls and scoped roots. Its existing checks
pass at the 16 KiB fact limit: reserved inverse capacity remains usable after
exhaustion, a stale comparison spends nothing, exact one-key restoration preserves
unrelated facts, and reservation owners survive collection without caller roots.
Releasing one owner's reservation is idempotent and preserves another's. The null
Oop represents absence while language nil remains a present value. No Store or Id
implementation changed. [Receipt](../measurements/store-compensation-oop-20261026.json).

## Cold-candidate task acceptance

The complete installed Host task group passes on cold candidate `28a2f117`:
task image/field recovery, 19 Concurrency service checks, multicore behavior,
closure observation and all 177 task-control checks. The worker startup deadline
remains 15 seconds. This is functional acceptance, not a multicore speedup claim,
and predates the root-provider correction below.
[Receipt](../measurements/cold-tasks-oop-20261026.json).

## Invoke root providers once per registration

Porting the root-provider test exposed duplicate callbacks. Collect explicitly
requested `Roots rootsDo:`, then tracing the Roots object invoked the providers
again because `referencesOf:do:` delegated to the same operation. The corrected
tracer reports only registry-owned references; provider invocation remains in
`rootsDo:`. Both use the existing snapshot of retained values and registered
providers, without a registry borrow spanning sends. Id and the public interface
are unchanged.

The declared C test fixture keeps child references in its own private state, so
they survive only when the registered providers report them. It checks 64 providers,
duplicate registration order, exactly one callback per registration and reclamation
after removing each child edge. Reclamation is checked by allocation identity,
without passing dead references back to Id. The regression fails before and passes
after the correction. The complete Oop core gate, four retention/publication
harnesses and all 870 authored checks pass on an isolated prefix containing the
corrected registry. The ongoing cold-candidate Host groups use the earlier runtime
and do not qualify this correction.
[Receipt](../measurements/root-provider-oop-20261026.json).

The CLI group additionally exposed an unfinished integration: its portable
projection fixture reaches `Machine recoverAt:with:`, which is not connected in
the current Execution facade. The accepted compiler/Execution plan retains frame
recovery. That assertion remains unchanged; restoration must connect the existing
recovery machinery, not merely install an error stub for the negative case.

## Frame inspection through Oop methods

The retained FrameView engine is now connected to `Machine currentFrameView` and
the existing identity, program-counter, slot and slot-kind getters. Before this
slice, those selectors were absent from the Oop facade. After it, an ordinary Id
send enters an Execution Method, which validates the existing arena/frame handle
and reads Execution-owned storage. Identity results use Word's existing constructor;
positions and counts use Integer. No Id operation, payload interpretation, registry
or application protocol was added. Execution reports the view's Machine reference
through its existing GC owner and retains the prohibition on cloning views.

The native regression fails against the preceding prefix and passes against an
isolated prefix with the new Execution library. It checks live reads, getter
replacement and captured getters, unrelated receivers, invalid slots, precise
collection with only the view retained, and stale reads after completion and
destruction. The complete Oop core gate also passes, including its negative
direct-dispatch contract. This restores inspection needed by recovery; it does not
implement pending-call recovery or complete cold-install qualification.
[Receipt](../measurements/frame-view-oop-20261026.json).

## Pending-call recovery through Oop callbacks

`withInline:at:owner:`, `recoverAt:with:` and `recoverAt:withGraph:` now connect
the Oop facade to the retained recovery engine. The candidate is a declared Method
Oop, replacing the old NativeEntry value. Its receiver is the Machine, followed by
the original receiver and arguments. A non-null result completes the pending call;
null declines. Successful recovery transfers the acquired entry to a baseline frame
and requires a null acknowledgement. Native callbacks still cannot suspend.

Before this connection, entering a registered Method acquired the entry, read
captures and constructed its baseline frame. After it, only a Method explicitly
constructed with an inline candidate first checks the eligible pending non-tail
call and current Machine controls. It invokes that candidate through existing
captured invocation. Decline proceeds to the same capture reads and baseline
construction; recovery validates and atomically publishes the requested frame.
Ordinary Id dispatch still enters the selected Method directly.

The added state belongs to Execution: an optional candidate and owner on its
registration, a scoped stack of pending recovery requests, and retained copies of
the four installed control Methods needed by the existing guards. The constructor
uses Id's existing Method comparison to recognize captured aliases and the retained
descriptor-copy operation to avoid sharing a mutable executable owner. Id's existing
`nativeBindingsMatch:` checks current controls without foreign payload access.
Integer, Float and ObjectModel construct graph nodes through their current messages;
ObjectModel and GC confirm aggregate layout and reference ownership. Id is unchanged.

The independent C callbacks exercise direct results, decline, flat and graph
recovery, balanced reentry, native suspension rejection, duplicate consumption,
malformed recipes and invalid post-recovery acknowledgement. They also cover
Machine control replacement and forwarding custom lookup, candidate-owner lifetime,
distinct equal number boxes, alias preservation and exact NaN bits. A replaced
Integer factory collects between graph-node allocations. The native frame-view
test and complete Oop core gate also pass. The installed portable execution
fixtures pass unchanged, including the original projection assertion.
[Receipt](../measurements/pending-call-recovery-oop-20261026.json).

This is explicit recovery acceptance. It does not claim migration of every retained
compiler optimization producer or native harness, or fresh cold qualification of
the corrected runtime.

## ObjectModel reference inventory through Oops

The retained `object_model_slots` harness now uses Oop messages, declared C
callbacks and scoped result roots. It preserves the checks for metadata-only
inventory, private-vtable inheritance, live layout changes during Integer
construction, reentry, exact failure propagation and release of query protection.
Allocation identities check reclamation without passing dead references to Id.
The fixture stores the target and detached vtable weakly, so it cannot conceal
missing runtime roots. Its parent-getter replacement forwards through an explicit
Method alias, allowing the collector to call it without allocating an argument
sequence. No runtime or Id change was needed.
[Receipt](../measurements/object-model-slots-oop-20261026.json).

## Remaining installed CLI checks

All checks after portable execution in the installed Host CLI group pass on the
staged prefix containing the root-provider, frame-view and recovery corrections.
Workers pass copied values, application/transfer errors, asynchronous requests,
zero-budget behavior, timeout/crash, wire framing, orphan reaping and authored
service cancellation. Project checks pass source diagnostics, build/run/install/test,
publication reuse and repair, path dependencies and pinned Git dependencies. The
three native Host callback checks and the project-query contract also pass under
collection. The 15-second worker startup deadline is unchanged.

This closes the checks that the earlier CLI run could not reach. It does not
combine different runtime prefixes into a full-group qualification: the corrected
runtime still needs its own fresh installation and complete acceptance run.
[Receipt](../measurements/remaining-cli-oop-20261026.json).

## Ordinary slot callbacks through Oops

The `object_model_scopes` harness now uses Oop messages and declared C callbacks.
It replaces the current Integer conversion owner, collects during conversion,
reenters the slot adapter and compares the exact reported roots before and after
the nested call. Success and failure restore the previous root inventory, raw
slot access remains a replaceable send, and the exact injected failure reference
is preserved. Once calls end, allocation-identity observations confirm that the
target, stored value and index are reclaimed. The test fixture holds these objects
only weakly. No runtime or Id change was needed.
[Receipt](../measurements/object-model-scopes-oop-20261026.json).

## Cold-candidate image acceptance

All 16 checks in the installed Host image group pass on the original cold
candidate `28a2f117`: optional Store participation, source-free startup and native
loading, Trace, frame calls, collection search, StringBuilder, OS services,
Alien recipes and publication, hashing, collecting reentry, and saved Host entry
roots. This completes that candidate's image run. It predates the root-provider,
frame-view and pending-call recovery corrections and does not qualify them.
The corrected candidate is being built independently from `52c617fe`.
[Receipt](../measurements/cold-image-oop-20261026.json).

## Corrected runtime from a fresh cold installation

Revision `52c617fe`, including the root-provider, frame-view and recovery changes,
builds from a clean detached checkout with fresh build directories and no installed
Birnam on PATH. Its schema-4 receipt verifies all 817 installed files, Id protocol 2
and exclusion of the build-only Lang seed. An archive of those exact bytes passes
signature, relocation, source/GC/image and compiled-startup checks while both the
original checkout and installed prefix are hidden. Both paths were restored after
the check. Complete installed Host acceptance remains pending. Initial concurrent
CLI and image runs hit existing 300-second subprocess limits; their failures are
retained for retries at lower concurrency. Generation and interaction now pass;
the task group reached failing multicore assertions and is under diagnosis. The
cold/archive result does not substitute for full Host or native acceptance.
[Receipt](../measurements/corrected-cold-install-oop-20261026.json).

## Globals ownership and partial image loads through Oops

The `owned_globals` harness now uses Oop callbacks and explicit optional Store
loading. It preserves conditional slot updates across a reentrant edit and GC,
exact failure propagation after a dictionary removal, retained callback changes
after partial image application, mapped/rebuilt recovery, alias and key identity,
nil bindings and independent Store descriptions. Allocation identities confirm
that Globals keeps binding keys/values alive and releases them after removal.

The old fixture assumed that the Id API receiver was the vtable metatable. Its
replacement removal Method therefore never intercepted the actual image edits.
The migrated hook selects the System vtable's real metatable and asserts that the
original Method exists. The obsolete Id `bindingOf:in:` assertion now uses the
existing Execution `directMethodFor:in:` query. The exact failure and partial-load
assertions remain intact; no runtime or Id operation was added or changed.
[Receipt](../measurements/owned-globals-oop-20261026.json).

## Integer policy through Oops

The `integer_values` harness now uses scoped Oop calls on the corrected cold
candidate. All 500 signed-boundary arithmetic cases pass, along with mixed Float
dispatch, overflow/zero-division/type errors, Error identity, repeated policy
loading and collection during arithmetic. Named Method reflection obtains the
authored body through Codegen's existing `implementationOf:` query, then inspects
that body's CodeObject. No native-entry carrier interface or runtime change was
needed. Full corrected-candidate Host acceptance remains incomplete.
[Receipt](../measurements/integer-values-oop-20261026.json).

## Pending Boolean native-boundary parity

This checkpoint is superseded by the incoming-adapter connection below. Its failing
receipt remains evidence of the earlier candidate.

The Boolean harness now uses Oops and reaches all its retained checks. Truth
tables, chosen-only callbacks, opaque result identities, reflection, Module reload
stability, pure Word identity selection and collecting replacements pass before
the final failure summary. Ten native-boundary checks still fail: malformed
argument/null rejection and native failures from selected callbacks or replaced
selection. These expectations remain active in the test; this is not completed
Boolean acceptance.

The authored Oop Boolean policy removed its former explicit incoming adapters.
Ordinary source evaluation now handles failures where the retained native contract
requires rejection or exact native propagation. A trial global strict-arity change
was rejected: the complete core gate showed that it breaks System `load:`'s retained
missing-argument nil result. Both production changes were reverted. Restoration
must belong to the explicit incoming boundary and preserve selected-branch
suspension; reinstating a synchronous deferred call is not sufficient. Id is
unchanged, and migration relying on this missing native contract remains pending.
[Evidence](../measurements/boolean-boundary-pending-oop-20261026.json).

## Corrected-candidate generation and interaction acceptance

The fresh corrected candidate `52c617fe` passes all 17 generation checks and all
6 interaction checks, including mapped/rebuilt adoption and rollback, 62 compiled
unit policies, collecting source-registry operations, persistent sessions, all 26
REPL golden cases and LSP diagnostics/versions/close behavior. These results do not
resolve the separate multicore failure or Boolean native-boundary gap. The two
initial CLI/image timeout failures remain queued for lower-concurrency retries.
[Generation receipt](../measurements/corrected-generation-oop-20261026.json),
[interaction receipt](../measurements/corrected-interaction-oop-20261026.json).

The unchanged multicore smoke passes all 27 assertions when rerun after the other
acceptance groups have stopped. The original 11 failures are retained as evidence;
one successful rerun does not establish their exclusive cause. The full task group
is being retried serially with the same assertions, request limits and 15-second
worker startup deadline. No runtime or performance adjustment was made.
[Isolated multicore receipt](../measurements/corrected-multicore-isolated-oop-20261026.json).

## Continuing incoming adapters and Boolean parity

The explicit incoming adapter now preserves Boolean's retained native contract
while deferring the chosen branch through Id's existing continuation messages.
Before this change, Boolean used ordinary source Blocks after its incoming
signatures were removed; malformed native calls and callback failures could become
ordinary language results. The existing adapter's deferred call also used a
synchronous send. After this change, Boolean again declares its signatures. The
adapter validates and converts arguments, evaluates its selector body within the
existing synchronous native scope, restores that scope, then returns prepared Id
work for the selected branch. No native frame remains around that branch.

Execution owns the immutable signature and arity facts. Its existing weak
admission table now also records native-adapter contexts; retirement removes those
keys before address reuse. Signature data contains no Oop references. The new
owner query `Execution nativeSignatureOf:` returns fresh bytes for an adapted Block
and nil for other objects, without querying an arbitrary implementation's metadata
or reading foreign payloads. Codegen uses this query during Method construction,
chooses the continuing entry and leaves native argument validation to Execution.
Ordinary named Methods retain their existing arity behavior, including System
`load:`'s missing-argument nil result. Id and its interface are unchanged.

Failure observers use a captured answer Method containing the original answer and
receiver. Execution records that Method as an alias of its own pending-call
continuation when applicable, keeping registered branches on the caller's Machine.
The alias uses the existing weak return table. No Id continuation representation
is inspected. Unmarked Error/Fault data remains data; native failures retain their
exact identity. Global Block value replacement remains observable.

The Boolean test now passes all its retained checks. Additional acceptance covers
immutable signature snapshots, arbitrary implementation objects, captured and
rebound Methods, observers on adaptation and callback failures, branch suspension
with and without an observer, failure-policy isolation while parked, collection,
and mapped/rebuilt image recovery. Block arity, Integer policy, pending-call
recovery and the complete Oop core gate also pass. This is staged-runtime evidence;
the cold candidate `52c617fe` does not include this later adapter correction.
[Receipt](../measurements/native-boundary-oop-20261026.json).

The complete installed Host task group also passes when rerun serially on cold
candidate `52c617fe`: task images/fields, 19 service checks, all 27 multicore checks,
closure observation and all 177 task-control checks. Code, assertions and the
15-second worker deadline are unchanged from the concurrent run. This qualifies
that candidate's task group; it predates the incoming-adapter correction above.
[Serial task receipt](../measurements/corrected-tasks-serial-oop-20261026.json).

The complete CLI group passes serially on the same cold candidate with unchanged
limits: exact installed manifest/relocation, native owners/backends/compilers,
artifact and language checks, Alien, optional-provider separation, ImageFormat,
documentation, portable execution, workers and project operations/queries. The
earlier concurrent timeout remains recorded. The image group is the remaining
full-group retry for this candidate, which predates the incoming-adapter change.
[Serial CLI receipt](../measurements/corrected-cli-serial-oop-20261026.json).

## Incoming byte snapshots before conversion callbacks

The migrated `native_incoming` harness exposed a retained ordering requirement:
the Oop adapter converted a Word before reading a later byte argument, so a
replaced Word factory could alter the bytes delivered to the body. Execution now
rejects required null inputs first, reads every byte view through its public owner
messages, and only then invokes Word/Bytes factories and the body. The snapshots
are private call-local byte vectors; no foreign payload is read and no Id operation
or representation catalog is added.

The independent C fixture mutates byte inputs and collects from both Word and
Bytes factories. The original snapshots reach the body in both cases. Full-width
Word values, nullable results, captured Method reuse, independent signature copies
and rejection before conversion effects also pass. Its loader observer proves
Execution initialization before Integer/String/Float/Vector/Map and explicitly
loads PortableCode at that same early point; the normal later load reuses the
initialized owner. This tests early capability without changing production load
order. Boolean acceptance and the complete Oop core gate pass as well.
[Receipt](../measurements/native-incoming-oop-20261026.json).

## Collecting value initialization preserves replacements

The `numbers_loading` harness now observes loading through Name messages and
declared Oop Methods. It enables precise collection with a one-byte budget before
ObjectModel initializes, keeps that pressure through native value construction and
extension installation, and restores the ordinary budget after PortableCode.
Authored startup follows at its normal budget. This matches the retained native
construction scope in the current load order without changing production startup.

Integer `+` and `to:` and String `size` and `split:` replacements survive the
remaining initialization and repeated Integer policy loading. Untouched mixed
arithmetic and String-to-Integer parsing still work. No runtime or Id change was
needed; the removed Id `memoryManager` query was replaced by the existing Namespace
GC binding in the test fixture.
[Receipt](../measurements/numbers-loading-oop-20261027.json).

## Primitive values and explicit outbound adapters

The `native_values` harness now uses Oop values and a declared C callback. It
preserves wrapping arithmetic, unsigned comparison/division, full-width rendering
and overflow, byte bounds, copy independence, live factory replacement under
collection and exact failure identity. The callback mutates the original Bytes
object before reading its argument through owner messages; the explicit outbound
adapter's snapshot remains unchanged. A later copied result also survives further
mutation and collection. No runtime or Id change was needed.
[Receipt](../measurements/native-values-oop-20261027.json).

## Numeric values, text conversions and collecting dependencies

The `numbers` harness now exercises its retained integer and IEEE Float matrices
through Oop messages, including bounds, signed zero, NaNs, mixed arithmetic,
rounding, exact-family equality and Error propagation. Text indexing/parsing and
builder checks remain, as do collecting replacements of numeric and String
constructors, inclusive range callbacks at the i64 limit and collection-pressure
checks. A declared C observer compares allocation identities to confirm that raw
numeric bits do not retain an object at that address, even after assigning the
Integer a private vtable. No runtime or Id change was needed.
[Receipt](../measurements/numbers-oop-20261027.json).

## String, builder and Error ownership

The `text_error` harness now uses Oop messages and declared C callbacks. Its
retained checks cover binary text, independent clones and byte copies, printing,
replacement, trimming, symbols, builders, live factory replacement, collecting
callbacks, Error propagation and constructor defaults. A collecting allocation
callback mutates the source byte provider after String snapshots it; construction
and cloning preserve the original bytes.

The ownership checks follow the current representation: String owns native byte
vectors behind zero-payload handles, so there is no foreign payload to corrupt or
separate managed Bytes backing to count. ObjectModel slot access is rejected;
private vtable changes preserve the bytes. Allocation identities prove message
retention by Error, reclamation after releasing roots and reclamation of 32
String/builder pairs. All 64 finalizers reject construction, observe no work from recursive
collection, and forward successfully. Prototypes survive releasing test roots,
and a copied String Method rejects an unrelated receiver. No runtime or Id change
was needed. [Receipt](../measurements/text-error-oop-20261027.json).

## Persistent collections and collecting callbacks

The `collections` harness now uses Oops while retaining its independent Rust
sequence comparisons: Vector growth across tail and tree boundaries, persistent
snapshots, stable sorting, folding, Map replacement and collisions, key-family
semantics, inclusive range endpoints and binary-safe String splitting. Callbacks
still collect and replace their own behavior during iteration; Map equality still
reenters insertion. Factory replacements remain observable.

A declared C fixture records allocation identities without retaining the observed
objects. It verifies that Vector protects earlier sort keys across later collecting
callbacks, then releases all keys after success or exact native failure. Folding
releases intermediate answers while retaining the returned answer. Collection
references retain String keys/values until their holder is released. Numeric bits
that match an object's address do not retain that object, including after private
vtable changes. Malformed reference-slot checks still reject invalid Map/Vector
storage. No runtime or Id change was needed.
[Receipt](../measurements/collections-oop-20261027.json).

## Restore native Block failure propagation and System output acceptance

Migrating `system_output` reproduced a regression: a native failure from the
current String factory became ordinary Error data when called through authored
Object `printString`. The unchanged pre-Oop harness passes on installed revision
`a271e4c5`; an independent probe there also confirms that a native caller of a
Block receives the native failure while source evaluation returns Error data.

The Oop Block entry added an answer continuation that called `language_result`.
That conversion belongs to language Machine completion, which already performs
it. The operation sequence is now:

```text
before: Block value -> prepare body with an extra answer -> convert failure -> caller
 after: Block value -> prepare body with the caller's answer -> caller
```

The extra answer implementation, its context allocation and its stored Method
are removed. No message, registry, representation or Id operation is added.
Execution continues to interpret its own Block fields and admitted arity; Id
still enters selected Methods directly. Wrong-arity rejection and current Block
method replacement remain observable. Machine pending-call completion retains
language Error conversion. A focused check distinguishes that conversion from
exact native failure propagation through synchronous compiled Block code.

The earlier Oop core fixture expected the added conversion on a direct native
Block call. Its expectation is corrected against the pre-Oop runtime, rather
than treated as authority over the retained behavior. Error data and marked
failure returns remain separate assertions; a fixed injected failure also checks
exact identity. The System output harness retains its exact binary/Unicode stdout
oracle, collecting factory/byte callbacks, nested printing, reload/replacement
and receiver lifetime checks. The obsolete raw-word rejection becomes a supported
immediate Word output check plus a null-argument rejection check.

The complete Oop core gate and the Block, Boolean, native-boundary, pending-call
recovery and System output suites pass. They include the forbidden-dispatch
oracle, 177 task-control checks, suspension and mapped/rebuilt image recovery.
[Receipt and baseline comparison](../measurements/block-native-return-oop-20261027.json).
The cold candidate `52c617fe` predates this correction and the incoming-adapter
fixes; full installation qualification for these later changes remains pending.

## Stdin, environment and file services

The `system_services` harness and its file checks now use Oop arguments/results
and declared collecting callbacks. Their retained checks cover binary and long
stdin lines, CR/EOF/error handling, environment encoding and missing values,
file roundtrips/truncation, invalid inputs without write effects, source calls,
late Error factory publication, native failure identity and replacements across
module reloads. A contents callback mutates the earlier filename byte-provider
Oop under collection; the write still uses its original filename snapshot. No
runtime or Id change was needed.
[Receipt](../measurements/system-services-oop-20261027.json).

## System allocation queries and GC controls

The migrated `system_gc` harness exposed a bare-Bootstrap regression before any
test replacement: `System liveObjects` sent `allocationCount` to the language
`Vtable` metatable, which is now distinct from the Id API receiver. The unchanged
pre-Oop harness passes on installed revision `a271e4c5`.

System now asks GC for `allocationCount`, alongside GC's existing accounting
queries. That one new GC method sends the existing Id `allocationCount` message;
System then sends `fromWord:` to the current Integer factory. The sequence changes
from `System -> Vtable allocationCount -> Integer` to
`System -> GC allocationCount -> Id allocationCount -> Integer`. Every lookup
remains live. There is no new Id operation, global binding, payload interpretation,
registry or cross-module implementation access.

Acceptance checks default queries, replacements at both GC and Id, constructor
selection after the query, full-width values, aliases and exact failures. The
remaining GC matrix retains conservative/precise payload modes, missing layouts,
numeric and Range references, tracing-time policy/layout changes, failure before
sweep, native stack roots, automatic-collection accounting and finalizer reentry.
The fixture writes ObjectModel slots through its public operations. String and
builder handles now have zero payload bytes, so their obsolete mirror-word writes
become rejected slot-write checks; native text bytes never retain address-shaped
objects in either mode. Allocation identities replace queries on freed Oops.

All three System suites and the complete Oop core gate pass on the updated staged
prefix. The GC fixture uses pre-interned selectors for exact charge comparisons,
so Host helper allocations cannot change the measured interval.
[Receipt and baseline comparison](../measurements/system-gc-oop-20261027.json).

## Complete corrected-candidate Host image acceptance

The serial image retry on cold candidate `52c617fe` passes all 16 checks, including
optional Store, Host/Image, native loading, authored Trace, frame calls, collection
search, StringBuilder, OS services, Alien recipes/publication/reentry/hashing and
saved-code startup. Existing assertions and limits were unchanged; the original
concurrent timeout remains recorded.
[Serial image receipt](../measurements/corrected-image-serial-oop-20261027.json).

Together with the recorded generation (17), interaction (6), task (6) and CLI (18)
groups, this completes all five Host acceptance groups for that specific cold
candidate. No qualification job remains live. It does not qualify the later
incoming-adapter, native Block-return or GC-query corrections. Those have staged
core/focused evidence and still need a fresh cold installation and full Host
acceptance. The remaining native test harnesses and retained compiler features
also prevent declaring the overall migration complete.

## Method binding through source, artifacts and images

The `method_binding` harness now uses Oops. The unchanged authored fixture builds
independent Methods from a template; receiver/argument identities and selected
bodies survive collection, artifact loading, and source-free mapped/rebuilt images.
Recovered Methods remain usable templates, and replacing a template's constructor
is observed by the next binding under collection.

The removed NativeEntry-based Method factory is no longer used as a test receiver.
Bare native and compiled-root Method Oops do not own `withImplementation:`; their
unsupported send returns `doesNotUnderstand` Error data. Valid template constructors
separately reject wrong arity. Neither case manufactures an invalid binding.
No runtime or Id change was needed.
[Receipt](../measurements/method-binding-oop-20261027.json).

## Method construction policy, reentry and prepared publication

The `method_invocation` harness now uses declared Oop callbacks and allocation
identities. Both the seed constructor and installed authored policy construct
sixteen distinct Methods without another TCC unit. Their retained Methods are
reclaimed after removal and scope/cache release. Live owner, factory and template
replacement, Error identity, native-failure conversion, refusal without
publication, nested construction, conditional cache updates without replay, and
delegated Reflect overrides pass. Templates use the existing declared Codegen
entry, so replacing TCC compilation must have no effect on their construction.

The old whole-Id `bindingEpoch` equality assertion failed: constructing a neutral
template and bound Method adds ten real private-vtable edits. Current candidate
preparation stores source and binding observations; the epoch protects coherent
reads rather than remaining frozen for a preparation token's lifetime. The
replacement acceptance exercises the intended feature directly in a fresh
full-language process: prepare a candidate, derive a Method, clear templates,
collect, validate the certificate, commit the original token, and roll back.
All fourteen checks pass. Globals and Store revision equality and the unchanged
target binding remain separate assertions.

No runtime workaround, registry or Id change was added to suppress those edits.
The historical baseline flag now selects the current seed constructor, not a
pre-Oop runtime. The emitted timing lines are diagnostics; resident-byte checks
remain conditional on the optional TCC metric.
[Receipt](../measurements/method-invocation-oop-20261027.json).

## Artifact templates and deferred image bodies

The `artifact_templates` harness now uses Oop callbacks. Synchronous and resumable
artifact bodies load from one supplied native unit with source emission disabled;
sixteen bindings retain independent receiver/argument/body behavior. Current
constructor refusal and collecting reentry remain observable. Named image bindings
now use declared Codegen templates rather than separately emitted C adapters, so
the test checks current constructor selection and distinct equal-arity template
objects instead of obsolete adapter-payload coalescing. Weak allocation identities
prove that staging alone retains the selected templates and cancellation releases
them. Historical activation measurement labels remain available to the existing
benchmark script; this checkpoint makes no timing-regression claim.

The deferred rebuild check exposed a result-convention regression. The pre-Oop
runtime answers `nil` when source emission rejects a body, but the Oop path
returned the Error object. The rebuild branch now restores `nil`, retaining its
pending request for retry. Loader and initializer Errors keep their identity,
and native failures keep their marker and identity. This changes one return path;
it adds no Id operation, callback, registry or other mechanism.

Acceptance also checks no eager body mapping, current loading/emission callbacks,
nested-cancellation refusal, success-only caching, independent dispositions,
retained code after stage cancellation, final code retirement, and deferred,
repeatable rejection of corrupt object data without source fallback. The copied
pre-Oop harness passes with only fixture-path adjustments. The complete Oop core
gate and source/artifact/mapped/rebuilt Method-binding suite pass on the corrected
staged runtime. [Receipt](../measurements/artifact-templates-oop-20261027.json).

## Portable materialization rejection and retry

A focused Oop harness restores the retained `nil`/`lastError` convention for
unadmitted code, rejected portable semantics, mismatched captures and compilation
refusal. Previously some branches omitted or changed the diagnostic, returned
ordinary Error data, or raised a native failure. Codegen now records the existing
mode-specific diagnostic and returns `nil` for these cases. Native failures still
propagate unchanged; the final Block constructor's own Error result retains its
identity. No Id operation or inter-module implementation access is added.

Both execution modes pass rejection, collecting replacement, exact native failure,
no-cache-on-failure, successful retry/diagnostic clearing and native-unit retirement
checks. The new target is included in the native-message gate. The pre-Oop full
materialization test, complete Oop core gate, artifact suite and Block arity suite
also pass in their respective runs. This does not mean the old full materialization
harness has been migrated. [Receipt](../measurements/materialization-errors-oop-20261027.json).

A separate source-level probe reproduces a captured-Block compatibility gap:
`methodFor: #value:` captures the new shared Block dispatcher rather than the
original Block body. Invoking that Method on another compatible Block consequently
runs the other body's code. The same source prints `73` before the refactor and
`19` on that staged runtime. The correction below supersedes this checkpoint.
[Probe](../measurements/block-capture-pending-oop-20261027.json).

## Capture a Block's body without bypassing current lookup

Execution's `methodFor:` now preserves the pre-refactor Block capture behavior.
Previously it captured the shared value dispatcher; invoking that Method on another
Block ran the other receiver's body. Reflection now performs current lookup first.
For Execution-owned Block storage whose selected Method is the standard value
implementation, it captures the stored body Method. Otherwise it captures the
selected Method unchanged.

Execution retains one context-bound snapshot of its own declared value Method and
compares it through the existing `sameNativeMethod:as:` message. It interprets only
its own Block fields. No registry, inter-module private representation, new Id
operation or ordinary-dispatch step is introduced. `methodFor:` remains inherited
and replaceable; no Block-specific shadow is installed.

The original source probe again prints `73`. The regression covers source and
both materialization modes, code retention after the original Block/cache release,
transfer to an alias, and retirement after final removal. Counterexamples cover
current value replacement, custom lookup, foreign receivers using the same handler,
and replacement of `methodFor:` itself. Core, arity, native-boundary, materialization
error and mapped/rebuilt Method-binding checks pass. The prior failing probe remains
as historical evidence. [Receipt](../measurements/block-method-capture-oop-20261027.json).

## Complete portable materialization acceptance

The full `materialization` harness now uses Oops and declared callbacks, with TCC
resource metrics enabled in a separate copied prefix. It retains canonical-code
identity, both execution dispositions, independent and nested captures, exact NaN
bits/binary text, lexical scope, capture-read ordering, live replacements, failed
construction cleanup, the 64-entry LRU bound/order, suspension across eviction,
alias ownership and exact native-unit/byte retirement.

The migration exposed three remaining compatibility gaps. Execution now answers
nil for unavailable capture inspection and invalid indices. Synchronous capture
reads go through the restored `Block captureWord:of:` owner entry: unsupported
receiver storage returns nil, while valid Blocks use their current capture reader.
Resumable entry validates its required owned capture storage before reading it.
Generated initialization also stops immediately when CodeObject admission returns
nil, so refused code cannot be published as executable.

All representation-specific work remains with Execution, PortableCode and the
compiler owners. No Id operation, ordinary-dispatch step, private cross-module
representation or new registry is added. An attempted result-conversion wrapper
on explicit invocation was rejected because it changed native recovery control
failures. It is absent from the final implementation. The harness instead checks
raw Oop invocation and the corresponding real language caller: native failure is
preserved, and the language Machine produces Error data at its pending call.
Ordinary Error values held in captures remain data.

Full materialization and recovery pass, as do seven adjacent artifact, capture,
arity, error, native-boundary and Method-binding/image suites and the complete core
gate. This supersedes earlier notes that the full materialization harness remains
unported; other native/compiler migration and latest cold-installation qualification
are still pending. [Receipt](../measurements/materialization-full-oop-20261027.json).

## PortableCode construction and ownership

The `portable_code` harness now uses Oop values and declared collecting callbacks.
It rebuilds every portable-program golden fixture through the public constructors,
checks the independent digest, and inspects all node, binding and statement forms.
Provenance updates, immutable identity, invalid-input messages, derived factories,
raw-allocation rejection and current dependency replacement remain covered.
Allocation identities prove retention through owner edges and reclamation after
anchor release without querying freed Oops.

The Oop initializer returns its original CodeNode prototype rather than the removed
Unit carrier. Reload still preserves an explicit Namespace replacement. No runtime
or Id change was needed. [Receipt](../measurements/portable-code-oop-20261027.json).

## Source loading and live registration dependencies

The `source_loader` harness now uses Oop arguments/results and declared callbacks.
It retains Module load-order/identity checks, exact source-revision hashes,
descriptions of nested declarations without execution, duplicate-source identity,
registration-before-evaluation, Namespace-only loaded notifications, diagnostic
ordering and registered-source retention after caller-root release.

Collecting callbacks cover parser, String construction, source revision,
registration and evaluation. Reentrant equality preserves nested registration;
mutating a path byte-provider during source inspection cannot redirect the earlier
file read. Native failures and ordinary registration Errors retain their distinct
results, and registration refusal prevents evaluation. Compiler construction
refusal and Module reload/replacement checks pass. No runtime or Id change was
needed. [Receipt](../measurements/source-loader-oop-20261027.json).

## Native loading and initializer results

The native-loader harness now uses Oop messages and an independent C fixture.
It retains the argument-order, collecting callback, replacement, failure-identity,
initializer lifetime and source-loading checks. Explicit language nil and immediate
Word results are valid Oops; native absence retains the receiver-returning convention.

Two compatibility gaps were repaired. Bootstrap previously tried to append native
absence to its result Vector; it now emits only the loaded status when the initializer
has no result. NativeLoader returns its actual receiver for that record, while a
present result, including language nil, is returned unchanged. NativeLoader also
copies the path through `Bytes fromBytes:` before argument callbacks, so a callback
cannot redirect loading by mutating a replaced String byte provider. The sequence is
path conversion and copy, argument collection, then native loading.

Bootstrap owns the loading result record; Bytes owns the copy and NativeLoader owns
language policy. These changes add no Id operation, dispatch step, registry or shared
private representation. The old fixture-build step was removed from the native gate;
the migrated test compiles its own Oop fixtures. Core dispatch and compiler/control
checks, native loading, source loading and explicit Store invalidation pass.
[Receipt](../measurements/native-loader-oop-20261027.json).

## Cold installation checkpoint after materialization acceptance

Revision `9f638b73` passed a fresh cold installation with no Birnam executable on
PATH and no build cache, followed by archive relocation with its original source
and installation paths hidden. The published layout contains exactly 820 recorded
files plus its receipt, excludes the Lang build seed, and uses Id protocol 2.
Serial Host acceptance is still running. This checkpoint predates the native-loader
fixes above and does not qualify those fixes or the remaining explicit compiler
features. [Receipt](../measurements/current-cold-install-oop-20261027.json).

## Embedded files and standalone generated libraries

The embedded-file harness now uses Oop messages and independent declared C
callbacks. It preserves binary data, prototype/private-vtable behavior, lexical
shadowing of System, compile-time-only file access, current Globals and dependency
replacement, exact native failure identity, path/size diagnostics, and payload
ownership after generated code retirement. Allocation-identity observations avoid
querying dead objects. String now owns its bytes directly, so the constructor checks
observe one String construction, one `asBytes` and one Bytes construction instead
of counting the removed managed Bytes backing allocation.

Standalone generated-library initialization had assumed that every module object
provided TCC's `declaredMethodAt:` and `retain:`. The emitter now restores the
explicit retention-owner argument used by standalone callers. With Namespace and
that owner, it obtains declarations through the native image's existing
`newMethodAt:` and retains registered Methods through the supplied owner's
`retain:`. TCC's Namespace-only form still uses its Unit for both operations.
There is no added Id operation, owner registry or private cross-module access.

System-compiled libraries pass after their source C and embedded input files are
removed, for both synchronous and resumable entries and nested Blocks. The core
gate, materialization rejection/retry and source-loading checks pass as well.
Full current cold qualification and the remaining native/compiler migrations are
still pending. [Receipt](../measurements/embedding-oop-20261027.json).

## Source compiler, control and native code lifetime

The general compiler checks now run as `compiler_source` through Oop messages,
without Store. The nine optimization groups remain in the existing `compiler`
target and are still pending migration; both targets remain in the native gate.
This split moves the general assertions rather than dropping the pending groups.

The migrated checks retain source reflection, arithmetic, chaining, closures,
captured-Method replacement, syntax/native compilation rejection, failed module
admission, collecting constructor replacement, suspended-body replacement,
boundary exit and abandoned-frame cleanup. Exact native-unit and relocated-byte
counts return to their pinned bootstrap baseline.

An independent C fixture compiled by TCC exercises actual pending-call recovery.
It removes its own binding, collects and checkpoints retirement, then publishes a
marker while still executing. The caller resumes once with 43; the candidate
mapping retires afterward, while the baseline stays live. Initializer reuse and
conflicting inputs retain their checks. The fixture gives its target a private
vtable through Id instead of assuming Blocks have separate vtables, and retains
TCC's declared Method context rather than overwriting that owner's state. The
obsolete v1 recovery fixture is removed.

The migrated target and the broader pending-call recovery target pass. No runtime
or Id change was needed. [Receipt](../measurements/compiler-source-oop-20261027.json).

## Cold candidate generation acceptance

All 17 installed Host generation checks passed on cold revision `9f638b73`,
including source/description restoration, ordinary editing, adoption and rollback,
mapped/rebuilt/resaved images, console/TCP activation and compiled-unit policy.
Interaction, task, CLI and image groups remain in the serial queue. This receipt
does not cover the subsequent native-loader and standalone-emitter corrections.
[Receipt](../measurements/current-generation-oop-20261027.json).

## Compiler allocation accounting

The compiler accounting group now runs within `compiler_source` through Oops.
An independent C fixture records allocation identities and requested payload sizes
around parsing, Method construction and execution, and observes successful external
charges through the current GC method. Rust checks that no collection occurred and
calculates physical storage using the independent header-and-padding formula.
The measured window excludes Host argument snapshots and fresh selector Names.

Physical pressure is compared after the established `prepareAllocation` checkpoint.
An experiment that reconciled it in the `allocationBytes` query was rejected by
the core replacement test: it overrode a user-supplied charge of 1 with the physical
size of 8. The query remains a policy observation and the runtime is unchanged.
Default physical-plus-external accounting, custom charges, collection inside a
charge callback, and reset after collection all pass. Eight compiler groups remain
pending. [Receipt](../measurements/compiler-accounting-oop-20261027.json).

## Compiled global bindings and explicit Store history

The global-binding group now runs through Oops in `compiler_source`. Its earlier
source/control/lifetime phase runs without Store; this group explicitly loads
Store and Relational. Checks retain Globals-first reads, nil-to-Namespace fallback,
missing names, explicit nil bindings, top-level publication and lexical isolation.
Already compiled readers see current bindings and collecting getter replacements.

Source assignment remains independent of Store descriptions. Explicit history and
relational queries preserve their values across later source assignments. Collecting
setter replacements retain exact Error/nil refusal results and native failure
identity, skip later effects and preserve preceding writes. A successfully published
Block survives collection through its global binding; its mapping and native bytes
are reclaimed when that binding is released.

The complete source target passes with these checks included. No runtime or Id
change was needed. Seven compiler groups remain pending.
[Receipt](../measurements/compiler-globals-oop-20261027.json).

## Compiled Methods and explicit publication policy

The compiler publication group now runs through Oops in `compiler_source`.
Collecting callbacks verify that ordinary Reflect definition/removal never call
Store publication, authorization or notification, even when the authorizer fails.
Explicit Store publication still calls its current hooks. False, nil, Error and
native failure denials preserve the previous binding and description without
retaining additional generated code.

Notification sees the committed Method and its Store description. Its native
failure leaves that commit intact, while a previously captured Method still runs
its old body. Replacing the authorizer and observer changes the next send. Explicit
removal releases the description and binding, and native-unit/byte counts return
to baseline. The test's callback state releases its submitted Method after use.

The full source target, explicit-invalidation and publication-contract targets
pass. No runtime or Id change was needed. Six compiler groups remain pending.
[Receipt](../measurements/compiler-publication-oop-20261027.json).

## Generated dispatch and current dependency effects

The compiler dispatch group now runs through Oops in `compiler_source`. Cold and
repeated execution preserve the effects of a replaced Integer constructor; replacing
addition changes the next compiled call. Collecting replacements of integer
conversion and Machine frame reads remain observable. The conversion check observes
the callback before the test itself converts the answer, so client-side inspection
cannot satisfy it accidentally.

Larger generated bodies and repeatedly invoked capturing Blocks retain their code
and release all extra native units and bytes when their roots disappear. A failing
test Method guards the absent legacy `fromNative:owner:` selector on Id's existing
Method behavior; ordinary compilation must never enter it. The test does not
reintroduce a Method factory or raw native-entry carrier.

The complete source target passes. No runtime or Id change was needed. The five
pending compiler groups are discarded-clone and numeric-predicate producers,
alias-return production, call-window preparation and authored call preparation.
[Receipt](../measurements/compiler-dispatch-oop-20261027.json).

## Alias-return producer experiment: not enabled

An attempted Oop producer reused the pure alias-return planner, declared a candidate
Method and attached it through existing `withInline:at:owner:`. Semantic identity,
current constructor effects, changed frame-reader fallback and bounded frame views
passed. Diagnostic tracing confirmed that the candidate was entered and returned
its input; it was not merely falling back.

Three runs without diagnostics used the same 177,480 Id allocations per variant
across 180 calls. Candidate elapsed time was 8–11% higher. These are paired local
observations under concurrent Host qualification, not a language-wide benchmark.
Immediate Oop indices removed the old source of allocation savings in this case.

The experiment is reverted; neither Id nor Execution was changed. Its patch and
measurements are retained for review. A user clarification is pending on retiring
the automatic compiler optimization versus investigating another implementation.
Explicit inline candidates and recovery remain supported. The five-group pending
count is unchanged until that decision is made.
[Evidence](../measurements/inline-return-rejected-oop-20261027.json).

## Cold candidate interaction acceptance

All six installed Host interaction checks passed on cold revision `9f638b73`:
Socket and REPL owners, parser/compiler-free published startup, interactive CLI
and project sessions, REPL goldens including mapped images and Alien wrappers,
and project LSP behavior. The task, CLI and image groups remain in the serial
queue. This receipt predates the later native-loader and standalone-emitter fixes.
[Receipt](../measurements/current-interaction-oop-20261027.json).

## Current binding proofs through Oops

The `binding_proof` harness now supplies receiver/selector/Method triples rather
than byte packets containing native addresses. It checks direct and inherited
selection, different declared implementations and contexts, captured aliases,
all 32 batch rows and incomplete-row refusal. Replacing a binding or restoring
lookup changes the next result.

Custom method lookup on a later row or on Id's receiver makes the proof decline
without calling that lookup. Captured proof and restoration Methods are installed
on a separate test receiver so the test can inspect and restore Id through normal
messages while Id's own lookup differs. Replacing the public proof operation also
makes a captured proof decline without invoking the replacement. No raw entry call,
private Method data or runtime change is introduced.
[Receipt](../measurements/binding-proof-oop-20261027.json).

## Cold candidate task acceptance

All six installed Host task checks passed on cold revision `9f638b73`, including
mapped/rebuilt task images, fields, source-free workers, concurrency service and
cleanup behavior, closure observations under both collection modes, all 27
multicore checks and all 177 task-control checks. The worker-ready deadline remains
15 seconds. CLI and image groups remain in the serial queue. This receipt does not
cover the subsequent native-loader and standalone-emitter corrections.
[Receipt](../measurements/current-tasks-oop-20261027.json).

## Authored call preparation and admitted plans

The Oop emitter now registers derived call plans through Execution's restored
`fromStep:descriptor:selection:owner:callPlans:` factory and emits one current
`prepareCall:selector:` send per call site. Plans retain receiver/argument sources
and ordered alias copies; top-level published bindings stay outside those copies.
Execution reuses the existing canonical admission checks before publishing a Method.
Truncated plans, wrong versions, invalid source slots and trailing data are rejected.
Canonical descriptor and image formats are unchanged.

Execution owns the plan and frame representation. Its preparation implementation
uses current frame reads/writes and call-site publication, preserving their existing
replacement behavior. It does not read foreign payloads, add an Id operation,
introduce a registry or retry an entered failure. The direct-frame experiment was
replaced with this single message-based implementation during validation.

The authored reference performs normal and tail preparation, including ordered
copies, collection and nested explicit Machine execution. Its outer callback is
explicitly materialized synchronously so it does not replace the target's logical
frame before inspection. The transfer body runs on the new Machine. Named Method
construction uses ordinary Reflect rather than the removed raw-entry adapter.
The reference passes on the bare composition and again after loading the compiled
stdlib and stackless-control artifacts through the existing startup path.

The core gate, all 26 Execution unit tests, the full source target, materialization,
frame views, pending recovery, native boundaries, embedding and mapped/rebuilt
Method-binding acceptance pass. Metadata-capture fixtures now observe the restored
five-argument factory. Four compiler groups and full latest cold qualification
remain pending. [Receipt](../measurements/prepared-call-oop-20261027.json).

## Native allocation-accounting witnesses

The `allocation_accounting` harness now uses declared Oop callbacks and cached
selectors throughout its measurements. Literal geometry expectations, repeated
16 MiB default thresholds, an explicit smaller budget, non-refunding detachment,
failed tracing without reset or retirement, and tracing-time charge observation
all pass. Current geometry replacement can collect or construct another object;
the outer charge still adds to the post-callback total and restoration is visible.

The fatal-overflow subprocess now uses nested maximum immediate Oop charges rather
than the removed native Word carrier. It verifies abort and the GC owner's exact
diagnostic. Oversized representable size queries decline without allocation;
invalid arguments remain failures. No runtime or Id change was needed.
[Receipt](../measurements/allocation-accounting-oop-20261027.json).

## Cold candidate CLI acceptance

All 18 installed Host CLI checks passed on cold revision `9f638b73`, covering
native entry/compiler replacement, artifacts and Lang, Alien adapters, operation
without Store, workers, relocated project check/build/run/install/test, dependency
resolution and collecting project queries. The image group remains in the serial
queue. Later native-loader, standalone-emitter and call-preparation corrections
still require a new cold candidate.
[Receipt](../measurements/current-cli-oop-20261027.json).

## External Bytes capacity and collecting accounting

The `external_allocation` harness now supplies Oop byte providers and observes
the current external-charge method. Two 16 MiB construction rounds retain their
buffers through the next collection checkpoint. Explicit automatic-collection
suppression, re-enabling, zero charges and invalid-input rejection remain covered.

A collecting accounting replacement verifies that Bytes publishes ownership and
roots before reporting capacity. Construction and append preserve every byte;
only capacity growth is charged, empty append does not double-charge, and restoring
the original method affects the next construction. The measurement uses cached
selectors so Host helper allocations cannot satisfy its threshold assertions.
No runtime or Id change was needed.
[Receipt](../measurements/external-allocation-oop-20261027.json).

## Collector activation, failure and repair

The `memory_activation` harness now starts from bare Id and Roots, creates an
opaque payload before loading GC, and runs through an independent declared C
module. It verifies failed activation, repair, complete initial-heap accounting,
stable object identity, inherited owner coverage, detached/cyclic parents and
uncovered allocations after activation. Precise payload mode makes the later
owner-coverage checks explicit; identity-only observations avoid querying freed Oops.

The test exposed partial accounting after activation failed: installing managed
Oop construction methods can allocate through a method installed earlier in the
same attempt. GC now restores its previous charge and measurement origin when
installation or validation fails. Installed methods remain available for repair;
successful activation publishes the complete inventory charge. A constructor that
allocates and then fails exercises the installation failure path as well.

The full core gate and activation, allocation-accounting, external-capacity and
System GC suites pass. The change is local to GC and adds no Id operation, registry
or payload classification. Full latest cold qualification remains pending.
[Receipt](../measurements/memory-activation-oop-20261027.json).

## Memory lifecycle and current tracing/finalization

The `memory` harness now uses declared Oop callbacks and a test-owned graph
representation. It retains duplicate-edge/cycle handling, phase checks, current
visitor and core-reference replacement, sticky ignored visitor failure, LIFO live
trace-owner replacement, creation-order finalization and live finalizer replacement.
Finalizers identify the exact expected allocations and read only their own payloads
while detached storage is still valid. A child process verifies that failed release
preserves detached storage, makes collection terminal and leaves a one-use release
handle. Conservative native roots and automatic budget collection remain covered.

Three rooted batches of 5,000 Symbols return to their exact count/name-byte
baseline, derived through existing allocation and Symbol messages. Weak-symbol and
vtable retirement are observed before finalizers run. The former test-only opaque
weak-binding injection had no production caller and belongs to the removed carrier
API; it is not restored in Id. Internal weak-key pruning remains covered by Id's
unit tests. Malformed visitor arity replaces an invalid old-carrier argument in the
sticky-failure check; immediate Words are valid Oops and cannot serve as fake pointers.

The memory suite and all 22 Id unit tests pass. No runtime or Id change was needed.
[Receipt](../measurements/memory-lifecycle-oop-20261027.json).

## ObjectModel construction, layouts and ownership

The general `object_model` harness now runs through Oop messages. It retains
private-vtable ownership and inheritance, clone sharing, current owner/retirement
hooks, copied layout descriptions with arbitrary opaque codes, shallow references,
raw padded cloning, rejection of unowned storage and collection during allocation
reentry. The snapshot-child check now verifies allocation identity as well as value.

An independent native oracle writes only the test-created opaque allocation after
checking its requested extent and geometry, then verifies all padded clone bytes.
Other storage mutation goes through ObjectModel. Census observations use identities;
metadata retirement is checked while finalizer arguments remain valid. All 24
private-object cycles finalize once, clones do not inherit finalizers, and one-byte
collection pressure preserves construction roots. Reflection uses existing allocation
and binding enumeration instead of adding retired count/index helpers to Id.

The general, slot-inventory and scoped-conversion suites pass. No runtime or Id
change was needed. [Receipt](../measurements/object-model-oop-20261027.json).

## Completed cold candidate Host qualification

All 16 image checks passed on cold revision `9f638b73`, completing all five serial
Host groups: generation 17, interaction 6, tasks 6, CLI 18 and images 16. The
candidate's exact 820-file manifest remained unchanged after qualification.
This completes that candidate's cold installation/archive/Host checkpoint, not
the remaining native migration or qualification of later runtime corrections.
A fresh cold build must include the native-loader, standalone-emitter,
call-preparation and failed-activation fixes.
[Receipt](../measurements/current-image-oop-20261027.json).

## Native module composition and result retention

The general `modules` harness now uses Oop messages and declared callbacks.
It retains foreign-thread refusal, idempotent Roots/Namespace/Boolean/nil/Bootstrap
initialization, current Symbol validation, binding enumeration and recovery from
visitor failure. Live binding inventory distinguishes raw absence from bound
language nil. Root retention during a visitor enters the next snapshot, and release
tokens remain one-use.

Boolean/nil identity and operator matrices, selected-branch counts and live branch
replacement pass; an explicit failing callback verifies that unchosen branches are
not entered. Immediate Word Oops are accepted as ordinary operands. Reloading a
native owner preserves replacements. Bootstrap retains duplicate and nested load
results independently and releases exactly those retained references.

The module, Root registry and native-loader suites pass. Symbol validation now
observes the existing `isSymbol:` operation and native loading observes the Name
owner's loader. Out-of-range index checks use representable size Oops instead of
the removed native Word carrier. No runtime or Id change was needed.
[Receipt](../measurements/modules-oop-20261027.json).

## Fresh cold installation with current runtime corrections

Revision `83737742` passed a new cold build from a clean detached checkout, with
no Birnam executable on PATH and no build cache. Its exact 821-file installation
excludes the Lang build seed. Archive qualification passed with the original
source and prefix hidden, including signatures, relocation, source/GC/image
behavior and startup without parsing or compilation. The hidden paths were restored.

This candidate includes the native-loader, standalone-emitter, call-preparation
and failed-activation corrections. Its full serial Host qualification is now
running. Remaining native/compiler migration and the alias-return scope decision
are still open. [Receipt](../measurements/latest-cold-install-oop-20261027.json).

## Store facts, history, snapshots and Cells

The full `store` harness now uses Oops and declared collecting callbacks. It
retains nil/absence distinction, cardinality, prospective history and exact as-of
retention, compaction plateaus with a growing-history control, fact-Cell ownership,
current accessors/factories, image transfer and transient exclusion. Snapshot
callbacks erase the original facts and collect; all rows remain valid during
delivery and are reclaimed after completion or failure. Phase checks still reject
Store mutation during tracing.

Source Globals remain independent of explicit Store facts. Authorization,
notification, nondurable method descriptions, backfill and explicit invalidation
retain their checks, including edits through captured dictionary Methods. No
universal mutation subscription is installed. Fact-Cell image restoration keeps
owner aliases and rejects removed Store APIs.

The 256-byte resource-limit case now runs on the normal Store-independent bootstrap
with production Error objects, removing the old fixture-only error sink. Collecting
construction uses explicit `Store new` while its Namespace name is absent; native
reloading preserves that absence and returns the original module receiver. No
runtime or Id change was needed. The Store harness, adjacent contract suites and
13 independent unit oracles pass; two manual scaling investigations remain skipped.
[Receipt](../measurements/store-oop-20261027.json).

## Relational queries and collecting callbacks

The `relational` harness now uses Oop messages and declared native callbacks.
It retains query order and deduplication, joins, predicates, negation, disjunction,
aggregates, historical views, recursive rules, nested pull and source spelling.
Scalar equality, numeric promotion, overflow and malformed inputs retain their
existing expectations.

Replacement Store traversal, Integer conversion, Vector and Map methods collect
and enter nested queries. Ephemeral entities survive callback delivery; expired
collectors reject later delivery, and failed result construction releases abandoned
entities. Reclamation checks use allocation identities instead of freed pointers.
Namespace factory replacement, LogicVar identity and repeated loading also pass.

No runtime or Id change was needed. The migrated harness and three adjacent Store
contract suites pass, as do 21 evaluator unit tests; one manual timing investigation
remains skipped. This does not complete the remaining native/compiler migrations
or the running cold Host qualification.
[Receipt](../measurements/relational-oop-20261027.json).

## Captured Method identity and initializer-result lifetime

Migrating `execution_methods` exposed two omissions. Execution's Oop setup did
not install the existing identity-preserving `Method clone` operation. It now
binds that selector to its already-declared identity implementation. Ordinary
dispatch still enters the selected Method directly; cloning is an explicit send.

The native-image owner also retained every initializer result in its permanent
registry. Consequently, releasing Bootstrap's result and removing the final
binding could not reclaim the returned Method or its context. The image owner
now retains its code declarations and literals, while the caller owns the returned
object. Bootstrap's explicit retention and release remain unchanged. This removes
a redundant field and root; it introduces no Id operation, payload interpretation
or module coupling. Native code mappings retain their existing residency policy.

The migrated test uses declared native Methods and Oop contexts. It preserves
binding and entered-call lifetime, self-replacement during collection, captured
entry/context identity, collecting argument reads and failure cleanup, nested
custom lookup, native initializer results and unreachable owner/binding cycles.
Reclamation is checked by allocation identity, not by querying a freed Oop.
Raw absence, immediate Words and native failures follow the Oop calling convention;
language Error conversion remains with explicit adapters and Machine execution.
The old combined Id/vtable fixture assumption is replaced by capturing lookup from
the actual vtable owner. The native gate no longer builds the v1 Method fixture.

The core Oop gate, 48 Id/Execution unit tests and eight native suites pass,
including compiler, loader, materialization, mapped/rebuilt method bindings and
native callback image checks. The running cold candidate predates these two
runtime corrections and does not qualify them.
[Receipt](../measurements/execution-methods-oop-20261027.json).

## Cold generation qualification after call-preparation integration

All 17 generation checks passed on cold revision `83737742`, including publication,
source registries, generation operations, drifted mapped/rebuilt images, console/TCP
activation and rollback, and 62 compiled-unit policy checks. The serial runner has
advanced to interaction; tasks, CLI and image groups remain queued. This candidate
includes call preparation and the earlier loader/emitter/GC fixes, but predates the
Method clone and initializer-result lifetime corrections above.
[Receipt](../measurements/latest-generation-oop-20261027.json).

## Machine registration and ordinary Method argument binding

The production `execution_machines` scheduling/control/lifetime checks now use
Oops and declared native steps. The older native optimization and recovery groups
remain in `execution_candidates`, still included by the native gate; moving these
checks does not qualify or retire those pending consumers.

The migration exposed two missing Execution cases. First, registration rejected
canonical descriptors with an implementation slot. The existing four-argument
constructor previously used its supplied owner as that slot's initial value.
Registration now supplies that value through the existing internal registration
field. Execution interprets the descriptor and initializes its own frame; no
foreign payload access or new Id operation is involved.

Second, pending calls rejected argument counts that direct Method invocation
already accepted. Both paths now use the established frame binding: missing
ordinary Method arguments become nil and extra arguments are unused. Optional
inline candidates still require the exact shape before entry. Block value methods
and explicit native signatures retain their own arity requirements.

The migrated checks cover normal/tail calls after replacement, collecting native
callbacks, independent Machine budgets and live/stale views, suspension, protected
exits, native control barriers, construction refusal during GC, one-shot and
wrong-Machine continuation restoration, shallow Block cloning, and the lifetime
of paused and detached frames. Allocation identities replace freed-object queries.
Additional checks compare direct/normal/tail argument binding and verify that an
arity mismatch uses the ordinary body without entering an inline candidate.

The core Oop gate, 26 Execution unit tests and eight native suites pass, including
Block/native arity, pending recovery, compiler behavior and mapped/rebuilt Method
bindings. Id and public operation sets are unchanged. Cold qualification of these
corrections and the previous lifetime fixes remains outstanding.
[Receipt](../measurements/execution-machines-oop-20261027.json).

## Explicit PC candidates through Oop Methods

Execution again provides the existing `Method optimizeWith:` operation. A complete
sequence supplies one declared Method per program counter; an empty sequence
withdraws the candidate. Admission reads the current sequence before changing the
installation. Invalid shape, invalid Methods, registered resumable steps and native
read failures leave the current installation and visit count intact.

Selection retains the existing 64-visit threshold and withdraws a candidate when
the relevant Machine bindings change. It uses Id's current binding proof, shared
with explicit inline recovery. Replacement during that proof takes precedence;
an entered candidate retains its Method/context through withdrawal and collection,
and its failure is never replayed through the ordinary step.

The existing frame owner reference now points at Execution's registration context,
which already traces its Method, step and owner. This gives the driver direct
access to the existing record, avoiding a record scan or another table on every
step. Candidate references and visit counts stay in that record. Id does not
interpret this context or any descriptor or frame payload. No new public operation
or implicit dispatch protocol is introduced; this restores an existing operation.

The migrated tests cover captured aliases, single/multiple-PC candidates, malformed
and failed admission, changed controls, reentrant replacement, failure identity,
entered-owner survival and 96 registration/collection cycles. The older numeric
consumer and recovery groups remain explicitly pending in `execution_candidates`.

The core Oop gate, 26 Execution unit tests and nine native suites pass, including
compiler/materialization, pending recovery, strict Block/native arity and
mapped/rebuilt Method/image checks. This is semantic and lifetime acceptance, not
a performance claim. The cold `642e79e2` candidate predates this restoration.
[Receipt](../measurements/execution-candidates-oop-20261027.json).

## Cold interaction qualification after call-preparation integration

All six interaction checks passed on cold revision `83737742`: Socket and REPL
owners, parser/compiler-free published startup, interactive/project CLI, REPL
goldens including image/native-wrapper behavior, and project LSP. The serial
runner has advanced to tasks, followed by CLI and images. This receipt predates
the later lifetime, Machine-entry and explicit candidate corrections.
[Receipt](../measurements/latest-interaction-oop-20261027.json).

## Explicit numeric evaluation over Oop Words

Execution again implements the existing `numericOperation:left:right:` query.
It retains the five checked signed operations and raw-absence refusal on overflow
or division by zero, including the existing `MIN mod: -1` result of zero. Full-width
Words are read through current `low32`/`high32` messages, and non-immediate results
use the current Namespace Word factory. Neither Integer nor Word payloads are read
by Execution. Integer still owns result construction and language errors.

The query propagates native getter/constructor failures exactly. An absent
constructor result is rejected rather than being mistaken for arithmetic refusal.
The test covers 500 independent arithmetic cases, signed bounds, malformed input,
collecting getters, live constructor/Namespace replacement and the Integer owner's
refusal to evaluate an authored arithmetic Method through a native visitor.

This does not restore the old automatic interception of selected native Methods.
That path depended on Execution doing work now owned by Id's direct dispatch. A
scope clarification is pending between retiring automatic interception while
retaining this explicit query and investigating compiler-generated use of it.
No Id hook or second implicit Method-entry protocol has been introduced.

The core Oop gate, 26 Execution unit tests and seven native suites pass, including
numeric policies, native adapters, Machine candidates, recovery and compiler
behavior. [Receipt](../measurements/execution-numeric-oop-20261027.json).

## Cold installation and archive after lifetime and Machine-entry fixes

Clean revision `642e79e2` passed cold installation without a Birnam executable on
PATH or a build cache. Its exact 822-file installation excludes the Lang seed.
Archive qualification passed with the original source and prefix hidden, including
signatures, relocation, source/GC/image behavior and parser/compiler-free startup.
Both hidden paths were restored. Full serial Host qualification is now running.

The first archive attempt exceeded the aggregate 300-second CLI limit. A diagnostic
run retained each command's existing 300-second limit and passed every assertion
in 281 seconds: full stdlib image save took about 218 seconds and image load about
12. The unchanged archive gate then passed on retry. No deadline was changed; the
failed attempt and diagnostic evidence remain in the receipt. No semantic failure
was reproduced, and the source of the timing variation is not established.

This candidate includes Method/result lifetime and Machine registration/argument
binding fixes. It predates explicit candidate and numeric evaluator restoration.
[Receipt](../measurements/machine-cold-install-oop-20261027.json).

## Cold task qualification after call-preparation integration

All six task checks passed on cold revision `83737742`, including task images,
workers, concurrency services and cleanup, closure collection behavior, multicore
and task control. The worker-ready deadline remains 15 seconds. CLI and image
groups remain in that serial queue; this receipt does not qualify the later runtime
corrections. [Receipt](../measurements/latest-tasks-oop-20261027.json).

## Recovery retains the baseline and releases finished candidates

The remaining recovery lifetime test reproduced an ownership regression: after
recovery published a paused frame and the dictionary binding was replaced, the
original baseline Method was reclaimed. `withInline:` had created another
registration, so the recovered frame retained that copy and its candidate data.

Inline Methods now annotate the existing baseline registration. Their contexts
trace the original registration context; the recovered frame retains that original
context, and only its retirement releases the registration lease. This removes
duplicate descriptor/registration construction. PC candidates installed through an
inline annotation belong to the same baseline, and reannotation does not retain
the superseded candidate. Id and its dispatch interface are unchanged.

The migrated tests check the paused-frame transition directly: the baseline and
code owner remain live, while the finished candidate and recipe are reclaimed.
Completion and abandonment release the baseline; failed admission releases its
temporary references. The rest of the older recovery matrix now uses Oops too:
constructor-time layout changes, temporary-node cleanup, exactly forwarding trace
owners, collecting/reentrant recipe reads, one-shot publication, custom lookup,
invalid graphs and native numeric storage refusal. The old carrier-based recovery
test is removed. Automatic numeric interception remains separately pending.

The core Oop gate, 26 Execution unit tests and nine native suites pass, including
compiler/materialization, frame inspection, strict Block/native boundaries and
mapped/rebuilt image checks. Both running cold candidates predate this correction.
[Receipt](../measurements/recovery-owner-oop-20261027.json).

## Physical Storage and Trace acceptance through Oops

The `storage_trace` and `trace_heap` harnesses now use declared callbacks and Oop
contexts. The real Storage inventory protects objects during owner and report
collection without counting that protection as declared graph roots. Reachable and
unreachable cycles remain distinct, and a rooted Integer containing another
object's address bits does not create a reference to that object.

The checks retain strict coverage refusal, nested observation refusal, swallowed
visitor-failure latching, expired callbacks, report failure cleanup, current
reference-owner replacement and explicit root removal. Ordinary GC reclaims the
graph after observation releases its protection. Reclamation checks use allocation
identities; malformed callback arity replaces the old invalid-pointer fixture.

An unfiltered report visited all 31,595 allocations once in identity order and
collected during completion. A failing replacement of `automaticCollection:` was
never entered: Trace did not disable automatic collection to pass. Both targets
pass without runtime or Id changes. The separate exhaustive logical-graph and
native-result harness remains pending.
[Receipt](../measurements/storage-trace-oop-20261027.json).

## Cold qualification checkpoints for both active candidates

Cold revision `642e79e2` passed all 17 generation checks, including the lifetime
and Machine-entry corrections. Its serial queue has advanced to interaction.
This candidate predates the later explicit candidate/numeric support and recovery
ownership correction. [Receipt](../measurements/machine-generation-oop-20261027.json).

Older cold revision `83737742` passed all 18 CLI checks, covering its native/source
owners, Store-independent core, project/build/packaging and query paths. Only the
image group remains in that queue. These are evidence for that older revision,
not qualification of subsequent runtime changes.
[Receipt](../measurements/latest-cli-oop-20261027.json).

## Exhaustive Trace graphs and native cleanup through Oops

The `trace_graphs` harness now runs all 4,096 directed three-node graph/root
combinations against independent bit closure. Equal-valued String nodes remain
distinct, and declared identities retain unsigned ordering through `2^63` and
`2^64 - 1`. Foreign endpoints, duplicate objects/identities and zero identities
reject before report callbacks; duplicate roots and edges remain valid.

Adversarial providers retain the checks for expired sinks, swallowed malformed
Word-conversion failures, missing completion, swallowed report errors and full-width
coverage masks. Cleanup collection preserves fresh values and native failures;
cleanup failure does not replace a primary body failure. Empty graphs complete once.

Return cases use owned Bytes, declared Method Oops and the ordinary Id receiver in
place of retired native carriers. The fixture still overwrites its source buffer
during cleanup. Native absence, language nil and unmarked Fault data stay distinct.
A deferred Bytes echo survives cleanup collection with the exact argument identity.
The complete test passes with `MallocScribble=1`; no runtime or Id change was needed.
[Receipt](../measurements/trace-graphs-oop-20261027.json).

## Block loops and native absence at language calls

The Block loop harness now uses declared callbacks and Oop contexts. It retains
the 10,000-iteration case, non-Block receivers, missing/extra arguments, Boolean
validation, exact-family Error handling, current condition/body replacement,
collecting Error lookup and transient-result reclamation. Bootstrap loops still
refuse suspension; installing the existing authored controls permits condition
and body suspension, including Blocks created before replacement. Reloading the
bootstrap loop module does not overwrite those replacements.

The migration reproduced a result-conversion omission: a replaced Error factory
returning native absence made a language loop return absence instead of nil.
Execution's existing language-result handler now performs the former Unit-to-nil
conversion for an absent Oop result at the Machine call boundary. Normal and tail
language calls are checked, while direct native sends and explicit captured Method
invocation still return raw absence. No Id operation, dispatch hook or generic
Method/Block wrapper was added. The transient-result fixture observes the current
Namespace `valueAt:` operation and checks reclamation by allocation identity.

The core Oop gate, 26 Execution unit tests, 11 native suites and all four retention
suites pass. The latter return to their pre-call allocation counts after collection;
native adapter and exhaustive Trace checks still preserve raw absence and exact
failure behavior. [Receipt](../measurements/block-loop-oop-20261027.json).

## Cold interaction and tasks after lifetime and Machine-entry fixes

Cold revision `642e79e2` passed all six interaction checks and all six task checks.
This covers its interactive/project sessions, compiled startup, Socket/REPL/LSP,
task images, workers, concurrency cleanup and task control. Its worker-ready
deadline remains 15 seconds. CLI and image groups remain in that queue.
[Interaction receipt](../measurements/machine-interaction-oop-20261027.json),
[task receipt](../measurements/machine-tasks-oop-20261027.json).

Those results predate explicit candidate/numeric support, recovery ownership and
language-absence conversion. A fresh clean cold build at `a3eaa227`, including all
those corrections, is now running without Birnam on PATH or a build cache.

## Generation projection, partial edits and failure-channel images

The generation projection and transaction harnesses now use Oop Methods and
declared collecting callbacks. Projection remains canonical and inert; repeated
reads agree, current String construction may collect, and a source edit during
construction rejects a mixed snapshot. Fixture rows are found by identity among
the actual compiled core records.

Both synchronous and resumable candidates retain stale/discarded-token rejection,
partial effects after setter failure, explicit undo conflicts, exact restoration
and preservation of unrelated callback edits and explicit Store facts. A failed
undo is not automatically redone. Constructor failure uses the current
`withImplementation:` operation and cannot replay activation. Mutation interception
uses the actual vtable owner rather than the old combined Id/vtable receiver.

The former publication-image defect characterization now requires the corrected
contract: the exact native invalid-request failure survives fresh artifact use
and both source-free mapped/rebuilt image paths. All three targets pass without
production changes. The removed whole-Id binding-undo primitive was not reintroduced;
its old harness remains subject to contract audit.
[Receipt](../measurements/generation-publication-oop-20261027.json).

## Completed cold Host qualification for the call-preparation candidate

Cold revision `83737742` passed all 16 image checks, completing all five Host
groups: generation 17, interaction 6, tasks 6, CLI 18 and images 16. Its exact
821-file installed manifest remained unchanged. This completes that revision's
cold installation/archive/Host checkpoint, not the remaining native integration
or qualification of later runtime corrections. The newer `642e79e2` queue and
current-runtime `a3eaa227` archive qualification continue separately.
[Receipt](../measurements/latest-image-oop-20261027.json).

## Candidate admission, owned factories and retirement through Oops

The candidate and retirement harnesses now use Oop Methods and declared callbacks.
Inspection and mapping remain inert, mapping uses the inspected bytes after the
author path is overwritten, and replaced TCC compilation methods are never entered.
Preparation tokens remain one-use and fresh; mutable descriptions and unrelated
Store facts do not become artifact authority, and Generation does not consult
Store's basis. Unstable method targets are rejected without specialization.

The independent factory check uses the current `candidateDataFor:` description and
`withImplementation:` constructor instead of unpacking the retired two-object
candidate format. It verifies helper capture identity and execution without
publishing dispatch or Globals bindings. The invalid-target case now restores valid
artifact bytes before inspection, avoiding rejection for the wrong reason.

Retirement covers unmapped, inert, resumable, synchronous and reentrant-callback
cases in isolated bootstrap/running/metadata-drift processes. File removal is
conditional on exact owned bytes and remains separate from logical discard.
Captured Methods stay callable after discard and release their mappings after the
last reference disappears; retained counts and bytes return to zero. No runtime
or Id change was needed.
[Receipt](../measurements/candidate-retirement-oop-20261027.json).

## Current-runtime cold installation and archive qualification

Clean revision `a3eaa227` passed cold installation without Birnam on PATH or a
build cache. Its exact 827-file manifest excludes the Lang seed. Archive checks
passed with the original source and prefix hidden, covering signatures,
relocation, source/GC/image behavior and parser/compiler-free startup; both paths
were restored, and no deadline changed. Full serial Host qualification is running.

This candidate includes explicit Execution candidate selection, numeric evaluation,
correct baseline recovery ownership and language nil conversion for absent native
results. Subsequent commits so far change tests and documentation only. Remaining
native/compiler integration is still open.
[Receipt](../measurements/language-cold-install-oop-20261027.json).

## Durable artifact activation and independent payload lifetime

The artifact activation harness now uses Oop Methods and scoped roots. Loading
works after deleting source and embedded assets, while failing parser/compiler
replacements remain unentered. Current inspection and relocation methods collect
before forwarding. The loaded Block retains one additional native unit; dropping
its last root releases that code while an independently retained binary result
remains valid.

The checks also preserve Symbol-name validation, exact native load failure,
unchanged artifacts after compilation failure, source-digest corruption refusal
and successful loading after restoring valid bytes. The target passes without
runtime or Id changes. [Receipt](../measurements/artifact-activation-oop-20261027.json).

## Inert Artifact inspection and descriptor lifetime

The Artifact inspection harness now uses Oop callbacks and owner messages. It
retains malformed/path/ABI/target/dependency checks, the sparse-file size limit,
all descriptor projections and invalid-index behavior. A real dylib constructor
sentinel remains untouched. Nine replaced String, Integer, Vector and ObjectModel
operations collect while constructing and inspecting the descriptor.

Native failures preserve identity and do not overwrite language diagnostics.
Transient input and descriptor reclamation are checked by allocation identity.
Embedded NUL handling, descriptor cloning, authored publication, live method
replacement, repeated module loading and compiled inspection all pass. No runtime
or Id change was needed.
[Receipt](../measurements/artifact-inspection-oop-20261027.json).

## Generation ledger drift and captured Method ownership

The generation-ledger harness now uses Oop Methods and declared save callbacks.
Adoption rejects opaque native implementations, source/body disagreement, missing
or wrongly present methods, changed target aliases and wrong global Block bodies.
It remains inert after source files and embedded assets are removed, and validates
the final writer across dependent modules rather than an overwritten definition.

The seven isolated cases cover direct dictionary edits, explicit Store removal,
source replacement, metadata A-to-B-to-A replacement, mutation during image-code
construction, read-only validation with a rejecting state setter, and nonportable
native replacement. Edited portable worlds remain snapshot-capable without
certification; nonportable or mid-save changes preserve the previous file.
Reconciliation reports drift without rewriting descriptions, and explicit
re-adoption can renew agreement after exact restoration.

The fixture uses explicit Store invalidation to remove descriptive mirrors before
testing the ledger's captured-Method ownership. It also avoids retaining the
Method returned by raw dictionary removal. No universal observer or removed Id
undo operation was introduced.

All seven cases pass. The raw-edit case was rerun after removing the fixture's
extra Method root; the six other cases were unchanged. Adjacent projection,
transaction, candidate and retirement suites also pass. No runtime or Id change
was needed. [Receipt](../measurements/generation-ledger-oop-20261027.json).

## Image interruption checks and optional Store composition

The image snapshot and partial-load harnesses now use declared Oop Methods and
ordinary context objects instead of the old NativeEntry/Operand/Outcome interface.
The save checks retain dictionary, Store retention-policy and provider-marker
changes made during compilation; a rejected save must leave the existing file
unchanged. Fresh image payloads and their children must survive a collecting
callback and become reclaimable after saving. The fixture observes allocation
identities rather than querying freed object references.

The partial-load checks retain callback dictionary edits, independent Global
bindings, explicit Store facts and newly recorded source. Native failures and
ordinary Error objects keep their identities. Both mapped and rebuilt restoration
retain the existing no-replay and partial-load requirements, including bound nil
and an anonymous target restored before its Global is published. The existing
authored-startup switches remain available.

The shared generation setup also now loads the authored Store module, as the old
optional-module setup did. Loading only native Store omitted its image-provider
registration. Earlier generation receipts remain evidence for that narrower
composition; the corrected composition is checked separately. These changes add
no Id mechanism and do not restore universal method-change subscriptions.

The two image harnesses pass, including mapped/rebuilt native failure and
mapped/rebuilt authored startup with a late Global, bound nil and an ordinary
Error. All seven ledger cases and adjacent projection, transaction, candidate and
retirement checks pass with authored Store enabled. Explicit invalidation and
Store checks also pass. [Receipt](../measurements/image-interruption-oop-20261027.json).

The separate current-runtime cold Host run is still unqualified: its generation
group stopped when compiled-unit baseline image preparation exceeded the existing
300-second subprocess timeout. No deadline was changed, and the cause has not yet
been established. [Failure receipt](../measurements/language-cold-host-timeout-oop-20261027.json).

## Isolate value conversion and worker reply failure

The Isolate harness now uses Oops, scoped roots and existing declared callbacks.
It preserves vector/map round trips, all supported copied value kinds, collecting
Integer construction, cyclic and nontransferable rejection, and oversized-container
errors. Native failures preserve their exact object identity both during decoding
and while constructing a reply from a ready wire peer; the latter terminates the
worker handle. The unchanged wire-only peer now lives with the Oop fixtures.

The harness passes without runtime or Id changes. The worker-ready deadline
remains 15 seconds. [Receipt](../measurements/isolate-conversion-oop-20261027.json).

## Alien callbacks and wrapper reclamation

The Alien harness now uses Oops and scoped roots. Signed, unsigned, floating-point
and by-value struct calls retain their checks, as do owned buffers, collecting
Integer construction, recursive callbacks and release during an active callback.
After explicit Store removal of wrappers and release of temporary roots, live
native code units return to the compiled floor and the foreign library can close.
The unchanged plain-C fixture is compiled at a fresh temporary path by the test,
so the native gate no longer writes it into an installed prefix.

These checks pass without runtime or Id changes.
[Receipt](../measurements/alien-callbacks-oop-20261027.json).

The unchanged compiled-unit CLI replay also passes on the current cold prefix:
baseline image preparation takes 222.6 seconds, followed by all 62 policy checks
in 15.9 seconds, within the original 300/900-second limits. This does not establish
the original timeout's cause or qualify relocation by itself. The original failed
attempt is retained; a fresh relocated retry and remaining Host groups are running.
[Replay receipt](../measurements/compiled-unit-timeout-replay-oop-20261027.json).

## Current cold generation qualification after unchanged retry

All 17 generation checks now pass across the initial attempt and the relocated
retry. The retry used a fresh copy of the same cold candidate and the unchanged
compiled-unit script, including its 300/900-second limits and all 62 assertions.
It completed in 237.8 seconds total. The original timeout remains recorded; its
cause is not established. The remaining Host groups continue separately.
[Receipt](../measurements/language-generation-oop-20261027.json).

## Generated call input lifetime

The generated-input harness now uses declared Oop checkpoints. A checkpoint
releases the caller's sole external input token, collects, and verifies both heap
inputs by allocation identity. Direct and nested calls preserve exact aliases;
fresh Word results and the original native failure retain their meanings.
Captured Methods still enter their old implementation after dictionary replacement,
while subsequent sends enter the replacement.

Required-object admission now checks non-null Oops: null is rejected before the
body, and a supported immediate is accepted. The old Operand word/object carrier
distinction no longer exists. The generated-input and adjacent native incoming
harnesses pass without runtime or Id changes.
[Receipt](../measurements/invocation-inputs-oop-20261027.json).

## Numeric owner temporary roots

The numeric-input harness now uses declared Oop callbacks, weak allocation
identities and the allocated-object list. It preserves zero redundant Root
registration checks, collecting validation, nested temporary-value reclamation,
exact failure propagation, failed-construction cleanup and result reclamation
after the caller releases it. The adjacent number harness also passes.

The initial port passed but spent 329 seconds rescanning all allocations for each
lookup made by explicit cleanup GC. A process sample identified this quadratic
fixture work. Cleanup collections now suppress recursive instrumentation while
numeric validation callbacks remain collecting and all assertions remain enabled;
the numeric harness passes in 5.8 seconds. This is a fixture correction, not a
runtime speedup. No runtime or Id changes were needed.
[Receipt](../measurements/native-numeric-inputs-oop-20261027.json).

## Collection read input and snapshot lifetime

The collection-input harness now uses Oop callbacks and weak allocation identities.
Map lookup releases the caller's external leases from its hash callback; reentrant
rehash and collection preserve the detached node until the active read finishes.
Vector conversion similarly detaches its tree and tail during both synchronous
and resumable reads. The tests verify survival during the callback and reclamation
afterward, exact native failures and zero redundant input Root registrations.

Reflect, Symbol hashing and allocation-permission forwarding retain their live,
collecting owner checks. Symbol hashing now observes the current `nameHash`
operation rather than the removed raw byte-carrier query. All checks pass without
runtime or Id changes. [Receipt](../measurements/collection-inputs-oop-20261027.json).

## Current cold interaction qualification

All six Host interaction checks pass on the current cold a3eaa227 candidate:
socket and REPL ownership, installed startup without compilation/parsing,
interactive CLI behavior, REPL golden cases and LSP project behavior. Tasks, CLI
and image groups remain separate ongoing qualification work.
[Receipt](../measurements/language-interaction-oop-20261027.json).

## Word fallback query order

The Word ordering harness now uses a declared Oop observer. Replacing the
representation query forces the ordered public path: left then right. A native
failure while inspecting the left operand wins before the right operand is
inspected, including heap, immediate, null and missing inputs. The test passes
without runtime or Id changes.
[Receipt](../measurements/word-order-oop-20261027.json).

## Capture access without unrelated name services

The capture-dependency harness now checks an actual compiled closure and its
captured marker through `captureAt:` and invocation. Both continue to return the
same marker while Name lookup and interning are replaced with verified failing
Methods; neither replacement is entered. The old raw capture-carrier query is
also required to remain absent. No borrowed-pointer adapter or new protocol was
introduced. The harness passes without runtime or Id changes.
[Receipt](../measurements/capture-dependencies-oop-20261027.json).

## Full image roundtrip and twenty-resave stability

The main image harness now uses Oop Methods and declared callbacks. Its full
roundtrip passes for mapped, rebuilt and native-code-stripped images, including
fresh-process mapping of rebuilt resaves. It preserves numeric bit patterns,
aliases, cycles, lexical captures, provenance, synchronous disposition, embedded
binary assets, inherited methods, private/shared Map behavior, Cell behavior,
Store history and cloned System behavior. Collecting restoration, partial mapping
and publication failures, unsupported capabilities and structural corruption also
retain their checks.

The separate stability chain passes an initial snapshot and twenty alternating
mapped/rebuilt resaves. From epoch 3 onward, each parity has unchanged retained
facts, fact bytes, native units and image size. Every epoch releases fixture
mappings against an independent source, mapped or rebuilt resident control. The
roundtrip comprises eleven successful process checks; the chain comprises
25 including controls and its parent.

Setup now retains only the Lang Methods needed for cleanup and releases temporary
setup roots. The old Module delete/reload step was removed because it discarded
Store's authored image-provider registration. The real bootstrap composition is
retained instead, with participation asserted. Direct binding observations use
Execution's current protocol. No runtime or Id changes were needed.
[Receipt](../measurements/image-roundtrip-resaves-oop-20261027.json).

## Global resolution through Oop getters

The global-resolution harness now uses declared Oop callbacks. It preserves
Globals-first precedence, nil fallback, getter order, temporary owner lifetime
across rebinding/collection, scope cleanup and exact native failure identities.
The observations retain allocation identities rather than the temporary owners.
Supported immediate owners dispatch through their current `at:` method, and
immediate results remain Oops; the old carrier distinction is not recreated.
Ordinary source-global owners still reject non-Symbol keys. No runtime or Id
change was needed. [Receipt](../measurements/global-resolution-oop-20261027.json).

## Current cold task qualification and completed older Host candidate

All six task-group checks pass on the current cold a3eaa227 candidate, including
mapped/rebuilt task images, task fields, concurrency CLI, multicore workers,
closure observation and 177 task-control assertions at both tested quanta. Current
CLI and image groups remain ongoing.
[Receipt](../measurements/language-tasks-oop-20261027.json).

The older 642e79e2 candidate has now completed all 63 Host checks. Its final image
group passes, and the exact 822-file installation manifest and receipt remain
unchanged. This closes that older candidate's qualification; it does not qualify
production corrections added afterward.
[Receipt](../measurements/machine-image-oop-20261027.json).

## Results through collecting cleanup

The result-cleanup harness now exercises a compiled synchronous result and all
three Reflect inventories through the production Host handoff. Replacing Roots
`release:` with a collecting Method does not reclaim the result before the caller
receives it. Observations use weak allocation identities and verify that the
watched allocation is the actual returned value.

The explicit `releaseTokensProtecting` failure case still drains both requested
tokens, keeps its result alive through every cleanup callback and propagates the
first native failure. Failed cleanup and a failed inventory handoff then release
their temporary values. Codegen/Execution owner-local scopes no longer call Roots
on each exit; the test uses the actual Host cleanup path instead of reintroducing
that implementation dependency or the old byte-token carrier. All checks pass
without runtime or Id changes.
[Receipt](../measurements/result-cleanup-oop-20261027.json).

## Observable Integer/Word conversions

The conversion-observability harness now uses declared Oop Methods. A replaced
Word constructor is entered for both Integer arithmetic inputs, can retain their
distinct temporary objects through collection, and can change the arithmetic
result. A replaced Word getter changes result conversion and is entered once.
Restoring the original methods restores the original arithmetic result.

Collection runs before the fixture reads the constructor's observations into the
caller scope, so those reads cannot accidentally supply the tested roots. All
checks pass without runtime or Id changes.
[Receipt](../measurements/integer-box-oop-20261027.json).

## Fixed native-adapter null validation

Porting the fixed-adapter harness exposed an input-validation regression: a null
result factory reached dispatch and returned an ordinary Error rather than the
native invalid-request failure required by the existing fixed-call contract.
Execution now checks required arguments for null together with arity, before any
conversion, target or factory send. The subsequent operation sequence is unchanged.
This adds no message, lookup, representation, registry or lifetime mechanism and
changes no Id operation. Execution owns the argument-shape check; Word/Bytes
owners still interpret their values.

The migrated harness covers all seven forms and every null argument position,
using distinct failing dependencies to reject validation that occurs too late.
It also preserves full-width values, optional absence versus zero, collecting
input lifetime, late factory/target replacement and exact native failures. Valid
immediate and boxed Oops remain accepted where their owner protocols apply; the
old carrier categories are not restored.

The before test fails, the corrected test passes, six native targets pass, all
26 Execution unit tests pass, and the core Oop gate passes including its dispatch
contract checks and independent C caller. The running a3eaa227 cold qualification
predates this correction and does not qualify it.
[Receipt](../measurements/fixed-adapters-oop-20261027.json).

## Byte snapshots through explicit adapters

The byte-adapter harness now uses Oop byte owners and declared callbacks. Targets
and result factories mutate the original byte owner, release its external token
and collect; the copied argument/result remains unchanged. Native failures retain
their exact identities. Absence and language nil normalize as specified, while
valid immediate results remain Oops.

The harness retains discard, Boolean/Word helper, malformed-input and retired-API
checks. It also verifies that invalid byte results never reach the factory and
that the Bytes facade's snapshot survives later source mutation. The byte and
fixed-adapter harnesses pass together on the diagnostic prefix containing the
null-validation correction. No further runtime or Id change was needed.
[Receipt](../measurements/byte-adapters-oop-20261027.json).

## Allocation guard integration through Method facts

The allocation-guard harness now uses current Oop Methods rather than the old
native-entry discovery helper and byte-packed rows. Exact contexts participate in
binding facts. Replacing ObjectModel allocation, vtable allocation or GC
preparation prevents omission without executing the changed operation. Current
query failures propagate, and forwarding replacements cannot make captured
canonical queries approve their own replacement.

A captured permission Method also declines an allocation owner with custom method
lookup without entering that lookup. Restoration is observed immediately. The
owner integration test and adjacent binding-fact test pass without runtime or Id
changes. [Receipt](../measurements/guard-batch-oop-20261027.json).

## Earlier cold candidate CLI qualification

All 18 CLI checks now pass for the a3eaa227 cold candidate, including standalone
artifacts, production Lang, Alien compilation, core operation without Store,
optional providers, image format, documentation, portable execution, workers and
project queries. Its image group remains running. This candidate predates the
fixed-adapter null-validation correction; the newer cold candidate is qualified
separately. [Receipt](../measurements/language-cli-oop-20261027.json).

## Schema-tagged table data through Oops

The tagged-table harness now uses Oop values and existing declared callbacks.
It preserves schema decoding, locator/index queries, source positions after
selection, and new base ordering after concatenation. Current Integer getter and
constructor replacements remain observable, including collection and changed
bounds. Schema tags belong to Bytes-owned table data; no payload-type catalog is
introduced in Id. The harness passes without runtime changes.
[Receipt](../measurements/tagged-rows-oop-20261027.json).

## Corrected adapter candidate cold installation and archive

Commit 1fa7e3c2 builds from a clean detached checkout with no Birnam on PATH and no
build cache. Its exact 826-file published closure verifies; the build-only Lang
seed is excluded. Archive qualification passes signatures, relocation, source,
diagnostics, GC, saved images and compiled startup without parsing while the
original source and prefix paths are hidden. Both paths are restored afterward.

Fresh immutable Host test binaries are prepared and the full serial Host groups
are now running against this candidate. This installation/archive receipt does
not claim completion of those groups or the remaining native/compiler migration.
[Receipt](../measurements/fixed-adapters-cold-install-oop-20261027.json).

## Reentrant invocation arguments

The invocation-argument harness now uses a declared native checkpoint and an
actual compiled Method with the existing nullable signature. Both collect and
reenter while preserving the outer Oop argument array. Native calls preserve raw
absence; the explicitly typed authored entry normalizes it to language nil.
Receiver, immediate, language-nil, byte-object and zero inputs remain intact.

All sixteen outer/nested checkpoint entries pass. The harness is now included in
the native-message gate. Raw function-pointer construction and carrier packets
were not recreated; no runtime or Id changes were needed.
[Receipt](../measurements/invocation-arguments-oop-20261027.json).

## Actual Globals initialization without Store

The isolated Globals harness now constructs its native owners through Oop messages
and loads the real compiled Error/Globals initializers. Its input directory has no
Store, Relational, Bootstrap, Lang or Generation library, and no Globals seed or
Namespace alias substitutes for the authored owner. The test-only composition
helper uses the existing public loaders and owner operations.

Bound nil, binding snapshots, identity keys, ordinary reachability, reclamation
and repeated initialization pass. A separate second-phase directory supplies Store
and Relational; loading them preserves the Globals owner, creates no implicit
global facts, and leaves explicit Store facts independent of Globals mutation.
The runner hashes the composition helper and allocation observer with its inputs.
No runtime or Id change was needed.
[Receipt](../measurements/globals-without-store-oop-20261027.json).

## Actual Task initialization without Store

The Task isolation harness now uses the Oop native-only composition and actual
compiled initializers. Existing source assertions remain byte-for-byte unchanged:
Cell identity replacement, Concurrency field aliases, shared REPL resource close,
local wait/resolve and cancellation cleanup all pass. A retained field view alone
keeps its owner and payload alive; removing that root reclaims all three, observed
by allocation identity.

Store, Relational, Bootstrap, Image, Alien and native Isolate remain physically
absent. Resource methods that require those optional services are not claimed by
this test. The smaller Globals isolation check also passes after the shared helper
extension. No runtime or Id change was needed.
[Receipt](../measurements/task-without-store-oop-20261027.json).

## Actual Alien initialization without Store

The Alien isolation harness now uses the same Oop service composition. Its actual
recipe initializer and unchanged source assertions pass for native wrapping,
collection, recipe reuse and authored replacement with registration retirement.
Store, Relational, Bootstrap, Image and Isolate are absent from the input directory.
No runtime or Id changes were needed; this does not replace full Host or image
qualification. [Receipt](../measurements/alien-without-store-oop-20261027.json).

## Cell, record and registry isolation through Oops

The Cell isolation harness now loads actual Error/Globals/Cell initializers through
the native-only Oop composition. The combined registry, ordinary-record and
Test/Bench run passes with the original exported programs. It preserves live
accessor/constructor replacement, snapshots, identity replacement, aliases,
registry reachability and reclamation after releasing the final test root.

Named adapters use the existing Codegen template operation and `withImplementation:`;
the retired NativeBackend template emitter is not restored. Reclamation uses
allocation identities instead of querying freed references. Four Cell-specific
helpers were added without changing the shared helper's existing functions, and
the adjacent Globals isolation check passes. No runtime or Id changes were needed.
[Receipt](../measurements/cell-without-store-oop-20261027.json).

## Image isolation and explicit Store participation

The isolated Image harness passes all ten cases through Oops: optional Store
save/mapped/rebuilt controls, Store-free save/mapped/rebuilt/resave, mapped/rebuilt
recovery of that resave, and rejection when an image's Store provider is missing.
Existing authored assertions, documentation hooks and CLI/LSP boundaries remain.
The base directory excludes Store, Relational, Bootstrap and Isolate; positive
provider controls add Store alone. No runtime or Id changes were needed.
[Receipt](../measurements/image-without-store-oop-20261027.json).

The corrected cold 1fa7e3c2 candidate also passes all 17 Host generation checks in
one unchanged run. Its remaining Host groups continue separately.
[Receipt](../measurements/fixed-adapters-generation-oop-20261027.json).


## Minimal mutation: passing behavior, failing construction purity

The isolated Oop fixture now uses the existing Codegen template operation,
`withImplementation:`, direct Method observations and declared callbacks. Store,
Relational, Generation, Bootstrap and Lang are absent. Unlike the old fixture,
it requires no Globals alias. Inheritance, warmed lookup, captured/entered Method
lifetime, current implementation sends, replacement epochs and Reflect mutation
all pass in a separate process. Reclamation checks use weak allocation identities.

The default runner also executes the construction-purity case and remains failing.
Codegen creates four metadata getter/binder Methods in a private vtable and
reassociates each new Method, changing the coarse epoch five times. That conflicts
with the documented zero-epoch-change preparation contract. The distinction is
now explicit: ordinary mutation behavior is proven, construction purity is not.
No runtime fix, new Id protocol or private metadata registry has been introduced.
[Evidence](../measurements/minimal-mutation-epoch-oop-20261027.json).

## Host qualification limits after the purity finding

The earlier a3eaa227 candidate has completed all 63 Host checks across its recorded
unchanged generation retry. Its 827-file installation manifest remains exact.
The corrected 1fa7e3c2 candidate has also completed all six interaction checks and
continues with tasks, CLI and image groups. These suites do not establish the
construction-epoch purity requirement: that independent contract still fails and
remains unresolved. The earlier candidate also predates the null-validation fix.
[Earlier image receipt](../measurements/language-image-oop-20261027.json),
[corrected interaction receipt](../measurements/fixed-adapters-interaction-oop-20261027.json).


## Capture reader acceptance through Oops

The old `capture_access` harness now exercises the current Oop operations. The
runtime sequence remains generated function → current `Block captureWord:of:` →
current `captureWordAt:` → Execution-owned capture storage. No Id or production
runtime code changed, and no raw-pointer capture borrowing or grant API was added.

The test retains alias, bounds, allocation-admission, replacement, failure and GC
checks. Both reader replacements return fresh results; collection during the
second read must retain the first result. A replacement installed during the first
read must affect the second. Nested invocation and collection preserve captures
held only by their Blocks, and weak allocation identities confirm reclamation once
those Blocks lose their roots. Explicit captured-body invocation on nil still
returns nil. Negative checks require the old borrowing and grant selectors to stay
absent. The fixture interprets no runtime payloads and uses only Id messages.

The migrated test, capture dependency test and captured-Block-body test pass. Early
fixture mistakes and their corrections are recorded in the
[receipt](../measurements/capture-access-oop-20261027.json). This is staged-prefix
component evidence; the Method-construction epoch violation remains open, and the
remaining old native harnesses still prevent full integration acceptance.


## Closure admission regression checkpoint

Before porting the remaining `closure_construction` assertions, the new Oop
`closure_admission` harness isolates the established contract from
[Method code admission](method-code-admission.md). Each case runs in a fresh
process; one failure cannot prevent the other cases from being exercised. The
native-message gate includes the new target, and the original harness remains
intact for its still-unported lifetime, image-shell and task coverage.

Six cases fail: a failing capture-count query is ignored during admission;
same-CodeObject admission re-queries shape; another CodeObject can overwrite the
association; synchronous and resumable Methods accept incorrect capture counts;
and raw absence is accepted as a capture. A supported immediate Oop is accepted
in the positive control, so the null assertion introduces no payload-type catalog.
Explicit external pairing retains its supplied CodeObject identity without changing
the original association and rejects a mismatched capture count.

The implementation explains these results. `oop_reflection::admit` only queries
arity, installs a new `codeObject` getter and overwrites the arity record on every
call. `from_operands` passes the supplied references directly to `make_block`,
which checks that a Method reference was supplied but not its capture count.
The external-pairing route has its own count comparison and passes that control.

This is failure evidence, not a completed migration. Restoring admission must
preserve the existing registration, replacement and lifetime contracts while
remaining entirely owned by Execution through Id messages. The outstanding
Method-metadata purity conflict must also be resolved before extending its
construction mechanism. No new Id hook, private cross-module API or replacement
application protocol is introduced by this checkpoint.
[Receipt](../measurements/closure-admission-regressions-oop-20261027.json).


## Root cleanup acceptance through Oops

The `root_batches` harness now exercises `releaseTokensProtecting` with Oop
tokens, replacing tests of the removed raw-byte batching APIs. Production behavior
is unchanged: validate all token representations, send the current `release:` in
reverse order, and return the first failure after attempting every release. Roots
owns the token inventory; the fixture uses only public Id messages.

The tests retain token identity, root inventory, malformed-input, replacement,
collection and failure checks. Exact successful inventory order matches ordinary
reverse release. Collecting callbacks replace `release:` during cleanup; logged
calls prove later releases enter the replacement, including after failures and
for repeated or absent tokens. Every successful token drains. Negative assertions
require the removed raw-byte batching selectors to remain absent.

An initial comparison found that the error path's temporary root protecting its
first failure changes swap-removal enumeration order relative to a plain loop
without error protection. The final test checks exact callback order and complete
draining on that path, without imposing the removed shortcut's success-only
inventory behavior. This distinction and the initial failure are retained in the
[receipt](../measurements/root-batches-oop-20261027.json). The migrated test,
result-cleanup and root-provider tests pass. Neither Id nor production runtime
changed; open metadata/admission regressions and other unported harnesses remain.


## Corrected cold candidate: task acceptance

The existing serial Host run on cold candidate `1fa7e3c2` has completed all six
task-group checks: task images, task fields, source-free worker concurrency,
multicore workers, closure observation and TaskControl. The final authored suite
reports 177 checks and zero failures, including quanta 128 and 1. No retry or
worker-deadline change was made. [Receipt](../measurements/fixed-adapters-tasks-oop-20261027.json).

CLI and image groups remain outstanding. These task results do not cover or repair
the independently reproduced Method-construction purity and closure-admission
regressions; full integration acceptance remains open.

## Generated root-token validation

Porting `root_reuse` exposed a missing validation in the Oop generated-code
prelude. `keep` accepted any immediate token, including encoded zero. A replacement
`Roots retain:` returning zero could therefore let later body sends run without
acquiring a root. The collecting checkpoint reproduced that continuation after
failed acquisition. The same final fixture fails against the earlier prefix.

The correction adds one zero-token check. Before: current `retain:` → check the
immediate representation → remember token → continue. After: current `retain:` →
check immediate representation and nonzero token → remember token → continue.
A failed acquisition still returns the exact failure; zero or absent tokens
produce the existing invalid-request failure. No message, lookup, allocation,
registry or lifetime mechanism is added. Roots owns token meaning; Id remains
unchanged and does not interpret the token or the protected object's payload.

The migrated test retains duplicate aliases, a captured marker whose external
root is dropped, forty simultaneous locals, precise collection during a body,
call-exit reclamation, exact acquisition failures, zero/absent token rejection,
and a successful call following failure. Observers retain only allocation
identities for values whose collection is being tested.

Root-registration multiplicity is recorded separately. The original synchronous
probe expected one fresh-local registration and zero capture registrations; the
current Oop emitter makes three and four, respectively. Its forty-local probe
makes 42 local registrations and one capture registration. These are callback
counts, not elapsed-time measurements. The
[original optimization contract](fixed-names-and-local-protection.md#protection-within-a-call)
explicitly excludes private registration multiplicity from required observables;
no new cache or capture grant was added to chase those historical counts.

This change affects newly generated code. The separate cold Host run still uses
candidate `1fa7e3c2`, whose compiled artifacts predate this correction. Its results
do not qualify the new generated prelude. The Method-construction purity and
closure-admission regressions also remain open.

Validation passes for the migrated test and four adjacent capture/input/cleanup
tests, plus the Oop core check, including the negative dispatch-contract test,
independent C caller, default and resumable compiler integration and 177 authored
TaskControl checks. [Receipt](../measurements/root-token-validation-oop-20261027.json).


## Word expression and helper acceptance through Oops

The Word expression and authored-helper harnesses now use Oop Methods, ordinary
Bytes for expression data and independently declared C callbacks. No production
runtime or Id code changed. The original authored arithmetic/helper sources are
retained. Explicit evaluation observes current primitive bindings and allocation
permission, including revocation without a binding change and collection plus
primitive replacement during permission. Refusal does not run the replacement;
subsequent ordinary invocation does. Maximum-size and malformed programs are
covered, as are supported immediate inputs and absent-input rejection.

Helper checks preserve current body, capture-reader and primitive replacement.
Replacing the helper body during its collecting capture read cannot redirect the
already-entered Method; the first call finishes with its original result and the
next call enters the replacement. This tests current and entered-call meanings
without reinstating the former compiler helper proof machinery.

The automatic producers are still missing. Both generated expression examples
allocate four Words, compared with the historical one-Word assertion; the explicit
evaluator still allocates one. The automatic helper-plan observer records zero
calls, compared with the historical one-call assertion. These are measured counts,
not elapsed-time results. The migrated tests preserve the language and explicit
owner-operation checks while recording this compiler gap separately; they do not
establish automatic producer completion or performance parity.

Both targets and the adjacent guard/Word-query-order targets pass.
[Receipt](../measurements/word-plans-oop-20261027.json). The metadata/admission
regressions, automatic producer work and remaining native/compiler migrations are
still open.


## Native publication preserves callback-local handling

Migrating the publication boundary harness exposed an implicit policy change in
typed native invocation. The adapter set Execution's ambient failure-preservation
flag while invoking its body. A compiled native-facing publisher consequently
prevented an authored observer from handling the Error produced by its own inner
native send. Native Store and the directly materialized synchronous policy both
allowed that handling; the compiled policy returned the raw failure and omitted
the callback's `caught` event. The final Oop fixture reproduces this on the earlier
prefix.

The fix removes the implicit flag change and its restoration. Before: validate
signature → set ambient flag → convert inputs → invoke body/callbacks → convert
result → restore flag → report failure or answer. After: validate signature →
convert inputs → invoke body/callbacks → convert result → report failure or answer.
There is no new message, lookup, allocation, registry or lifetime mechanism. Id is
unchanged; Execution still owns conversions and Machine result handling. Existing
explicit operations are not replaced by a private publication protocol.

All 27 retained publication comparisons pass: nine scenarios against native Store,
a directly materialized synchronous policy and an explicitly compiled native
policy. They preserve pre-commit denial/failure, post-commit failure, ordinary Error
values, callback-local handling, exact native identity, current replacement and
non-replayed effects. A normal source caller handles the native failure at its own
boundary. Six adjacent native/conversion targets pass, including native-boundary
mapped/rebuilt images, and the Oop core check passes with its negative dispatch
check and authored control coverage.

The earlier document's adapter-comparison experiments are explicitly superseded
by the recorded user rejection of dual language error conventions and backwards
compatibility for that experiment. Their negative assertions are excluded from the
active test; the initial local-preservation difference is retained as historical
probe evidence, not repaired by restoring the rejected policy. The authored
fixture itself remains unchanged. One intermediate test classifier used Symbol
display text instead of identity; its failure is recorded, and the final rerun
compares Symbol identities.

[Receipt](../measurements/publication-callback-policy-oop-20261027.json). This is
not a new cold installation qualification. The serial Host run on `1fa7e3c2`
predates this correction and the root-token fix. The Method metadata/admission
regressions and remaining compiler/harness work remain open.


## Generated call preparation acceptance through Oops

The former compiler call-window group now runs in `compiler_source` through the
current `prepareCall:selector:` and frame operations. Both original source programs
remain: ordinary/tail calls and ordered local aliases. Exact identities survive
collecting replacements of preparation, frame reads/writes, call/tail-call entry,
Namespace lookup and allocation preparation. A failed preparation is entered once
and returned without replay. Budget-one runs expose actual argument identities,
collect between resumptions and reject stale views after completion. Native units
and bytes return to their pre-test baseline.

This uses the existing declared test callbacks and changes no runtime or Id code.
The old optimization-mask comparison, optional `tryPrepareCall:slots:...` routes
and Integer-index boxing guard are not recreated for the current emitter, which
uses immediate Oop indices and the established owner preparation operation. The
new check establishes behavior and live operations, not the former optimization's
allocation/timing claim. Authored preparation remains separately exercised before
and after compiled stdlib loading by the same suite.

The full `compiler_source` target passes. The old compiler target now retains
three groups: discarded-clone, numeric-predicate and alias-return. Other legacy
harnesses and the documented metadata/admission regressions remain open.
[Receipt](../measurements/compiler-frame-calls-oop-20261027.json).


## Discarded clone acceptance through Oops

The discarded-clone group now runs in `compiler_source`. It retains the original
source cases for ignored results, selector aliases, lexical bindings and escaping
clones. Replaced clone methods execute once, collecting callbacks preserve the
operation, and actual allocation/GC operations remain current. The explicit
`ObjectModel canDiscardClone:of:` query accepts the canonical empty clone and
rejects a different Method, nonempty source or changed declared dependency.
Paused-frame reads use no test root for the returned slot value; weak allocation
identity confirms that the frame keeps its discarded clone alive during collection.
Generated native units and bytes return to their prior baseline.

The first port assumed that clone entered public `ObjectModel allocate:bytes:`.
Source history before the Oop migration (`cf655b68^`) already uses the private
`m.allocate` helper, exactly as the current clone does. The old broad callback
count also included runtime bookkeeping. The final test preserves the explicit
permission dependency and observes clone's actual behavior allocator; no extra
public self-send or runtime mechanism was introduced to reproduce that counter.

Ordinary execution currently creates one physical discarded clone. The former
automatic elision and its timing/allocation-reduction comparison are not restored
or claimed by this test migration. The full compiler-source and ObjectModel targets
pass. Shared old helpers now live in the legacy compiler harness solely for its
two remaining groups, numeric predicates and alias returns.
[Receipt](../measurements/compiler-clone-oop-20261027.json).


## Corrected cold candidate: CLI acceptance

The serial Host run on cold candidate `1fa7e3c2` has completed all 18 CLI
checks, including provider-independent core execution, worker behavior, native
wrapping, portable execution and project queries. Image checks are now running.
No failed CLI check was retried or deadline changed.
[Receipt](../measurements/fixed-adapters-cli-oop-20261027.json).

This candidate predates the generated root-token validation and typed-callback
policy corrections. Its passing CLI group does not qualify those later fixes or
repair the open Method metadata/admission contracts.


## Numeric predicate acceptance through Oops

The numeric-predicate source group now runs in `compiler_source`. It retains
checked arithmetic, signed extrema, overflow/division errors, existing comparison
rounding, exact equality, live method/factory/allocation effects, selector aliases,
live Error-family designation and mixed Float inputs. Collecting replacements keep
execution correct. Paused-frame inspection confirms the same intermediate Integer
identity after collection without keeping that Integer in the caller's test scope.
Native units and bytes return to their previous baseline.

A separate `integer_predicate` target exercises the existing explicit Integer
predicate operation on the native-only floor. This makes its successful path
observable independently of the published authored arithmetic: the collecting
visitor commits once, while arithmetic refusal, stale epoch, changed numeric
Methods and custom visitor lookup commit nothing. Exact visitor failures propagate
once and subsequent evaluation still succeeds. The native-message gate includes
this target. No runtime, Id message or private cross-module interface was added.

The initial port counted incidental additions as source-operation repetitions;
the final declared fixture retains the old receiver-specific counter and forwards
other calls through their captured Method. The same filter scopes the additional
replacement-result check. Those failed fixture probes remain in the receipt.

The full compiler-source suite, explicit Integer predicate test and explicit
Execution arithmetic test pass. Automatic predicate evaluation still records zero
calls in generated source, so its former optimization is not claimed. Only the
legacy alias-return compiler group remains in `compiler.rs`; the other documented
metadata/admission and harness work remains open.
[Receipt](../measurements/compiler-predicate-oop-20261027.json).


## Alias-return acceptance and compiler harness consolidation

The alias-return group now runs in `compiler_source`. The original alias-chain
body preserves its input identity across binding changes. A real source literal
supplies the Integer-constructor effect, replacing the old assumption that routing
indices require Integer allocation. Current constructor/frame-reader effects agree
for separately created equivalent bodies; collecting replacements, changed callee
behavior, bounded frame visibility and code reclamation pass.

The old `compiler.rs` harness and its remaining Operand/Outcome helpers are removed
from the active gate. Its source acceptance groups now live in the Oop compiler
suite. The explicit pending-call candidate/recovery target also passes. This does
not enable the rejected automatic alias-return producer: compilation records zero
candidate attachments, and the earlier no-savings/slower experiment remains valid
historical evidence. No production runtime or Id change was made.
[Receipt](../measurements/compiler-alias-oop-20261028.json).

The remaining legacy runtime harnesses, automatic producer gaps and documented
Method metadata/admission issues still prevent full integration acceptance.


## Fixed-adapter cold candidate: all Host groups complete

The serial Host run on cold revision `1fa7e3c2` is terminal and passed all 63
checks across generation, interaction, tasks, CLI and image groups. The final
image group passed 16 checks. The source checkout remains clean at that
revision, and the original receipt and exact 826-file installation manifest were
reverified after the run. No failed group was retried.
[Image receipt](../measurements/fixed-adapters-image-oop-20261028.json),
[complete Host receipt](../measurements/fixed-adapters-host-complete-oop-20261028.json).

This closes qualification of that specific cold candidate. It predates the
root-token validation and callback-policy fixes and does not qualify them. The
open Method purity/admission failures and remaining runtime harness work also
remain independent blockers to declaring full language integration complete.


## Explicit numeric owner acceptance and remaining compile blockers

The remaining numeric-consumer harness now uses Oops in separate native-only and
published-language processes. In each context, 350 ordinary sends still work with
a failing optional numeric query installed: Id enters the selected Method directly.
Explicit evaluation recognizes canonical native arithmetic, preserves a captured
Method's meaning after replacement and declines authored or changed-context
Methods. Current visitors can collect, decline or fail; refusal and exact failure
handling pass without invoking an authored entry through the native evaluator.
The two adjacent explicit numeric targets pass as well.
[Receipt](../measurements/numeric-owner-oop-20261028.json).

The old automatic hot-Machine interception is not restored or claimed by these
checks. Its uncompiled harness is removed, along with the native-message script's
injection of the old execution-machine dylib into the supplied prefix. Explicit
PC/inline candidate tests remain separate. No runtime code or Id operation changed.

An actual compile-only audit accounts for all 96 distinct Rust targets in the
native-message gate: 93 compile; `binding_undo`, `shared_invocation_probe` and
`closure_construction` still fail on removed carrier APIs. No target is unaccounted
for. This is not a passing full gate: the isolated checks remain separate, and the
compilable closure-admission and construction-purity tests retain known failures.
[Compile audit](../measurements/native-gate-compile-audit-oop-20261028.json).

A fresh cold installation at `31514b0c` has completed successfully with no Birnam on
PATH or build cache. It includes the root-token and callback-policy fixes. Exact
installation verification passed; archive relocation is now running with the build
source and original prefix hidden. No archive or full Host success is claimed yet.


## Root-token and callback-policy fixes: cold installation and archive

Cold candidate `31514b0c` builds from a clean source checkout with no Birnam on PATH
and no build cache. Its relocated archive passes with the original source and
prefix hidden, and the exact installation manifest verifies after those paths are
restored. This candidate includes the root-token validation and callback-policy
corrections. [Receipt](../measurements/latest-fixes-cold-install-oop-20261028.json).

It predates the pending image-shell correction. Full Host qualification of a final
corrected candidate remains outstanding, along with the documented Method
purity/admission failures and remaining legacy runtime harnesses.


## Pending image shells and closure construction

The Oop closure-construction port reproduced an execution-readiness defect:
`imageShellFromMethod:codeObject:` marked a Block as pending, but `value` ignored
that state and `clone` dropped it. An unfinished closure could therefore run with
unrestored captures. The valid construction, nested-closure and task cases passed
independently before the fix.

Execution now rejects its existing pending flag before entering a Block body and
copies that flag with a cloned Block's fields. The clone snapshots it before any
outgoing callback and sets it before returning the copy. There is no new message,
allocation, registry or lifetime mechanism; Id and generic allocation/dispatch/GC
remain unchanged. Execution alone interprets this existing owner state.

Both synchronous and resumable shells and their clones reject execution until
restoration. Wrong-size restoration leaves them pending; successful restoration is
one-time. A root Machine reports the ordinary invalid-request Error before
restoration and returns the captured value afterward. Capture ordering, aliases,
constructor-time collection, admitted-code lifetime, twelve nested captures and
zero-capture literals pass. The original task source text is unchanged and passes
wait/resume/GC/cancel/cleanup at quanta 128 and 1.

The four isolated closure groups, adjacent capture/materialization checks, image
snapshot check and Oop core check pass.
[Receipt](../measurements/closure-shell-oop-20261028.json). This does not repair
Method purity or code/count admission: all six dedicated admission failures were
rerun and remain failures. Additional historical admission-validation variants
remain part of that future correction, rather than being certified by valid-path
construction tests.

The compile-only native gate audit now accounts for 94 compiled targets and two
legacy failures: `binding_undo` and `shared_invocation_probe`.
[Audit](../measurements/native-gate-closure-audit-oop-20261028.json). Full runtime
acceptance and cold qualification of the shell correction remain outstanding.


## Confirmed construction and rollback contracts (2026-10-28)

The user explicitly reaffirmed construction purity as a project invariant. The
current per-Method metadata-vtable implementation is a violation, not a reason to
allow constructor epoch advances. Current Invariants now expressly forbid that
workaround and require coherent owner metadata above Id, with tracing/retirement
and admission designed together. The failing purity and admission assertions are
unchanged. The architecture-cleanup document records this decision; no particular
replacement representation or new Id API has been approved.

The user selected value-checked rollback for simplicity. The obsolete atomic Id
undo/ABA harness is removed. Existing authored transaction acceptance now explicitly
checks that a differing current binding conflicts, returning to the expected
binding permits rollback despite epoch changes, completed rollback is idempotent,
and loading coordination does not install the retired Id restore primitive.
The transaction target passes for its synchronous and resumable candidate paths.
No runtime change was needed. [Receipt](../measurements/value-checked-rollback-oop-20261028.json).
