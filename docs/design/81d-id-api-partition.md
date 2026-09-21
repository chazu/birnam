# 81D — Restructuring IdApi around the Id object model

> **Status:** Historical architectural position and original v1 field inventory
> (2026-09-18, `162b68fe`). Production extraction has since reached 81CA;
> [current status](81-production-extraction-status.md) and
> [IdApi v20](../../abi/idapi-v20.json) supersede its current-state counts.
> Proposed destinations remain subject to semantic and caller review.
> [Design 82](82-minimal-production-id.md) supersedes the extraction strategy and
> loading boundary: primitive native module loading is an Id message protocol;
> higher module policy stays above it. Removing table fields is not evidence of
> decoupling while Id still requires the extracted implementations.

[81](81-toward-a-birnam-implementation.md) owns the self-hosting roadmap;
[81A](81a-current-bootstrap-and-runtime-layers.md) describes today's system;
[81B](81b-siphon-handoff-and-construction-graph.md) proposes the siphon handoff;
[81C](81c-thin-id-runtime-layers.md) describes the larger runtime decomposition.
This document refines their Id boundary, including keeping Symbol and interning
in the foundation rather than deferring them to a general naming layer.

[81E](81e-seed-sufficiency-audit.md) records the upper-layer integration
obligations. [81F](81f-isolated-id-seed-proof.md) executes a bounded standalone
Rust seed/private-behavior witness. The table below preserves the starting
partition, while 81J–81CA record implemented production boundaries.

[81J](81j-private-behavior-extraction.md) starts the production refactor: private
behavior convenience moves into a loaded object-model library and IdApi v2 drops
that field. The 54-field mapping below remains the original inventory.

## 1. Reference model and position

