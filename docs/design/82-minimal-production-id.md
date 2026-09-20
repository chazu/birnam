# 82 — Minimal production Id: correcting the dependency direction

> **Status:** Architectural decision and production migration contract,
> 2026-09-20. Written before starting the decoupling.
> Runtime observations are against the 81CA implementation at `e928780c`;
> `53404cce` closed that extraction phase without changing runtime code.
> This document does not claim that production Id is already minimal.
>
> This supersedes the next-project recommendation in the
> [81 closeout](81-production-extraction-status.md), the separate first-entry
> proposal in [81B](81b-siphon-handoff-and-construction-graph.md), and any earlier
> recommendation to continue extracting individual fields. The broader
> [self-hosting objective](81-toward-a-birnam-implementation.md) remains.

## 1. What we got wrong

We treated **moving implementation out of Id** as if it meant **removing Id's
dependency on that implementation**. It does not. We reduced IdApi and created
many small libraries while retaining the complete runtime's construction,
coordination and lifecycle in Id. The host then had to prepare those libraries
before Id could initialize. The dependency graph still points the wrong way.

The intended architecture was already clear: Siphon loads Id, and everything
else grows above Id. The implementation campaign optimized for locally passing
extractions instead. The following mistakes made that possible:

- We used smaller files, fewer IdApi fields and more independently loaded
  libraries as progress measures without requiring independent Id startup.
- We moved backing containers while leaving their policy and callers in Id.
  Even an allocation-identity counter became a prerequisite service supplied by
  a supposedly higher module.
- We preserved the aggregate native initialization contract by adding explicit
  capability arguments. An explicit dependency is easier to see; it is still a
  dependency.
- We proved behavior preservation repeatedly, but did not test the architectural
  requirement that Id work with the higher modules absent.
- We treated a smaller experimental seed as evidence of production progress
  while the production construction path remained different.
- We let the next small extraction become the work queue, rather than measuring
  the remaining distance to a complete ownership boundary.

The extracted value implementations and regression coverage are useful. The
claim that this process was decoupling production Id was wrong. It is acceptable
to undo an extraction when it made the intended dependency direction worse.

## 2. The production evidence

| Evidence | What it establishes |
|---|---|
| [`startup::load_floor`](../../rust/birnam-host/src/startup.rs) prepares twenty services before calling `id_init` | Id cannot currently initialize independently of upper modules. Moving this sequence into Id would merely hide the inversion. |
| [`initialize_once`](../../rust/birnam-id/src/public_api.rs) calls `build_bootstrap`, publishes canonical exports and installs execution objects | `id_init` constructs much more than an object model. |
| [`Runtime::new` and `RuntimeState`](../../rust/birnam-id/src/runtime.rs) construct namespace, execution, observation, collection and optimization state | Id still owns integration and lifecycle after the container extractions. |
| [`Heap`](../../rust/birnam-id/src/heap.rs) and [`Vtables`](../../rust/birnam-id/src/vtable.rs) construct external services | Even the core storage structures require the pre-Id preparation phase. |
| [`birnam-id/Cargo.toml`](../../rust/birnam-id/Cargo.toml) has normal dependencies on Codegen, Native Compiler and Artifact | A dynamic-library dependency listing alone cannot establish that Id is independent of compiler semantics. Rust code is also linked into it. |
| [`IdApi` and `ModuleInitFn`](../../rust/birnam-abi/src/lib.rs) have eighteen fields and twelve initialization arguments respectively | Shrinking the public table did not eliminate the larger initialization contract. |
| [`floor_symbol`](../../rust/birnam-codegen-runtime/src/lib.rs) discovers private exports through the library containing `api.dlsym` | Private symbol discovery couples consumers to Id beyond the public table. |
| [`System loadModule:`](../../rust/birnam-native-loader/src/lib.rs) needs String, Error Value and the aggregate initialization capabilities | This convenience loader cannot be the primitive mechanism for constructing those services. |

The twenty preparations are Namespace, Vtable Enumeration, Mutation Epoch,
Key Index, Method Observation, Root Registry, GC Settings, Payload Layout,
Trace Hooks, Finalizers, Native Stack, Native Mark, Allocation Budget,
Collection Phase, Storage Observation, Allocation Identity, Deferred Identity,
Deferred Index, Dispatch Cache and Guard Verdict. These are twenty preparation
operations, not twenty distinct library files.

[81A](81a-current-bootstrap-and-runtime-layers.md) records the current complete
startup. Its diagram is a picture of the problem, not the target architecture.

