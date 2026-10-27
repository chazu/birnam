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
retained for retries at lower concurrency. The other three groups continue. The
cold/archive result does not substitute for those groups or the remaining native
migration work.
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