The reference is Ian Piumarta and Alessandro Warth,
[*Open, extensible object models*](https://tinlizzie.org/VPRIPapers/tr2006003a_objmod.pdf),
VPRI TR-2006-003-a; its cover says *Open Reusable Object Models*.

The paper starts with three object types and five methods:

| Type | Essential methods |
|---|---|
| Object | None initially |
| Symbol | `intern` |
| Vtable | `addMethod`, `lookup`, `allocate`, `delegated` |

`send`, `bind` and explicit construction establish the initial universe. Lookup
is behavior: a vtable need not be a dictionary. The default lookup delegates by
sending `lookup` to its parent. A special case in binding terminates metacircular
recursion. See §§3.1–3.3 and Appendix B.

**For Birnam, retain this small, self-supporting object model and implement
additional services above it.** Minimize the native interface without reducing
access to, or replaceability of, the behavior that implements the object model.
This is the proposed architecture, not a claim that copying the paper's sample
code supplies Birnam's full lifetime, persistence or execution contracts.

Three distinct things must be separated:

1. **Id implementation:** storage, object/behavior association, Symbols, default
   behavior objects, resolution and invocation. Ultimately this is loaded Birnam
   code; during extraction it remains Rust.
2. **Id protocols:** the replaceable methods through which objects and behavior
   objects are created, queried and changed.
3. **IdApi:** the smallest native entry/handoff needed to reach that universe.
   It should not duplicate every useful method as an independent native function.

Id is the first semantic layer. Its own implementation obtains platform memory
and constructs its objects. There is no Rust allocator, registry or dispatch
runtime beneath the Birnam replacement. A collector mounts later; initial
construction uses bounded, tracked storage with explicit ownership and roots.

## 2. Proposed Id boundary

### Keep a complete object-model seed

Keep Object, Symbol and Vtable; symbol identity and interning; initial storage;
the default lookup/addMethod/allocate/delegated implementations; and the
send/bind/invocation mechanism. Raw construction calls are necessary before
messages work. They are implementation operations, not a reason to expose a
permanent parallel API that bypasses the installed methods.

Basic interning belongs here. Do not divide a handful of primordial selectors
from a later interner that must reconstruct or adopt their identities. Namespace
binding, symbol printing and String-valued convenience methods can still live
above Id. The symbol implementation must operate on bytes without first needing
the general String library. Weak interning, reclamation and borrowed name
lifetimes must be reviewed when extracting today's implementation.

Treat the association between an object and its behavior as fundamental.
Treat the current default vtable layout as an implementation of a behavior
protocol, not a mandatory representation for every possible behavior object.

### Prefer messages after construction

The proposed native handoff provides access to the primordial objects/selectors
and an entry for ordinary sending. Whether particular raw representation or
invocation operations also require direct entries remains to be demonstrated
with actual consumers. This document intentionally does not freeze a replacement
C struct or promise a particular field count.

`proto`, `method`, `alloc` and `intern` currently offer direct native access to
operations that also have, or can have, object-protocol forms. Migrate ordinary
callers toward those protocols. A call requesting ordinary allocation must honor
the selected allocation method. An explicit primitive used to implement that
method has a different contract and must be named and scoped accordingly.

The existing methods pass some sizes and method addresses as pointer-encoded
arguments. Removing direct fields does not make those arguments ordinary objects;
the replacement calling and representation contracts must state their meaning.
Do not force boxed numbers into the construction path merely to remove a field.

### Expose representation without imposing a universal schema

Id must provide enough access to its own state for extension, storage management
and eventual collection. It need not require all objects to use a universal slot
schema, nor force every behavior object into Rust's `VtableRecord` representation.

Put private-vtable conveniences, default-vtable parent/owner access and indexed
reflection into an object-model library above the seed. That library must work
through declared representation primitives and ordinary protocols. It must not
access Rust-private structures or bypass supported mutation visibility.

GC and Image both need representation information. Define a shared description
protocol above Id for user payloads; Id supplies descriptions/root access for its
own internal objects and storage. Keep collection policy out of that protocol.
A raw storage fact, such as allocation extent, may need a narrow direct interface
if a consumer cannot safely obtain it through an allocating or reentrant send.
That exception requires a concrete justification, not an omnibus IdApi table.

## 3. Services to extract above Id

These are the recommended responsibility boundaries. They need not each become
one dylib, nor form one linear startup chain.

| Service                        | Owns                                                                                                              | Relation to Id                                                                    |
|--------------------------------|-------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------|
| Object-model library           | Private-vtable convenience, default-vtable reflection, symbol presentation, enumeration and observation protocols | Uses Id behavior/representation contracts; does not dictate all lookup semantics  |
| Values                         | Integer, Float, Word, String, language nil and Booleans                                                           | Constructs objects and installs behavior on Id                                    |
| Environment and module binding | Named exports/imports and reverse lookup                                                                          | Uses Id Symbols; distinct from Store-backed Globals                               |
| Representation descriptions    | Payload slot descriptions consumed by GC and Image                                                                | Extends Id's own storage/edge descriptions without becoming a prerequisite schema |
| Collector                      | Root integration, tracing, collection policy and finalization orchestration                                       | Uses Id storage and registered descriptions; no private RuntimeState access       |
| Executable ownership           | Callable ownership, publication, leases and retirement                                                            | Cooperates with mutation/invocation through explicit contracts                    |
| Frames, Machine and control    | Resumable execution and continuation mechanisms                                                                   | Supplies invocation behavior without embedding its driver in Id                   |
| Optimization                   | Optional caches and compiled specializations                                                                      | Depends on the applicable semantic and mutation contracts                         |

Platform loading is separate from this semantic graph: move loader access out of
IdApi to the platform boundary described in 81B. It is not another Birnam runtime
layer below Id. Compiler, Store, libraries and image restoration build on their
actual service requirements. Images and source retention remain intended.

The first extraction starts with Id, then mounts the required services above it.
It does not begin by extracting the optimizer. Consumers receive the specific
objects/capabilities they use; there is no replacement aggregate compatibility
facade under a different name.

## 4. Historical mapping of all 54 v1 fields

The original [v1 ABI manifest](../../abi/idapi-v1.json) records 54 fields:
44 function pointers and 10 object pointers, occupying 432 bytes. The current
[Rust declaration](../../rust/birnam-abi/src/lib.rs) implements v20, with 18
fields / 144 bytes; do not use this historical table as a removal checklist.
The following mapping records each exactly once. These are recommended semantic
owners and migration directions, not finalized replacement signatures.

### Id seed and native entry candidates — 9

| Current field | Proposed treatment |
|---|---|
| `object_vt` | Keep primordial identity; provide through the construction handoff |
| `vtable_vt` | Keep primordial identity; avoid treating its default layout as universal |
| `symbol_vt` | Keep Symbol in the seed |
| `intern` | Keep interning behavior in Id; ordinary callers should reach the installed method |
| `proto` | Keep default behavior creation as `delegated`; review need for a separate native entry |
| `method` | Keep method installation as `addMethod`; separate executable publication accounting |
| `alloc` | Keep initial storage and `allocate`; separate collector policy and ordinary versus raw allocation |
| `send` | Keep the native route to ordinary dispatch; separate Machine driving and optional optimizations |
| `vtableOf` | Fundamental behavior association query; review minimal raw versus message exposure |

Nine candidate fields do not imply a nine-field final API. Nor are these nine
fields the whole seed: lookup currently appears as a method, not an IdApi field.

### Object-model library and observation — 10

| Current field | Proposed treatment |
|---|---|
| `symbolName` | Symbol presentation/byte-access protocol with explicit borrow lifetime |
| `ensurePrivateVtable` | Library operation using behavior creation, association and mutation protocols |
| `setParent` | Default-vtable protocol; do not impose a universal single-parent representation |
| `setOwner` | Default private-vtable association protocol, not executable ownership |
| `vtableOwner` | Query that association through its protocol |
| `vtableParent` | Query default delegation state, not an assumption every behavior must satisfy |
| `vtableCount` | Enumeration service for consumers such as Store; define snapshot/root semantics |
| `vtableAt` | Same enumeration service, without leaking the Rust registry |
| `vtableSelectorAt` | Default method-dictionary reflection protocol |
| `setMethodOverwriteHook` | Replace the singleton hook with explicit observation/publication coordination |

Id still owns its essential mutations. Higher observation policy must not make
Store or an optimizer mandatory for those mutations. Conversely, moving the
observer API must not lose Store arbitration, callback ordering or executable
lifetime protection once those services mount. Cache invalidation cannot depend
solely on a user-replaceable observer that may fail to report a mutation.

### Storage and representation integration — 3

| Current field | Proposed treatment |
|---|---|
| `objectPayloadSize` | Id storage fact; expose only through the storage/representation interface consumers need |
| `setLayout` | Representation-description protocol above Id; preserve core edge descriptions independently |
| `slotKind` | Query the same description protocol for GC/Image and other representation consumers |

### Values — 18

| Current field | Proposed treatment |
|---|---|
| `smallint_vt` | Values: Integer identity |
| `mkSmallInt` | Values: Integer construction |
| `smallIntValue` | Values: Integer extraction |
| `isSmallInt` | Values: Integer representation test |
| `float_vt` | Values: Float identity |
| `mkFloat` | Values: Float construction |
| `floatValue` | Values: Float extraction |
| `isFloat` | Values: Float representation test |
| `word_vt` | Values: Word identity |
| `mkWord` | Values: Word construction |
| `wordValue` | Values: Word extraction |
| `isWord` | Values: Word representation test |
| `string_vt` | Values: String identity |
| `mkString` | Values: String construction and owned byte storage |
| `stringBytes` | Values: byte borrowing with explicit root/lifetime contract |
| `nilObj` | Values: language nil, distinct from a raw missing-binding sentinel |
| `trueObj` | Values: language true |
| `falseObj` | Values: language false |

Id implementation arithmetic does not require boxed Word/Integer objects.
Today's `object_identical` returns language Booleans; mount that ordinary method
with values while keeping raw identity available for constructing Id itself.

### Environment — 4

| Current field | Proposed treatment |
|---|---|
| `import` | Environment lookup |
| `exportObj` | Environment publication |
| `isExported` | Environment convenience, derivable from lookup under its contract |
| `exportedNameOf` | Environment reflection/recovery binding |

### Collector and reclamation integration — 8

| Current field | Proposed treatment |
|---|---|
| `gcSetStackBottom` | Collector/native-root integration |
| `gcRegisterRootProvider` | Root-provider registration with explicit provider lifetime |
| `gcCollect` | Collector operation |
| `gcLiveCount` | Collection/storage diagnostics outside ordinary IdApi |
| `gcSetAuto` | Collection policy |
| `gcSetPrecise` | Tracing policy |
| `setTraceHook` | Trace-provider integration with representation descriptions |
| `setFinalizer` | Reclamation/finalization protocol; preserve cleanup and callable lifetime |

The current placement of trace hooks in vtables and finalizers in heap records
is not a reason to make collector callback types part of the minimal API.
Id owns storage reclamation mechanisms; higher services determine reachability
and cleanup ordering. Define explicit disposal and collector attachment so that
construction storage, String buffers and native resources are not stranded.

### Platform loading — 2

| Current field | Proposed treatment |
|---|---|
| `dlopen` | Platform loader interface outside IdApi |
| `dlsym` | Platform loader interface outside IdApi |

Host/Codegen/Image must receive explicit library handles or provider identity.
Using the address of `dlsym` to locate the library containing runtime capabilities
is a current implementation convention, not a property of Id.

## 5. Semantic gaps to resolve before mechanical extraction

The important comparison is behavior, not matching field names to the paper.

**Default delegation currently walks Rust records.**
[`Vtables::lookup`](../../rust/birnam-id/src/id/vtable.rs) loops over registered
parents; `set_parent` requires a registered vtable or null. The canonical lookup
method in [`dispatch.rs`](../../rust/birnam-runtime/src/dispatch.rs) calls that walker.
Our proposed boundary must allow a parent to participate through its own lookup
behavior. Investigate the current guard paths and representation restrictions,
and demonstrate the desired behavior before extracting them unchanged.
A direct walk may be an optimization only where its equivalence is established.

**Dispatch reaches execution machinery.** `send_abi` uses dispatch-read leases,
guarded Machine operations and stackless target resolution. Define the minimal
native invocation contract and the later execution-provider contract separately.
A synchronous Id method must not require a Machine. Once dynamic code ownership
mounts, no call or replacement may bypass its lifetime obligations.

**Method mutation reaches registration machinery.** `Runtime::add_method` and
removal update execution binding counts, Block catalogs and staged journals.
Separate dictionary mutation from publication/retirement through a coordinated
protocol; merely deleting that accounting would break live-code safety.

**Allocation and initialization bring up upper layers.** Current allocation can
collect; bootstrap constructs canonical values and the environment; public API
initialization installs execution objects. Extracting Id means removing these
upward dependencies, not returning fewer pointers from the same aggregate boot.

**State must follow ownership.** `PUBLIC_RUNTIME` and `RuntimeState` currently
join heap, vtables, symbols, values, exports, execution and registration. Give
services independent state and explicit contracts, not an interface accepting
the entire Runtime. Relevant source is in
[`public_api.rs`](../../rust/birnam-runtime/src/public_api.rs) and
[`runtime.rs`](../../rust/birnam-runtime/src/runtime.rs).

## 6. Performance without freezing behavior

The paper's §3.5 introduces inline and global method caches. §5.4 explicitly
excludes invalidation machinery from its sample and evaluation, and notes that
exposing `send`/`bind` is harder than exposing lookup.
[Source](https://tinlizzie.org/VPRIPapers/tr2006003a_objmod.pdf#page=15).

For Birnam, specify when a lookup result can be reused before choosing cache
placement. A custom lookup can perform effects or depend on mutable state beyond
a vtable dictionary. A matching receiver vtable is not sufficient evidence that
skipping lookup preserves current Birnam behavior.

The initial semantic oracle should work without caches. An optimized path must
establish the applicable lookup semantics, account for every dependency it
skips, and handle mutation and foreign reentry at the required observation
boundaries. Keep active implementations alive while subsequent sends see the
appropriate current bindings. Do not promise arbitrary live replacement of
object representation or the whole dispatcher without a state/lifetime design.

Fast native method bodies and low-overhead dispatch remain goals. A layer split
must not impose a service call for every internal load/store. Any direct API
exception retained for speed needs representative measurements, a semantic
justification and evidence that the corresponding malleable protocol remains
authoritative. More ambitious inlining/unboxing is later compiler work, not a
prerequisite for understanding or extracting Id.

## 7. Bottom-up implementation sequence and acceptance

1. **Establish the semantic reference.** Trace the seed, send/bind, default and
   overridden lookup, interning, allocation and method installation. Review exact
   signatures, callers, state access, reentry, lifetimes and first boot use.
   Resolve default parent delegation and the metacircular recursion boundary.
2. **Define the minimal handoff and raw exceptions.** Separate construction-only
   operations from ordinary methods and specify the representation/invocation
   contracts needed by later services. No nine-field target is presumed.
3. **Extract a standalone Rust Id.** It constructs its graph, interns selectors,
   installs/invokes methods and supports replacement and custom lookup without
   Values, GC, Store, Machine, compiler or optimizer. Rust remains the temporary
   implementation language, not the final installed substrate.
4. **Mount the upper services against it.** Extract values, environment,
   representation descriptions and collector integration, then executable
   ownership and execution layers according to their actual dependencies.
   Move native initializers and generated bindings to their required services.
5. **Replace thin implementations with Birnam.** Preserve independent stage 0;
   the stage-1 siphon loads the Birnam Id and required support modules/images.
   Low-level Birnam authoring remains separate compiler work in 81B.

The standalone proof must exercise method replacement/removal, delegation through
custom behavior, lookup side effects and selector identity. It must not silently
construct the old Runtime behind the smaller API. Later proofs cover collector
attachment to the original storage, foreign reentry, code retirement, Store
observation, image identity and behavior with optional optimizations absent.

Backward compatibility is not a gate. Update consumers, generated bindings,
function-identity checks, ABI fingerprints and admission checks together; reject
stale artifacts instead of interpreting them using the new layout. Retain old
fixture provenance while replacing obsolete compatibility gates with independent
checks for the new contract. No unrelated image-format change is implied.

Follow the [validation workflow](../validation.md) for implementation changes,
including exhaustive runtime/ABI/lifetime coverage and appropriate installed
import audits. This document records a proposed design, not completed checks or
authorization to omit existing semantic tests.

[81K](81k-owner-and-parent-protocols.md) continues the production refactor by
removing the four owner/parent fields and migrating ordinary consumers to
default-vtable messages. IdApi v3 has 49 fields / 392 bytes.