## 3. The required construction model

```text
OS memory and native-library facilities
  |
Siphon: ad-hoc implementation of the Id loading protocol
  |
  +-- load Id --> real, self-contained Id implementation
                    |
                    +-- message sends load subsequent modules
                          |
                          +-- each module uses already available semantics
                          +-- later modules extend or replace earlier behavior
                          +-- the loaded system owns further startup and CLI
```

**Siphon knows how to load Id using an ad-hoc Id API. After that first load,
the real Id API is used to load everything else, through message sends.**

The ad-hoc implementation is the finite bootstrap exception. It needs only the
native loading and handoff operations necessary for that first request. It does
not create a substitute object universe, intern a permanent set of fake Symbols,
prepare runtime services or construct the full production IdApi. Any temporary
native bootstrap tokens belong to that request and must not escape into Id's
object graph.

Id constructs and implements itself. It does **not** own preparation of its
implementation dependencies in upper modules. No module catalog, GC factory,
Namespace factory, execution registry or compiler is needed to initialize it.
Platform allocation and native loading are the facilities beneath it.

The real loader needs a byte path, a defined native entry contract and a way to
return the initialized module's result. It must work before String, Integer,
Error, Namespace, the language Module manager or source compilation exist.
Native byte spans, sizes and entry addresses are bootstrap operands, not those
future language objects. Their ABI must distinguish them from traced object
references and define their borrow duration.

The first handoff and later loads must share the loading protocol; they must
not become unrelated special-purpose loaders. This decides the architecture,
not an as-yet-unimplemented C struct layout. The native carrier must contain
only what is needed to enter the message protocol and identify its initial
receiver/selectors. It must not accumulate every upper module's capabilities.

Siphon may obtain the installed Id path and a generic continuation from its
process/install inputs. It must not choose the order of the language's modules.
That sequence belongs to loaded bootstrap code. Id loads a requested module;
it does not know what PEG, Codegen, Prelude or an application is.

## 4. What belongs in Id

The criterion is whether a responsibility is needed to implement Id itself,
not whether another module happens to call it frequently.

| Responsibility retained in Id | Boundary |
|---|---|
| Physical object storage and identity | Allocation extent, exact membership, stable identity, behavior association and safe storage release. No deferred-value journal, collection policy or language value representation. |
| Initial object/behavior graph | Enough construction to establish Object, Symbol, default behavior objects and their metacircular associations. No Nil/Boolean singletons, environment or Machine objects. |
| Symbols | Byte-oriented canonical names and interning. No dependency on String or a general export namespace. |
| Default behavior storage and primitive mutation | The dictionaries and representation operations that actually implement Id, including their consistency bookkeeping. No Store log, executable-generation registry or future consumer's callback slot. |
| Binding and native invocation | Fresh metacircular lookup with an explicit terminating case. No compulsory Machine, resumable-code classifier or optimization registry. |
| Primitive module loading | Native mapping, entry invocation and the lifetime needed for the loaded native implementation. Higher-level naming, artifact policy, recovery and replacement belong above it. |
| Observation of Id-owned facts | Scoped access to allocations, identities and Id-owned references, plus retirement of Id's own indexes before storage reuse. No opaque access to a hidden larger Runtime. |

Internal counters and dictionary indexes are implementation details, not
automatically separate runtime layers. If Id needs one to implement itself,
keep it in Id. In particular, a physical allocation identity counter must not
require loading Storage first. Preserve the existing dictionary indexing
algorithm where appropriate; removing an inverted dependency does not require
discarding its performance properties.

The upper owners are:

- **Object-model extensions and values:** private-behavior policy, ownership
  conventions, Nil/Boolean identities, Integer, Float, Word, String and collections.
- **Namespace:** general publication/import, enumeration and eventual Globals
  behavior. Interning a Symbol does not require this service.
- **GC:** reachability policy, payload descriptions, trace providers, external
  roots, finalizers, native-stack scanning, budgets and collection coordination.
- **Executable ownership and execution:** admission, publication, aliases,
  acquired methods, generations, retirement, Frames and Machine transitions.
- **Optimization:** dispatch caches, guard verdicts, deferred materialization,
  specialization and recovery policy.
- **System construction:** artifact handling, compiler, image loading, language
  Module management, CLI and application startup.

These are ownership groups, not a demand for another library per row or per
field. Module granularity follows complete responsibilities and dependencies.
Their final order is a dependency graph, not an invented linear list. Existing
cycles must be removed by separating construction from later extension, not by
reserving empty cells for future services.

