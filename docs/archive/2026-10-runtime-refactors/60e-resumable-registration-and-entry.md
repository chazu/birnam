# 60E — Resumable Registration and the Entry Seam

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

> **Status:** S0 decision record, accepted 2026-08-04; recognition, late-bound-
> microcode, same-execution-entry-scope, binding-lifetime/resolve-and-pin, and
> live-dispatch-timing amendments accepted 2026-08-05; Runtime ownership
> and code-disposition staging accepted 2026-08-06, together with the S1
> Machine-microcode protocol; S2's retained implementation object, physical
> carrier, Design 49 execution-disposition, durable `ResumableManifest`, lazy
> block-vtable recovery, and no-IdApi-growth amendments accepted 2026-08-14.
> The shared private target-resolution amendment was accepted 2026-08-15:
> ordinary `IdApi.send` and Machine Call/TailCall now use one Runtime-owned
> resolve-and-pin authority, while entry thunks remain Method-shaped carriers
> and compatibility fallbacks rather than the primary ordinary-send path.
> The implemented S1 substrate was aligned with that amendment and its
> correctness, structural-fast-path, and replicated performance gates passed
> 2026-08-22. S3 entry-barrier enforcement and Frame-to-Continuation pin
> transfer were implemented and accepted 2026-08-23.
> The 60G/60J lifetime closure completed 2026-08-27: dynamic vtables and
> Symbols are weak identities, formerly method-bearing generations reclaim
> after complete holder quiescence, and fixed-live-set evolution plateaus.
> This record closes the registration seam that
> [60A](60a-stackless-execution-semantics.md) §5 and
> [60C](60c-stepmethod-calling-abi-options.md) §4.2 both depend on and both
> defer. It decides what a vtable holds for a resumable implementation, how a
> send from unmigrated code reaches one, and how the driver recognizes one. It
> does not define compiler lowering, liveness, or source maps; those are defined
> by [60F](60f-resumable-compiler-lowering.md). Exact descriptor, manifest,
> image-v5 stackless, and native-envelope bytes belong to
> [60I](../../design/60i-stackless-artifact-byte-grammar.md).

## 1. The gap this closes

60C freezes a StepMethod as an ordinary `Method` invoked with a Machine receiver
and answering a StepAction. 60A §5 then requires the driver to distinguish a
resumable target and push its Frame rather than synchronously invoking it.
Neither record says what performs registration, where the record lives, or what
an ordinary send does when it reaches a resumable selector.

That omission is not cosmetic. The floor installs methods as bare addresses:
`birnam_install_method` in birnam-codegen-runtime reaches the Store MOP
`addMethod:on:with:`, whose `tracked_add_method` transmutes the carrier and
calls `api.method`. `Vtables::lookup` answers that address and `send_abi` calls
it immediately. `MethodAddress` is a bare non-null pointer with no tag. Nothing
on that path can consult a registration record.

That paragraph describes the pre-amendment S1 implementation, not the accepted
S2 substrate. The 2026-08-15 amendment changes the private `send_abi` decision
point after lookup; it does not change what lookup returns or any public IdApi
field.

So if a StepMethod address were installed in a vtable under its own selector,
every ordinary send to that selector — from unmigrated Rust methods, the REPL,
`modules/*.birnam`, or reflection — would invoke it with a non-Machine receiver
and a nonzero argument count. Per 60C §4.1 the entry answers an AbortAction, and
**the caller receives that AbortAction as its ordinary result value.** A wrong
answer, not a failure. This record removes that possibility by construction.

## 2. S0 decisions

| Area | Decision |
|---|---|
| Vtable contents | A vtable never holds a StepMethod address. It holds an ordinary Method: a synchronous implementation or an **entry thunk**. |
| Private dispatch authority | Ordinary `IdApi.send` and Machine Call/TailCall use one Runtime-owned **resolve-and-pin** operation: ordinary metacircular lookup followed, under uninterrupted lifetime protection, by classification as a pinned ResumableEntry, a protected synchronous Method, or unbound. There is no second lookup, vtable, inheritance walk, or selection policy. |
| Driver path | A Call or TailCall consumes that classified result. A resumable result pushes a frame and calls the StepMethod; a synchronous result is invoked under its protection. **The thunk is not invoked.** |
| Send path | An ordinary `IdApi.send` consumes the same classified result. A resumable result enters or drives the Machine directly and answers the ordinary result; a synchronous result retains the existing direct call. The thunk is not the primary send path. |
| Nested entry | A resumable target selected inside an inherited **same-execution entry scope** pushes onto the scoped Machine above an entry-barrier delimiter rather than allocating a second Machine. The driver establishes this scope around StepMethod and synchronous-Method invocation. Unrelated host/reactor work suppresses it and starts a fresh Machine. |
| Registration record | A Runtime-scoped, process-local **ResumableEntry** owns its unique thunk address, StepMethod address, FrameDescriptor, physical `CodeGeneration` registration handle, declared arity, portable implementation identity, descriptor hash, and optional implementation-anchor handle. The anchor is required by 60F's named-method adapter and absent for a generic block-vtable entry. Resolve-and-pin acquires the separate physical call-code pin transferred to a Frame; publication separately associates the entry with a logical DispatchGeneration. |
| Runtime authority | The active `birnam-id::Runtime` is the sole semantic authority for Machine state, Runtime-local entry recognition, binding retirement, dispatch-read protection, implementation anchors, and physical code pins. The registration shim targets the explicitly entered Runtime; TLS may route the call but is not the authority. |
| Code custody | Codegen/TCC or another process code repository owns physical mappings. It may share immutable code across isolated Runtimes, but disposal waits for every Runtime lease; it cannot unregister or unload code behind a Runtime's execution state. |
| Candidate discard | Logical candidate retirement and physical mapping disposition are separate outcomes. Logical discard may complete while the repository reports `Retained(codeGenerationPinned)`; this is not failure of the logical transition. |
| Recognition | The active Runtime maps each unique live **entry-thunk address** to at most one ResumableEntry. The shared resolver classifies the exact carrier returned by metacircular lookup and acquires the entry plus a physical call-code pin before protection ends. The thunk address remains the Method-compatible tag; no public tag or IdApi field is added. |
| Binding lifetime | Each direct installation owns one binding reference. Aliases own independent references; inheritance does not duplicate the defining reference. An accepted S2 unit owns one manifest reference for each declared entry, including a block entry whose vtable remains unallocated. Lookup results are borrowed, and retaining a raw Method carrier requires an explicit generation lease. Retirement and address reuse wait for manifest and binding references, dispatch readers, acquired entries, Frames, Continuations, and leases to quiesce. |
| Preservation | Migration preserves result and error values for a matching argument count. It preserves neither mismatched-arity behavior (§6) nor maximum call depth (§5.3). |
| Failure surface | A 60A §9 driver failure crossing an ordinary synchronous send or fallback-thunk boundary is fatal, never an ordinary result value. |
| Registration and publication surfaces | The emitted registration operation prepares one Runtime record independently of installation. The Store MOP publishes named bindings, direct `birnam_method` publishes block bindings, and `Tcc unit install:as:on:` publishes relocated symbols; image boot replays the same artifact registration before restoring those bindings. All resumable surfaces associate the published thunk with the same record shape. |
| Durable image tag | One pointer-free `ResumableManifest` enumerates every named and block entry. Image v5 carries the same manifest plus tagged generated-block-vtable references. Boot verifies image, artifact, registry, resolver results, and final bindings before user entry. The Runtime supplies image discovery through a private side catalog, not an IdApi field. |
| Microcode | Machine microcode is an ordinary late-bound Birnam protocol. Its checked primitive implementations may be redefined, removed, or shadowed. A Birnam-authored replacement using public capabilities receives no raw Frame or arena pointer; incompatible changes may alter or invalidate execution, but the driver still validates the final StepAction before applying its requested control transition. Prior valid checked mutations are not transactional or rolled back. Trusted native artifacts retain native authority. |

## 3. One resolver, two continuations

This is the distinction the rest of the record depends on, so it is stated once,
explicitly:

| Caller | Resolution | Cost |
|---|---|---|
| Migrated: a StepMethod issuing a Call or TailCall | The driver consumes the shared resolver result, then pushes a resumable frame directly or invokes the protected synchronous result | One protected resolution plus one frame push for a resumable target. No thunk invocation, native frame, or Machine acquisition on that path. |
| Unmigrated: any ordinary `IdApi.send` | `send_abi` consumes the same resolver result, then enters/drives a resumable target directly or invokes the protected synchronous result | One native `send_abi` activation plus a nested or fresh drive. No entry-thunk activation on the ordinary path. |

Neither primary path passes through a thunk. An implementation that routes a
migrated Call or TailCall through one reintroduces exactly the native-stack
growth 60 exists to remove. Routing ordinary `IdApi.send` through one remains
semantically valid as a compatibility implementation, but duplicates
classification and makes stacklessness an adapter accident rather than a
property of the Runtime substrate; it is not the accepted S2 architecture.

For a named method, the pushed Frame is 60F's retained-implementation adapter.
The entry owns the exact object supplied to `defineMethod:on:as:` and the adapter
performs a second, fresh `value:`-family lookup on that object. That inner call
may push the independently registered block Frame or enter a protected
synchronous Method. “No thunk invocation” does not mean “no second lookup”: the
latter is existing Birnam malleability and remains part of the method contract.

### 3.1 Thunk shape and reduced role

The thunk remains an ordinary Method and introduces no new function shape:

```c
static Oop example_entry(Oop receiver, const Oop *arguments, size_t count) {
    return birnam_enter_resumable(EXAMPLE_ENTRY, receiver, arguments, count);
}
```

`birnam_enter_resumable` is a host-provided runtime symbol in the same family as
`birnam_method` and `birnam_install_method`. It is not an IdApi field, so 60C's
commitment that the public id floor does not grow for stackless execution
holds; 60C §14's evidence item 8 is amended to match (§9.3).

The thunk has two jobs only:

