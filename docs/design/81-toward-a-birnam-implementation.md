# 81 — Toward a Birnam implementation

> **Status:** Exploration, 2026-09-17. Records the proposed direction after
> stackless execution and the current concurrency increments. This is not an
> implementation authorization or a claim that the remaining runtime can already
> be translated without performance or semantic changes.
>
> The source-verified starting point is [81A — Current bootstrap and runtime
> layers](81a-current-bootstrap-and-runtime-layers.md). Related contracts:
> [44 — Toward self-hosting](44-toward-self-hosting.md),
> [60B — Machine and Frames](60b-stackless-machine-and-frames.md),
> [80 — Concurrency](80-concurrency-domains-and-structured-tasks.md), and
> [79C — Materialization ownership](79c-materialization-ownership.md).

## 1. Objective

Reduce the Rust and C implementation in preparation for a system whose compiler,
libraries, object runtime, and execution machinery are implemented in Birnam.
The largest enabling task is making Birnam capable of implementing its own
substrate efficiently, with its existing semantics intact.

Stackless execution makes control state explicit. The concurrency work supplies
ordinary Birnam task policy over bounded Machine driving and process-domain
transport. Neither automatically supplies the low-level compilation and memory
facilities needed to implement the remaining runtime. Thread hosting and the
other unimplemented portions of Design 80 are not prerequisites to be assumed
complete by this document.

Distinguish three milestones:

1. The compiler is written in Birnam and compiles itself and the shipped modules.
2. The normal compiler/runtime implementation is built from Birnam, with a
   documented platform interface and external library dependencies.
3. The independent source-bootstrap implementation has also been reduced or
   replaced, if that is subsequently chosen.

The first two do not require deleting the independent Rust cold bootstrap.
Retain a from-source route requiring no prebuilt Birnam executable, image, or
compiled module. The installed execution boundary is specified below. Removal
of generated C and the treatment of external platform/FFI libraries remain
separate design decisions.

### Installed stage-1 requirement — clarified 2026-09-17

**The installed stage-1 executable should essentially be the siphon.** It loads
the components that bring the system up; the language/runtime implementation
comes from Birnam modules rather than statically linked Rust code. This is a
construction and execution boundary, not merely a ban on duplicate source files
or a request to move the same Rust implementation into a dylib.

Supported compositions include compiled Birnam modules (dylibs), an image, an
image plus dylibs, and images/dylibs supplemented by source. Retained source,
including source held in images and embedded-source fallback compilation, is
intended. It is not the redundant executable implementation this requirement
excludes. Source compilation must use the compiler brought up through the same
loaded system; it must not secretly invoke a statically linked Rust compiler.

Stage 0 may retain the independent Rust/C implementations needed to produce the
stage-1 components. Stage 1 must not call those bootstrap implementations to
bring up its Birnam replacements and then continue with them hidden underneath.
The loader must establish a first executable component and hand over to the
loaded system. Specify the siphon's minimal loading/platform operations and
implementation separately; do not use that small boundary to retain built-in
language, compiler, GC, Machine, or library behavior.

**Id is the first semantic layer.** Its loaded Birnam implementation constructs
its own object graph and manages the initial storage/ownership needed to do so.
Platform memory services are allowed, but no hidden Rust allocator, registry or
runtime sits below Id. Symbol and interning belong to this seed. Values, the
environment, GC, executable ownership, Frames, Machine
execution, control and optimization are thinner subsequent layers; see
[81C](81c-thin-id-runtime-layers.md). The collector mounts through Id's explicit
storage/root interfaces rather than being bundled into the first component.

Extract the Rust layers from Id upward. Backward compatibility is not a
constraint: minimize IdApi, remove higher-layer services from it, and update
consumers without retaining an aggregate compatibility facade. See the complete
field partition in [81D](81d-id-api-partition.md).

81D proposes these boundaries against Piumarta and Warth's object model. Exact
interfaces still require field/caller review, including the semantic difference
between direct parent-chain traversal and delegation through lookup messages.

The current installer copies the bootstrap host and native libraries unchanged.
It therefore does not meet this target: both the host's linked semantic machinery
and the Rust-authored native runtime rungs need to be accounted for. The retained
embedded sources are not themselves a defect. During migration, enumerate native
mechanisms still awaiting Birnam implementations as unfinished work.