## 5. No forward declarations of future semantics

A loaded module can implement behavior using what already exists and can later
be extended by another module. That is different from Id knowing in advance
that GC, Machine or Namespace will eventually arrive.

For this design, an upward dependency includes all of the following:

- a required pre-`id_init` provider or factory;
- a statically linked upper implementation;
- an empty callback, provider slot, initialization argument or registry in Id
  whose purpose is to be filled by a future layer;
- a private export that forwards to a supposedly extracted responsibility;
- a generic service bag that carries the same aggregate dependencies under
  another name;
- direct access to private Id state that substitutes for a specified protocol.

Later behavior is installed through the existing object model. It must take
effect for actual production callers. A message-based method installer is not
enough if native clients still call an unconditionally direct `api.method`;
the same applies to `proto`, `alloc` and `intern`. The current wrappers in
[`public_api.rs`](../../rust/birnam-id/src/public_api.rs) are part of the migration,
not exceptions to ignore.

A low-level representation primitive may remain non-overridable where Id needs
it to implement the protocol. Its callers and scope must be explicit. It must
not silently become the normal path that bypasses installed upper behavior.

## 6. The actual bootstrap and integration seams

These are the difficult parts to solve, not reasons to keep the full Runtime
inside Id. The ownership decisions above are firm; exact production protocols
for the following transitions still need to be implemented and verified.

### Metacircular construction and native operands

Id must directly create its first graph and first method bindings before it
can use its own sends. [`build_bootstrap` and `bind_active`](../../rust/birnam-id/src/dispatch.rs)
already demonstrate that finite construction and lookup termination. Keep the
exception explicit and bounded. Do not let it grow into an alternate startup
path for upper modules.

Before value modules exist, paths, sizes and native method entries cannot be
expressed as String or Integer objects. Specify the native operand carrier and
method lifetime as part of the production ABI. Today's pointer-encoded sizes
and method addresses are not evidence that every argument is a language object.
The experimental carrier is prior evidence only; it is not automatically the
new production ABI.

### Allocation before and after GC

Id allocates the initial universe without a collector. A later collector must
operate on those same allocations and identities. There must be one physical
object storage authority, not an immortal bootstrap arena beside a later heap.

Move GC policy out of allocation while preserving the original collection
opportunities when the complete system is mounted. Audit instance allocation,
cloning, Symbol creation and behavior creation; wrapping only one public
allocation function leaves bypasses. Observation must retain its scoped,
non-reentrant rules. Retire weak Symbols, behavior indexes and other relevant
metadata before an address can be reused. Collector policy owns the transaction;
Id owns the integrity of its storage and indexes.

### Dispatch and executable lifetime

Today's `send_abi` enters an Execution-owned read scope **before effectful
lookup**, and `resolve_stackless_target` classifies the exact returned method
without looking it up again. Read-scope exit can drain pending retirements.

The upper execution layer must preserve that protection and ordering without
Id constructing ExecutionState or reserving an execution callback. Simply
installing a trampoline that acquires ownership after lookup is insufficient.
Likewise, pinning all replaceable generated code forever would erase the
existing retirement contract.

The production dispatch design must name the entry path that owns the scope,
how native and resumable lookup implementations reenter it, how the exact
lookup answer remains callable, and what happens on scope exit. If this uses a
replaceable send protocol, specify how its own dispatch terminates and how its
implementation remains live. Saying "use message sends" does not settle that
recursion or lifetime problem. **This contract must be resolved before the
dispatch cutover; this document does not pretend it has already been proved.**

### Method mutation and representation consumers

`Runtime::add_method` currently observes direct binding presence, snapshots the
Store notification, invokes it before mutation, and then updates executable
ownership/publication accounting. Move the upper responsibilities with those
ordering requirements. Do not leave a consumer-specific notification registry
in Id as the new abstraction.

The current 80-byte raw Vtable payload also includes trace, layout and owner
fields. Moving their Rust owners while retaining required upper-service slots
in Id's representation is incomplete. Migrate native and image consumers with
the representation contract. Preserve current parent validation and lookup
behavior; changing delegation semantics is a separate language decision.

### Compiler, image and private native entry discovery

The first Id implementation can remain Rust while we fix production layering.
This does not require another authoring experiment or a Birnam rewrite first.
Later, a Birnam-authored Id will require code generation that does not assume
an initialized runtime; today's generated module prelude is not that facility.

