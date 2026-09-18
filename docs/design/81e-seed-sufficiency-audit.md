# 81E — Can the upper layers be implemented through the seed?

> **Status:** Source-level dependency audit and proposed contracts, 2026-09-18.
> Runtime source checked at `162b68fe`. [81F](81f-isolated-id-seed-proof.md) now
> executes the first bounded Rust seed/object-model witness; [81G](81g-integer-module-proof.md)
> adds an Integer module through the unchanged boundary. [81H](81h-storage-observation-and-tracing.md)
> executes storage observation and separate tracing. Production runtime
> extraction and Birnam primitive authoring remain unproved. This refines
> [81D](81d-id-api-partition.md), not a claim that all boundaries are executable.

## 1. Result and scope

**The five-method object-model seed alone is insufficient as a declared
interface for all the proposed slices.** The architecture remains plausible,
but additional contracts are needed wherever an upper layer must observe or
change state owned by Id. Other requirements belong to the compiler's low-level
facilities, platform services, or already-mounted upper layers.

The audit covers the eight responsibility groups in 81D, separating Frames,
Machine and control, and checks collections, Store, module/compiler services and
images as consumers. It is not a complete audit of every native module export.
The reference object model remains the paper linked in 81D; the integration
contracts below are proposals for Birnam's additional requirements.

Three concrete counterexamples establish why a message entry and five methods
are not enough by themselves:

- `ensurePrivateVtable` must attach a new behavior to an **existing** object.
  Creation and method installation alone do not expose that operation.
- A collector must discover allocations/roots and reclaim storage. A send
  interface does not make private allocation registries observable.
- Dynamic code must remain callable across replacement and reentry. Installing
  a bare method address does not specify how long that address remains valid.

These are interface gaps, not reasons to leave Values, GC or Machine inside Id.
They also mean our earlier instruction to move reflection wholesale above Id
needs refinement: higher-level reflection can move; access to authoritative
seed state must be supplied by the seed implementation.

## 2. What counts as implementing a layer through Id

A layer may use:

- **S — Seed semantics:** Object/Symbol/Vtable, interning, allocation, default
  behavior construction/mutation, ordinary resolution and invocation.
- **I — Declared Id integration contracts:** the additional access described in
  section 3, justified by actual consumers rather than copied from RuntimeState.
- **L — Low-level Birnam facilities:** native entry definitions, raw arithmetic,
  branches, calls, and access to the layer's own declared payloads/buffers.
- **P — Explicit platform facilities:** memory mapping, loader operations, native
  root capture, I/O and foreign ABI operations where required.
- **U — Named upper dependencies:** services already mounted on the same Id.

It may not use a private Rust registry, `&Runtime`, `&mut Heap`, an assumed
`VtableRecord` layout, or a callback to the replaced stage-0 implementation.
Raw memory access to a layer's own representation is L; poking into another
layer's hidden state is not an acceptable substitute for I or U.

A native method written in Rust can exercise a prospective boundary during
extraction. That would prove a Rust interface split, not that Birnam can yet
express the method. Today's [C emission prelude](../../rust/birnam-codegen/src/lib.rs)
references boxed values, globals and other runtime services. L therefore remains
a separate, unproved authoring capability, as recorded in [81B](81b-siphon-handoff-and-construction-graph.md).

The contract is not "everything depends directly on five functions." A Machine
can depend on Frames and ownership, and an image reader can depend on collection
and representation services, provided the transitive graph reaches declared
Id/platform boundaries without hidden native semantics.

## 3. Minimum integration obligations exposed by the audit

These identifiers name responsibilities for review, not new IdApi fields or a
second aggregate table. Prefer methods on the relevant seed objects. Use direct
entries only where phase, representation or invocation requirements justify them.
All are owned by the loaded Id implementation; none is a semantic layer below it.

