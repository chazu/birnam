# 90 — Functional modules, minimal dependencies

> **Status:** Accepted organizing direction, 2026-09-22. Concrete message
> contracts are specified beside their owning implementations as they are
> migrated. This is a break with extraction-driven modularization, not another
> installment of it. No runtime migration is performed by this document.

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

The remaining helper crates (`payload-layout`, `trace-hooks`, `finalizers`,
`native-stack`, `native-mark`, `dispatch-cache`, `method-observation`) are not seven
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
- Do not expand safety into anticipating every erroneous native module. Native
  code remains trusted. Maintain the invariants needed for serious language-level
  software, especially rooting, mutation, recovery and code lifetime.
- Do not redesign unaffected algorithms, the language syntax or the image format
  simply because their integration is being repaired. Do not create parallel
  implementations, experiments or a sequence of helper-extraction designs.
- Use targeted debug checks, actual production loading, representative dependency
  replacement and end-to-end source/image/worker checks as those paths return.
  Release validation and forced-source worker startup timing are not required.

Three areas require substantive interface decisions rather than mostly
implementation work:

- **GC:** how owners expose references and cleanup, and which operations are
  permitted during collection. Support opaque representations without unsafe
  reentry or a catalog of module layouts.
- **Execution and compiled code:** how methods are installed, invoked, retained
  while active and retired safely, including deoptimization.
- **Images:** how each owner restores its objects and compiled artifacts without
  exposing foreign layouts or recompiling everything unconditionally.

Resolve these against existing consumers using the smallest sufficient messages.
Do not predesign a universal framework for them. This document does not invent
selectors or prescribe extra services before those requirements are established.

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