1. it is the unique Method-shaped carrier installed in a vtable and classified
   by the private Runtime resolver; and
2. it preserves correct behavior for an ABI-compatible caller that directly
   invokes a retained Method carrier instead of entering through `IdApi.send`.

It is not the Runtime's normal dispatch adapter. `send_abi` already owns the
semantic send boundary and must classify the carrier before deciding how the
selected implementation executes.

A per-implementation thunk is the obvious emission and is what a compiler will
produce. It is **not** required, but the alternative is not one shared thunk.

A single shared thunk cannot work: the Method ABI conveys no selector, so a
thunk invoked under an arbitrary selector has no way to learn which
implementation it is standing in for, and 60D §4 forbids the ambient state that
would supply one. Recovering the entry from the receiver's vtable is not a way
out either: inherited or custom metacircular lookup need not identify that
receiver vtable as the binding's home.

What preserves the no-code-generator path is a **pool of distinct runtime-owned
thunks**, one bound per registration. Each pooled address is unique, so it
identifies its entry without a selector, and none of them has to be compiled at
registration time. That is what lets a resumable implementation be registered
without running a code generator, which 60C §13.4 preserves for a future
non-native or sandboxed artifact format and which an interpreter tier would
need. Whether the pool is pre-built, grown, or exhausted is not decided here.

The uniqueness lets the shared private resolver recognize the exact address
returned by metacircular lookup without invoking it. Ordinary sends and driver
calls therefore share both lookup and classification; only their continuation
after a resumable classification differs.

### 3.2 Resumable entry behavior

Whether called directly by `send_abi` with an already acquired entry or reached
through a fallback thunk, the common resumable-entry helper performs:

1. consume the already acquired ResumableEntry, or classify the fallback
   thunk's carrier and acquire its live generation pin;
2. consult the Runtime entry-scope stack (§5.2) to decide fresh or nested entry;
3. push the initial frame, binding receiver and arguments into traced frame
   slots under the §6 arity rule;
4. drive until that frame returns;
5. answer the returned value as an ordinary Oop.

A completed drive is indistinguishable from a synchronous return. A caller that
never mentions Machine never sees one.

### 3.3 Driver failures do not become values

Step 5 answers only language values. This matters because 60A §9 classes driver,
ownership, malformed-action, and barrier failures as out-of-band typed runtime
errors, while an ordinary synchronous `IdApi.send` or direct fallback-thunk
call **has no out-of-band channel** — its only carrier is the returned Oop.

Converting those failures into ordinary error values would recreate the §1
hazard in a new place, and the floor would not treat them consistently.
`vector_do` in birnam-collection discards its block's result entirely, so a
driver failure raised inside `do:` would vanish and the loop would continue;
`block_loop` and `integer_to_do` in birnam-prelude check for errors and return
them, aborting their loop. The same runtime fault would end `whileTrue:` and
`to:do:` as an ordinary error value and be invisible in `do:`. Worse, because
this language has no non-local return and errors are ordinary values, `on:do:`
would happily catch a driver failure and continue running against a Machine the
driver has just declared invalid.

**Decision.** A 60A §9 failure that crosses an ordinary synchronous send or
fallback-thunk boundary is a fatal invariant, reported through the same host
path `send_abi` already uses when `doesNotUnderstand:` is unbound. It is not
returned.

A later record wanting a recoverable subset cannot get one by choosing an error
kind. `kindMatches:` in `src/id/stdlib.birnam` answers true whenever the wanted
kind is `#error`, before consulting the receiver's kind at all, so `#error` is
an unconditional wildcard over every `ErrorVt` value and `on:do:` reaches
`kindMatches:` only for values that are already `ErrorVt`. A recoverable subset
would therefore need a representation that is not an `ErrorVt` value, and it
must amend 60A §9, which today says none of its kinds are in-band.

### 3.4 The failure surface is not uniform during migration

§5.1 makes a drive entered from unmigrated code a nested drive, and §3.3 makes
its failures fatal, while a drive the host starts directly reports 60A §6's
`Failed(driver error)` as a value. Until the S4 primitives are migrated, most
drives are nested, so the same runtime fault aborts the process in one context
and answers a host result in another, decided only by whether an unmigrated
frame happens to sit below it.

That non-uniformity is accepted rather than hidden: the alternative is the §1
hazard. It is a reason to prioritise S4, and a reason the S3 host-drive surface
should be exercised on a fully migrated probe rather than on ordinary program
text.

## 4. The ResumableEntry record

    ResumableEntry
      unique entry-thunk address  (what ordinary lookup answers)
      StepMethod address           (what the driver calls)
      FrameDescriptor identity     (60B §4)
      physical CodeGeneration identity and registration handle  (60B §9)
      declared arity
      portable implementation identity  (for FrameView and source maps)
      descriptor SHA-256 and entry kind
      optional implementation-anchor handle
      manifest and direct-binding reference state

The registry belongs to one Runtime, is owner-thread-checked in the first
delivery, and is not a GC root set. Its recognition key is `(Runtime identity,
entry-thunk address)`, so the same mapped code may be registered independently
by future in-process Runtimes without sharing execution state. Entries are also
validated by index plus generation when held as capabilities, following the
Machine registry pattern of 60B §1.

The implementation-anchor handle indexes a separate dense Runtime-owned root
table; it is not an Oop and the entry registry is not interpreted as a root set.
Each stable anchor slot contains the exact implementation Oop for one named
adapter. The collector traces the slots directly and may rewrite them for a
moving collection. A generic block-body entry owns no slot because its captures
belong to ordinary heap block objects and, once invoked, Frame slots.

`Prepare` installs the named implementation Oop in its anchor before the staged
entry can become visible. Failure or `Abort` removes the unpublished anchor and
publishes nothing. Resolve-and-pin reads the anchored Oop under the entry's
existing lifetime protection; successful Frame construction copies it into the
descriptor-declared implementation slot before that protection ends. The
Frame's PC root map then owns the execution root. Aliased bindings share the
entry's one anchor while retaining independent binding references.

An anchor remains live while a manifest or binding reference can make its entry
callable. Retirement first prevents fresh resolution and waits for the existing
reader, acquired-entry, Frame, Continuation, and explicit-lease quiescence; only
full entry teardown releases the anchor. Image recovery recreates and anchors
the implementation Oop before restoring a binding. Anchor indices and slot
addresses are process-local, never serialized, exposed through IdApi, or
projected as mutable Store authority.

The authoritative registry, generation-manifest references, block-vtable image
catalog, and retirement/read-epoch state live with dispatch in
`birnam-id::Runtime`, not in the compiler, image codec, or loader. Codegen's
process state is a Codegen context, not a second Birnam Runtime. The emitted
registration shim uses the explicitly entered Runtime and fails when none is
active. A process code repository may own the physical mapping and aggregate
leases from several isolated Runtimes; the Runtime retains the safety decision
about whether its entries, block vtables, bindings, Frames, Continuations, or
readers can still reach that generation.

The catalog is a private Runtime service passed explicitly to the image writer
and loader. It is not an IdApi query. It maps an instantiated generated block
vtable Oop to its owning artifact authority, canonical manifest index, 60F
§13.4 durable entry key, and current binding fingerprint. Recovery passes the
checked index directly to that artifact's idempotent resolver and verifies the
returned Oop against the catalog; it never searches by the `u64` implementation
identity. The catalog contributes any required vtable Oops to existing Runtime
root enumeration; generated native statics are never accepted as the only GC
root.

### 4.1 Resolve and pin is the private dispatch substrate

No Runtime caller may hold a bare Method address across a lifetime gap. The
single private `resolve-and-pin` authority used by both `send_abi` and the
Machine driver begins before ordinary metacircular lookup and ends only after
the result has become one of:

- a strong ResumableEntry reference plus a call-generation pin;
- a synchronous Method whose code generation remains protected through its
  immediate invocation; or
- an unbound result that follows the existing DNU path.

Conceptually the private result is
`ResolvedTarget::{Resumable(PinnedEntry), Synchronous(ProtectedMethod),
Unbound}`. This name and Rust representation are not ABI, but the three-way
semantic result is frozen. The resolver does not execute the target and does
not decide scheduler or compiler policy.

`send_abi` consumes the result by invoking a protected synchronous Method,
entering/driving a pinned resumable entry, or following the existing DNU path.
The Machine driver consumes the same result by invoking a protected synchronous
Method or pushing/replacing a Frame. A fallback thunk may enter through the same
resumable-entry helper when a retained raw Method carrier is invoked directly,
but no primary Runtime send path performs a second lookup or classification.
No new argument or ambient Machine is added to the Method ABI.

For a resumable result, a successful Frame push transfers the call-generation
pin to the Frame; a failed push drops it. The ResumableEntry retains its own
registration-generation handle until retirement completes. A synchronous
result needs the same lifetime principle because concurrent unload of an
ordinary Method between lookup and invocation would be the same use-after-free
without a ResumableEntry.

Resolution remains independent of how lookup found the address. An inherited
dictionary binding and a custom metacircular lookup that returns the same live
registered thunk both remain resumable. No second parent-chain walk,
defining-vtable reconstruction, or exception to ordinary late binding is
required. A custom lookup result is a borrowed dispatch result: retaining its
raw Method carrier beyond that resolution requires an explicit generation
lease. Native code that retains an unleased carrier already violates the same
code-lifetime contract for synchronous Methods.

Within one Runtime, a live thunk address names at most one ResumableEntry and
each entry owns a unique thunk address. A raw StepMethod address is never
installed or returned by lookup. An address never known as a live or retired
thunk receives the existing synchronous-Method interpretation. A known retired
thunk presented by a reader begun after retirement is a stale carrier failure,
not a synchronous miss; returning a raw StepMethod under the synchronous
interpretation is malformed native behavior, just as calling any Method address
under the wrong semantic contract is today.

The protection covers **lookup through classification and pin acquisition**;
atomic recognition relative to unregistration alone is insufficient. Otherwise
one executor can resolve the old thunk, a mutator can retire and recycle it,
and the executor can then misclassify or invoke the recycled address.