| Contract | Required operations and limits | Why it belongs at the seed boundary |
|---|---|---|
| I1 — Behavior association | Read the behavior attached to a live object; change it without changing object identity; define checks, roots and mutation visibility | Private-vtable construction and image reconstruction cannot update a hidden object header independently |
| I2 — Default behavior and Symbol access | Read/mutate default behavior state through its declared protocol; obtain direct bindings/parent data when requested; borrow Symbol bytes with a lifetime contract; provide a way to discover preexisting default behaviors | Those facts live in Id today; the protocol must not claim all behavior objects have the default representation |
| I3 — Storage ownership | Query membership/extent, traverse owned allocations in a defined stable phase, expose core roots and strong/weak relationships, detach/reclaim storage, and retire seed indexes before address reuse | GC must consume ownership facts while Id remains the storage owner; mark queues and reachability policy stay in GC |
| I4 — Collection attachment | Establish allocation safepoints/root scopes; mount providers over the original construction heap; define callback restrictions, disposal order and collector-unavailable behavior | Allocation and reclamation cross layer boundaries; the collector must not reach into Id to coordinate them |
| I5 — Callable publication and invocation | Coordinate binding install/remove with acquired callable ownership; protect resolution through invocation and reentry; notify binding retirement, including collected dictionaries; route registered execution forms | A method address can outlive its dictionary binding; the ownership registry and Machine driver remain above Id |
| I6 — Supported mutation visibility | Define authoritative mutation events and observation ordering for behavior association, bindings and default delegation; optional optimizers can validate relevant changes | Store/ownership/cache consumers cannot independently infer all changes from a single overwrite hook |

I2 does not necessarily require today's global indexed enumeration API. A
construction snapshot plus registration of later behavior objects may suffice;
late mounting must still discover everything it is supposed to mirror. Its
exact carrier and completeness rules are open. Private-vtable owner bookkeeping
can be maintained by the upper object-model library; that alone does not justify
retaining `setOwner` as a seed primitive.

I3 is not an instruction to export `Heap` or all its current methods. Propose
opaque allocation identities and controlled traversal/reclamation rather than
Rust object pointers. Core roots/edges include the behavior graph and Symbols'
owned bytes/index relationships. The interner's weak entries must be removed
before freed identities can be reused. A collector may maintain its own marks;
Id need not mandate the current mark-bit representation or algorithm.

I4 needs a concrete phase contract. Today collection prohibits allocation during
mark/finalize operations, and callbacks run without a Rust state borrow held.
Simply replacing each callback with an arbitrary allocating send would introduce
recursion and stale traversal state. Prepare/pin suitable provider entries before
the phase or otherwise demonstrate equivalent nonrecursive operation. Providers
may be malleable while still obeying a declared invocation/lifetime contract.
No collection policy or permanent String/Frame-specific finalizer belongs in Id.

I5 must be effective **before lookup can reenter foreign code**, not only after
lookup returns a method. Seed and early support code can remain mapped for the
process lifetime; dynamic unloading starts only after ownership is mounted.
The default seed path must work without an execution registry. The attached
provider handles resumable entry without putting its Machine loop back in Id.

I6 describes supported mutation, not a universal promise to detect arbitrary raw
writes. An optimizer must refuse assumptions it cannot validate. Custom lookup
that depends on unrelated mutable state or has effects cannot be justified by a
default-vtable mutation counter alone.

## 4. Layer-by-layer sufficiency assessment

"Constructible" below means a concrete source-level implementation approach
exists under the listed assumptions. It is not a compiled or executed proof.
"Blocked" means a named interface must be designed before extraction, not that
the architecture is impossible. Every row still depends on L for its eventual
Birnam implementation. 81F supplies an isolated Rust execution receipt for the
private-behavior part of L1, 81G for the Integer part of L2, and 81H for the
storage-description/reachability parts of L4/L5. The other obligations remain
unexecuted.

### L1. Object-model library and reflection — private behavior witnessed; reflection pending

**Evidence:** `Runtime::ensure_private_vtable` in
[runtime.rs](../../rust/birnam-id/src/runtime.rs) reads owner metadata, creates a
vtable, sets its owner, writes the object's header through `Heap::set_vtable`,
and records mutation. [vtable.rs](../../rust/birnam-id/src/vtable.rs) owns
registered parents, direct bindings and enumeration. [symbol.rs](../../rust/birnam-id/src/symbol.rs)
owns borrowed Symbol bytes separately from the object payload.

