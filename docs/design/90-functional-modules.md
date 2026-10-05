# 90 — Functional modules, minimal dependencies

> **Status:** Accepted organizing direction, 2026-09-22. Concrete message
> contracts are specified beside their owning implementations as they are
> migrated. This is a break with extraction-driven modularization, not another
> installment of it. Implementation progress is recorded below.

**Current priority (2026-09-24):** Pause additional optimization/deoptimization
work and reconnect the remaining functional modules through Id messages until the
system as a whole works again. Keep existing correct optimizations and execution
fallback/recovery/lifetime behavior; unfinished optimizer families are not
prerequisites for module migration. This supersedes earlier implementation-order
wording that places those families ahead of upper integration. GC and numeric
visitors now use receiver sends; optimization remains paused.

## Decision

Organize modules first by what the code does, then by the irreducible dependencies
between those functions. Do not organize them around the existing crate inventory,
old ABI tables, initialization stages, commit sizes or opportunities to extract
another helper. Module boundaries and work batches are different decisions.

Birnam objects are opaque references. Modules interact through Id messages.
Siphon has only the ad hoc knowledge needed to load Id; the real Id protocol loads
everything above it. Id implements itself and does not know the upper language.

This design governs the remaining refactor. It carries forward Design 84's
prohibition on parallel native APIs and the semantic obligations retained from
Design 83. Earlier documents describe history and preserved behavior; their crate,
factory and table boundaries are not requirements to recreate.

## What went wrong

We split implementations into many libraries while preserving their coupling in
shared descriptors, function tables, foreign payload layouts and aggregate runtime
state. Loading those tables through Id did not make their use message-based.
Further extraction multiplied interfaces without establishing functional owners.

The correction is to keep useful algorithms, place their state with the function
that owns it, replace cross-owner calls with messages, and delete the superseded
integration. Moving the same tables to another crate or hiding their pointers in
Words repeats the mistake.

## Ownership

These are functional responsibilities, not a mandate for one new crate per row.
An owner can contain ordinary private Rust modules and helper functions.

| Function | Responsibility and boundary |
|---|---|
| Id | Identity, allocation records, Symbols, dictionaries, dispatch and native loading. No upper-module catalog or language service dependencies. |
| Object behavior | Additional language semantics for cloning, private behaviors and reflection above Id. Remove redundant wrappers around operations Id already supplies. |
| Roots and Namespace | Retention and publication of completed objects. Keep their existing working protocols; neither becomes a repository of module APIs. |
| Memory management | Collection phases, allocation accounting, marking, sweeping and finalization. Consolidate supporting lifetime, tracing, mark-scope and native-stack machinery where it serves only this function. |
| Values | String, Integer, Float, Word and Error own their respective representations and operations. Text construction includes String Builder. Each value owner enumerates its own references and manages its own native resources. |
| Collections and control | Vector, Map and Range own distinct collection implementations. Collection owns shared collection semantics. String splitting belongs with String; block iteration with Block/control behavior. Separate installer crates are not required. |
| Execution | Calls, Blocks, frames, continuations, scheduling, suspended execution and recovery. Guarded dispatch caches and method observation belong with the execution/optimization machinery that uses them. |
| Parsing and compilation | PEG owns matching; Lang owns Birnam grammar; Codegen owns translation and optimization; TCC owns native compilation. Artifact codecs and native compiler helpers can remain internal libraries. |
| External services | File access, environment, terminal I/O and foreign calls remain distinct functions. Sharing the System receiver does not imply one implementation module. |
| State and persistence | Store owns facts/history; Relational owns queries; Image owns persistence/restoration. Image uses owner protocols rather than interpreting foreign object layouts. |
| Workers | Isolate owns worker lifecycle and communication. Execution owns local scheduling. Preserve the implemented concurrency behavior without expanding its scope. |
| Composition and tools | Bootstrap orders construction; Host manages process/application startup; CLI parses and dispatches commands; the build tool builds and installs. These do not own the language functions they assemble. |

`runtime` must stop being the residual owner of unrelated functions. Move retained
implementations to their functional owners when repairing those paths, without
creating a new universal Runtime service object containing every operation.

The initial helper inventory (`payload-layout`, `trace-hooks`, `finalizers`,
`native-stack`, `native-mark`, `dispatch-cache`, `method-observation`) did not imply seven
new service designs. Keep their useful algorithms with the actual owner unless
an independently useful function justifies a separate module. Representation
layouts stay private to their representation owners. Generic language-slot
metadata belongs to the implementation of those slots, not a shared description
of every native value. Storage diagnostics may consume observations without
becoming a prerequisite for ordinary execution or collection.

## Dependencies and construction

Depend on a function because an operation needs it, not because its predecessor
constructor happened to accept a descriptor for it.

- Collection needs Id, roots and a reference-enumeration protocol. It does not
  need a catalog of String, Integer, Execution or future modules. It must work
  over the objects already present; later completed owners register their roots
  and reference enumeration through messages.
- Basic String construction does not inherently need numbers. String operations
  returning numbers or converting text do. The current String base/text split
  is useful implementation structure, not a reason to add another library.
- Integer arithmetic does not inherently need Float. Mixed arithmetic needs both;
  formatting needs text. The existing Error representation needs String, so error
  construction retains that dependency unless its representation is deliberately
  changed. Do not change language error behavior just to simplify a graph.
- Execution can invoke already available methods without a source compiler.
  Compilation produces executable methods. Optimization uses execution's recovery
  contract; numeric value implementations do not depend on their optimizer.
- Store need not depend on Relational or Image because they consume it. Image
  needs restoration protocols for the representations it persists. Workers need
  completed execution and their transport/serialization facilities.

A dependency cycle is a reason to inspect the functions on each side, not to
introduce an empty service slot. An extension installed after its dependencies
exist is valid: its implementation is complete when installed. Forward-filled
callbacks, placeholder objects and dependency bundles are not.

Native Words/Bytes carry bounded native data where language values are not yet
available. They never carry a dependency descriptor or foreign layout. Native
entries support actual method invocation and necessary synchronous callbacks;
they do not authorize capturing another owner's service implementations. Future
sends observe replacement. An already selected active call retains the code it
needs until return, including suspension and recovery.

## Alien code, trust and proportionate protection

Clarification (2026-09-24): simplicity, late-bound Id message passing and minimal
dependencies govern defensive implementation too. Birnam does not promise to
make arbitrary alien (wrapped C) code safe. Calling native code in the same
process is a trust decision, not a security isolation boundary.

Alien callers and wrapper authors own the foreign library's requirements:
static/global state, initialization and teardown, thread safety, reentrancy,
pointer validity, buffer sizes, callback lifetimes and foreign resource ownership.
A wrapper must report and retain Birnam references that its native state keeps,
and use the existing owner/thread and callback contracts when entering Birnam.
Birnam does not discover hidden references, repair foreign state, or add global
locks, per-library runtime machinery or module coupling to compensate for an
alien library's behavior. Library-specific adaptation belongs in its wrapper.
The wrapper's actual C ABI calls are private implementation details; other Birnam
owners use its objects and messages.

Birnam takes reasonable, local security precautions at the boundaries it owns:
for example, validating lengths and executable metadata it consumes and honoring
existing access restrictions. Each added protection must address a concrete
problem with proportionate complexity. This is not a mandate to contain hostile
native code, sandbox every foreign call, or create a new security framework.
An inability to guarantee arbitrary C behavior does not justify ignoring a
concrete vulnerability in Birnam's own boundary handling.

Compiled Birnam remains subject to the ordinary language contract. The compiler
and runtime must preserve dispatch, identity, roots, recovery and code lifetime;
they need no separate defensive architecture merely because execution uses native
instructions. Retain the declared owner of an entered call; do not inspect or
take over arbitrary native context internals. These obligations preserve Birnam's
own semantics and do not extend to repairing a native author’s contract violations.

Neither safety nor security authorizes bypassing late binding. In particular:

- Do not capture another owner's service methods, add parallel function tables,
  decode its private payloads, or introduce dependency bundles to obtain a
  supposedly safer path. Ordinary inter-owner operations remain current Id sends.
- An explicitly captured Method, an already selected active call, and a guarded
  optimization retain their established semantics. A guard must preserve current
  lookup/replacement behavior and fall back when its assumptions fail; it is not
  permission to freeze a dependency for future calls.
- If an optimization cannot preserve its assumptions across a message or callback,
  materialize, recover or decline it. Do not suppress replacement or strengthen
  module coupling to make the optimization applicable.

Review defensive additions against the concrete Birnam invariant they preserve
and the owner that needs them. A hypothetical misbehaving C library is insufficient
justification for complicating the shared implementation. This clarification
governs the remaining batches and supersedes broader interpretations of earlier
native-safety requirements.