This is a semantic linearization contract, not a commitment to one lock. The
owner-thread S1 Runtime serializes physical access, but metacircular lookup may
re-enter mutation before returning. Its read protection may therefore be a
plain owner-thread epoch/depth token, amortized across the current Machine turn
or top-level send. The fast path must add no mutex, atomic read-modify-write,
heap allocation, or reference-count operation per ordinary resolution.
Registry classification remains a non-allocating lookup, and the minimal
generation handle is acquired only when a resumable Frame is pushed or an
ordinary send acquires an entry for immediate frame construction. The Runtime
must implement the API in this shape rather than scattering bare-address
lookups, so later protection does not require separately rewriting send and the
driver.

Before shared-Runtime parallelism or concurrent code evolution is enabled, 60D
requires a real read-side protocol. An epoch/RCU-style token is preferred: it
may be amortized across a Machine turn or top-level host send, nested resolution
reuses it, and it is not a Rust borrow or lock held across metacircular code.
Immutable snapshots or another reviewed mechanism remain permitted. Mutation
may publish immediately, but reclamation waits for older readers to drain; the
lookup itself is never retried merely because an epoch changed, since custom
lookup may have effects.

### 4.2 Address recycling, in both directions

Code addresses are recyclable: `dlclose` runs in birnam-codegen-runtime's
`discardCandidate:` and `releaseCandidate:`, and a later `dlopen` may map new
code at the same address. Two directions follow, and only the first is a
registry problem.

**False positive.** A stale vtable may retain a thunk address after its registry
entry is removed. If a later generation reused that address and registered a
new entry, the driver would push a frame for the wrong implementation. Ordering
registry removal before `dlclose` is not sufficient: address reuse is forbidden
until the old generation has no registration handle, binding reference,
pre-retirement dispatch reader, acquired entry, Frame, Continuation, or explicit
generation lease that can still produce or invoke the address. The conservative
first delivery therefore refuses unload for every generation that installed a
method (§7).

**False negative, and it is worse.** A live vtable may hold a method address
from an unloaded generation. Block vtables are the case: generated code builds
them inline at first evaluation and installs `value:` through `birnam_method`,
and while the module-local `static` cache dies with the generation, the vtable
Oop and every block instance holding it do not. Store such a block in Globals or
a fact, activate a successor generation, and `releaseCandidate:` the old one —
it is neither active nor prepared, so nothing refuses, and `dlclose` runs. A
later send to that block calls freed code.

This is a pre-existing defect, not one 60E introduces, but 60E changes its
character: the driver checks the registry, correctly misses, concludes
"synchronous Method", and invokes the recycled address through the synchronous
barrier — the one path with no validation at all. A crash becomes a silent
misdispatch. §7 states the gate that closes both directions.

### 4.3 Registration surfaces, and the one that cannot carry a registration

Three surfaces are live installs that can associate an already prepared
registration with a published thunk: the Store MOP for named methods, direct
`birnam_method` for block bodies, and `Tcc unit install:as:on:`, which resolves a
symbol from a relocated unit and calls `ensurePrivateVtable` plus `api.method` —
the path `Alien wrap:signature:as:on:` is built on. The fourth cannot reconstruct
a registration as a side effect of installation, and that is the one that
breaks if the emitted operation is omitted.

**Image boot restores installs without registrations.** The image heap encoder
skips `SLOT_CODE` words outright, so no method address is serialized at all.
Addresses come back only by re-running installs: boot maps the image's compiled
code unit and lets its initializer install, through `birnam_install_method` for
named methods and `birnam_method` for the block vtables rebuilt from source.
Both signatures carry a vtable, a selector, and an address, and neither can
carry a FrameDescriptor, declared arity, generation pin, or implementation
identity. So a thunk address comes back into the vtable and no ResumableEntry
comes back with it.

Under §3.3 that is not a degraded mode, it is an abort: the first send to a
migrated selector classifies a restored thunk, finds no live entry, and raises a
60A §9 failure at the synchronous send boundary. The rebuild path is unaffected, since it
re-evaluates method definitions through the Store MOP and recreates the
Runtime-qualified address mapping, so mapped and rebuilt images would differ in
correctness — two routes to the same image answering differently, which is the
kind of divergence 60 §3's compatibility invariants exist to prevent.

**Decision.** Registration is an emitted artifact operation, not a side effect
of installation. Artifact lowering emits a registration call beside each
install. The entry and its registration-generation handle exist before the
binding becomes visible; successful publication adds that direct binding's
reference, while rejected or failed installation rolls back any unpublished
registration. Image boot re-registers before any send can reach a restored
thunk; and a restored thunk with no live entry answers
`resumableEntryUnrestored` as an **image validity failure** rather than a runtime
fatal on first use.

S1.3 freezes the semantic v1 carrier accepted by that registration operation.
It provides:

- a schema version and byte size;
- the thunk carrier and StepMethod addresses;
- immutable FrameDescriptor identity and contents;
- selector-determined language arity and the descriptor's driver argument
  count;
- the exact canonical manifest entry index assigned after manifest sorting;
- a portable implementation identity;
- a process-local physical CodeGeneration identity; and
- the physical code lease from which Runtime-local registration and Frame pins
  are acquired.

Version 1 rejects a different schema version, a record shorter than its required
fields, an invalid address/descriptor/arity combination, or a generation that
cannot be leased. It does so before the binding becomes visible. A successful
call leaves no borrowed carrier memory in the Runtime: it copies or interns the
immutable descriptor metadata and owns or transfers every required generation
handle before returning. For S2, the immutable descriptor contents include 60F's
entry locations, call-staging description, colored-slot count, per-PC root maps,
per-edge transfer plans, and logical-value source-map identity. The exact C
field layout and blob encoding are owned by 60I: one sized native envelope
carries the artifact-local manifest index, process-local pointers, the optional
implementation Oop, and a physical CodeGeneration lease beside a canonical
pointer-free descriptor-v2 blob. The
Runtime validates and owns every decoded component before publication.
The registration shim remains covered by the runtime ABI hash, so incompatible
physical carriers do not silently interoperate.

The check point has to be stated precisely. Physical mapping first exposes the
static manifest and resolver without invoking the generated Birnam initializer.
The loader copies and validates that manifest and opens a staged-unit context
binding its canonical indices to the physical CodeGeneration; native static
constructors may not register or publish resumable entries. Restored thunks then
come into being *while* the generated initializer runs, and that initializer is
ordinary program text interleaved with installs. Every S2 artifact therefore
registers all entries named semantically by 60F §13.4 and encoded by 60I's
pointer-free `ResumableManifest` during unit initialization, including a
block body whose literal has never been evaluated. This metadata operation does
not allocate its block or vtable. Named installs and block vtables created by
top-level evaluation may consume prepared entries during the transaction; the
remaining exact entries are retained by generation-manifest references after
the initializer returns.

The artifact loader then compares every manifest-indexed prepared and published
entry with the already validated export and atomically commits that exact set to
the successor logical DispatchGeneration. Missing, extra, duplicate, wrong-kind,
wrong-arity, or descriptor-mismatched entries roll back the unit. For image
recovery, image v5 first supplies the independently framed expected manifest.
Before the generated initializer the loader requires byte identity between image
and artifact manifests; afterward it commits the registry set, materializes
heap and fact references to generated block vtables through the artifact's
idempotent resolver, reconciles bindings, and performs one final check before
the image entry point. A digest match alone is not sufficient evidence.

The durable projection is therefore self-describing about every resumable named
adapter and block body. It stores stable authority, implementation, descriptor,
selector/arity, and source-binding identities, never native addresses. The
image codec obtains block-vtable identity from the Runtime's private catalog;
neither IdApi nor malleable Store facts become native-code admission authority.
Any failed final check answers `resumableEntryUnrestored` before a send can
reach the affected thunk.

Two costs, stated plainly. Registration needs its own emitted symbol, since
`birnam_method`'s signature cannot express an entry and widening it would break
every existing artifact — so §9.3's amendment names two symbols rather than one.
And the registration shim joins the image's hashed runtime shim list, changing
the runtime ABI hash, so images written before this lands degrade to the rebuild
path. Design 40 already designs for that degradation; it is a cost to state, not
a defect. S2 also makes a clean image-v5 cut for the `Resumables` record and
tagged block-vtable references. The Codegen shim table is not hashed and does
not carry that cost.

## 5. Entering from unmigrated code

60A §5 requires that a synchronous Method's send may enter resumable methods and
must drive them to completion. Every thunk invocation is that case, and it is
the path every unmigrated caller takes throughout S2 and S4.

### 5.1 Nested entry reuses the current Machine

**Decision.** When `send_abi` selects a resumable target inside a
same-execution entry scope established by an active drive, it does not allocate
a second Machine. It pushes the new frame onto that Machine's arena above an
**entry-barrier delimiter**, and runs a nested drive that may not pop below that
delimiter. A directly invoked fallback thunk follows the same rule. The driver
establishes the scope around both StepMethod and synchronous-Method invocation,
because either native activation may make an ordinary send and must remain live
until that send returns. A resumable entry with no inherited scope starts a
fresh Machine; §5.2 explains why the predicate is logical scope rather than thread
activity. When the nested frames
return, the thunk answers and the intervening synchronous activation returns
normally to the outer driver.

The alternative — one fresh Machine per crossing, recycled through a pool — was
rejected. It allocates a MachineState and arena per boundary crossing on a path
that is already the expensive one; it produces N unlinked Machines for one
logical call stack, which no backtrace or future debugger can walk; and pooling
Machine identity is unsound, because 60B §1 and §3 make registry-slot reuse
advance the capability generation precisely so a stale Machine Oop fails rather
than aliasing a new execution. A pool that hands the same Machine Oop back
defeats that: a FrameView retained from one drive would validate successfully
against the next and read another execution's slots through a stale arena
handle.

Sharing one Machine between stacked drives is only sound under four rules. Each
closes a defect that the naive reading permits, and none is optional.