Acceptance must trace calls during initial bring-up, ordinary execution, source
compilation and image recovery. None may depend on a statically linked Rust
implementation of the Birnam system. Merely replacing method entries after
executing their Rust bootstrap versions does not establish this property.

## 2. Six major workstreams

### 2.1. Make the compiler an ordinary Birnam program

Move grammar construction, syntax lowering, portable-code construction,
resumable-frame descriptions, and emission into Birnam. Start with canonical
compilation; migrate optimization passes after there is a dependable compiler
to build and investigate them with.

The existing split between the pure `birnam-codegen` crate and the integration
in `birnam-codegen-runtime` is useful, but it is not yet a complete self-hosting
boundary. The latter also owns compilation policy and lifecycle machinery.
Separate algorithmic compiler responsibilities from process-local code ownership
before treating either crate as a single migration unit.

First substantial acceptance: the Birnam compiler compiles itself and the shipped
Birnam modules, successive compiler stages reach a deterministic output fixed
point, and independent semantic/format checks agree. Specify which outputs are
compared; freshly compiled dylib bytes can include incidental toolchain metadata.
An image that merely contains a compiler calling Rust for all lowering is not
this milestone.

### 2.2. Establish facilities for runtime implementation

Audit and complete efficient machine-word arithmetic, byte buffers, memory
layouts, loads/stores, native entry points, and explicit root handling. Existing
Word, ForeignAddress, Alien, and compiler machinery are starting ingredients,
not proof that the complete programming model exists.

Use representative kernels to determine whether these facilities compose:
a hash-table probe, parser matching loop, or frame-arena operation. Measure
dispatch and allocation costs as well as total time. Do not introduce an opaque
native implementation behind every new Birnam wrapper and count that as migration.

Low-level optimization must preserve late-bound behavior. Guards and recovery
must account for changed methods and native participation, object identity,
shared Cells, observable Frames, roots, finalizers, and code lifetime. Decide
explicitly what primitive operations mean and how they can be replaced.

### 2.3. Move data structures and algorithms

Candidates include collections, the PEG matcher, Store indexes, and relational
evaluation. Query construction and result handling can move ahead of the hot
inner loops. HAMTs, RRB structures, parsing, indexing, and joins need measured
acceptance on representative workloads.

Design 44 classified several kernels as native for performance reasons. This
proposal reopens their eventual implementation language: those measurements
constrained the then-current compiler, not every future Birnam implementation.
They remain evidence to respect, not permission to accept regressions silently.

Acceptance requires both deleting or removing the old implementation from the
normal dependency closure and preserving behavior, memory/lifetime properties,
and suitable performance. Retained bootstrap copies must be accounted separately.

### 2.4. Move object and execution machinery

The deepest migration covers dispatch, vtables, interning, allocation, tracing,
frame storage, Machine transitions, and code-lifetime management. Existing object
interfaces permit mechanisms to migrate without requiring every caller to change.

Decompose these responsibilities first according to
[81C](81c-thin-id-runtime-layers.md). Id owns initial object storage, but GC is
a later layer; Frames, execution, control and optimization are also separate.
Each layer owns its state and uses explicit cross-layer interfaces rather than
continuing to mutate a single shared RuntimeState.

Task scheduling, cancellation, shielding, cleanup, and worker policy already have
substantial ordinary Birnam implementations. Avoid listing that work as wholly
native or proposing to reimplement it from scratch. The question is what remains
beneath those methods and which checked mechanisms they actually need.

GC is a separate design problem within this stream. Explicit Frames help expose
roots, but the current floor still supports conservative native-root discovery.
A Birnam collector needs a defined execution context, allocation discipline,
temporary-root protocol, foreign callback treatment, and finalizer behavior.
Moving the collector source alone does not settle those circular dependencies.

### 2.5. Replace generated C and libtcc

A Birnam compiler that emits C is a valuable intermediate result. Removing C from
the operational compiler chain requires a further backend: initially arm64
instruction emission, calling conventions, relocations, executable memory,
symbol binding, and native-code registration and retirement.