## Optimization preserves language behavior, not allocation history

Decision (2026-09-24), following the user's clarification and a
[review of Self's paper and implementation](90-self-optimization-review.md):
optimization exists to improve performance while preserving Birnam's late-bound
behavior. Existing machinery is not evidence that its policy is necessary.
This decision supersedes allocation-equivalence requirements in earlier designs,
receipts and migration queues, including 74T/74V/74Y/74AD and 81BR/81BZ.

Keep these obligations:

- Subsequent sends observe current lookup and method replacement. Optimized
  operations must establish the semantics of the actual selected implementation;
  a selector name is insufficient. Guard, recover or use ordinary execution when
  assumptions fail. Preserve effects, errors and their language-defined order.
- Preserve observable object identity, aliases and shared mutable Cells. An
  existing object remains that object; equal values do not authorize merging
  distinct observable identities. Materialize a virtual value when execution,
  escape, reflection, continuation capture or frame inspection requires it.
- Reconstruct the values and logical frames required at supported recovery and
  observation points, including inspectable values no longer used by subsequent
  computation. Retain roots and active/suspended native code ownership throughout.
  Recovering current state must not replay effects already performed.
- Preserve explicit collection operations and real finalization/resource duties.
  Allocation or finalization with program-visible effects is not pure merely
  because its result does not escape. Unknown or replaced constructors, clone
  methods and finalizers require ordinary execution unless their effects are
  specifically accounted for by the optimization.

Drop these obligations:

- Equal baseline/optimized allocation counts, allocated bytes, reclaimed counts,
  automatic collection thresholds reached at identical program points, or identical
  automatic finalizer timing. Memory diagnostics describe the execution that
  actually occurred; their results may change with optimization.
- Charging for objects never allocated; journaling dead payloads or counted index
  boxes; recreating unreachable objects solely to reproduce the baseline heap,
  finalizer observations of that heap, or collection statistics.
- Prepaid allocation identities and single-charge materialization as prerequisites
  for elision. Recovery allocations are real allocations and use normal owner
  construction/accounting. Support collection and callbacks with rooted state and
  revalidation where needed; no simulated charge or callback-free service path.
- Restoring retired allocation-budget/deferred-identity protocols simply because
  retained optimizers used them. Identity bookkeeping needed by a concrete virtual
  value belongs in that optimizer's recovery state; it does not require a new
  global identity owner or cross-module protocol.

An explicitly requested collection still runs over the actual heap with the
existing root/finalization contract. Its statistics need not describe objects an
unoptimized execution would have allocated. An optimizer may not discard a real
cleanup obligation; automatic cleanup timing is not a promise of equivalence to
the unoptimized allocation schedule.

GC's checked accounting for actual construction remains useful. The speculative
`canAllocate:` query was removed in the subsequent reassessment: no production
consumer needed it. Keep threshold policy private to GC; do not introduce an
optimizer budget protocol without a concrete semantic need.

Acceptance compares language results, effects, replacement behavior, observable
identity/aliasing, recovery/control state and lifetime. Collect deliberately at
supported observation and materialization boundaries. Measure actual allocation
savings and execution/construction cost separately; do not require old receipts'
allocation/GC histories or treat a working candidate as evidence of a speedup.
Inter-owner operations remain late-bound Id messages. Self's VM-private machinery
does not authorize service tables, foreign-layout access or extra module coupling.

## Starting point

At commit `0f564c25`, the converted production prefix is Siphon → Id → Bootstrap,
with Bootstrap constructing Roots, Namespace, Boolean and Nil. Production tests
exercise that prefix, replacement and nested-load retention. Collection is not
active there; Boolean/Nil printing still awaits String.

Thirty-eight upper-module crates still require integration conversion, and the
seven helpers above require migration into suitable owners or a justified retained
boundary. These counts are an inventory, not a work plan or progress metric.
Host's new `load_bootstrap` handoff exists, but ordinary startup still refers to
the removed construction path. Generated-code integration also remains. The full
workspace and CLI are not yet buildable. Earlier installed test results do not
validate this deliberately broken intermediate state.

## Remaining work

The remaining work is primarily integration replacement and consolidation, not
another round of extracting modules. The useful implementations largely exist;
their old connections have been deleted. Reconnect them correctly while removing
duplication. There are five concrete obligations:

- **Give each function one owner.** Consolidate lifetime/collection helpers and
  remove redundant Runtime wrappers. Keep representation handling inside the
  corresponding value or collection owner. Moving a helper into its owner should
  usually be an ordinary code move, not a new protocol or architectural project.
- **Replace actual cross-module operations.** Cover construction, value conversion,
  reference enumeration, method installation, execution and persistence. Update
  both sides together and delete displaced tables, pointer casts and attachment
  machinery. An initializer accepting opaque objects is insufficient if later
  operations still bypass messages.
- **Restore execution and compiler integration.** This is the largest coupled
  area. Generated calls, Blocks, suspended frames, scheduling, optimizations,
  recovery and executable-code lifetime must agree on the same object model.
  Preserve their algorithms; replace dependency access and construction paths.
- **Restore the remaining functional branches.** Collections, I/O,
  Store/Relational, foreign calls, images and workers follow their actual
  dependencies. Service adapters should often shrink when descriptor plumbing
  disappears. They do not each need a new design.
- **Finish real startup and remove the abandoned path.** Bootstrap must construct
  the complete language, and ordinary Host/CLI startup must use it. Build/install
  references and semantic tests must follow that path. The passing prefix is only
  the beginning of this obligation.

The order below follows the dependency structure; independent branches do not
need to wait for unrelated functions. Extend the real Bootstrap sequence as each
function becomes available. Commit coherent progress without turning each commit
into another architectural extraction.

1. **Memory management and object behavior.** Establish their owners, absorb
   redundant helper plumbing, and replace the actual callers. Resolve reference
   enumeration, collection/reentry phases and native-resource cleanup together.
   Demonstrate collection and finalization over the current Id/Roots prefix,
   including stale-address retirement. No dependency on Execution or compilers.
2. **Values and their extensions.** Convert String, Error, Integer, Float, Word
   and text construction in the order their operations require. Remove shared
   codecs and direct foreign payload access from consumers. Restore arithmetic,
   errors, printing and conversions with live dispatch and collection safety.
3. **Execution and collections.** Keep their functional ownership separate while
   repairing dependencies between them. Restore calls/Blocks, collection families,
   frames, scheduling, suspension, continuations and recovery. Preserve argument,
   result and active-code lifetime across callbacks and collection.
4. **Parsing, compilation and executable installation.** Convert PEG, Lang,
   Artifact's language-facing adapter, TCC, Codegen integration and Portable Code.
   Reconcile generated calls with the same Id message contract. Preserve guarded
   optimizations, materialization, deoptimization and safe code retirement.
5. **Services along their dependency branches.** Restore System operations and
   source/native loading; Store then Relational; foreign calls; Image persistence
   and recovery; worker startup and communication. I/O need not wait for a compiler.
   Normal compatible image startup uses compiled code, not unconditional source
   recompilation.
6. **Complete application startup and retire the abandoned path.** Wire ordinary
   Host/CLI startup to the completed Bootstrap sequence. Update build/install
   references and current documentation; remove obsolete exports, fixtures and
   crates with their last callers. Do this cleanup throughout the work rather
   than accumulate a second integration path for a final cleanup project.

Each item includes its production callers and tests. Merely compiling its owner,
adding a native module entry, or demonstrating a separate seed is not completion.

The first substantial implementation target is **working collection over the
existing Id/Roots prefix**, with reference enumeration, finalization and retirement
functioning through messages. It must exercise the production owners and provide
the foundation for restoring values safely. The deliverable is working behavior
and deleted scaffolding, not seven replacement helper services.

## Memory-management checkpoint (2026-09-22)

The first target now runs in the production prefix: Bootstrap loads Collect with
Roots, publishes `GC`, and activates collection over the existing Id heap. See the
[owning protocol](../../rust/birnam-collect/README.md) for receiver/argument/result
contracts and phase rules. The implementation uses fresh Id messages throughout;
only selector identities and one-use detached allocation ownership are retained.

Collection owns its state, native capture, immediate-mark/LIFO traversal, live
reference/finalizer selection and ordered reclamation. No collector singleton,
foreign layout decoder or replacement service table was introduced. Id supplies
non-collecting storage operations; complete upper construction methods add phase
checks and allocation accounting without capturing those Id implementations.

Deleted the obsolete Collect state/coordinator/construction tables, live Trace
graph adapter, Lifetime, Native Mark, Native Stack, Trace Hooks and Finalizers
helper crates, their export/install entries and Runtime adoption wrappers.
At that checkpoint, Payload-layout's remaining generic language-slot implementation
belonged with object behavior; it was not loaded or consumed by GC. Its migration
is completed in the next checkpoint below. Diagnostic storage tracing remains
separate. Neither is a prerequisite for collection over the converted prefix.

Targeted production tests exercise activation failure/retry on one heap, declared
and native roots, automatic collection, inherited/live enumeration, failed tracing
without sweep, nested collection suppression, allocation bans during callbacks,
weak Symbol/behavior retirement and detach/finalize/reclaim with live replacement.

This completes the memory-management portion over the existing prefix, not all of
item 1 or the full language. Object behavior/value owners still need conversion.
The retained Execution, concurrency, deoptimization and deferred code-retirement
algorithms still require their message integration and suspended-task/code-lifetime
tests when Execution is restored. No thread-hosted domain backend or generalized
retirement framework is part of this change. Full Runtime/CLI startup is still
unbuildable during the broader transition.

## Object-behavior checkpoint (2026-09-22)

Bootstrap now loads Object Model after GC, with Namespace as its opaque input.
It publishes `ObjectModel`, installs ordinary `clone`/`new` and the existing
private-behavior/owner messages, and registers its roots, references and retirement
through messages. See the [local protocol](../../rust/birnam-object-model/README.md).

This completes the basic object-behavior portion of item 1 over the production
prefix. Ordinary payload storage, shallow cloning, exact slot metadata and private
behavior associations have an instance owner. Native values retain responsibility
for their own payloads and resource-aware cloning. Object Model does not copy
foreign nonempty payloads or Id's behavior representation.

GC admits behavior references in header/parent/owner/key order and sends metadata
retirement before Id retires indexes or reclaims storage. Association maps are
weak: a live behavior traces its owner, but the map does not root every association.
Cloning protects copied references across allocating reentry, including source
mutation. Construction admission applies even when a private behavior is reused.

Deleted Payload Layout and its exports/build entries, the redundant behavior
and selector enumeration adapter, Runtime adoption wrappers, and the test-only
Object Model trait bridge. Runtime's installer no longer overwrites completed
Id/GC/Object Model methods. Existing Id reflection messages supply the facts;
no replacement module table or generic construction framework was added.

Targeted production tests cover cloning, independent private method mutation,
inherited lookup, exact slot descriptions, reachability, callback replacement,
reentrant clone allocation, metadata retirement before finalization, finalizer
non-copying and collection of unreachable object/private-behavior cycles. Payload
layout copy/boundary semantics are also retained in local unit tests.

## Text and Error checkpoint (2026-09-22)

Bootstrap now loads String and Error after Object Model and connects Boolean/Nil
printing to the completed String object. Initializers take Namespace and return
opaque completed objects. The [String protocol](../../rust/birnam-string/README.md)
and [Error protocol](authored-small-modules.md#error) document their messages.

String owns its byte buffers and String Builder's text construction. Buffer keys
are weak; GC finalization releases each buffer through a fresh message. Raw
payload words and private behavior changes cannot redirect native ownership.
String clones copy into independent buffers; Builder retains its existing empty
`new`/`clone` semantics. Borrowed bytes are copied before allocating or reentrant
sends. Buffer borrows never span calls into dependencies. Roots, selectors and
mutable storage belong to the initialized instance, with no new singleton.

Error's two reference fields use ordinary Object Model allocation and slot messages,
so its tracing and shallow cloning need no separate storage or GC adapter. Error
keeps its constructors, predicates, field access and propagating DNU. Object DNU
constructs its diagnostic through String and sends the current `kind:message:`
method to Error. The completed factories remain ordinary replaceable objects.

Removed String's shared-table construction/adoption path and Runtime String
adapter, Error's cached entries and descriptor initialization, and the separate
String Builder crate and install entries. Builder is still a language object;
its native implementation lives with String. Deferred Execution, concurrency,
deoptimization and code-lifetime algorithms and their semantic tests remain
preserved for conversion; no compatibility tables were reintroduced for them.

Targeted production tests exercise real Siphon/Id/Bootstrap loading, opaque binary
text, text operations, private behaviors, independent clone buffers, mutable
builders, allocating/collecting callbacks, live factory/finalizer replacement,
Error propagation and reference reachability, automatic collection, and repeated
buffer retirement. Existing memory and Object Model tests continue to pass.

## Numeric values and text checkpoint (2026-09-22)

Bootstrap now loads Integer then Float after Error. Both take Namespace and
return completed prototype Objects. It then sends Integer `installFloat:` and
String `installNumbers:` with the completed dependencies. Extensions preserve
prior user replacements; no future object slot or API table is installed.
See [Integer](../../rust/birnam-integer/README.md),
[Float](../../rust/birnam-float/README.md), and
[String](../../rust/birnam-string/README.md) for the concrete messages.

Integer owns signed 64-bit interpretation; Float owns IEEE bit interpretation.
Their raw storage uses Object Model allocation and shallow cloning, with inherited
GC owners declaring no object references even through private behaviors. Numeric
recognition preserves exact-family behavior. Cross-family reads/construction use
`wordOf:`/`fromWord:` and `bitsOf:`/`fromBits:` messages. Receivers, arguments and
intermediate results stay rooted across live callbacks. Mutable installation state
and dependency identities remain with the owning instance.

Preserved checked arithmetic, mixed-number results, same-family equality,
comparison rounding, non-finite Float behavior, conversion limits, formatting,
and live Error/ErrorVt behavior. Integer `to:do:` only needs a callable object
responding to `value:`; no Block construction or Execution dependency is added.
String size, indexing, slicing, search, bracket balance and numeric parsing are
restored, as are Builder size and addByte:. Native Words still carry native bits;
language results remain numeric Objects.

Deleted Integer/Float storage descriptors, global mounting/extension contexts,
legacy Integer control initialization, Float's Runtime adapter, and both test
mounting bridges. The descriptor-specific numeric declarations v4/v5 and native
construction export are retired: their old effect proofs do not cover these
message implementations. Runtime's Integer admission checks and deferred batch
algorithms remain for Execution migration, along with existing deoptimization,
concurrency and code-lifetime work. No replacement optimizer framework or claim
of current optimization admission is introduced.

Production tests compare boundary arithmetic and conversions, non-finite values,
mixed operands, text algorithms, live constructor/reader replacement, Error
propagation, callback iteration and raw tracing. A second Bootstrap test triggers
collection throughout upper construction and verifies that mixed/text extensions
preserve methods replaced before installation. Existing prefix tests still pass.

## Collection checkpoint (2026-09-22)

The remaining value/collection branch is now in production Bootstrap: Word,
Vector, Map and Range take Namespace and return completed objects. Range installs
Integer `to:` after both exist; String receives the completed Vector through
`installCollections:` and owns `split:`. Both extensions preserve existing
replacements. See the owning protocols for [Word](../../rust/birnam-word/README.md),
[Vector](../../rust/birnam-vector/README.md), [Map](../../rust/birnam-map/README.md),
[Range](authored-small-modules.md#range), and [String](../../rust/birnam-string/README.md).

The Collection crate contained no shared algorithms: Bootstrap now constructs
and publishes its existing parent behavior directly. Deleted that installer and
the String Collection crate, their build/install entries, Word's storage table and
Runtime adoption/test-mounting wrapper. No shared value catalog or replacement
function table was introduced. Existing Vector trie and Map HAMT algorithms,
including full-hash collisions, remain with their owners. The unused Rust-only Vector kernel was subsequently removed in the cleanup below;
the earlier claim that it had Rust consumers was incorrect.

Map and Vector consume `collectionEquals:`; Map also consumes `collectionHash`.
Object Model provides the identity default and Symbol name hashing. Integer,
Float and String implement their former exact-family value rules themselves.
Hash algorithms, signed-zero normalization, same-object NaN recognition and Word's
identity-key rule are preserved independently of public `=`. These are live
messages with a documented key-stability contract, not a catalog of value types.

Object Model owns allocation/slot tracing; each collection interprets only its own
payloads. Scoped Roots messages cover construction paths, callback arguments,
fold accumulators, copied sort keys and traversal snapshots. Native reads validate
storage extents. Map's added-key flag is local to an insertion; no global flag,
thread-local sorting roots or cached dependency calls remain. Word/Range raw bits
never become reference edges, including after behavior privatization.

Targeted tests load the real production dylibs through Siphon/Id/Bootstrap. They
exercise persistent versions through both Vector growth boundaries, HAMT collisions,
value/identity key distinctions, reentrant equality, live callback/factory replacement,
Range endpoint/overflow behavior, binary splitting, tracing/retirement and malformed
reference slots. Bootstrap also runs with collection at each upper allocation and
preserves preinstalled numeric, text, Range and split replacements.

This closes the value/collection batch. Next restore the Execution owner as a
coherent production branch: calls/Blocks, frames, scheduling, suspension,
continuations and recovery, preserving concurrency and deoptimization work. Then
repair compiler/generated-code integration against those real execution messages.
The full Runtime/CLI, images and workers still await integration; the working
prefix is not the complete language.

## Converted-prefix cleanup (2026-09-22)

Removed the second Vector algorithm: its only consumers were its own tests.
Production trie tests now supply the evidence for Vector behavior. Removed unused
Id compatibility/test hooks, the ABI-hosted IdApi test helper and its cached-service
lookup tests/driver. TCC/Codegen registration, retirement, recovery and concurrency
implementations remain untouched for their upcoming message integration.

Retired the installed native-adoption driver and `native-modules` task. The live
`native-messages` task now audits converted dylib exports and dependency boundaries
as well as running the production tests. Source/image behavioral probes remain;
removed their requirements for deleted collection libraries and updated String's
Vector replacement expectation to its live message behavior. Recovery's build list
no longer names the deleted Collection crate. Historical native fixtures retain
semantic cases needing new setup; none is evidence of current upper integration.

Roots registration now grows with its existing vector. Removed the historical
32-enumerator ceiling; preserved insertion order, duplicate registration, snapshot
visitation and the `register:`/`rootsDo:` messages. A production test registers 64
objects plus a duplicate and proves that their reported children survive until
the enumerators stop reporting them. There is no new registry or provider API.

Strengthened collection tests to return callback-created Integer objects without
harness retention. They check sort-key survival across collecting callbacks, key
reclamation after success and native failure, and fold accumulator survival with
reclamation of obsolete intermediates. These checks pass through the existing
production implementations; no extra rooting layer was introduced to satisfy them.

## Execution and compilation reconnection plan (2026-09-22)

The converted prefix is ready for this work. The next two delivery targets are
**working Execution over that prefix**, then **source compiled and executed through
it**. These are integration batches, not new crate boundaries.

### Keep the algorithms; replace their connections

The current sources establish the work, not the old crate boundaries:

| Existing code | Treatment in this work |
|---|---|
| `birnam-runtime/src/execution.rs`, `execution/`, and the execution portions of `runtime.rs` | Keep Machine/frame/continuation algorithms, bounded drives, native barriers and recovery. Put their state with the Execution instance. |
| `runtime/method.rs`, execution code-generation leases and deferred retirement | Keep owned Method behavior and active-code lifetime together with their execution consumers. Replace the removed method-owner/lifetime APIs; do not extract replacement services for each registry. |
| `runtime/scheduling.rs`, `runtime/task_control.rs`, and existing Birnam task/control policy | Preserve the division between native execution mechanics and language scheduling/cancellation policy. Do not rewrite that policy in Rust to make the first batch appear complete. |
| `runtime/compiler.rs`, mixed optimization code in `runtime/native_tier.rs` and `runtime/numeric.rs`, Codegen and its language adapter | Compilation owns construction/lowering. Execution-time selection, guards, evaluation and recovery stay with their execution consumers. Split by function, not whole file; invoking an existing method must not require constructing a compiler or depending on Codegen. |
| TCC and `birnam-native-compiler` | TCC owns native compilation and its code resources. Keep useful compiler helpers private; replace TCC's old cached registration/retirement calls and foreign value access. |
| Runtime's old heap, globals, constructor bundles and dependency wrappers | Reuse the production Id, Roots, GC, ObjectModel and value owners. Delete displaced integration as its callers move; do not port an aggregate Runtime under a new name. |

Pure artifact/descriptor codecs can remain private implementation libraries;
preserving their supported formats does not require preserving old native
registration carriers or exported service functions.

### Close the actual execution/compiler seam

Before replacing this seam, trace its existing producers and consumers together:
`runtime/method.rs`, `registration.rs`, the drive/recovery code, TCC registration
and teardown, and Codegen's generated calls. Record the resulting small contract
beside the owning implementation, including each receiver, selector, operands,
result, retained objects and borrowed data. This is the first implementation task
within the batches, not a separate framework or open-ended design project.

Use these constraints to settle it:

- **Reuse language operations.** Owned Methods already have `invokeOn:` and
  `invokeOn:arguments:` (receiver object and an argument sequence read through
  `size`/`at:`). `methodFor:` takes a selector and answers an owned Method or nil;
  `addMethod:as:` installs that Method on a behavior. Keep those semantics,
  including exact-entry invocation after replacement. Ordinary future sends
  still perform current lookup. Preserve Block `value...` and Machine/frame
  operations instead of adding a second call API.
- **Execution adapts language lookup.** Id expects `NativeEntry` or `Unit`;
  language-defined lookup answers an owned Method or nil. Keep that conversion
  with Execution, preserving the selected function and context across nested
  lookup, replacement and collection. Id must not learn the Method representation.
  Retain the relevant `legacy_method.rs` behavioral tests, not its general legacy
  calling-convention bridge.
- **Installation must establish retention.** A callable's native code and native
  context must remain valid while installed, held by an owned Method, executing,
  suspended or needed for recovery. Identify the actual binding-to-owner edge
  and its replacement/removal path; an address-to-owner registry alone does not
  establish reachability. Connect installation, replacement/removal and reference
  tracing before migrating callers, using the existing memory protocols. Id
  binding operations alone do not root native entry contexts. Do not add compiler
  ownership or a compiler catalog to Id.
  The native author remains responsible for its context and reference reporting;
  Execution retains the declared owner without interpreting arbitrary contexts.
- **Registration carries executable data, not dependencies.** Actual entrypoints
  use the existing Id `NativeEntry` carrier; owner references are opaque objects.
  Existing frame/recovery descriptions may cross as bounded data copied or
  decoded by their consumer during the send. Never transport a Rust structure,
  function table, service address or foreign payload pointer in Words/Bytes.
  A private helper call within one owner needs no message protocol.
- **Generated code has the same boundary.** Translate generated inter-owner calls
  to Id sends and remove the matching runtime symbol/table imports. Preserve the
  existing guarded optimization and recovery behavior, including invalidation
  when relevant methods change. Calling an already selected executable entry is
  legitimate execution; saving a dependency's service entry for later calls is
  not. No second generated-code ABI catalog belongs in `birnam-abi`.
- **Retirement follows existing execution checkpoints.** TCC keeps ownership of
  its native mappings; Execution keeps its active/suspended/recovery retention.
  Repair their registration and release through messages. Replacement must not
  unmap active code, and finalization must not run asynchronous task cleanup.
  Keep current lease/checkpoint algorithms where needed; do not introduce a new
  lease service, generalized resource manager or parallel collector.

### Batch 1: Execution in the production prefix

Load completed Execution objects through the real Id/Bootstrap sequence, using
the existing Namespace and memory protocols. Restore owned Methods, Blocks,
Machines, frames, continuations, calls/returns, bounded drives, suspension and
recovery mechanics as one functional branch. Keep mutable state with its owning
instance. Register its references through the existing GC/Roots messages; include
ready, waiting and suspended state, temporary call arguments/results, and code
retention. No owner may hold an internal mutable borrow across a message send.

Targeted production tests must demonstrate:

- A real Machine invokes installed code and a Block, passes arguments/results,
  and observes method replacement on a subsequent send. An owned Method still
  invokes the implementation it captured. Language-defined lookup works through
  nested sends and collection with the selected function and context intact.
- An installed binding alone retains its callable: release temporary owner roots
  and owned Method values, collect, then invoke it. Remove the last binding and
  reference; collection and the existing safe checkpoint release Execution's
  retention. Observe that release independently of an immortal fixture dylib;
  real compiled-mapping reclamation is a batch 2 check.
- Calls/returns, suspension/resumption, continuation restoration and bounded
  execution preserve the existing frame/control behavior. Collection during
  callbacks and while paused retains live state; completion/abandonment releases
  it at the established lifecycle boundary.
- Replacement while code is entered or suspended preserves its execution and
  recovery state until release. Execution releases its retention after the last
  required reference/pin is gone. Native barriers and collection critical-phase
  restrictions survive reconnection.

Use native executable fixtures through the production loader where source
compilation is not yet available; these must exercise the real Execution objects,
not a second seed/runtime. Restore existing mechanics tests rather than inventing
another harness: `runtime/scheduling_tests.rs` covers bounded drives, two-Machine
interleaving, precise roots and wait versus runnable resumption. This proves
Execution integration, **not** the language task scheduler or compiler-generated
deoptimization. Batch 2 supplies real compiler code-resource/recovery evidence;
source task policy awaits Store as described below.

### Batch 2: Compilation uses that Execution

Scope amendment (2026-09-23): allocation-elision optimizers move here by explicit
user decision. Reconnect useful compiler analyses and recovery consumers under
[the revised optimization contract](#optimization-preserves-language-behavior-not-allocation-history).
Do not restore their retired allocation-budget or deferred-identity machinery.
The completion record below identifies the retained algorithms for reassessment.

Allocation-elision follows the [alien-code and trust boundary](#alien-code-trust-and-proportionate-protection)
above. Reuse only the identity and recovery algorithms required by actual
observable behavior, through the smallest owner messages required by consumers.
Do not recreate retired service tables or freeze dependencies to obtain a callback-free path. Prove each
optimized region's assumptions against current dispatch; recover or execute the
ordinary path when those assumptions do not hold.

Reconnect PEG, Lang, Codegen, TCC, the language-facing Artifact adapter and Portable
Code along their real dependencies. Convert both sides of executable installation,
generated invocation, recovery registration and retirement; remove their displaced
tables, cached service calls, foreign value decoding and native service exports.
Extend Bootstrap with completed objects only. Keep ordinary algorithm helpers
private and keep compiler construction out of Execution startup.

The production integration must compile source, install its method, invoke it on
the real Machine and exercise a compiled Block. Repeat binding-only and
active/suspended lifetime checks with actual reclaimable compiled units. Exercise
live replacement, guarded fallback/deoptimization, materialization and Execution's
cancellation/cleanup mechanics with generated code. Existing
`runtime/boundary_control_tests.rs` and `runtime/v2_control_tests.rs` cover
compiler-generated boundary transfer, retained frames, fresh cleanup dispatch and
cleanup suspension; bring those cases through the production owners. Adapt the
relevant independent semantic cases from `method-values-smoke`, `method-unit-lifetime-smoke`,
`stackless-control-smoke` and recovery probes; their old Host/CLI setup and image
round trips are not prerequisites for testing this production path.

The existing source TaskControl/TaskExecutor policy in
`src/id/stackless-control.birnam` eagerly initializes `db` schema and stores fields
through `db cellFor:attr:`. Its full interleaving/wait/cancellation/cleanup tests
(including `task-control-smoke`) therefore await the Store branch, not just a
compiler. Preserve that implementation and its tests. Do not substitute a new
scheduler or fake Store to bring them forward, and do not claim complete language
concurrency from Execution mechanics tests. Loading a source control unit with
these dependencies must wait until those dependencies exist.

### Scope and stopping point for these batches

Each batch includes the production Bootstrap integration, targeted debug checks,
deletion of its displaced plumbing, local protocol documentation and a commit.

Do not pull Image restoration, process-worker integration, multi-threaded domain
hosting, full Host/CLI restoration, new optimization strategies or new scheduling
policies into these batches. Preserve their useful code and semantic tests for
their existing later integration steps. Source supplied directly to the loaded
compiler is sufficient here; restoring the CLI or all file-loading services is
not a hidden dependency. A concrete newly discovered dependency must be justified
by its consumer and resolved locally, not used to import the old Runtime bundle.
No release validation, new benchmarks or generalized compatibility layer is needed.

Stop this phase when real Execution and source compilation use the message-based
production prefix, the execution/recovery behavior covered above works, and the
replaced connections are gone. Record still-deferred source task-policy,
image/worker/startup checks explicitly. Do not describe this checkpoint as
completion of the full language refactor, and do not continue extracting helpers
after it is achieved.

Three independent adversarial reviews covered scope, the Id/executable-lifetime
seam and acceptance. The revised plan passed re-review after correcting mixed-file
ownership, lookup adaptation, binding-only lifetime coverage and the hidden Store
dependency of source task policy. This approves the bounded plan, not an unbuilt
implementation or a finalized registration protocol.

## Initial Execution callable checkpoint (2026-09-23)

Batch 1 has begun. Bootstrap now loads the Execution owner after collections,
publishing Method/MethodVt and completed owned-callable operations. Its
[local protocol and producer/consumer trace](../../rust/birnam-execution/README.md)
record the current contract and the remaining frame/registration seam.

Live Id dictionary enumeration supplies binding-to-Method-to-owner reachability;
Execution keeps no second binding table. Native replacement/removal therefore
changes that edge directly. Active invocation and argument reads use scoped roots
registered with Roots. Method/nil language lookup is adapted to Id's existing
NativeEntry/Unit convention, preserving exact functions and contexts. Resident
image pins wait for an idle Execution checkpoint after metadata retirement.

Production checks cover binding-only retention, native replacement while entered,
exact captured invocation, nested lookup with collection, argument lifetime and
failure cleanup, language result conversion, and last-reference owner reclamation.
An independent C callable fixture is loaded through Bootstrap. The converted-prefix
suite and export/dependency audit include this owner.

This is **not completion of batch 1**. Machine/frame/Block, bounded drives,
suspension, continuations, recovery and generation-lease integration remain, as
does deletion of their displaced Runtime integration. Their existing algorithms
and tests, including the old Method lease code, remain available for that work;
the old Runtime is not loaded by this prefix. No suspended-code or compiler
acceptance is claimed. The next task is to move those retained mechanics into
the Execution instance and settle descriptor registration with their actual
consumers, then restore the production scheduling and code-pin tests.

## Keep the refactor bounded

For every change, ask: **does it remove an obsolete connection, establish a
necessary functional boundary, or preserve required behavior?** If it does none
of those, it is outside this refactor.

- Reuse an existing message when its meaning fits. Add only operations needed by
  concrete consumers; document receiver, arguments, result and borrowing rules
  locally. Do not build a central protocol catalog or a service framework.
- Delete displaced tables, adapters and constructors in the work that replaces
  them. The workspace is already allowed to be broken during migration; do not
  construct compatibility layers to conceal unfinished callers.
- Preserve arithmetic, error, identity, malleability, memory-safety, concurrency,
  deoptimization and image behavior. Retain their independent semantic tests;
  retire tests whose sole purpose was validating deleted table layouts.
- Apply the [alien-code and trust boundary](#alien-code-trust-and-proportionate-protection):
  native callers/wrappers own foreign behavior. Preserve Birnam's rooting,
  mutation, recovery and code lifetime without speculative defenses, captured
  dependency services or additional coupling between owners.
- Do not redesign unaffected algorithms, the language syntax or the image format
  simply because their integration is being repaired. Do not create parallel
  implementations, experiments or a sequence of helper-extraction designs.
- Use targeted debug checks, actual production loading, representative dependency
  replacement and end-to-end source/image/worker checks as those paths return.
  Release validation and forced-source worker startup timing are not required.

The remaining interface work must implement established semantics, not reopen
them. In particular, **GC preserves the established collection contract while
replacing its integration**. Designs [82](../archive/design/82-minimal-production-id.md) and
[83](../archive/design/83-production-id-construction-and-lifetimes.md#5-collector-activation-is-an-explicit-transition)
already require one allocation universe, complete roots before activation,
non-collecting failure of activation, and retirement before address reuse.
[81O](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81o-production-live-gc-traversal.md) and
[81P](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81p-production-collection-coordinator.md) specify observable tracing and
finalization behavior: live hook selection, no allocation during collection,
suppressed nested collection, no internal borrows across callbacks, and no sweep
after a failed trace. Their frozen native service tables do not survive.

GC's remaining implementation choices are the smallest sufficient owner messages
for references/cleanup/retirement, placement of existing phase checks around fresh
dispatch, and removal or privatization of redundant records. Design 83's former
permission to capture Id methods at construction and bypass later lookup is
removed. Neither safe collection nor code lifetime justifies that exception.
Do not choose a new collector, change finalization policy, or introduce a service
framework as part of repairing these calls.

Two other interfaces need the same discipline:

- **Execution and compiled code:** how methods are installed, invoked, retained
  while active and retired safely, including deoptimization.
- **Images:** how each owner restores its objects and compiled artifacts without
  exposing foreign layouts or recompiling everything unconditionally.

Resolve these against existing consumers using the smallest sufficient messages.
Do not predesign a universal framework for them. This document does not invent
selectors or prescribe extra services before those requirements are established.

## GC and concurrency

The accepted [domain model](80-concurrency-domains-and-structured-tasks.md)
does not require a shared-heap concurrent collector. Each domain owns its heap
and has one execution owner; local tasks interleave, while parallel domains have
separate heaps and communicate explicitly. Preserve these existing obligations:

- **Root all live task state, not just the running stack.** Ready/waiting tasks,
  paused Machines, Continuations, pending outcomes, cancellation and cleanup
  retain their object references and entered-code pins until the existing
  lifecycle releases them. Suspension is not a reason to drop those roots.
- **Collection is a runtime critical operation.** Do not switch to a peer task,
  park a tracing callback or deliver asynchronous task control through its active
  collection frames. Supported synchronous hook/finalizer effects still occur
  under the collection-phase rules. Task cleanup may suspend under its ordinary
  lifecycle; GC finalization may not. Do not use finalization to drive asynchronous
  structured cleanup. Bounded Machine drives and timeslicing do not waive native
  barriers or critical-operation restrictions.
- **Transport does not introduce foreign-heap roots.** The process-worker
  transport holds owned bytes; object encoding/decoding runs on the owner.
  Release transport locks before message sends, and root source objects and
  partially decoded results across allocating calls. Pending language requests
  and replies remain rooted through their owning tasks/handles.
- **Object collection does not release active executable code.** Preserve code
  ownership across waits, cancellation cleanup, nested native calls and recovery.
  Reclamation still follows the established execution checkpoint rules.

Source inspection on 2026-09-22 confirms that the retained Machine implementation
traces frame/control state, bounded stops retain frames, and asynchronous process
transport stores bytes rather than background Oops. This is a compatibility
assessment, not a passing concurrency/GC test result: upper integration is still
broken. Restore the existing suspended-task, cleanup, pending-request and code-pin
tests through production messages as those paths return.

Thread-hosted domains remain separate future work. Current Id, Roots, Namespace,
Bootstrap and value-module singletons do not support multiple independent domains
in one process. Birnam-owned state, callback routing and any shared code-mapping
leases must become domain-correct before enabling that backend, as Designs
[60D](60d-stackless-concurrency-fit.md) and
[80X](80x-asynchronous-process-domains.md#next-boundary-thread-hosting) already
require. This does not justify a global GC lock, concurrent tracing, task migration
between heaps or expansion of the present refactor into a thread-hosting project.

## Stopping condition

The refactor is complete when the production startup path constructs the language
through real Id messages, every cross-module dependency uses opaque objects and
messages, and the old parallel API system has no live callers. The relevant
behavioral checks must pass for values/collections, GC, replacement and active-code
lifetime, source execution, compiled images and workers. Build/install paths and
current documentation must match that implementation.

The result should have fewer integration concepts and no duplicate runtime path.
Line counts and crate counts can reveal accidental growth, but are not a reason
to delete useful algorithms or compress clear code. Once these conditions hold,
stop: further self-hosting or feature development is separate work.

## Native Machine/control checkpoint (2026-09-23)

The next batch 1 slice is implemented in the production Execution owner. The
retained frame/continuation registry, scheduling, boundary, tail-entry and recovery
algorithms now live there; the old all-Oop wrappers in those engine files are
removed. Native Method registration consumes NativeEntry plus copied canonical
FrameDescriptor-v2 and one-row ArtifactSelection bytes and an opaque owner. The
[local protocol](../../rust/birnam-execution/README.md) records all operands,
retention and remaining consumers.

Bootstrap publishes completed Machine, Block, FrameView, Continuation and action
objects. Real Id-loaded C fixtures exercise calls/returns/tail calls, captured
Blocks, interleaved bounded drives, wait/resume, one-shot continuation restoration,
protected boundary exit, native barriers and collection-critical rejection.
Paused replacement and detached-continuation tests retain only the Machine or
Continuation, collect, resume the original implementation, then observe owner
reclamation. The retained 18 engine tests and converted-prefix suite pass.

This extends, rather than closes, batch 1. Mixed Runtime drive/Method/task-control
wrappers and compiler-dependent execution-time selection/recovery still need their
remaining callers migrated and obsolete connections removed. No production
compiler-generated recovery or compiled mapping reclamation is claimed. Those
fixtures belong to batch 2; Store-backed source task policy and image/worker/CLI
integration remain at their previously documented gates.


### Native recovery checkpoint

Production Execution now connects the retained pending-call admission and pin
transfer algorithms to `Machine recoverAt:with:` and `recoverAt:withGraph:`.
`Method withInline:at:owner:` captures an executable native candidate, its opaque
owner and the baseline Method registration. Eligible calls either commit an
inline result, decline to baseline, or consume exactly one scoped recovery request.
The concrete contract and recipe encoding are in
[`birnam-execution/README.md`](../../rust/birnam-execution/README.md#inline-candidate-and-recovery-contract).

Graph validation moved from Codegen into Execution as a private shared algorithm.
Construction uses live Integer, Float and ObjectModel messages, scoped roots and
current GC reference ownership. No foreign payload or compiler registry is needed.
Production tests use the separately loaded C fixture and cover recipe/factory GC,
reentrant consumption, control/custom-lookup fallback, layout/trace-owner changes,
identity/bit preservation, and baseline-owner retention followed by reclamation
on completion, abandonment and failed admission. All 21 Execution unit tests and
all 11 production prefix integration binaries plus export/dependency audit pass.

Batch 1 remains open for execution-time numeric/region/closure selection, guards
and evaluation in `runtime/native_tier.rs` and `runtime/numeric.rs`, and deletion
of mixed Runtime Method/driver/task-control/recovery wrappers as their remaining
consumers migrate. This checkpoint does not substitute a generic candidate API
for those algorithms. Batch 2 still supplies compiler producers, generated recovery
and real compiled-mapping reclamation. Store-backed task policy and image/worker/
CLI restoration remain deferred under the original scope.


### Batch 1 completion: native selection, evaluation and cleanup (2026-09-23)

Batch 1 is complete under the explicit scope adjustment moving allocation-elision
to batch 2. This supersedes the open-batch statements at the earlier checkpoints.
Production Execution now owns hot PC candidate selection, current Machine guards,
entered-candidate retention and permanent withdrawal on guard failure.
`Method optimizeWith:` accepts already constructed native Methods per baseline PC;
its contract is documented in [Execution](../../rust/birnam-execution/README.md#hot-selection-and-scalar-evaluation).

Scalar numeric observation and checked evaluation also run inside the real Machine
call boundary. Integer validates the exact selected native entry and performs its
own operand/error handling, including the mixed-number extension, through a live
owner message. Execution supplies the arithmetic receiver; it does not decode
numeric payloads or cache dependency services. Codegen shares that private operator.

The displaced Runtime language Method, driver, task-control, scheduling and
recovery wrappers are deleted. Only the compiler-facing Method ownership bridge
remains until TCC's producers migrate. Source/compiler/Store acceptance cases are
preserved as migration inputs; their old Runtime harness is not current proof.
The production dependency audit confirms Execution has no Runtime/compiler edge.

Precise-GC stress also exposed native callable-context reuse while the retired
engine still indexed its function/context pair. Execution now reserves the dead
native allocation until generation checkpoint, without tracing its dead language
edges. Owner Objects are reclaimed after their final live edge, while subsequent
registrations cannot collide with a retiring context.

Acceptance: 21 Execution engine/planner/graph tests, 9 Codegen numeric-chain tests,
and all 11 production prefix integration binaries plus dylib export/dependency
audit. Added cases cross hot gates, exercise replacement/withdrawal and exact
native admission, collect while a candidate is entered, validate live ErrorVt and
numeric error behavior, and repeatedly reclaim/register without checkpoints.
These are native execution proofs, not compiled mapping or speedup claims.

Batch 2 retains compiler-generated PC/region construction, numeric chain/loop
reuse, closure elision, unboxed numeric values, discarded clones and
virtual aggregates. Their allocation-equivalence dependencies are retired by the
2026-09-24 decision above. Reconnect real producers and necessary owner messages
and prove generated recovery, portable admission and compiled-unit reclamation. The native candidate interface
does not stand in for these algorithms. Store-backed task policy, Image, workers
and full CLI startup remain at their previously stated integration gates.


### Batch 2 implementation order (2026-09-23)

Implement the first three areas together: PEG/Lang initialization, Codegen and
generated calls, and TCC installation/code lifetime. Allocation-elision remains a
separate subsequent implementation; it is not required to close this work.

The compiler consumes CST through its existing language messages and retains the
pure lowering, slot coloring and descriptor codecs. Generated entries use the Id
NativeEntry calling convention and live owner messages. TCC owns each compiled
Unit and its native mappings. Installation constructs an Execution Method with
that Unit as owner, then uses `addMethod:as:`; resumable entries supply canonical
descriptor and selection bytes to `fromStep:descriptor:selection:owner:`.
Binding, Method, frame and recovery reachability therefore retain the actual
compiled Unit. Retirement detaches dead native resources and releases them only
at an idle checkpoint. No legacy service-table compatibility layer is restored.

Acceptance includes source parsing and lowering, compiled methods and capturing
Blocks on real Machines, current dispatch after replacement, generated recovery,
and independently observed compiled mapping retention/reclamation. Pure compiler
or native fixture passes alone do not close these areas. Store task policy,
Image, workers and full CLI restoration keep their existing later scope.


### First three compiler areas delivered (2026-09-23)

PEG/Lang initialization, Codegen/generated calls, and TCC installation/code lifetime
now run in the production Bootstrap prefix. Allocation-elision remains a separate
implementation, as requested. The source path does not restore the full workspace.

The existing matcher and grammar policy are retained; three independent matcher
comparisons still pass. Codegen projects CST through messages and reuses portable
encoding, logical lowering, slot coloring and canonical descriptor validation.
Its production native backend emits the existing typed Id ABI and Machine actions.
There is no generated Runtime service catalog. Top-level bindings use Namespace;
lexical captures use ordinary owned Blocks.

TCC compiles a native Module initializer with Namespace and its Unit owner. That
initializer creates Execution Methods using descriptor/selection Bytes and retains
its native-context Objects through Unit messages. Named adapters use the additional
implementation operand on Method construction: code Unit and implementation Block
are traced separately. Captured Methods and suspended frames preserve their exact
old implementation across replacement. The obsolete Runtime function-address
owner bridge and native registration exports are deleted together with TCC's old
cached registration/retirement wrappers.

Acceptance passes 97 Codegen, 21 Execution, 3 matcher, 2 private native-compiler and
4 build-tool tests, plus 12 production integration binaries and dylib audits.
Real compiled Units survive binding-only, captured, entered, waiting and recovery
references. Both native Unit count and libtcc's independent relocated-byte counter
return to zero after the final reference and idle checkpoint. A separately authored
C recovery candidate compiled by TCC recovers into a Codegen baseline and survives
collection after removing its own binding while still executing. This is compiled
recovery-boundary evidence, not completion of deferred optimizer emission.
Generated protected-boundary exits deliver into the caller once; destroying a
waiting Machine releases its compiled mappings. Syntax, native compilation and
initializer admission failures also leave no retained mappings after collection.

The first three areas' source-to-Machine path is complete. The broader batch still
contains allocation-elision and the full Artifact/PortableCode language adapters.
Image/Store publication, semantic inventories and portable materialization
remain migration work. Their uncompiled legacy implementation and tests were
subsequently removed; the [compiler migration reference](../../rust/birnam-codegen-runtime/README.md#remaining-migration-reference)
pins their Git history and outstanding behavior. They are not passing coverage. Store-backed task policy, Image, workers
and full CLI keep their previous later integration gates.

### Batch 2 compiler-generated PC candidates (2026-09-24)

Historical slice: automatic emission was removed by the reassessment below.
The following records the delivered experiment, not current compiler behavior.

The first subsequent functional slice connects the retained per-PC specialization
strategy to the production native backend and Execution's existing candidate
protocol. Codegen emits a shared body per PC, the ordinary baseline dispatcher,
and specialized entries for bounded multi-PC functions. The initializer creates
all baselines before constructing candidate Methods with `fromNative:owner:` and
attaching their Vector with `optimizeWith:`. These are current Id sends; no new
owner protocol, service table, foreign payload access or compiler dependency in
Execution is introduced.

Baseline and candidate preserve the same message order, allocations and Machine
actions. PC conversion still calls the live Integer owner; an unexpected converted
PC takes the ordinary dispatcher without repeating that callback. Execution owns
hot selection, current Machine guards and permanent withdrawal. Candidates retain
their actual TCC Unit, and selected entries retain their Method independently of
candidate replacement/removal. Optional emission is limited to 2–64 PCs and
256 KiB of generated body text per function; other functions remain baseline.
Candidates compile eagerly in the baseline Unit, so this slice adds construction
cost and does not claim a measured speedup.

Production acceptance observes the actual compiler-created entries crossing the
hot gate, the replaced Integer factory's call effects, fresh numeric replacement
and collecting PC conversion, guard fallback after Machine replacement, collecting
candidate construction, withdrawal while entered, captured values, bounded
baseline fallback and rejected candidate initialization. Unit counts and libtcc
relocated bytes return to zero after the final references and checkpoint.

This closes compiler-generated PC candidate construction/installation, not
allocation-elision. Fused regions, numeric chain/loop reuse, closure and aggregate
elision, and their generated recovery remain subsequent work, subject to the
revised optimization contract. Deferred accounting identities are not
prerequisites. Full Artifact/PortableCode language adapters and the previously
deferred upper integrations retain their existing scope.

### Batch 2 exact allocation accounting and admission (2026-09-24)

Subsequent correction: actual-allocation accounting remains; the unused public
`canAllocate:` preview described in this delivery record was removed below.

GC's production allocation path now uses checked accumulated charges rather than
saturation. Id answers `allocationChargeFor:` for a payload-size Word using its
own storage geometry; GC queries it through current dispatch during activation
and ordinary construction. The allocated object remains rooted across that send,
and charging uses the post-callback total so a collecting or allocating replacement
is observed correctly. Physical freeing does not refund charges. Successful
collection resets them; failed activation/tracing does not publish/reset accounting.
Committed overflow retains the established fatal diagnostic.

GC exposes `allocationBytes` and non-mutating `canAllocate:` for current charges
and checked whole-region preflight. The latter takes an additional total charge
and answers Word 1 only while idle and strictly below the current threshold.
Overflow or a non-idle phase refuses. It neither reserves budget nor excludes
callbacks or explicit collection. Future optimizer consumers must still preserve
late binding and revalidate/recover/decline as required. The same private preview
serves ordinary construction and `prepareAllocation`; no helper library, native
table, dependency capture or foreign-layout knowledge is introduced.

Production checks use literal geometry expectations, exact-threshold and overflow
refusal, a subprocess fatal-overflow witness, unchanged charges after physical
freeing and failed tracing, and live Id-query replacements that collect or allocate
recursively. Activation's charge and a complete source compilation/execution charge
are compared with independently summed Id allocation inventories. These checks
run with the production owners, including generated candidates and real TCC Units.

This completes actual-allocation accounting and a non-reserving preview query;
no allocation-saving or performance claim follows from these checks. The later
2026-09-24 optimization decision removes the proposed deferred identities,
dead-allocation journals and single-charge materialization from the batch 2 queue.
Actual elision and generated recovery remain work under that revised contract.

### Batch 2 optimizer reassessment (2026-09-24)

The [code audit and next consumer](../archive/design/90-optimizer-reassessment.md) replace automatic
restoration of retained optimizers with decisions based on actual language needs.
Codegen no longer emits eager per-PC candidates or constructs their Methods and
Vectors. One resumable entry uses an ordinary switch; the intermediate dispatch
and per-PC body functions introduced for candidate sharing are also removed.
Generated operations still use current Id messages. Explicit native candidates
and their guards/recovery/lifetime contract remain available in Execution.

GC's unused `canAllocate:` message and generalized preview are deleted. Ordinary
construction and `prepareAllocation` share a private scalar threshold comparison;
checked real-allocation charges and collection reset behavior remain unchanged.

Production tests preserve source replacement, collecting callbacks, captured
Blocks and actual code reclamation while requiring zero speculative candidate
construction. Separate native candidate and compiled recovery fixtures still pass.
The full 13-binary native-message suite and export/dependency/install-name audits
pass. This removes known construction work; it is not a measured runtime speedup.

The first allocation-elision case to derive is a discarded ordinary clone with no
observable result. It requires a compiler proof of dead/non-escaping identity and
owner-established semantics of the actual clone/allocation path. It requires no
virtual-object materializer, identity service, journal or region allocation budget.
The existing clone's nested replaceable allocation path still needs concrete owner
admission; no placeholder protocol or claim of implemented elision is introduced.


### First discarded-clone consumer (2026-09-24)

The [implementation and evidence](90-discarded-clone.md) complete the first real
allocation-elision case identified above. Codegen recognizes a whole, uncaptured
callee that sends a zero-argument message to an entry argument, discards its
result, and returns an entry argument. Execution uses its existing inline-call
boundary. ObjectModel recognizes its actual clone implementation and zero-slot
source; ObjectModel, Integer, GC and Id establish their own current semantics
through messages. A declined guard executes the baseline.

Bounded calls retain normal steps and inspectable Frames. Nonempty payloads,
lexical bindings, captures and escaping clones remain baseline-only. This slice
needs no virtual objects or new deoptimization machinery. The production witness
removes eight physical allocations per invocation, preserves replacement effects
and source/answer identity, and releases the actual TCC mappings after final use.
Broader producers and full workspace/CLI restoration remain outstanding.


### Numeric chains and loops reassessed (2026-09-24)

The [numeric-chain review](90-numeric-chain-review.md) separates reusable checked
arithmetic/dataflow from legacy prediction registries, boxed driver fusion and
loop caches. Do not restore those integrations automatically. Investigate one
straight-line chain together with unboxed intermediates; final result construction
and supported Frame observation require a concrete recovery/fallback design.
Loop caching stays deferred without current profitability evidence. Optimizer-only
tail-stamp bookkeeping has no current production consumer and is a cleanup
candidate; ordinary Frame identity and lifetime mechanisms remain required.


### First combined numeric/unboxed consumer

The [numeric predicate](90-numeric-predicate.md) implements one arithmetic result
used only by a comparison, with an explicit final return and no captures. Integer
owns current operation recognition and raw computation; its result message lets
Execution publish the existing Boolean before root-release callbacks. Bounded
calls and unsupported shapes run the ordinary baseline. The production witness
removes the intermediate Integer and 18 generated PC/index boxes. This resolves
the boundary for an existing Boolean result; general numeric-result allocation
and recovery remain subsequent work. No old loop cache, alternate driver or
allocation-history protocol is restored.

### Store and Cells reconnected (2026-09-24)

Bootstrap loads [Store and Cells](../../rust/birnam-store/README.md) before
Execution/compiler. Fact-log, schema, history, retention, compaction and snapshot
algorithms are preserved. Owner-private state and rooted snapshots survive
collecting callbacks; retirement releases dead native state.

Globals retains Store-backed `__globals__` facts. MOP authorization, notification,
managed installation/removal and mirror backfill remain available through sends;
Id dictionary changes notify Store through `methodChanged:on:`. Authorizers are
Object receivers and managed methods are owned Execution Methods. Deferred
compiler publication/compensation callers still need migration.

GC enumeration now passes Object receivers and sends each reference through
`visit:`; allocation/binding enumeration also sends to receivers. GC preallocates
its visitors, preserves traversal and phase rules, and sends Id an explicit
release request after finalization. Id keeps detached storage until that request;
no native visitor or release callback crosses the GC boundary. See the
[protocol and checks](../../rust/birnam-collect/README.md).

Acceptance: ten Store algorithm tests (including the independent snapshot oracle),
five Id unit tests, all fourteen production integration binaries and dylib audits.
Tests cover collecting callbacks, hook lifetime/current lookup, Globals history,
MOP mirrors, resource/phase refusal and owner-level image transfer. Two manual
timing tests remain ignored. Full Relational, Image, task policy, compiler globals
integration and Host/CLI startup remain separate work; optimization stays paused.

### Relational reconnected (2026-09-25)

[Relational](../../rust/birnam-relational/README.md) now loads after Store in
Bootstrap. Its existing Store selectors, LogicVar/Entity surface, query shapes,
rule facts, result ordering, history windows and evaluator algorithms are
preserved. The adapter uses current Store, value, collection and GC messages;
snapshot collectors receive `value:value:value:`. Invocation roots cover nested
queries, destructive enumeration and result construction. There is no new query
API, service table or optimizer work. Production validation uses the real dylib
and compiled source. The obsolete service-table adapter, C probes and their
validation runner have been removed. Compiler publication, task policy, Image and Host/CLI remain
separate integration work.

### Compiler Globals reconnected (2026-09-25)

Generated source global reads now send `Globals at:` and preserve the old
Namespace fallback on nil. Top-level writes send `Globals at:put:`; a rejected
write returns its exact result without executing later effects. Nested bindings
remain lexical. Store history and queries observe source writes, and compiled
Blocks observe later replacements through current lookup. The production compiler
test covers those rules, collecting accessors, exact native failures and compiled
code retention through Globals. No transaction or optimization policy is added.

### Reflect publication reconnected (2026-09-25)

`Reflect defineMethod:on:as:` now sends the existing `Store addMethod:on:with:`
protocol with an owned Method. Store handles authorization, mirrors and notification;
Reflect keeps its target result on success and propagates refusals/failures.
Production tests cover collecting and replaced hooks, notification after commit,
and Method/Unit lifetime. Staged publication/compensation remains deferred.

### System construction and printing/output reconnected (2026-09-25)

Prelude, Printing and System Output now load through Bootstrap using Id sends.
They preserve System identity, Object/Symbol conversion, exact-String output,
flushing and receiver returns. Production tests cover actual stdout bytes, live
replacement, collecting conversion and nested output. Native stdout I/O remains
private to its owner; the old dependency tables are removed. See the
[contract and validation](../../rust/birnam-system-output/README.md).

### System input and environment reconnected (2026-09-25)

`readLine` and `getEnv:` now load through Bootstrap and use Id sends for their
language dependencies. Binary input, EOF, exact String names, Unicode errors,
lazy Error lookup and the existing stdin-lock lifetime are preserved. Isolated
production tests cover collecting methods and reloads. The modules/text-error
fixtures now forward through messages rather than invoking saved native entries.

### System file I/O reconnected (2026-09-25)

`readFile:` and `writeFile:contents:` now use Id sends and load through Bootstrap.
The user-approved filename snapshot is taken before reading contents, replacing
legacy callback-driven mutation of borrowed paths. Existing binary I/O, UTF-8 path
checks, one-GiB read limit, error precedence and receiver returns remain. Production
tests verify the destination cannot be redirected during a collecting contents
send. See the [contract and checks](../../rust/birnam-file-io/README.md).

### System GC controls reconnected (2026-09-25)

The four existing System GC methods use Id sends. GC owns automatic/precision
flags; representation owners query precision and enumerate their own payloads.
The conservative default and live mode/layout changes are restored without native
settings tables, foreign layout access, or changes to the visitor API. Native root
capture and the current budget remain unchanged. See the bounded
[contract and validation](authored-small-modules.md#system-gc).


### Native Loader reconnected (2026-09-26)

[Native Loader](authored-small-modules.md#native-loader) now loads through
Bootstrap and uses Id sends for String, Integer, Error, roots and registration.
The existing System methods, path snapshot, sequence order, initializer results
and error mapping are preserved. Id retains sole ownership of native loading.
Production C fixtures and compiled source cover real loading, collecting calls,
transient arguments, live replacements and reloads. Source Loader, Image and
Host/CLI integration remain separate work; optimization stays paused.


### Source Loader and compiler registration reconnected (2026-09-26)

Codegen's existing revision/registration/readback/seal and loader-facing evaluation
selectors now use Id messages and explicitly traced source state. System `load:`
runs through Bootstrap and the authored Module manager executes unchanged in the
production integration test. Preserve Globals/Namespace separation, registration
before evaluation, nil/Error behavior and immediate Store MOP publication.
See the [Source Loader contract](../../rust/birnam-source-loader/README.md).
The subsequent embedding slice below restores the file consumer. Artifact
activation, generation/Image reconstruction and Host/CLI startup remain separate.


### Bytes and embedding reconnected (2026-09-26)

Bytes is a clonable prototype with its own behavior, sharing String's private
buffer storage/lifetime machinery. Compile-time embedding sends to current System
and copies the returned Bytes through messages; generated code retains the payload
and constructs Bytes through sends. The existing portable codec suffices. No new
native module or service framework is needed. [Design 56](56-embedded-files.md)
records the contract and real dylib validation. Image round-trip and project/Host
reconnection remain deferred by the user's explicit scope decision.


### Bootstrap Block loops reconnected (2026-09-27)

Block Loop now loads after Execution and uses Id sends for callbacks, roots and
errors. The missing Block prototype ownership registration is repaired, allowing
ordinary Reflect publication to reach compiled Blocks. The existing native
suspension barrier and authored resumable replacement remain distinct; see the
[contract and production checks](authored-small-modules.md#block-loops).


### Artifact inspector reconnected (2026-09-28)

Artifact now loads through Bootstrap and uses Id sends for dependencies, roots
and ObjectModel descriptor storage. Its existing API, diagnostics and codecs are
preserved. [Production checks](../../rust/birnam-artifact/README.md) cover collecting
replacements and inert dylib inspection. PortableCode, activation and Image remain
separate migration slices.

### PortableCode ordinary objects reconnected (2026-09-28)

Bootstrap now loads CodeNode, CodeStatement, CodeProgram, CodeLexicalBinding and
CodeObject through Id sends, including GC visitors and finalization. Their APIs,
diagnostics and canonical encoding are preserved; [production checks](../../rust/birnam-portable-code/README.md)
cover all thirteen golden programs and collecting dependencies. Execution-owned
Block inspection and synchronous/resumable Codegen materialization now also use
owner messages, including canonical Bytes and capture reads. The production
`materialization` test covers identity, native control boundaries, bounded cache
leases and reclamation; Image and optimization remain separate.

### Image persistence reconnected (2026-09-29)

Bootstrap Image now restores ordinary graphs, Store facts/history/Cells, source
registration and owned executable Blocks through Id messages. Compatible loading
relocates saved objects without compilation; explicit rebuild uses canonical
programs. Store state exchange and rooted old Methods provide publication
compensation. [The Image contract](../../rust/birnam-image/README.md) records the
supported families, additive v8 fields and acceptance evidence, including the
previously deferred embedded-Bytes round trip. Host/CLI and broader generation
replacement remain separate; optimization is unchanged.