**Epochs are monotonic and never restored.** The Machine may reuse scratch
StepAction objects (60B §5), and a nested drive rewrites the same scratch set.
If the outer drive's epoch were restored on return, an outer action Oop would
validate again while carrying payload the nested drive wrote — an ABA that
hands the driver a ReturnAction where it expected a CallAction, and a wrong slot
index with it. Every drive entry advances the epoch, so an action issued before
a nested drive is permanently stale afterward.

**The driver never re-reads an action payload across generated code.** It copies
every operand it needs into driver-local scalars before releasing its borrow to
invoke a synchronous Method, per 60D §6.2. Monotonic epochs make a violation
fail loudly; copying makes it not arise.

**Delimiter searches terminate at the innermost entry barrier.** 60A §4 lets
Abort unwind to a delimiter and 60A §6 lets Restore install at a compatible one.
With one Machine, the outer drive's delimiters are in the same chain. Unwinding
or splicing past an entry barrier would pop frames whose native activations are
still live on the C stack, returning into a driver whose current frame no longer
exists. Abort, Restore, and Capture all stop at the barrier and fail
`continuationBarrier`.

**A Return at the barrier frame ends the drive, not the Machine.** 60A §5 says a
Return with no caller completes the Machine, and a thunk-pushed frame has no
caller *frame*. Taken literally that would complete a Machine whose outer drive
is still parked in a native activation, and 60B §3's teardown would release
chunks and generation pins the outer frames still hold. MachineState therefore
carries a drive depth; Running to Completed requires depth zero.

**Arena slot generations advance on pop.** Nesting pushes and pops at the same
arena offsets repeatedly, which makes slot reuse the common case rather than a
rare one, so the generation counter is the only thing distinguishing a stale
FrameView from a live one. It must advance on every pop, not only on chunk
release.

Reusing the current Machine also makes three properties fall out of mechanisms
60B and 60A already define rather than being asserted separately:

- the entry-barrier delimiter is a delimiter record (60B §8), so capture above
  it fails with the existing `continuationBarrier` — no new kind, and 60A §7's
  requirement that the barrier be dynamically scoped and visible to capture
  validation is satisfied by construction;
- suspension requested above an entry barrier fails the same way, which is what
  60A §5 already requires of a drive entered from a synchronous activation;
- one logical execution keeps all its frames in one Machine's arena, so a
  backtrace enumerates that arena — the same mechanism tracing uses below —
  rather than reconstructing a chain across unlinked Machines.

Drive depth is tracked explicitly on MachineState rather than read off the frame
chain, because the caller chain is severed at every entry barrier: a barrier
frame's caller is a native activation, not another frame.

Tracing must follow the arena, not the frame chain. A chain walk from the
current frame terminates at an entry-barrier frame, because that frame's caller
is a native activation rather than another frame — and the outer frames it
stopped short of hold Oops that the native-stack scan does not cover, since they
live in arena chunks rather than on the C stack. 60B §6's trace hook therefore
enumerates every live frame in the arena. The scoped root provider may name one
Machine once per stacked drive, so marking must tolerate duplicates.

### 5.2 The entry-scope stack is the one piece of ambient state

Deciding fresh-versus-nested requires knowing whether the send belongs to a
native activation of the current logical drive, and the thunk holds no Machine
— that is the whole nature of the seam. The Runtime therefore owns an
entry-scope stack. The driver pushes a same-execution scope immediately before
invoking a StepMethod or synchronous Method and pops it when that activation
returns. A resumable microcode replacement reached by the StepMethod therefore
uses the same Machine and the same entry-barrier rules as a resumable send made
by an unmigrated synchronous Method.

The predicate is inherited logical scope, not thread activity, and the
difference is not pedantic. A Machine may be driving when the host or reactor
begins unrelated work—a fresh reactor item, Store reconciliation, codegen
authorization, or an unrelated FFI callback—and that work may select a
resumable carrier through ordinary dispatch. The host entry operation
explicitly suppresses any inherited
same-execution scope. Nesting that work onto the driving Machine would give a
new logical execution no identity a scheduler could queue, no ability to
suspend, a lifetime tied to an unrelated Machine, and a backtrace interleaved
with one. A resumable entry selected with an active drive but no inherited scope
starts a fresh Machine, which is what 60D §3 wants when it lists several
Machines on one owner thread as a direct fit.

60D §4 forbids ambient execution identity, and that prohibition stands where it
was aimed: **generated StepMethod code and Machine microcode never consult the
entry-scope stack.** They receive Machine explicitly as their receiver. The
entry-scope stack is consulted only by the Runtime resumable-entry helper used
by `send_abi` and the fallback thunk, at the one boundary where no Machine is
in hand, and 60D §4 is amended to say so rather than left to be read as
forbidding it.

Pushing and popping the dynamic scope are point operations. The scope record
remains installed while the native activation runs, but no entry-scope-stack,
registry, or MachineState borrow is held across generated code, `IdApi.send`,
allocation, collection, or a foreign callback, per 60D §6.2. The entry-scope
stack must be a distinct borrow domain from the machine registry, or the nested
case self-deadlocks on the second entry.

The Machine has one drive epoch, not one per drive, and validation always
compares against that single current value. It advances on drive entry **and on
drive exit**. Recording a per-drive epoch and comparing against the innermost
active drive would not give the property §5.1 requires: on return the innermost
drive is the outer one again, its recorded value is its pre-nesting epoch, and
an action issued before the nested drive would validate against it while
carrying payload the nested drive overwrote. Advancing on exit is what makes the
staleness permanent.

### 5.3 What migration does and does not preserve

Migration preserves the result value, error values, and selector-determined
arity. **It does not preserve maximum call depth, and the change is not always
favorable.**

For a chain of migrated calls, native depth becomes constant where it was
proportional to Birnam depth. That is the win. For a chain that alternates
migrated and unmigrated frames, native depth stays proportional to the number of
boundary crossings and the per-crossing constant *grows*: driver, barrier,
synchronous method, `send_abi`, `bind_active` with its nested metacircular
`lookup:`, thunk, nested driver. Stated honestly:

> native depth goes from Θ(Birnam depth) to Θ(boundary crossings) × a larger
> per-crossing constant.

That is a win only where migrated regions are long runs. During S2–S4 it is not:
while `do:`, `whileTrue:`, `to:do:`, `inject:into:`, and `on:do:` remain
synchronous, every recursion passing through one of them crosses a boundary per
level, so a deep recursive tree walk can reach the native stack limit *sooner*
after migration than before.

Four consequences. First, no artificial depth bound is introduced: adding one
would convert a program that answers today into one that answers an error, and
the OS stack limit remains the bound it is today. Second, this reorders the
argument for S4: the callback-driving primitives are not a convenience
migration, they are what converts the alternating case into the long-run case.
S2 must measure and publish both axes — per-iteration entry cost in a loop, and
maximum reachable depth for a recursion through an unmigrated primitive —
because the second is the one that regresses.

Third, S1.2 publishes owner-thread diagnostics for this transitional risk. Each
drive records current and high-water synchronous-boundary depth, incrementing
before the driver enters a synchronous Method and decrementing after it returns.
The counters are raw MachineState fields: updating them allocates nothing and
uses no lock, atomic, or reference count. They are diagnostics, not a new depth
limit or a recoverable stack-overflow promise, and their cost is included in the
S2 boundary measurements.

Fourth, and least comfortable: **suspension is largely unreachable until S4.**
Capture and suspension fail above an entry barrier (§5.1), and boundary
crossings sit near the top of a program's dynamic extent — `main` reaches `do:`
reaches everything — so during S2–S4 the barrier sits below essentially all real
work. `collection do: [:each | ... ]` cannot suspend at any depth until `do:` is
migrated. S3 can therefore be demonstrated only on a fully migrated probe, and
the reactor rationale of 60D §4 is not supported by ordinary program text until
S4 lands. That is an argument for treating S3's acceptance as probe-scoped, not
for weakening the barrier: the barrier is what a live native activation
physically requires.

## 6. Arity

The floor has no per-method arity. `add_method` stores selector and address
only; `doesNotUnderstand` fires exactly when lookup misses; `send_abi`'s only
argument check is the null-with-nonzero-count invariant. Arity is carried by the
selector spelling, so an ordinary compiled send cannot mismatch. Where a
mismatch does arrive — a native caller, or DNU forwarding — a codegen'd method
today ignores the count and a Rust method answers nil.

Two different checks are easy to conflate here, and they answer differently.

**Language arity**, at frame push, is shared by the driver path and the entry
path and is variadic-tolerant: bind `min(count, arity)` slots and nil-fill the
remainder. It does not fabricate a `doesNotUnderstand` result.

**Native buffer arity**, at StepMethod invocation, is strict: a count the
FrameDescriptor does not declare is a malformed artifact and is rejected, per
60C §4.1. That buffer is produced by the driver, not by a language caller, so
tolerance there would only hide a broken artifact.

Language-arity tolerance is a behavior change, and §2's preservation claim is
correspondingly narrow. A synchronous Rust method today typically answers nil
*without running* on a count mismatch — `vector_do` returns nil when its count
is not 1 — whereas a migrated version runs its whole body with nil-filled
arguments. Ordinary compiled sends cannot reach this, since the selector fixes
the count, but native callers, reflective invocation, and DNU forwarding can.

`doesNotUnderstand:` itself is not migratable in S1–S4. `send_abi` forwards an
unbound send to it with `1 + min(n, 16)` arguments, so a resumable
`doesNotUnderstand:` declaring arity 1 would mismatch on every keyword miss;
answering DNU on mismatch would then resolve to the same implementation and
recurse. The floor already fixed this shape of bug once — a `count != 1` guard
in `object_dnu` that made every keyword and binary send answer nil instead of
erroring, corrected in "fix(error): raise doesNotUnderstand at every send
arity" — and the entry seam must not reintroduce it.

## 7. The unload gate

60B §9 forbids disposing code while a Frame or Continuation pins its generation.
The current paths do not implement that, and today's refusal is not observable
as a result: `discardCandidate:` and `releaseCandidate:` refuse only while a
candidate is active or prepared, call `note_error`, then return the unit itself.