**Construction:** implement private-vtable reuse and owner bookkeeping above Id,
using I1 to reattach behavior and I2 for seed representation access. Define
reflection for default vtables as their protocol; custom behavior supplies its
own reflection if it supports it. Bind observations through I6.

**Proof required:** attach private behavior to one of two objects sharing a
behavior; only that object changes. Exercise a custom parent resolver and a
late-mounted observer. Reject any implementation that reads a Rust registry or
assumes all behavior objects use `VtableRecord`.

**Executed subset:** [81F](81f-isolated-id-seed-proof.md) passes private-behavior
construction/reuse, existing-object reassociation, custom parent lookup, method
removal/replacement and reentry in a standalone seed and separate upper dylib.
It uses explicit I1 entries and part of I2's method protocol. Complete reflection,
late-mounted observation and production migration remain open; uncached sends
observing changes do not establish I6's observer contract.

### L2. Values and collections — Integer witnessed; remaining values and lifetime integration pending

**Evidence:** [values.rs](../../rust/birnam-id/src/values.rs) writes numeric
payloads after allocation; Strings additionally own byte buffers in a side map.
[Collection](../../rust/birnam-collection/src/lib.rs) registers layouts, root
providers and finalizers, and extends the String parent protocol.

**Construction:** use S for types/objects/methods and L for each layer-owned
payload. Store buffers inline where suitable or own them with explicit disposal.
Normal reclaiming operation uses representation and collector services, I4 and
I6 as applicable. General collections must not be a hidden prerequisite of the
seed's own dictionary implementation.

**Proof required:** bounded numeric and collection operations without Machine,
Store or compiler; preserve object identity and overridden operations. Then
mount GC and reclaim owned buffers exactly once while honoring outstanding
borrows. A bounded no-GC demonstration alone is not production acceptance.

**Executed subset:** [81G](81g-integer-module-proof.md) supplies a separate Integer
dylib using the unchanged 81F seed/ABI. Construction, payload access, checked
arithmetic, method/factory/allocator replacement and private-behavior composition
pass in debug/release. An Integer-only process proves no dependency on the
object-model library. Other value families, collections, reclamation and Birnam
authoring remain unproved.

### L3. Environment and binding — constructible above Symbols/collections

**Evidence:** [env.rs](../../rust/birnam-id/src/env.rs) stores Symbol/object pairs
with forward and reverse indexes and supplies both halves as roots. It has no
need to define object identity or lookup semantics.

**Construction:** implement a dictionary and reverse-name protocol using S and
layer-owned state. Register its roots with the collector when mounted. An
explicit bootstrap handle passes the environment to consumers; Id does not need
an environment to find itself. Store-backed Globals remains a later service.

**Proof required:** publish, replace and reverse-query bindings, preserving aliases
and roots through collection, without Store or Machine. No seed mutation may be
required merely to add an exported name.

### L4. Representation descriptions — tracing subset witnessed; image/external storage pending

**Evidence:** default vtable layouts are stored in
[vtable.rs](../../rust/birnam-id/src/vtable.rs); both GC and
[Image](../../rust/birnam-image/src/lib.rs) consume slot kinds/payload size.

**Construction:** layer-owned descriptions distinguish object references, raw
words and executable references. Use I2/I3 for seed facts; do not reinterpret
seed-private bytes. Descriptions for foreign buffers and Frame arenas come from
their owners. GC tracing and durable image serialization need different visitors:
a trace callback alone cannot reconstruct an object's representation.

**Proof required:** one description supports correct tracing and image relocation
without making those consumers share private state. Include a custom behavior
representation and a buffer outside the Id allocation registry.

**Executed subset:** [81H](81h-storage-observation-and-tracing.md) obtains core
roots/edges and allocation extents through a seed-owned observation message.
Upper modules describe their payload slots by representation identity; Pair
tracing survives reassociation with Integer behavior. Missing and conflicting
descriptions fail closed. External native buffers and image relocation remain
unproved.