Image restoration and live replacement must use this backend with the same
ownership guarantees. Existing native methods and FFI remain participants in
the runtime even if Birnam-generated code no longer passes through C.

Keep frontend self-hosting and backend replacement separately testable. Taking
on both at the first compiler migration would obscure the origin of failures.
The old backend can remain an independent comparison/recovery path during cutover.

### 2.6. Close the host, image, and bootstrap loop

Move build orchestration, artifact handling, startup/project policy, and image
writing/restoration as appropriate. Identify the process-local platform
operations that remain rather than labeling the entire host irreducible.

The final build must demonstrate which components it can rebuild without the
Rust host/compiler/runtime, and which toolchains or seeds it still requires.
Exercise source bootstrap, successive self-hosted builds, ordinary startup,
saved-image recovery, and packaged execution as distinct paths.

## 3. Proposed sequence

The siphon requirement makes the construction boundary the first investigation.
The existing stage-0 compiler can compile Birnam implementations before it is
itself self-hosted; a Birnam compiler is not a prerequisite for beginning the
runtime migration. The following steps replace the earlier compiler-first outline.

### Step 1 — Specify the siphon handoff and construction dependencies

Use 81A's actual load sequence to map what each component needs before its
initializer can execute: allocation, object/vtable construction, dispatch,
symbols, roots, compiler services, loader services and platform operations.
Identify cycles explicitly, especially the runtime needed by generated code,
the allocator/collector relationship, and image restoration before a full object
environment exists.

Specify what the siphon may do before handoff, the first module's entry contract,
how it obtains platform services, and who loads each subsequent component. The
target must not hide the current Rust host behind a renamed launcher. Define the
minimum executable support needed to restore an image as part of this contract.

Deliverable: a proposed dependency graph and permitted entry/import surface,
with unresolved low-level capabilities identified rather than assumed.

[81B — Siphon handoff and construction graph](81b-siphon-handoff-and-construction-graph.md)
provides the first proposal: a platform-only boot entry, Birnam-owned primordial
Id construction, thin subsequent layers and explicit image/source support closure.
Its Step-2 findings identify what today's compiler has not yet demonstrated.
[81C](81c-thin-id-runtime-layers.md) supplies the eight-layer libid decomposition
and its state/ownership boundaries.