S1.4 must introduce a structured physical-disposition outcome before
registration or disposal is exposed to ordinary artifacts. S1.3 may exercise a
test-only registration while physical unloading is unconditionally disabled.
The code repository reports either `Disposed` or `Retained(reason)`, with
`codeGenerationPinned` as the conservative-retention reason defined here. This
process-local outcome is carried by the Codegen discard receipt, not by
expanding IdApi. The disposal condition is wider than 60B §9 states, because of
§4.2's second direction:

> Disposal is refused unless the generation has zero registration handles,
> manifest references, direct binding references, pre-retirement dispatch
> readers, acquired ResumableEntries, live frames, live continuations, and
> explicit generation leases, and no reachable vtable holds a method address in
> its mapped range.

The third clause is the honest one and it is not currently satisfiable, which
this record states rather than papers over. Vtables are allocated permanent and
pushed onto an ordered registry that has no removal path, so enumerating with
`vtableCount` / `vtableAt` / `vtableSelectorAt` — and resolving each selector
through the floor's internal lookup, since the enumeration answers selectors
rather than addresses — reports *registry membership*, not reachability. Every
generation evaluates blocks, every block evaluation installs into a vtable that
the registry then holds forever, so a membership test answers "held" for every
generation and disposal never runs again.

The consequence is that until the vtable registry can distinguish reachable from
merely registered, the gate degenerates to refusing disposal outright. That is
the safe direction — leaked mapped code rather than misdispatch into freed code
— but it is a leak per evolution cycle, and it means §4.2's false-negative
direction is *contained, not closed*. Closing it needs one of: a collectable
vtable registry, re-homing block vtables onto a surviving trampoline at unload,
or an explicit unload-time sweep that rebinds affected selectors. That work
was assigned to the lifetime record (60G), and the recognition path of §4.1 did
not depend on it. The selected collectable-registry implementation has since
closed that gate.

> **60J decision, 2026-08-24:** the collectable-registry option is selected.
> Post-bootstrap vtables are collectable; the registry and block catalog are
> weak indexes; live object headers and retained Store facts are semantic roots.
> `vtableCount`/`vtableAt` enumerate the current live set in survivor creation
> order, not all identities ever created. This J1 prerequisite is now used by
> 60G code reclamation and 60J bounded-process compaction.

**Staging.** The gate ships with recognition in S1. Since reachable vtables are
not yet computable, S1 retains every generation that installed a method; a
generation that installed none may still answer `Disposed` once its ordinary
mapping leases drain. The conservative rule is stated in terms of reachability
so that making the registry collectable later narrows it rather than rewriting
it. S3 adds Continuations to the holder accounting. Implemented 60G may answer
`Disposed` for a formerly method-bearing generation only after it closes the
vtable-reachability problem and every Runtime has released its leases.

Logical discard is a separate transition. A retained mapping must not suppress
removal of the author file and `.candidate` sidecar or assertion of the
`state discarded` fact, which are Store state Design 49's reconciliation reads.
The discard operation returns one process-local `DiscardReceipt` identifying
the generation and recording `logical = Discarded` plus
`physical = Disposed | Retained(reason)`. A successful logical transition never
returns only the physical outcome, and a retained physical mapping never turns
the logical field into failure. Expected retention is not a Machine driver
failure and must not make a self-hosted compiler retry a logical discard that
already completed. The receipt belongs to the Codegen protocol and does not
expand IdApi or become durable image state.

S1 also exposes retained-generation count and retained mapped bytes. These are
required telemetry, not proof of pin correctness. Conservative retention is a
known leak per evolution cycle at the S1 checkpoint. Implemented 60G now closes
reclamation, and 60J enforces and proves the bounded-process equation.

**Evidence.** Because the refusal is unconditional in practice, disposal
behavior cannot serve as evidence that generation pinning works — a stubbed pin
implementation would produce an identical observation. S1 must observe the
minimal registration and Frame pin across resolve, push, failed push, pop, and
binding retirement. S3 adds direct evidence across capture and consume. These
are 60 §7 proof points 8 and 11 respectively.

For a future multithreaded Runtime, retirement and disposal are ordered against
§4.1 protected resolution. Mutation first detaches or replaces the binding and
retires its reference; reclamation then waits for read-side epochs, manifest
and binding references, entry and Frame pins, Continuations, and explicit
leases. The choice of snapshot, reference count, epoch, or RCU is not frozen,
but an address may not return to the recognition map for a new generation
before the old generation reaches that quiescent state. A process mapping
shared by several Runtimes needs process-shared generation lifetime accounting
even though each Runtime keeps an independent recognition map.

### 7.1 An active frame does not consult the registry

A frame captures its StepMethod address, FrameDescriptor identity, and
generation pin when it is pushed. The registry is consulted at Call and TailCall
resolution and at entry — never per step for a frame that already exists.