### L5. Collector — reachability witnessed; reclamation and lifetime integration pending

**Evidence:** [gc.rs](../../rust/birnam-id/src/gc.rs) marks through `Heap` and
captures native roots using a platform register-save operation.
`gc_collect_from_native_range`, `run_mark_phase` and `mark_floor_roots` in
[runtime.rs](../../rust/birnam-id/src/runtime.rs) directly consult values,
exports, execution registries, Symbols and vtables, retire bindings, and invoke
finalizers before reclaiming detached storage.

**Construction:** implement tracing/sweeping above I3, with the collector's own
work memory and marks. Each mounted service supplies roots/descriptions. Use I4
for phase control and P for native root capture. Before reclaiming a dictionary,
retire its callable references through I5. Retire weak Symbol/index entries and
invalidate affected address-derived metadata through declared contracts.

**Proof required:** attach to the original seed heap without duplicating or
leaking it; trace native reentry roots, cyclic graphs and Frame roots; retire weak
Symbols and dead method bindings; preserve finalizer ordering and allocation
restrictions; reclaim while the optimizer is absent. Check construction-root
release and safe address reuse. General root callbacks alone do not prove this.

**Executed subset:** 81H loads a tracer over existing allocations, computes exact
live/dead partitions including cycles/shared references, and checks a guarded
owner-thread observation phase. Native roots are explicit, Symbols remain strong,
and no memory or indexes are reclaimed. This establishes part of I3/I4, not a
complete collector or automatic production safepoint/root discipline.

### L6. Executable ownership — blocked on I5 and binding retirement

**Evidence:** [ExecutionState](../../rust/birnam-id/src/execution.rs) currently
owns dispatch-read scopes, acquired entries and generation leases.
`Runtime::add_method`/removal adjust binding counts and staging journals.
[registration.rs](../../rust/birnam-id/src/registration.rs) validates carriers,
while live ownership and publication live elsewhere.

**Construction:** a separate owner registry retains mapped code and supplies
opaque callable/lease handles through I5. Loaders and compilers use that same
registry; neither is required to own it. Synchronous seed entries are initially
process-pinned. The seed cannot expose an unprotected bare address and expect an
upper layer to repair its lifetime after reentrant lookup.

**Proof required:** replace a method during a callback while the old invocation
continues; new sends observe the replacement, and unload occurs only after all
active calls, bindings, Frames, continuations and rollback leases release it.
Repeat retirement through dictionary collection, not just explicit removal.

### L7. Frames/descriptors — constructible on storage, values and ownership

**Evidence:** `FrameArena` in [execution.rs](../../rust/birnam-id/src/execution.rs)
owns slot vectors, descriptors, program counters and generation-checked handles;
`trace_roots` uses the descriptor/root-state PC. Other execution-object helpers
currently accept `Heap` directly for membership, extent and payload operations.

**Construction:** move arena storage and descriptor validation to this layer.
Use its own L-backed buffers plus I3 queries where objects cross the boundary.
Root reporting and retained code owners use collector/L6 contracts. A general
Frame arena does not need a scheduler or optimizer.

**Proof required:** create/pop/reuse Frames, reject stale handles, trace only the
appropriate slots at transitions, and retain executable owners without loading
Machine driving or optimization. Do not replace typed-slot checks with unchecked
raw writes merely to eliminate the `Heap` parameter.

### L8. Machine driving — blocked on I5's resolution/invocation split

**Evidence:** `resolve_stackless_target` in
[runtime.rs](../../rust/birnam-id/src/runtime.rs) binds through Id, then classifies
the exact returned carrier in ExecutionState. `materialize_registered_target`
also reads capture storage directly. The driver resides in the same Runtime.

**Construction:** the Machine resolves each semantic send through Id and receives
a protected target; it owns Frame transitions and resumable driving. Id invokes
ordinary native methods without a Machine and routes registered resumable forms
to the attached provider. There must be no second independent lookup by the
provider, and no second resolver hidden in Machine.