Prepare those boundaries in the existing implementation from Id upward. Begin
with [81D's IdApi partition](81d-id-api-partition.md), remove unrelated services
from the contract, and extract independently operable Id state and dispatch.
Then extract object-model libraries, values/environment, collection, ownership,
Frames, execution/control and optimization against the lower boundaries. This
can proceed independently of
the Birnam Id compiler-facility investigation. Rust extraction enables
replacement but is not self-hosting progress by itself.

### Step 2 — Establish the facilities to compile the first runtime component

Audit today's generated code and primitive facilities against that handoff.
Determine how Birnam-authored code expresses memory layout, word operations,
loads/stores, native entry points and root discipline before ordinary object
services are available. Add only the facilities required by the first slice.
Define their semantics and replacement behavior explicitly.

The stage-0 Rust compiler may initially emit these modules through C/libtcc.
Generated code must not call Rust implementations of the operations claimed to
have migrated. Performance probes qualify the facilities; they do not establish
that the whole runtime has migrated.

### Step 3 — Demonstrate a real siphon-to-Birnam runtime slice

Build a separate experimental stage-1 product alongside the retained stage-0
tool. Have its siphon load a stage-0-compiled Birnam Id module that establishes
a bounded object/dispatch path: construct initial objects/vtables, install a
method and send a message without the Rust libid implementing that path.

Specify allocation and root behavior for the slice, inspect its linked/imported
dependencies and exercise the actual call path. A wrapper that delegates back
to libid or a trivial native function that never exercises runtime construction
does not meet this milestone. This is a bounded proof, not a complete stage-1
replacement or a usable-system claim.

Id must implement its own initial object storage without a hidden native runtime
underneath. Collector mounting and higher-layer service publication are later
gates, not justification for putting GC into IdApi or the first Id component.

### Step 4 — Expand the loaded Birnam substrate

Migrate the remaining object, memory and execution mechanisms in the dependency
order established above: object-model libraries, values/environment, the
collector, executable ownership, Frames/descriptors, canonical Machine execution, continuations/control and
optional optimization. Collection structures, Store and the other runtime
libraries build on these capabilities. The graph branches; both Rust extraction
and target construction begin at Id, as described in 81C/81D.

Bring up the existing Birnam module manager, stdlib, CompiledUnit and concurrency
policy on that substrate. Remove the need to execute superseded Rust control
methods during startup, not only their use after method replacement. Validate
semantics, GC/FFI/code lifetimes, and representative performance as each native
dependency leaves the runtime closure.

### Step 5 — Bring the compiler into the loaded system

Move grammar construction, PEG matching as needed, canonical lowering and
emission, then optimization and materialization policy into Birnam components.
Compiler algorithm work may begin earlier using the current runtime, but its
stage-1 acceptance requires execution on the loaded Birnam substrate.

Demonstrate compilation of the compiler and shipped modules, successive-stage
output agreement and the absence of calls to the Rust bootstrap compiler.
Source evaluation and retained-source recovery remain supported through this
loaded compiler. Decide the direct-machine-code backend separately: its C/libtcc
removal milestone is additional to eliminating Rust system implementations.

### Step 6 — Complete every supported construction path

Move remaining host and image responsibilities into the loaded system. Exercise
dylib-only startup, image startup, image plus dylibs, and images/dylibs plus source
through the siphon. Verify replacement, source fallback, foreign binding recovery
and executable ownership in each applicable path. Retain source in images and
embedded resources as intended.

### Step 7 — Make the separation the installed product

The installer must publish the siphon and its Birnam components rather than
copying the stage-0 host/runtime into place. Keep the independent bootstrap tool
available as a distinct build/recovery product. Check installed binary/import
inventories and exercised execution paths to establish that no statically linked
Rust system implementation supplies startup, runtime, compilation or recovery.

Step 1 now has the handoff proposal in 81B, decomposition in 81C and IdApi
partition in 81D. [81E](81e-seed-sufficiency-audit.md) audits the integration
requirements, and [81F](81f-isolated-id-seed-proof.md) executes the first isolated
Rust seed/private-behavior witness. [81G](81g-integer-module-proof.md) adds an
Integer module without changing that seed or ABI. [81H](81h-storage-observation-and-tracing.md)
adds storage observation and a separate tracer while keeping the native API
carrier unchanged. [81I](81i-ordinary-storage-reclamation.md) adds ordinary storage
reclamation and transactional metadata retirement with forced address reuse.
[81J](81j-private-behavior-extraction.md) begins production extraction with the
existing private-behavior operation. Substantial new ownership/unloading designs
are outside this refactor; preserve existing semantics and lifetime machinery.
These guide bottom-up extraction
and Step 2's low-level authoring design. No document assumes
that today's compiler can already emit a self-sufficient Birnam Id module.

## 4. Progress and acceptance

Track responsibilities owned by Birnam, dependencies removed from normal builds
and running systems, remaining seed requirements, and native semantic code
deleted. Count Rust/C source reduction as supporting evidence; report tests,
fixtures, third-party code, generated output, and bootstrap copies separately.

Preserve independent compiler/format oracles and existing ABI, image, ownership,
and malleability contracts unless a separately specified migration changes them.
Correctness and measured performance are separate acceptance claims. This
exploration does not change current optimization defaults or validation gates.

## 5. Questions for the next design pass

- What are the actual stage 0, stage 1, and runtime layers? What replaces what?
  [81A](81a-current-bootstrap-and-runtime-layers.md) records the current answer.
- Which compiler operations can be authored immediately with today's objects,
  and which require new low-level facilities?
- What is the smallest complete runtime kernel that would meaningfully test
  those facilities and expose the dispatch/allocation cost?
- Which components must be available before the compiler and collector exist,
  and what explicit construction protocol breaks each cycle?
- What exact loading/platform operations belong to the siphon, and which
  external-library dependencies remain in the loaded system?

[81K](81k-owner-and-parent-protocols.md) continues the production refactor by
removing the four owner/parent fields and migrating ordinary consumers to
default-vtable messages. IdApi v3 has 49 fields / 392 bytes.

[81L](81l-production-integer-extraction.md) extracts the production Integer
family, primitive operations and methods. Its explicit native dependency reduces
IdApi v4 to 45 fields / 360 bytes. The remaining Machine and optimizer components
consume that dependency; the host and those runtime components are still Rust.
Focused validation passes; release validation was explicitly waived.

[81M](81m-production-storage-observation.md) adds protected production storage
facts and a separately loaded description validator. GC policy, native-root
capture and legacy tracing remain in the current runtime; their coverage gaps
are explicit. [81N](81n-production-storage-tracer.md) promotes the scoped graph
tracer over those facts into a loaded production module. Automatic GC's mark
path still awaits its root/provider and collection migration.

[81O](81o-production-live-gc-traversal.md) moves the actual production worklist
into the tracer using ordered live root/admission/edge callbacks. Every collection
path uses it; native-root and payload policy adapters, retirement and physical
reclamation remain in Id. Observation gaps are not used to authorize collection.

[81P](81p-production-collection-coordinator.md) moves collection coordination into
`birnam-collect`: phase transitions, settlement ordering, metadata retirement
before detachment, live finalizer invocation and completion. Owner-specific
metadata operations and physical allocation/reclamation remain in Id.

[81Q](81q-production-float-methods.md) begins the remaining primitive families
with Float's seventeen methods, loaded independently of Prelude. Float storage,
canonical objects, constructors and native consumers still await their own
checkpoint; the current IdApi stays unchanged.

[81R](81r-production-float-storage.md) moves Float representation and its actual
consumers onto an explicit dependency. [81S](81s-production-word-methods.md)
and [81T](81t-production-word-storage.md) do the same in two checkpoints for
Word. IdApi v6 now has 37 fields / 296 bytes. The installed modules and host
remain Rust; these are extraction boundaries for the later Birnam rewrite.

[81U](81u-production-boolean-methods.md) extracts Boolean's existing methods from
Prelude. The singleton representation and shared IdApi fields remain in Id for
a separate caller/startup audit; this checkpoint adds no native interface.

[81V](81v-production-nil-methods.md) moves the five existing Object/nil methods
out of Collection. Nil's identity and roots still await their separate audit.

[81W](81w-production-string-methods.md) extracts fourteen String methods with
explicit numeric dependencies and selector roots. Array splitting stays with
Collection, and String's byte ownership/cleanup remains in Id for later review.

[81X](81x-production-string-storage.md) moves String's canonical objects, owned
byte buffers, primitive operations and cleanup into its loaded module. The floor
supplies generic native construction with the existing collection/copy/finalizer
schedule; retirement uses String's exact ownership proof. IdApi v7 has 34 fields /
272 bytes. Array splitting and StringBuilder remain in Collection.

[81Y](81y-production-string-builder.md) moves StringBuilder's native buffer,
cleanup and seven selectors from Collection into its own module. It loads after
String, preserves the existing Vector publication guard and live allocation
callbacks, and leaves IdApi/image encoding unchanged.

[81Z](81z-production-range-methods.md) extracts six Range methods. Collection
uses its construction dependency at the original registration point, preserving
publication and reload semantics. Range storage and its two Vector-backed methods
remain for later construction and storage checkpoints.

[81AA](81aa-production-range-storage.md) moves Range representation, canonical
objects and Integer `to:` into the Range module. Explicit construction stages
retain the existing interleaving with Vector/Map; the two remaining Vector-backed
methods read bounds through the immutable Range dependency. IdApi v7 and Image
encoding remain unchanged.

[81AW](81aw-production-namespace-state.md) moves ordered raw export bindings
into Namespace. [81AX](81ax-production-namespace-names.md) moves reverse-name
lookup and its Image, Alien, Reflect and Portable Code consumers onto a live
Namespace capability. IdApi v8 has 33 fields / 264 bytes. Import, export, presence
and enumeration remain in Id for subsequent extractions.

[81AY](81ay-production-namespace-enumeration.md) moves raw enumeration operations
into Namespace-owned descriptors and migrates Reflect while retaining its live
selected-floor discovery. The two Id operation exports are removed; import,
export and presence are still later work.