Without that rule, binding retirement would reach into live execution.
`Reflect defineMethod:` for a selector whose implementation is on the frame
stack would retire its binding, and a driver that re-resolved on the next step
would fail. Design 49's quiescence guard covers `CompiledUnit
activate`, not reflective redefinition, so the case is reachable. It would also
contradict 60A §8, 60B §9, and 60D §11, which all say an active frame continues
through its pinned implementation generation while new sends resolve late.

60C §8 step 2 is amended to match: resolve-and-pin is used at Call or TailCall
frame push, while a running Frame invokes its captured StepMethod without
consulting dispatch again.

### 7.2 Manifest and bindings retain entries; one overwrite does not unregister an alias

Nothing about generation unload covers a method being replaced, but removing
one binding cannot unregister a thunk that another selector, vtable, or explicit
lease can still produce. A unique thunk address identifies one implementation;
it does not imply that the implementation has only one binding.

Each direct installation therefore owns one binding reference to its
ResumableEntry. Installing the same thunk under another selector or vtable owns
another reference. Inheritance reuses the defining binding and adds no
reference. The authoritative `add_method` and `remove_method` mutation paths
already receive the previous Method address, so they increment the new direct
binding and retire only the replaced or removed direct binding. No defining-
vtable reconstruction, selector sealing, or change to lookup order is required.

An accepted S2 unit additionally owns one generation-manifest reference for
   every entry declared by its exact 60I manifest satisfying 60F §13.4. That
   reference keeps a
never-instantiated block body registered without forcing its private vtable to
exist. It also permits a still-pinned old Frame to evaluate one of its block
literals after the entry's original named binding has been replaced. The
manifest reference is released only when that mapped generation can no longer
execute; it is not a permanent Runtime-global root and does not substitute for
the wider disposal gate.

A custom lookup result is borrowed for the protected resolution of §4.1. A
custom lookup or native cache that must retain the carrier independently of a
direct binding must hold an explicit generation lease. Without that lease, an
address returned after its final binding has retired is stale native behavior,
not an implicit permanent root.

The implemented candidate repository applies the same rule to rollback. Before
activation can replace a resumable predecessor binding, it acquires one explicit
lease for each distinct predecessor thunk through a process-local selected-floor
seam outside IdApi. A successful rollback restores the binding before releasing
the lease. Conservative physical retention keeps an unconsumed rollback lease
with its retained mapping until 60G's complete-reclamation gate passes.

When the last binding and manifest reference retires, the entry moves to a
retirement set; it is not immediately erased. Readers that began before
retirement may still convert the address into a pin. Reclamation and
unregistration occur only after those readers, acquired entries, Frames,
Continuations, and explicit leases have drained. A Runtime may then release the
registration-generation handle, and the mapping-generation owner may unload or
recycle the address only after the wider §7 gate also passes.

S1 has owner-thread serialization and an unload refusal for method-bearing
generations, so its binding counts and retirement set use ordinary mutable
state. It must nevertheless implement correct alias counts and the same state
transitions now. Turning every removed binding into an immortal entry would
hide the bug behind an unbounded code-generation leak; immediately erasing the
entry would preserve the alias and lookup race. Neither shortcut is accepted.

## 8. Keeping the microcode late-bound

60C §15 Q4 asks how primitive-backed Machine methods are identified so an
arbitrary resumable method cannot displace one. The accepted answer is that
they are not privileged bindings: Machine microcode is an ordinary Birnam
protocol and uses the same late-bound lookup as every other send.

The initial implementations are synchronous Rust primitives because private
Frame storage needs a small regress breaker. Each canonical primitive validates
the Machine capability, owner, lifecycle, drive epoch, descriptor, slot bounds,
and slot kind before touching that storage. No Birnam-authored replacement using
the public protocol receives a Frame pointer, arena pointer, or unchecked
storage capability merely by occupying the same selector. The outer driver
independently validates the canonical StepAction returned by the StepMethod
before changing Machine state.

These checks are local, not transactional. A Birnam-authored replacement may
perform a valid mutation through a checked primitive and then return a malformed
StepAction. The malformed action prevents the requested driver transition and
moves the Machine to Failed; it does not roll back that earlier valid mutation.
Transactional Frame journaling or copy-on-write is not part of S1.

Add, overwrite, removal, inherited replacement, a private-child shadow, and a
custom metacircular lookup all retain their ordinary meanings. Their effects
are visible at the next resolve-and-pin, including one issued by a Frame that
was already active across the mutation's turn boundary. The Frame's currently
captured StepMethod and descriptor are not rewritten. Under later concurrent
mutation, resolve-and-pin linearizes the send as observing either the old or new
binding. An incompatible change may alter the program, return an unexpected Oop, fall
through `doesNotUnderstand:`, or ultimately cause a typed driver failure. The
runtime promises storage safety under such a change; it does not promise that
an active computation remains semantically correct after its execution
protocol is incompatibly redefined.

This is the same malleability boundary as replacing any foundational protocol.
Native artifacts remain trusted native code and can already violate process
memory safety independently of method lookup; sealing selectors would not form
a security boundary. A future versioned or generation-pinned microcode protocol
may improve compatibility diagnostics, but S1 adds neither a mutation veto nor
an exception to ordinary late binding.

## 9. Amendments incorporated into 60C

These amendments were incorporated when 60C was accepted. They remain recorded
here because the entry seam supplies their rationale.

### 9.1 The driver argument count

60C freezes "the initial driver contract uses zero arguments" while its own §15
Q1 asks whether that is right. Zero is the wrong thing to freeze. The buffer
already exists in the frozen Method ABI and costs nothing, while recovering the
program counter through a microcode send costs a full dispatch on every step —
60C's own §7 example spends its first send doing exactly that.

**Amendment.** The driver invokes a StepMethod with a descriptor-declared
argument buffer of length 0 or 1. When the descriptor declares one, `args[0]` is
the resume token: a SmallInt program counter for the frame being entered.

A zero-argument descriptor is limited to a single-entry StepMethod that does not
inspect its current PC; every multi-PC StepMethod declares the resume token.

This is not the rejected Option A of 60C §13.1. That option made `args` a
control array carrying an ephemeral lease — an object-shaped identity with no
Birnam protocol, borrowed from native state. A SmallInt program counter is an
ordinary Birnam value with an ordinary protocol, declared by an immutable
descriptor and validated like any other operand. The objection in §13.1 was to
smuggling native control identity through the argument buffer, not to the buffer
carrying a value.

### 9.2 Late-bound microcode

60C §15 Q4 is answered by §8: Machine microcode remains an ordinary late-bound
protocol. Its checked primitive implementations own the initial private-storage
mechanism, while replacement remains normal Birnam behavior and the driver
retains the final StepAction validation boundary.

### 9.3 Evidence item 8

60C §14's evidence list requires "generated C that uses no stackless-specific
native symbols". Generated C needs two: the entry shim a thunk calls, and the
registration shim of §4.3, since `birnam_method`'s signature cannot express a
ResumableEntry and widening it would break every existing artifact.

**Amendment.** Item 8 becomes "generated C that uses no stackless-specific IdApi
fields, and no stackless-specific native symbols other than the entry and
registration shims". The commitment worth keeping is that the independently
useful id floor does not grow; host runtime shims in the existing `birnam_*`
family are the same category as `birnam_method`, which generated code already
calls.

## 10. Tail calls across the seam

Neither 60A nor 60C says what a TailCall does when its target is not resumable,
and the answer is not benign.

A synchronous Method has no FrameDescriptor, so 60A §5's "reuses the current
frame storage when the new descriptor fits" has no meaning for it. In practice
it degenerates to Call-then-Return: the driver enters the barrier, runs the
Method to completion, writes the result, and pops. The current frame is retained
across the callee, which is not a tail call.

**Decision.** Proper tail behavior exists only for a TailCall whose resolved
implementation names a live ResumableEntry. A TailCall resolving to a
synchronous Method degrades to Call-plus-Return, and the drive counts it. 60A §5
is amended to scope its no-retention rule to a resumable target, since it cannot
hold for one that has no FrameDescriptor.

The counter is not decoration. 60 §7 proof point 2 and 60C §14 item 1 both state
their million-transition requirement unconditionally, but a mutually tail-
recursive cycle with one unmigrated member — or one that routes through
`ifTrue:ifFalse:`, which codegen emits as an ordinary send served by a Rust
primitive that sends `value` to the chosen block — grows the native stack per
iteration and overflows long before a million. Nothing else in the design
detects a partially migrated tail cycle.

The counter is per drive, not per Machine: §5.1 puts nested drives on one
Machine, so a per-Machine counter would let an unrelated crossing's degraded
tail call fail another drive's proof. The S1 probe publishes it and the S2 tail
proof asserts it is zero for the drive under test.

## 11. Cost and the budget 60C leaves unset

60C §14 defers a fast tier until measurements "fail an agreed budget", but no
budget is written anywhere, which makes the gate unfalsifiable. This record sets
a provisional one — and first disqualifies the obvious anchor.

**`sends.unary` is not the send constant.** `bench/micro.birnam` installs its
probe methods through `Reflect defineMethod:`, which installs a *trampoline*
rather than the method body: the emitted function does a `birnam_global` lookup
for the retained block — an intern through a linear symbol-table scan plus a
Globals read served by a reverse scan of the fact log — then interns `value:`
and performs a *second* send into the block. Every measured op is therefore two
sends plus the symbol and Store work, which is the reflective-defineMethod
idiom, not dispatch. On the Rust floor the case reports ~1400–1443 ns/op while
`blocks.value` reports ~91–96 ns/op.

The gap is not wholly attributable to the trampoline — an ordinary compiled send
also resolves free names through `birnam_global`, so `blocks.value` pays some of
the same cost and does not isolate it — but the doubled send and the extra
symbol/Store work are verified in the emitted trampoline, and that is enough to
disqualify the case as a dispatch anchor.

[46](../../design/46-benchmarks.md) calls `sends.unary` "the send constant itself" and pins a
release acceptance anchor to it. That reading is not disturbed for 46's purpose,
which is regression detection on a stable idiom; what this record rejects is
using it as the *denominator of a ratio budget* for an operation it does not
measure. A budget so denominated would be roughly fifteenfold too loose on the
Rust floor.

**Before S0 acceptance no existing case measured what the budget needs, but a
Birnam surface produces it.** The two definition paths that install trampolines
are `Reflect defineMethod:on:as:`, whose trampoline does the global lookup and
second send described above, and artifact/image lowering, whose trampoline
reads an indexed candidate root and then sends. Neither is what `S` should
measure.

`Tcc unit install:as:on:` is: it resolves a symbol from a relocated unit and
calls `ensurePrivateVtable` plus `api.method` directly, with no trampoline, no
Store MOP, and no second send. `Alien wrap:signature:as:on:` is built on it, and
`bench/micro.birnam`'s `ffi.wrapped-call` already times an ordinary unary send
to such a body — it differs from `S` only by the syscall inside it.

S0 adds `sends.compiled-unary`, a micro case timing an ordinary unary
send to a body installed through `Tcc unit install:as:on:` with a trivial
return. That keeps `S` inside the same harness as the anchors below, which a
one-off Rust benchmark would not.
`bench/micro.birnam` requires new cases to be appended at the end, because case
order is load-bearing for its store-backed cases, and its exit gate asserts a
fixed case count that the new case must bump. Call that constant `S`. Budgets
are multiples of it:

| Measure | Budget |
|---|---|
| Continue transition | ≤ 2 × S |
| Resolve-and-pin plus synchronous leaf invocation | ≤ 2 × S |
| Call/Return pair, migrated to migrated | ≤ 4 × S |
| Ordinary `IdApi.send` entry into a resumable target, with zero thunk activations | ≤ 6 × S |
| Direct fallback-thunk entry | reported separately; correctness-gated, no S2 fast-path budget |
| Microcode sends per source-level step | ≤ 4, reported per step kind |
| Maximum reachable depth through an unmigrated primitive | ≥ 0.5 × the pre-migration depth, reported with the measured native-stack capacity |

The S1 owner-thread resolve-and-pin path has structural performance gates in
addition to the ratio above: zero heap allocations, mutex acquisitions, atomic
read-modify-write operations, or reference-count operations per ordinary
resolution. Its read protection is a plain owner-thread epoch/depth token,
amortized over a Machine turn or top-level send; registry classification is a
non-allocating lookup. The benchmark must exercise the actual driver path to a
synchronous leaf, not a helper that bypasses metacircular lookup or
classification. A resumable hit separately reports registry probes and
generation-pin operations per Frame push.

The 2026-08-15 shared-resolution amendment changes every ordinary send by
adding private carrier classification. Its substrate-alignment gate therefore
runs a tracked, replicated Design 46 A/B against the pre-amendment S1 code.
`sends.compiled-unary` must remain within the existing 10% micro-regression
threshold, and both absolute values must be published. Later transition ratios
use the post-amendment value of `S`; publishing both prevents a slower send
denominator from disguising stackless overhead.

Before concurrent code evolution is enabled, the concurrency follow-up adds
uncontended and contended read-epoch measurements: cost per Machine turn and
top-level host send, nested-token reuse, throughput with dispatch mutation, and
retirement-queue drain latency. It must prefer an amortized read token over a
mutex, global atomic increment, heap allocation, or `Arc` clone on every send.
Any implementation choosing a different mechanism must publish same-run
evidence that it meets or improves that shape.

The S0 acceptance run used an isolated ReleaseSafe Rust prefix and quick harness
mode and measured `S = 377 ns/op` on the 2026-08-04 development machine. That
number is scale evidence, not a durable release baseline: each S1/S2 budget is
evaluated against `sends.compiled-unary` from the same benchmark run so host and
layout noise divide out. Fixed absolute ceilings are deliberately absent.

The S1 acceptance run on 2026-08-06 used a fresh isolated Rust release prefix,
quick micro-harness mode, and the release-mode ignored test
`runtime::tests::publish_stackless_s1_transition_measurements`. In that same
run, `sends.compiled-unary` measured `S = 419 ns/op`; the hand-written driver
measured Continue at `64.805 ns/transition` (`0.155 x S`) and actual
metacircular resolve/classify plus synchronous leaf invocation at
`333.068 ns/invocation` (`0.795 x S`). The Continue, Call, and Return probe
steps each issued exactly one Machine-microcode send. These are development-
machine acceptance numbers rather than a durable release baseline, but both
timed ratios pass the provisional `2 x S` budgets. The ordinary resolution
path is separately guarded by
`resolve_and_classify_fast_path_has_no_allocation_lock_atomic_or_refcount_surface`,
which source-checks dispatch-read entry, registry classification, and
resolve-and-pin for allocation, lock, atomic-RMW, and reference-count surfaces.
The measurement probe remains ignored in normal test runs and requires
`BIRNAM_S_NS` from the same micro-suite process before it will publish a ratio.

The shared-resolution alignment run on 2026-08-22 compared three fresh quick
ReleaseSafe micro processes for pre-amendment `c773115` and the aligned
worktree. `sends.compiled-unary` samples were 421/437/425 ns/op before and
428/430/425 ns/op after, for medians of 425 and 428 ns/op: a 0.7% increase,
inside the 10% gate. With aligned `S = 428 ns/op`, the ignored release probe
measured Continue at 69.126 ns/transition (`0.162 x S`) and synchronous
resolve/classify plus leaf invocation at 332.940 ns/invocation (`0.778 x S`).
All 83 active `birnam-id` tests passed; ordinary and Machine registered entry
assert zero thunk activations, while explicit raw-Method entry asserts one.

The S2 entry/call acceptance run on 2026-08-23 used a fresh isolated Rust
release prefix and the ignored release probe
`runtime::tests::publish_stackless_s2_entry_and_call_measurements`. In the same
run, quick `sends.compiled-unary` measured `S = 431 ns/op`. A registered
migrated-to-migrated Call/Return loop measured 504.298 ns/pair (`1.170 x S`,
inside the `4 x S` budget), while ordinary `IdApi.send` entry into the same
registered target measured 563.819 ns/invocation (`1.308 x S`, inside the `6 x
S` budget) with zero thunk activations. Direct fallback-thunk entry measured
519.054 ns/invocation and remains report-only. The drive recorded two live
Frames at high water, zero synchronous-boundary depth, 528 bytes of observed
native-stack-address movement across 200,000 pairs, a 1,087-byte canonical Call
descriptor fixture, and 268 bytes of Runtime-owned Call descriptor storage.
Each Call, child Return, Continue, and final parent Return issued one microcode
send. These are development-machine ratios, not fixed release ceilings; reruns
must take `BIRNAM_S_NS` from their own same-run micro process.

The companion fixed-stack depth probe deliberately remains a failing ignored
gate. On 2026-08-23, three consecutive Release runs with a 2 MiB thread stack
each reached a synchronous pre-migration baseline depth of 3,557 and a mixed
resumable/synchronous-barrier depth of 1,277: `0.359 x` baseline, below the
provisional `0.5 x` budget. The comparator recursively enters the direct
synchronous Method path for the baseline; the mixed path enters a registered
resumable method, calls one unmigrated synchronous Method, and re-enters the
registered method above the required barrier. This is the boundary §5.3 and
this section anticipated: it triggers S4 migration of callback-driving control
primitives, not 60C's optimizing direct-call tier. The retained
`runtime::tests::publish_stackless_s2_depth_measurement` prints both capacities
and fails explicitly while the ratio remains below budget; normal suites keep
it ignored because stack exhaustion is isolated through child processes.

Two anchors are recorded so a reader can sanity-check scale, both from the
Rust-floor snapshot `bench/results/v0.11.0-rust-2fe44d2-…json` that
[61A](../design/61a-r6-performance-tradeoffs.md) accepts as evidence, run 1 of 3:
`blocks.value` 96 ns/op and `loops.to-do` 214 ns/op. Neither is `S`.
`bench/results/v0.11.0.json` must not be used: it predates the Rust floor.

The last row is the axis §5.3 says regresses, and it is stated as a budget
rather than a reporting requirement because a measurement nobody can fail is
what this section exists to correct. Its threshold is provisional in a stronger
sense than the others: §5.3's own accounting—driver, barrier, synchronous
method, `send_abi`, `bind_active`, and nested driver replacing roughly four
activations—puts the design near the line before anything is measured, and the
0.5 figure is a placeholder to be set once `S` and the native-stack capacity are
both known. It belongs to S2, with the entry measure.

Its remedy is also different, and conflating the two would waste a stage.
Exceeding the first four budgets is the trigger 60C §13.3 reserves for an
optional fast tier. Exceeding the depth budget is **not**: §13.3 is the
borrowed-execution-window tier, which reduces microcode dispatch cost and
removes exactly zero native activations per crossing. The remedy for a depth
regression is S4 sequencing — migrating the callback-driving primitives so the
alternating case becomes the long-run case — as §5.3 says. The numbers are
provisional and may be
amended by an explicit record; they may not be quietly retired because a
measurement missed them.

## 12. Failure taxonomy additions

Extending 60A §9. None is an ordinary value:

- `resumableEntryUnregistered` — a carrier classified or directly invoked as a
  resumable thunk has no live or retiring entry. A registry miss is not a
  failure only when the address was never known as a thunk; that is how
  synchronous Methods are reached (60A §5 step 4);
- `resumableEntryRetired` — a known retired thunk was returned by a reader begun
  after its retirement epoch, usually an unleased native or custom-lookup
  cache. It is never reinterpreted as a synchronous Method;
- `resumableEntryStaleGeneration` — the entry's generation was released;
- `resumableEntryUnrestored` — an image validity failure: a restored thunk has
  no live entry after boot re-registration (§4.3). Unlike the others this one is
  raised during image validation rather than at a synchronous entry boundary.

Capture, suspension, Abort, and Restore that would cross an entry barrier all
use the existing `continuationBarrier`. No depth failure kind is introduced;
§5.3 explains why.

`codeGenerationPinned` is instead the reason carried by the §7
`Retained(reason)` physical-disposition outcome. A low-level operation that
specifically requires immediate unmapping may treat it as a typed refusal, but
a completed logical candidate discard does not become a runtime failure.

## 13. What this record freezes

Frozen:

- a vtable never holds a StepMethod address;
- ordinary `IdApi.send` and Machine Call/TailCall share one private
  Runtime-owned resolve-and-pin authority over the exact metacircular lookup
  result;
- either caller resolving to a live ResumableEntry enters or pushes a frame
  without invoking a thunk; the thunk remains the installed Method carrier and
  direct-invocation compatibility fallback;
- registration produces one ResumableEntry regardless of surface;
- one `birnam-id::Runtime` owns semantic execution state and Runtime-local entry
  registration; compiler and loader state never own Machines or recognition;
- an emitted registration operation targets the explicitly entered Runtime and
  is artifact ABI rather than an IdApi field;
- the S1.3 semantic registration carrier is versioned and sized, provides the
  thunk and StepMethod addresses, immutable descriptor, arities,
  portable implementation identity, process-local physical CodeGeneration
  identity and lease, and optional implementation anchor input; it rejects an
  invalid record before publication and leaves no borrowed carrier memory in
  the Runtime;
- S2 artifacts register every canonical manifest entry during unit
  initialization; acceptance atomically commits an exact generation-manifest
  reference set, including entries for block vtables that remain lazily
  unallocated;
- image v5 carries one self-describing `Resumables` record and tagged generated
  block-vtable references, and mapped boot compares image, artifact, registry,
  resolver results, and final bindings before user entry;
- the Runtime's block-vtable image catalog and artifact resolver are private
  side services: no durable native address, malleable Store fact, IdApi field,
  object/vtable slot, exported generated name, or changed lookup rule carries
  resumable identity;
- a process code repository may own and share immutable mappings, but disposal
  waits for every isolated Runtime's leases and Runtime-local execution pins;
- logical candidate discard is independent of physical mapping disposition;
  S1.4 answers a generation-qualified `DiscardReceipt` containing
  `logical = Discarded` and `physical = Disposed | Retained(reason)`,
  conservatively retains every method-bearing generation, and exposes retained-
  generation count and bytes;
- S3 adds Continuations to the S1 holder accounting, while implemented 60G
  owns actual reclamation of formerly method-bearing mappings;
- lookup through classification and pin acquisition is one logical protected
  `resolve-and-pin` operation; it returns a pinned ResumableEntry, a lifetime-
  protected synchronous Method, or unbound, never a bare address whose code may
  retire before use;
- recognition is independent of inherited or custom metacircular lookup, and a
  registry miss retains the synchronous-Method interpretation;
- each accepted S2 manifest and each direct binding own distinct references;
  aliases own independent binding references, inheritance does not duplicate
  one, and retaining a borrowed custom-lookup result requires an explicit
  generation lease;
- retirement detaches only the affected binding and delays unregistration and
  reclamation until older manifest and binding references, dispatch readers,
  acquired entries, Frames, Continuations, and explicit leases are quiescent;
- a thunk address cannot be reused until that complete quiescence condition
  holds across every Runtime sharing the mapped generation;
- nested entry reuses the scoped Machine above an entry-barrier delimiter, and
  is chosen by inherited same-execution entry scope rather than by thread
  activity; the driver installs that scope around StepMethod and synchronous-
  Method invocation, while unrelated host/reactor work suppresses it;
- a Machine has one drive epoch, advancing on drive entry and exit, and
  validation always compares against that single current value;
- Abort, Restore, and Capture terminate at the innermost entry barrier;
- a Return at a barrier frame ends the drive, not the Machine;
- frame tracing enumerates the arena rather than walking the caller chain;
- arena slot generations advance on pop, not only on chunk release;
- S1.1 initializes every live Oop slot before collection, excludes retained free
  capacity from tracing, retains free chunks until Machine destruction, and
  publishes live-frame and reserved-byte diagnostics;
- S1.2 publishes per-drive current and high-water synchronous-boundary depth as
  allocation-free owner-thread diagnostics and introduces no artificial depth
  limit;
- registration is an emitted artifact operation, and image boot re-registers
  before any send can classify or directly invoke a restored thunk;
- a driver failure crossing an ordinary synchronous send or fallback-thunk
  boundary is fatal, not a value;
- migration preserves result and error values for a matching argument count, and
  preserves neither mismatched-arity behavior nor maximum call depth;
- proper tail behavior requires a resumable target;
- Machine microcode remains ordinarily late-bound and unsealed. A Birnam-
  authored replacement using public capabilities receives no raw Frame or arena
  pointer, and incompatible replacement cannot bypass final driver validation;
  prior valid checked primitive mutations are not rolled back. Trusted native
  artifacts retain their existing native authority;
- an active frame captures its step address, descriptor, and generation pin at
  push, and does not consult the registry per step.

Not frozen:

- per-implementation emission versus a runtime thunk pool, and how the pool is
  sized;
- the entry shim's exact name;
- ResumableEntry layout, indexing, or registry data structure;
- the synchronization mechanism a future multithreaded Runtime uses to provide
  protected resolve-and-pin, except that it may not hold a lock or Rust borrow
  across metacircular lookup and must meet §11's performance evidence;
- automatic arena trim heuristics beyond S1.1's retain-until-destroy baseline;
- descriptor blob encoding and physical home, which 60I now freezes;
- the §11 budget numbers, which are provisional by construction.

## 14. Acceptance tests

Each item is labelled with the stage that can first run it. S1 gates on the S1
items; the later ones gate their own stage.

Registration and entry are covered when tests demonstrate:

Microcode memory-safety evidence stays at its owning boundaries rather than
being duplicated here: 60B §12 test 8 covers checked primitive validation before
each primitive mutation, and 60A §10 test 9 covers malformed final-action
failure before the requested driver transition. Neither test promises rollback
of an earlier valid checked mutation. Item 12 below proves only the late-binding
integration seam.

1. **(S1)** an ordinary send to a migrated selector answers the same Oop as
   before migration, for the same selector-determined argument count;
2. **(S1)** no vtable holds a StepMethod address, asserted by enumerating with
   `vtableCount` / `vtableAt` / `vtableSelectorAt` and resolving each selector
   through the floor's internal vtable lookup, since the enumeration answers
   selectors rather than addresses and the `lookup` primitive is not reachable
   from Birnam source. The enumeration must skip the floor's opaque internal
   dictionaries, whose values are exported Oops rather than method addresses;
3. **(S1 amendment)** a resumable method reached by ordinary send from
   unmigrated code is classified by the shared private resolver, completes and
   returns without invoking the entry thunk or exposing a Machine; a separate
   raw-Method fixture directly invokes the thunk and proves the compatibility
   fallback answers the same Oop;
4. **(S1)** a live frame keeps running after its selector is reflectively
   redefined and its binding retired, because the frame captured its step
   address, descriptor, and generation pin at push;
5. **(S1)** nested entry from both a synchronous Method and a resumable
   microcode replacement pushes onto the scoped Machine above an entry-barrier
   delimiter; the Machine count does not grow, the waiting outer Frame survives,
   and the nested work cannot suspend or capture across the delimiter;
6. **(S1)** a thunk reached by unrelated host or reactor work with an active
   drive but a suppressed entry scope starts a fresh Machine rather than
   nesting;
7. **(S1)** the entry-scope stack is a distinct borrow domain: nested entry does
   not panic or deadlock on a reentrant borrow;
8. **(S1)** an outer drive's StepAction is stale during a nested drive **and
   remains stale after it returns**, so a driver that re-read a rewritten
   scratch payload would fail rather than decode it;
9. **(S1)** a Return at the barrier frame ends the nested drive with the Machine
   still Running, and the outer drive's frames, chunks, and generation pins
   survive it;
10. **(S1)** forced collection during a nested drive preserves a value live only
    in an outer frame slot;
11. **(S1)** a FrameView taken on a nested frame is stale after that frame pops
    and an outer Call reuses its arena offset;
12. **(S1)** add, overwrite, removal, inherited replacement, private-child
    shadowing, and custom lookup of a microcode selector retain ordinary late-
    binding behavior. Mutation occurs after a StepMethod returns while its Frame
    remains active; that Frame's next resolve-and-pin observes the new binding,
    while its captured StepMethod and descriptor remain unchanged;
13. **(S1)** installing one thunk under two direct bindings gives it two binding
    references; overwriting or removing one retires only that reference and the
    other binding remains resumable. After the final reference retires, a
    deterministic pause between lookup and classification still resolves to an
    acquired entry plus directly observed generation pin, never a synchronous
    misclassification; failed Frame push releases that pin. Runtime-local
    registries may independently recognize the same mapped thunk address without
    sharing entry state. A custom-lookup cache retaining the carrier with an
    explicit lease remains valid through binding retirement; the same cache
    without a lease receives `resumableEntryRetired`, never synchronous
    invocation;
14. **(S1)** a driver failure raised inside `do:` is fatal rather than discarded
    by `vector_do`, and is not catchable by `on:do:`;
15. **(S1.4)** discarding a generation that installed no method answers a
    `DiscardReceipt` naming that generation with `logical = Discarded` and
    `physical = Disposed` once its leases drain. Discarding one that installed a
    method completes the author-file, `.candidate`-sidecar, and `state
    discarded` logical transitions and answers `logical = Discarded` with
    `physical = Retained(codeGenerationPinned)`. Retained-generation count and
    bytes rise observably. Registration and Frame pin counts are observed
    directly across resolve, push, failed push, pop, and binding retirement
    rather than inferred from conservative retention;
16. **(S2)** a migrated-to-migrated Call is served by a frame push with no thunk
    invocation and no native-stack growth, asserted by a thunk-activation probe
    counter that must read zero;
17. **(S2)** the same holds when the implementation is **inherited** and when a
    custom metacircular lookup returns its registered thunk address, with the
    probe counter still zero in both cases;
18. **(S2)** a resumable **block body** registers and executes through the
    direct `birnam_method` surface, not only through the Store MOP;
19. **(S2)** live and AOT registration produce identical FrameDescriptor
    identity, declared arity, and implementation identity for the same source;
20. **(S2)** a TailCall to a synchronous Method increments the drive's
    `tailCallToSynchronous`, and the tail-recursion proof asserts it is zero for
    the drive under test;
21. **(S2)** image v5 carries exactly one canonical `Resumables` record for its
    Code authority; a saved migrated named method and generated block vtable
    boot through mapped code with the image and artifact manifests, exact
    generation registry, resolver-returned vtable identity, and final bindings
    all agreeing before user entry. Mapped and rebuilt paths answer the same
    result and preserve block-vtable sharing. Missing, extra, duplicate,
    wrong-kind, wrong-arity, wrong-descriptor, wrong-resolver, or drifted
    entries answer `resumableEntryUnrestored` at the §4.3 checkpoint rather than
    aborting on first send. A never-evaluated block body is registered without
    allocating its vtable, and frozen IdApi layout/fingerprint fixtures remain
    byte-identical;
22. **(S3)** capture, suspension, Abort, and Restore that would cross an entry
    barrier each fail with `continuationBarrier`, and the outer drive resumes
    correctly afterward;
23. **(60G)** after actual reclamation of a method-bearing mapping, no Runtime
    registry or reachable vtable names an implementation in the unloaded
    generation's mapped range;
24. **(S3)** a live Continuation contributes a directly observable generation
    pin and the repository reports `Retained(codeGenerationPinned)` while it is
    held. Pin counts are observed across push, pop, capture, consume, and
    finalization; conservative method-generation retention alone is not accepted
    as evidence that Continuation pinning works;
25. the §11 measures are published whether or not they pass. S1 publishes the
    Continue, synchronous resolve-and-pin, and microcode-per-step measures using
    its hand-written StepMethod, against the `sends.compiled-unary` value `S`,
    and asserts the zero-allocation/lock/atomic/refcount structural fast path.
    S1.2's hand-written synchronous-entry probe asserts that current boundary
    depth rises once per active synchronous crossing and returns to zero, while
    its per-drive high-water value records the peak. The Call/Return, ordinary
    stackless entry, fallback-thunk entry, and native-stack depth measures need
    two migrated implementations and a thunk, so they belong to S2 and publish
    the diagnostic high-water value alongside measured native-stack capacity.
    Concurrent evolution later publishes the
    read-epoch and mutation-contention measures before it can be accepted.

## 15. Stage-owned follow-up decisions

No item in this section blocks S1.1. Each unresolved representation or
integration choice has an owner and a latest decision point:

1. **Migration as a change event — resolved by 60F §13.3.** lookup answers the
   thunk address, and `tracked_add_method` retracts the old carrier and asserts
   a new one on every install. SourceRevision remains unchanged for a carrier-
   only switch; the successor DispatchGeneration records `Synchronous` or
   `Resumable { descriptor_sha256 }`, and reconciliation compares generation-
   owned fingerprints rather than durable native addresses.
2. **Arena trim — S1.1 baseline frozen; later policy measurement-owned.** 60B
   §3 retains free chunks until Machine destruction, traces only live frames,
   initializes live Oop slots on every push, and publishes high-water and
   reserved-byte telemetry. Whole-chunk zeroing and automatic trim heuristics
   are not S1 correctness requirements; any later policy must be justified by
   those measures and preserve the frozen tracing and generation rules.
3. **Descriptor carrier semantics — resolved by 60F §§13.1–13.2 and 60I.**
   §4.3 freezes the carrier's required values, version/size validation,
   and lifetime transfer. S2 uses the accepted sized native envelope plus
   pointer-free canonical descriptor-v2 blob, including root maps,
   colored-slot/entry/call-window layout, edge transfers, source maps, SHA-256
   validation, and owned decoding. 60I freezes every field and offset.
4. **Durable resumable tag semantics — resolved by 60F §13.4 and 60I.** One
   canonical pointer-free
   `ResumableManifest` is exported by the artifact and projected into image v5's
   required `Resumables` record. Tagged heap/fact references use an idempotent
   unit-local resolver for private generated block vtables; the Runtime supplies
   discovery through its side catalog. §4.3's post-initializer and final-binding
   checks cover named and block entries without expanding IdApi.
5. **Depth diagnostics — S1.2 resolved.** §5.3 requires per-drive current and
   high-water synchronous-boundary depth without an artificial limit. S2's
   alternating-depth evidence publishes the counter alongside measured native-
   stack capacity.
6. **Shared private target resolution — implemented and gated 2026-08-22.**
   `send_abi` and Machine Call/TailCall consume the same protected three-way
   classification. The thunk remains the installed Method carrier and working
   direct-invocation fallback. Registration, DNU, inherited/custom lookup,
   redefinition, lifetime, nested-entry, failure-boundary, fast-path, and
   IdApi-floor tests passed; counters prove ordinary send and Machine calls do
   not activate the thunk while direct raw-Method invocation does. The tracked
   Design 46 A/B result is recorded in §11.