**Proof required:** synchronous/resumable transitions, tail calls and reentrant
lookup preserve operands, effects and code ownership. A custom resolver returning
different targets on successive sends must work with optimization absent.

### L9. Continuations/control and task policy — depends on Frames/Machine, not new seed policy

**Evidence:** [scheduling.rs](../../rust/birnam-id/src/runtime/scheduling.rs) wraps
bounded drives and suspended-Machine state through Runtime. Continuation state
and root helpers are in [execution.rs](../../rust/birnam-id/src/execution.rs).
Task policy already exists in [stackless-control.birnam](../../src/id/stackless-control.birnam).

**Construction:** provide capture/resume, boundary and bounded-driving protocols
above L7/L8, with owner/epoch checks and retained roots. Keep scheduling and
cancellation policy in its Birnam layer. Explicit stackless state does not
require introducing native-stack capture into Id.

**Proof required:** capture, suspend, collect and resume; preserve code pins,
cleanup ordering and boundary checks. Run existing task policy against the
extracted mechanisms without a compiler or optimizer on its execution path.

### L10. Optimization — blocked on I5/I6 and recovery contracts

**Evidence:** [guarded_lookup.rs](../../rust/birnam-id/src/runtime/guarded_lookup.rs)
and [dispatch_cache.rs](../../rust/birnam-id/src/runtime/dispatch_cache.rs) read
vtable epochs, heap membership and execution classification directly.
[recovery.rs](../../rust/birnam-id/src/runtime/recovery.rs) reaches entry ownership,
Frame recovery authority and scoped roots; native tiers also compile code.

**Construction:** optional lookup caching consumes validated seed/default-behavior
facts; native tiers additionally consume ownership, compiler and canonical
Frame/recovery services. Custom behavior need not participate in caching. A
cached dictionary answer must still undergo required target acquisition.

**Proof required:** compare cached/uncached results and effects under method and
parent changes, foreign reentry, collection/address reuse and changed entry
classification. Recover active old implementations correctly. Lower layers must
build and execute with optimizer and native compiler dependencies absent.

### L11. Store/Globals — blocked on observation completeness, not on a new object floor

**Evidence:** [Store](../../rust/birnam-store/src/lib.rs) uses vtable/selector
inventory to backfill its mirror. Tracked mutation preserves retract, dictionary
write, assert and observer ordering; layouts and roots are registered separately.

**Construction:** facts, indexes and Cells use collections and values. I2/I6
supply a complete initial view and supported mutation events. The Store mirror
must not become a second dispatch authority. Custom behavior requires an honest
reflection contract; do not pretend every resolver exposes a dictionary.

**Proof required:** mount over already-existing seed bindings, then mutate and
remove methods with native participation. Check mirror/order/history behavior
and retained roots. Establish which facts custom behavior can meaningfully
expose rather than silently imposing the default representation on it.

### L12. Module admission and compiler — blocked on explicit service wiring and L

**Evidence:** ordinary generated C in
[codegen](../../rust/birnam-codegen/src/lib.rs) references values, globals,
private-vtable/method helpers and portable-code services. Current admission and
compilation are combined in [Codegen runtime](../../rust/birnam-codegen-runtime/src/lib.rs).

**Construction:** a small loaded coordinator wires explicit seed/service handles.
Precompiled admission uses codecs, P loading and L6 ownership; it must not depend
on source analysis. The compiler later consumes PEG/Lang, values, collections,
environment and materialization services. Seed-authored primitives cannot be
compiled using a mode that secretly requires these upper services at runtime.

**Proof required:** load a precompiled upper module with compiler services absent,
then separately compile and load source with the compiler mounted. Audit generated
imports as well as handwritten ones. Stage-0 compilation is permitted; calling
stage-0 runtime implementations from the loaded result is not.

### L13. Image restoration — blocked on descriptions, ownership and identity reconciliation