For this decoupling, move runtime/compiler work out of Id and remove its normal
Codegen and Native Compiler dependencies. Migrate Codegen's private symbol
discovery to the actual upper owner; keeping forwarding exports in Id fails the
boundary. Load the image reader before asking it to reconstruct an image. Source
recovery must use the loaded compiler. Neither belongs in Siphon or Id.

## 7. Production migration and stopping point

Do this in the existing production crates, installer and regression harnesses.
Do not add another seed, alternate implementation workspace or demonstration
that the shipped executable does not use. Existing experiments remain historical
evidence, not the next work queue.

Use three substantial work packages, with buildable commits inside them:

1. **Make Id self-contained and put runtime policy above it.** Separate the
   actual storage/Symbol/behavior/dispatch owner from `RuntimeState`; move upper
   state and lifecycle together. Reunite any necessary Id-owned backing storage.
   Resolve the dispatch, allocation and mutation contracts in section 6 against
   their production callers. A renamed or relocated full Runtime does not meet
   this package's acceptance.
2. **Cut over construction and module loading.** Replace pre-Id preparation and
   the aggregate initializer with the ad-hoc-to-real Id loading protocol.
   Migrate the existing module initializers and move the module sequence into
   loaded bootstrap code. Exercise the actual production Id both alone and as
   the foundation of the complete product. The first two packages are coupled;
   do not declare either boundary complete while it still needs the old path.
3. **Complete consumer migration and delete the scaffolding.** Move private
   symbol consumers, compiled callers, image/recovery entry points and host
   command orchestration to their actual owners. Remove obsolete factories,
   adapters, build/install entries and temporary compatibility paths. Update
   the startup inventory and diagram to show the verified construction path.

Every implementation commit should state which ownership/dependency change it
makes and what remains. Commit completed progress, but do not invent a design
number, ABI family or separate installation campaign for every field. Existing
ABI consumers can migrate together; backward compatibility is not a reason to
retain the aggregate facade.

The stopping point is **the production system loading from a self-contained Id
through the intended protocol**, with existing supported behavior preserved and
the old preparation path removed. Rewriting Integer or another extracted module
in Birnam follows that result. It is not the next substitute for achieving it.

## 8. Evidence required to call this complete

Use debug builds and the existing independent oracles. Release validation was
explicitly waived. Extend production coverage where the architectural invariant
is missing; do not clone the old per-extraction fixture matrix.

| Acceptance | Evidence required |
|---|---|
| Id has no upper prerequisites | Initialize the actual installed Id with every upper Birnam library unavailable. Inspect normal Cargo dependencies, native imports, dynamic loading and initializer behavior; a symbol-list check alone is insufficient. |
| The first load uses the specified protocol | Exercise the real Siphon handoff. No twenty-service preparation, semantic module list, hidden compiler or alternate test-only Id path. |
| Real Id loads the next module | Through a message send, load an actual production module using only Id and explicitly available predecessors. An ad-hoc fixture or a renamed full runtime is not the result. |
| Later behavior is effective | Replace lookup, allocation and mutation behavior through the supported protocols and exercise native as well as compiled clients. Detect cached/direct bypasses. |
| Identity and lifetime survive layering | Preserve live replacement, alias identity, lookup effects, GC across early allocations, weak Symbol/behavior retirement, finalization, acquired executable lifetime and BUSY observation boundaries. |
| The complete shipped paths work | Existing debug core, loaded-source/compiled, mapped/rebuilt/baked image and relevant worker coverage, using the same Id artifact as the independent startup check. |
| The old architecture is gone | No unused pre-Id factories, aggregate compatibility table, private forwarding facade or installer dependency that reconstructs it. Documentation and diagram agree with executed startup. |

Passing regression tests establishes preserved behavior. Passing the independent
startup and dependency checks establishes the new architecture. Both are needed.

## 9. Rule for future refactors

Before accepting an extraction as decoupling, answer:

1. Which complete responsibility leaves Id, and which dependency edge disappears?
2. Can Id still construct and operate with the extracted module absent?
3. Does the new owner use only Id and already loaded predecessors?
4. Which production path proves that result, and which lifetime/mutation contract
   could the change break?

If Id still requires the extracted component, classify the change as internal
modularization. Do not count it as progress toward an independent Id. If the
answer requires another future callback or prepared factory in Id, correct the
ownership boundary before proceeding. **The measure is removed dependencies and
completed responsibilities, not the number of libraries or deleted API fields.**