**Evidence:** [Image](../../rust/birnam-image/src/lib.rs) inspects payload kinds,
Symbols, default parents/owners and canonical value identities. `load_v8` stages
code ownership and bindings, materializes heaps, and replays facts; even mapped
recovery currently invokes Codegen services.

**Construction:** a reader above values/environment, L4, collector attachment,
L6 and admission decodes bytes and builds relocations using Id allocations.
Core identities reconcile with the existing seed through explicit protocols.
Use P for loader/I/O and reacquire native resources. No image can invoke its own
first reader before executable support exists.

**Proof required:** restore shared/cyclic identity, Symbols, private behavior and
code references without a second seed graph; retain source and apply the selected
recovery path. Compiler-free mapped restoration needs a distinct proof after
admission is separated; it is not established by today's mapped path.

## 5. Construction cycles and proposed resolution

| Apparent cycle | Proposed resolution | Evidence still required |
|---|---|---|
| Values/collections need GC; GC needs data structures | Bounded rooted construction; collector owns noncollecting work storage and mounts before ordinary reclamation is needed | No leaked construction heap, no recursive collection, correct root transfer |
| GC needs descriptions; descriptions are allocated objects | Id supplies core roots/edges; higher providers prepare and pin their descriptions before the phase | Description registry roots and safe callback lifetimes |
| Id needs ownership to call; ownership must mount through Id | Process-pin seed/support code first; attach I5 before publishing unloadable code | Reentry-safe transition with no gap in lifetime protection |
| Machine uses Id send; Id can invoke Machine methods | Separate resolution/native invocation from the attached resumable provider | One lookup per semantic resolution, exact acquired target, no hidden driver in seed |
| Store mirrors Id; Id mutations must reach Store | Seed mutation contracts work with zero observers; mount snapshot plus ordered participation later | Complete late-mount view and existing mutation arbitration/order |
| Module manager needs the modules that implement its facilities | Loaded coordinator passes explicit early handles, then hands policy to Module | No global namespace or compiler assumed before mounting |

These are construction phases, not permission to leave unbounded permanent
scratch storage or duplicated language services in stage 1. The seed's own
objects stay in the same storage/identity domain when GC and libraries attach.

## 6. What this audit settles and the next bounded proof

No examined requirement forces a Values, GC, Frame or Machine implementation to
remain inside Id. However, **the sufficiency of a concrete minimal API for all
layers is not proved**: 81F implements a bounded subset of I1/I2 with pinned
storage/code. 81H establishes storage observation and reachability through part
of I3/I4, while reclamation, lifetime integration and L remain open. Their
grouping must not be mistaken for six new mandatory C tables.

The first bounded work, now executed in [81F](81f-isolated-id-seed-proof.md),
establishes **Id behavior association and default lookup/mutation**, using the
private-behavior part of L1 as the first upper-layer witness:

1. Specify the seed's existing-object behavior reassociation operation and the
   default behavior access needed to build private-vtable convenience above it.
2. Specify/verify parent lookup through behavior, the metacircular termination
   case, method removal and mutation visibility. Distinguish necessary primitive
   access from higher-level enumeration/convenience.
3. Build a standalone Rust Id and a separately owned object-model consumer that
   uses only those declared contracts. Include replacement during reentry and a
   custom parent resolver; no GC/Machine/compiler inside the test product.
4. Inspect imports/state ownership to show no fallback to the aggregate Runtime.
   Only then report that first slice as executable through the seed.

81F meets this bounded acceptance with a new isolated Rust implementation. It
does not mechanically extract production Id or provide mutation observers.

[81H](81h-storage-observation-and-tracing.md) now supplies the read-only storage
and tracing witness. Follow with reclamation/index retirement and callable ownership before
claiming the harder layers are extractable. Each L1–L13 proof above is a separate
acceptance obligation. Runtime changes use the [validation workflow](../validation.md);
existing aggregate-runtime tests remain useful regressions but cannot prove an
isolated boundary by themselves. Birnam replacement additionally requires a
compiled primitive-body demonstration with stage-0 runtime libraries unavailable.
