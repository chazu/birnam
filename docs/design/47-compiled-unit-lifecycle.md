# 47 — CompiledUnit Lifecycle: Staged Dispatch Generations

> **Status:** Implemented, 2026-07-13, rev. 3 after a second adversarial review.
> Rev. 2 corrected the Store/id MOP seam, candidate roots, stable-target rule,
> immutable load paths, actual-base validation, recovery ordering, and composed
> image authority. Rev. 3 closes the remaining interleaved-edit, stale-rollback,
> dependency-effect, artifact-trust, and unspecified-binding-ledger gaps without
> adding an `IdApi` field.
> Refines [45 — Compiled Units Are Primary](45-compiled-units.md) with an
> implementable live-reload boundary. Governed by
> [42 — Object Canonical, Relational Derived](42-object-canonical-relational-derived.md),
> especially the Store-arbitrated MOP and exact-removal codifications.
>
> **Current boundary:** this page remains the retained low-level candidate
> lifecycle. The later file-backed, replacement, and cross-worker rollout policy
> described by designs 49/50 was withdrawn; the runtime keeps the
> runtime-wide-generation and host-safe candidate mechanism already implemented.

## Decision in one paragraph

A `CompiledUnit` is a reflected, store-recorded **dispatch generation** backed by
a relocatable dylib. A candidate may add, replace, or explicitly remove methods on
named live targets without changing dispatch while it is inspected and validated.
Activation applies the complete prevalidated binding set through Store's tracked
MOP; rollback restores replaced/removed bindings and exactly removes additions.
The active object dictionary remains canonical, Store records the unit graph,
history, and MOP mirror, and compiled code remains the primary executable layer.
Arbitrary module top-level effects are deliberately outside the transaction: this
is exact dispatch-generation rollback, not a fiction that I/O, FFI, and every live
object mutation can be undone.

## 1. Why this is the next boundary

The system already has every adjacent capability:

- `[Codegen compile:output:]` emits relocatable dylibs;
- `[Artifact inspect:]` returns a complete inert descriptor without installation;
- `[Codegen load:]` fixes up and executes an artifact;
- `Module` stores the dependency graph as facts and loads dependencies first;
- Store mirrors method installation under the consistency law;
- images checksum and compose compiled `.code` generations; and
- method/module source is retained for explicit recovery.

What is missing is separation between **open** and **activate**. Today `load:` runs
`__ev`, whose artifact lowering installs methods immediately. A broken or unwanted
unit can be rejected before compilation or `dlopen`, but not after inspection and
before dispatch changes; there is no exact rollback generation.

## 2. Constitutional constraints

The implementation is accepted only if all of these remain true.

1. **The live object environment is canonical.** Dispatch reads native vtable
   dictionaries. Within the supported lifecycle surface, `#active` is reported only
   after ledger reconciliation and mirrors actual activation; it never commands it.
   A raw floor mutation explicitly exits that guarantee until reconciliation.
2. **Store arbitrates the supported MOP.** Candidate activation/removal goes through
   Store's tracked operations. Raw id mutation remains bootstrap/private mechanism.
3. **The mirror is current or absent, never false.** Replacement orders
   retract→set→assert; removal orders retract→remove.
4. **Source and binary delivery are semantically identical.** Source candidates
   compile to the same artifact ABI and enter the same activation path as prebuilt
   candidates.
5. **Compiled units remain primary executable state.** Facts/source are reflection,
   provenance, authoring, and recovery material—not a store-driven send path.
6. **The send hot path is untouched.** All new work is compile/open/activation time.
7. **The Zig additions are mechanism only.** No native lifecycle registry, policy
   state machine, dependency solver, generation allocator, or history database.
8. **Images are honest.** Saving after activation or rollback must boot/rebuild the
   behavior that was current at save time. Raw pointers and `dlopen` handles never
   become durable facts.
9. **Managed state is conditional on an exact live base.** The public
   `CompiledUnit` queries and every lifecycle/save operation reconcile the active
   unit against Codegen's transient binding ledger. A supported non-lifecycle edit
   of a managed pair marks that generation `#drifted`; a raw floor escape may be
   discovered lazily, but can never be activated over or rolled back through.
10. **No new id API seam.** Exact removal is an internal vtable message reached via
   the existing `send`; direct-binding reads use the existing selector enumeration
   plus `lookup`. Rooting, ledgers, mutation plans, and lifecycle policy live above
   id.

## 3. Scope: a dispatch transaction, not a world transaction

### Candidate-safe operations

A candidate source is declarative. The first slice accepts:

- one optional `[Module named: #x requires: #[...]]` declaration;
- `[Reflect defineMethod: #sel on: NamedTarget as: ...]` and its doc form; and
- `[Reflect removeMethod: #sel on: NamedTarget]`.

`NamedTarget` must resolve by a stable Globals name at validation and activation,
and it must **already own** its family/private vtable (`vtableOwner(vt) == target`).
Validation is read-only and never calls `ensurePrivateVtable`: a first definition
on an unspecialised singleton would change the target's vtable identity, which
method removal alone cannot roll back. The compiler rejects duplicate operations
on one `(target, selector)` in a single candidate.

Definitions add a missing direct binding or replace an existing one. Explicit
removals require an existing direct binding. Absence from a candidate means "leave
unchanged," not "delete everything formerly supplied by this module"; removal is
never inferred from a partial patch.

### Deliberately excluded

- arbitrary top-level sends or object creation;
- creating/replacing user globals;
- FFI open/wrap/close and callbacks;
- filesystem, network, process, or other external effects;
- vtable parent/layout changes;
- first-time copy-on-write vtable specialisation of a singleton;
- automatic dylib unloading; and
- transactional replacement of a whole arbitrary module initializer.

Normal module source/binary loading remains available with its immediate semantics
for these cases. A future
declarative global/object-initialization model may widen the candidate language,
but it must name reversible effects rather than pretend arbitrary sends are a
transaction.

## 4. Domain model in ordinary facts

`modules/compiled-unit.birnam` owns policy and reflected state. A unit is an ordinary
entity with cells/facts such as:

| Attribute | Meaning |
|---|---|
| `#unitName` | Stable module/patch name (Symbol) |
| `#generation` | Monotonic generation within that name |
| `#kind` | `#candidateArtifact`, `#adHocBaseline`, or `#composed` |
| `#sourceHash` | Deterministic source identity |
| `#artifactChecksum` | Integrity value of the dylib inspected |
| `#platform` / `#runtimeAbi` / `#candidateAbi` | Compatibility metadata |
| `#requires` | Many-valued unit/module dependency names |
| `#provides` | Many-valued binding-operation entities |
| `#state` | `#candidate`, `#active`, `#superseded`, `#drifted`, `#rolledBack`, `#historical` |
| `#supersedes` | Prior active unit, if any |
| `#rollbackAvailable` | Honest current-process capability flag |
| `#recoveryPortable` | Every forward and inverse operation has rebuildable source |
| `#composedInto` | Image-composed executable generation that later materialised it |

Each provided operation is another entity:

| Attribute | Meaning |
|---|---|
| `#targetName` | Stable Globals name, never a process pointer |
| `#selector` | Selector Symbol |
| `#operation` | `#install` or `#remove` |
| `#definitionSource` | Portable definition/removal source |
| `#operationSequence` | Explicit recovery-order number |
| `#priorWasDirect` | Activation-time provenance; no raw pointer stored |
| `#priorUnit` | Managed generation that supplied the prior definition, if known |

Activation events are append-only entities (`unit`, prior unit, action, basis).
Store history answers "what was active when?" without a second policy registry.
Process-local dylib handles, roots, raw binding snapshots, and prepared commit plans
live only in a Codegen-owned transient ledger keyed by unit identity. They are
mechanism caches, not lifecycle authority, and never serialize. `#contentPath`, if
surfaced for diagnostics, is likewise transient rather than a durable fact; the
durable artifact identity is its checksum.

## 5. MOP prerequisite: exact direct removal

### id mechanism

Add one internal metaobject method, `vtableRemoveMethod`, beside lookup/add:

1. search only the receiver vtable's direct key/value arrays;
2. if absent, answer raw absence without mutation;
3. call the registered method-mutation hook;
4. retain the removed method pointer as the answer;
5. shift the dense tail left and decrement `tally`.

It has no Store, CompiledUnit, history, or error policy. It is registered as an
internal vtable message and invoked through existing `IdApi.send`; **IdApi does not
grow**.

### Store arbitration

Store adds `removeMethod:on:` as the supported MOP operation. `Reflect` adds the
stratified public spelling `removeMethod:on:`. In this lifecycle it accepts only an
already-stable owned/family vtable; it does not create a private vtable while
removing. The supported Store/Reflect operation answers self/status or a kinded
error, never a raw native method pointer. Candidate-private Codegen mechanism
captures old pointers before calling it.

The existing overwrite hook becomes a general direct-binding mutation hook: it
retracts any current mirror value, including `#primitive`, before replacement or
removal. Tracked replacement asserts the new value afterward; tracked removal does
not. The removed binding remains available in history when that selector opts into
history.

### Direct lookup needs no new floor mechanism

Codegen derives `methodAtDirect(vt, sel)` by enumerating `vtableSelectorAt` and,
only on a direct-name hit, sending the existing raw `lookup` message. Enumeration
proves the returned pointer is direct rather than inherited. The Store mirror cannot
provide the pointer reliably because honest absence and `#primitive` markers are
legal.

## 6. Candidate artifact ABI

Ordinary artifacts retain their immediate-load semantics and gain one version symbol
alongside the current ABI:

```
__birnam_artifact_abi
__birnam_artifact_descriptor
__birnam_unit_init
__ev
```

`__birnam_artifact_abi` is new at the portable-log cutover. The candidate-validator
contract is ABI v2: v1 candidates with the former no-op export are rejected. Absence identifies a
pre-cutover artifact whose `__ev` may still append to `__methods__`; post-cutover
ordinary loading rejects it and uses the existing source-recovery/recompile path.
The other symbols and immediate-load semantics remain unchanged.

A candidate-capable artifact additionally exports:

```
__birnam_candidate_manifest   versioned, pointer-free bytes
__birnam_candidate_validate   artifact-enumerated mechanical target/binding/ABI checks; no policy
__birnam_candidate_prepare    create private block state; no public dispatch mutation
__birnam_candidate_activate   tracked MOP commit using prepared state
__birnam_candidate_rollback   exact inverse while process-local pointers exist
```

Candidate delivery is a small bundle: a pointer-free manifest envelope plus the
dylib bytes whose checksum it names. The envelope is readable as data **before**
`dlopen`; it contains format version, candidate-ABI version, runtime-shim ABI hash,
unit name, source hash, platform, artifact checksum, dependencies, and ordered
binding operations with target/selector, operation kind, block arity, and portable
source. It contains no Oops, vtable addresses, function pointers, or dylib handles.
The exported manifest must byte-match the envelope after mapping. A detached
sidecar is acceptable for the first implementation; a later single-file container
may embed the same envelope without changing its schema.

The two compatibility identifiers are deliberately separate. The runtime-shim ABI
hash answers whether this dylib can call this runtime at all; the candidate-ABI
version answers whether Codegen and `CompiledUnit` agree on manifest fields,
validation, activation slots, and rollback semantics. Both must match before
mapping.

### Trust boundary

Candidate loading is native-code loading, not a security sandbox. The first slice
accepts only artifacts produced by the matching Birnam candidate compiler or by an
otherwise trusted build pipeline. A checksum proves byte identity, not provenance;
an arbitrary dylib can execute constructors during `dlopen` or lie in its exported
hooks. Pure inspection therefore reads only the envelope and dylib bytes. Mapping
is a separately named, explicitly native step, and the compiler contract forbids
initializer sections and non-allowlisted dependency load commands: generated
candidate code runs only through the named hooks and resolves its runtime shims
against the already-running host. Inspection checks those Mach-O properties before
mapping.
Supporting hostile third-party native artifacts would require authentication and a
loader/sandbox boundary outside this proposal.

After the explicit mapping step, `__birnam_unit_init` may fill artifact-private
intern/global-name slots because it does not install methods or mutate user-visible
dispatch. Candidate validation runs after fix-up and before candidate block roots or
dispatch changes are created. The native validator is deliberately mechanical:
envelope/export agreement, ABI integrity, unit/dependency metadata agreement, named
target resolution, selector/block arity, stable vtable ownership, and
duplicate/direct-binding checks. Dependency, generation, and supersession policy
remains Birnam.

The compiler emits candidate hooks only for candidate-safe source. Normal `__ev`
still exists so the same artifact may be delivered through ordinary load with the
ordinary semantics; the manifest marks ineligible source rather than silently
giving it partial candidate behavior. The manifest unit name comes from the
candidate compile request, not its authoring path; when source carries a `Module`
declaration, its name and dependency list must match both candidate metadata and
the artifact descriptor exactly.

## 7. Narrow Codegen mechanism

Codegen adds thin mechanism operations (names provisional):

```birnam
[Codegen candidateManifestOf: path]
[Codegen mapCandidate: unit]
[Codegen validateCandidate: unit]
[Codegen prepareActivation: unit plan: publicationPlan]
[Codegen commitPrepared: token]
[Codegen prepareRollback: unit plan: publicationPlan]
[Codegen commitPreparedRollback: token]
[Codegen discardPrepared: token]
```

`candidateManifestOf:` is byte inspection and never maps code. `mapCandidate:` is
the explicit `dlopen`/`dlsym` boundary. The remaining operations validate or apply
an already-mapped trusted candidate. They do not mint units, choose generations,
solve dependencies, or decide which unit may supersede another. Pure-Birnam policy
constructs a complete publication plan before preparation; Codegen copies and
validates that plan into native transient memory before any dispatch mutation, then
mechanically applies it after the native binding commit. It does not invent policy.

These methods are private mechanism in the same sense as the raw id vtable messages:
Birnam has no access modifier, but the only supported lifecycle surface is
`CompiledUnit`. Calling a `commitPrepared...` method directly is an explicit escape
that forfeits managed-state guarantees. Tokens are opaque, process-local, bound to
one unit/base snapshot, and one-shot, so a prepared validation cannot be replayed
after the base changes.

`discardPrepared:` consumes an unused token and releases preparation-only roots when
no active/predecessor ledger can reach them. Prepared tokens also carry a finalizer
with the same conservative cleanup, so abandoning a validation cannot leave a live
capability indefinitely. Dylib mappings remain subject to the no-unload rule.

They also never open a mutable authoring path directly. Codegen copies the completed
dylib to a fresh file while hashing those same bytes, verifies the envelope checksum,
publishes the file to its checksum-addressed path with an exclusive atomic rename,
reopens and rehashes the published destination, and only then calls `dlopen`. It
rejects a pre-existing destination whose bytes disagree and never follows a caller-
controlled symlink. Process-local handles/root state are keyed by `(checksum, unit
identity)`, not the caller's path: macOS `dlopen` caches by path, so overwriting and
reopening `./candidate.dylib` could otherwise return the old mapping and fake a
reload.

Generated candidate hooks reach two Codegen-owned runtime shims:

- tracked add/replace: call `Store addMethod:on:with:`;
- tracked remove: call `Store removeMethod:on:`.

Before the first dictionary mutation, activation validates the complete operation
set and captures for every binding: direct presence and old method pointer when
present. Raw pointers stay in artifact static rollback slots. Only then does it
perform tracked operations in manifest order.

### Transient binding ledger

Codegen owns a process-local binding ledger beside its handle/root cache. For every
managed `(targetName, selector)` it records:

- resolved target and owned-vtable identity;
- expected direct presence and method pointer for the active generation;
- the candidate's post-activation presence and pointer;
- the captured predecessor presence and pointer needed for live rollback; and
- the portable forward/inverse operation identities, but not lifecycle policy.

An activated generation inherits its predecessor's managed entries and overlays its
own operations, so later candidates can validate pairs untouched by the immediately
preceding patch. A previously unmanaged **absent** direct binding may be adopted
mechanically as absence. A previously unmanaged present binding requires explicit
Birnam adoption and known portable recovery source before a rollbackable candidate
may touch it. This is the actual-base table used by validation; it is a transient
mechanism cache, not the forbidden native lifecycle registry.

On mapped/rebuilt image boot, after code resolution, Codegen reconstructs the
composed generation's ledger by enumerating the managed pairs named by the normalized
portable operation stream and capturing their current direct states. No raw address
crosses the image boundary.

### Candidate block roots

Candidate trampolines do **not** use the ordinary artifact `__amblk_*` Globals.
That would make opening/rolling back a candidate mutate durable namespace state and
would cause image compaction to rebuild obsolete candidate closures. Instead
Codegen owns a transient candidate-root table keyed by checksum/unit/binding index,
registers its existing `gcRegisterRootProvider` callback once, and exports narrow
set/get shims used by candidate activation and trampolines. Activation creates each
method block and registers it there before changing dispatch; inactive/rolled-back
candidates retain roots while some active generation can still restore their code.
Roots become releasable when the unit is rolled back/forfeited and no descendant's
rollback ledger names it; the first implementation may conservatively retain such
releasable roots until process exit.

The same root provider marks every Oop retained by transient ledger entries and by
the native plan behind a still-live prepared token. It does not root the token object
itself—the caller must retain that capability, and its finalizer removes the plan.
Raw code pointers and dylib handles are not Oops and are never traced.

The table is mechanism only: it contains Oops and native handles, never lifecycle
policy, and is not serialized. A mapped/rebuilt image compiles the effective
portable method operations through ordinary artifact lowering, so its active blocks
receive the existing image/code recovery treatment rather than candidate roots.

### Close the existing artifact MOP gap

Ordinary artifact **user-method** lowering changes from raw `birnam_method` to a
Codegen runtime shim that calls `Store addMethod:on:with:`. This makes source and
binary user definitions equally tracked and lets actual-base adoption see complete
post-cutover bindings. Raw `birnam_method` remains for artifact-private block-vtable
plumbing and pre-Store mechanisms, whose public MOP mirror may honestly be absent.

The same cutover changes portable recording. `__methods__` is frozen as the ordered
legacy base present when lifecycle support starts. Every later supported user-method
install, replacement, or removal—ordinary source eval, ordinary artifact load,
candidate activation, and rollback—appends to the single sequenced `__methodops__`
stream. Pre-cutover artifacts have the old `__methods__`-writing ABI and must be
recompiled; the runtime rejects them rather than allowing a late legacy write to be
replayed before an older candidate operation.

A supported non-lifecycle edit of a ledger-managed pair also notifies
`CompiledUnit` after the tracked mutation/log append, transitioning the prior active
generation to `#drifted`. The observer sits at Store's completed tracked-MOP
chokepoint, so direct Store installs/removals notify immediately too, though they
remain non-portable unless accompanied by a sequenced source operation. Raw floor
mutation remains the explicit lazy escape; every
lifecycle operation, public active-unit query, and image save independently compares
the dictionary with the ledger and performs the same drift reconciliation before it
trusts lifecycle facts.

Direct Store mutation remains a valid Store-arbitrated MOP operation and preserves
the mirror law; CompiledUnit marks it as drift immediately, and adoption rejects it
unless the caller also supplied portable operation/provenance.

## 8. Lifecycle algorithms

### Open candidate (pure Birnam policy)

1. Read the pointer-free envelope, hash the finished artifact, and reject forbidden
   Mach-O initializer sections or dependency load commands without mapping it.
2. Reject unsupported manifest/candidate ABI, runtime-shim ABI, platform, integrity,
   metadata disagreement, or unsafe source.
3. Register/verify dependency **names** through `Module`, but do not load anything.
4. Materialize the CompiledUnit and binding-operation facts as `#candidate`.
5. Report any unloaded dependencies as `#dependenciesMissing`; do not change
   dispatch, Globals, or `#active` state.

Opening the same `(name, artifactChecksum, platform, runtimeAbi, candidateAbi)` is
idempotent while that unit remains `#candidate` or `#active`. Restaging the same
artifact after a terminal rollback/historical transition mints a new generation and
fresh base snapshot. Reusing an authoring path with different bytes necessarily
produces a different content path and mapping.

Dependency loading is deliberately outside the candidate transaction. The caller
may explicitly ask `Module` to load the reported dependencies, accepting their
ordinary `__ev` effects, then resume candidate mapping/validation. The first slice
never loads a dependency as a hidden side effect of open, validate, or activate.
Thus a candidate rejection cannot be blamed for dependency behavior; dependency
preparation is a separate, visible operation and is not rolled back.

### Map candidate (explicit trusted-native boundary)

1. Require every declared dependency already loaded.
2. Materialize and verify the immutable checksum path.
3. `dlopen` the trusted compiler output, compare its exported manifest to the
   inspected envelope, and run only `__birnam_unit_init`.
4. Cache the handle under unit identity; do not create candidate blocks or mutate
   public dispatch.

A mapping or fix-up failure leaves the unit `#candidate` with an error and no
binding mutation. Mapping may intern artifact-private names; the zero-effect claim
is specifically no public dispatch, Globals binding, module initialization, or
candidate block creation—not sandboxing hostile native code.

### Validate

1. Every target name resolves in Globals.
2. Every target already resolves to a stable owned/family vtable; validation never
   calls `ensurePrivateVtable`.
3. Selectors and operation pairs are unique.
4. Explicit removals name direct bindings.
5. Selector arity agrees with the method block, and manifest name/dependencies agree
   with the compile request, optional module declaration, exported metadata, and
   reflected unit facts.
6. The current active generation fact names the expected base **and** every touched
   managed pair's target/vtable identity and direct binding pointer/presence matches
   that base's transient ledger. An unmanaged present pair requires explicit adoption
   with portable source; absence may be adopted directly.
7. If any managed pair differs, transition the purported active unit to `#drifted`
   and reject as stale before mutation. Facts alone are insufficient: raw mechanisms
   can change dispatch without changing lifecycle state.
8. The native mechanical validator agrees before any mutation.

### Adopt a drifted live base

`[CompiledUnit adoptLive]` is the explicit bridge back from immediate editing to
managed generations. It makes no dispatch change. Pure Birnam verifies that every
changed managed binding has a sequenced portable operation, mints an
`#adHocBaseline` generation in state `#active`, marks the drifted predecessor
historical, and asks Codegen to snapshot the current target/vtable/direct-binding
states as that baseline's ledger.
An unlogged raw change, unknown target replacement, or non-portable native binding
rejects adoption. This is the required step before a later candidate can activate
over live edits; absence is never silently interpreted as an expected binding.

### Activate

1. Pure Birnam checks transition/dependency policy and builds the complete
   publication plan: forward operation-log records, unit state changes,
   `#supersedes`, and activation event.
2. `prepareActivation:plan:` copies that plan into transient native memory,
   revalidates the lifecycle fact, ledger base, target identities, and complete
   operation set, runs candidate-private preparation, captures old direct states,
   and returns a one-shot token. It changes no public dispatch.
3. `commitPrepared:` revalidates the token's binding snapshot, applies every
   install/remove through Store's tracked MOP, appends every forward operation to
   `__methodops__`, overlays the transient binding ledger, and mechanically applies
   the already-validated publication facts.
4. The token is consumed whether commit succeeds or traps; a recoverable rejection
   happens only before the first dictionary mutation.

The runtime is single-threaded today. After complete prevalidation, the ordinary
commit path uses the native Store MOP operations and, after mutation begins, performs
only direct calls/sends to the well-known Store implementation over data copied
beforehand. It invokes no candidate block, collection protocol, or replaceable
`CompiledUnit` helper. Allocation failure remains fatal as elsewhere. Thus no
*concurrent ordinary send* observes a half-generation and there is no normal return
with dispatch changed but lifecycle publication missing. Direct use of the private
commit mechanism or an adversarial raw override is an explicit escape. A future
concurrent runtime must put step 3 under its
single-transactor/dispatch mutation lock rather than treating the sequential loop
as intrinsically atomic.

### Roll back

Rollback first performs the symmetric stale check missing from rev. 2: the unit must
be `#active`; every target must still be the captured target/vtable; and every
touched direct binding must equal the candidate's recorded **post-activation**
presence and pointer. Any mismatch marks the unit `#drifted` and rejects rollback
without mutation. It never overwrites an intervening ordinary edit.

Pure Birnam then prepares the rollback publication/inverse-log plan. Codegen copies
it and, for every operation in reverse order:

- candidate replaced a direct binding → tracked-replace the captured old pointer;
- candidate added a direct binding → tracked-remove it, revealing inheritance;
- candidate removed a direct binding → tracked-add the captured old pointer.

The same non-failing commit appends each portable inverse to `__methodops__`, restores
the predecessor ledger, marks the candidate `#rolledBack`, restores its predecessor
to `#active`, and records the event. Repeating rollback on that already-rolled-back
unit is a true idempotent no-op; attempting it from any other incompatible state is
a kinded lifecycle error.

## 9. Portable method-operation history

Process-local pointer restoration is insufficient: an image saved after rollback
must rebuild the rolled-back behavior, not the last candidate definition it saw.
Legacy `__methods__` is frozen at a recorded cutover basis and remains the
pre-lifecycle base stream. All supported post-cutover activity appends to an
explicit `__methodops__` **method-operation log**:

- install/replace appends the complete `Reflect defineMethod...` source;
- removal appends a complete `Reflect removeMethod...` source;
- rollback of a replacement/removal appends the predecessor's definition source;
- rollback of an addition appends removal source.

Each operation captures the current Store basis immediately before its own unique
log assertion and uses that basis in a sequence-bearing key; the assertion itself
advances the basis, so consecutive records remain unique even when a dictionary
removal produced no retained mirror fact. Replay and `compileImageCode:` evaluate
the immutable legacy base first, then sort `__methodops__` by the explicit numeric
sequence; they do not rely on `attrAt:`'s current first-appearance implementation.
Later operations win exactly as live mutation did. The sequence contract is tested
across image save/boot and is part of the format's recovery semantics.

The cutover is also an artifact-ABI boundary. Post-cutover Codegen never writes
`__methods__`; old ordinary artifacts that embed such writes are incompatible and
must be rebuilt. This prevents a later ordinary edit from being replayed before an
earlier candidate operation.

Managed unit records retain the portable source for each binding. In the first
slice, **activation** is rejected unless every inverse has portable source, even if
a raw pointer happens to remain callable. `#recoveryPortable` is therefore true for
every active candidate. `#rollbackAvailable` is narrower: it is true only while the
required mapped handles, roots, prior-pointer ledger, and exact current post-state
still exist in this process. This prevents a live state that cannot honestly be
saved/rebuilt while keeping portable recovery distinct from immediate native
rollback capability.

Adopting a pre-lifecycle binding into a rollbackable baseline therefore requires a
known recorded definition source. Primitive bindings are not replaceable by a
first-slice candidate unless a named kernel/module recovery operation can reproduce
them without a pointer. Ordinary immediate live replacement remains available
outside candidate semantics.

## 10. Image behavior and composed authority

No image **format** change is required for the first slice, but a small image
introspection seam is: while loading `.code` records, Image computes the ordered
composition identity of their checksums and exposes it after code resolution. It
does not mint lifecycle objects or choose states.

The existing persistence pieces remain:

- CompiledUnit/binding/event facts serialize as ordinary durable data.
- The pure-Birnam CompiledUnit prototype and its shared instance vtable have stable
  loader exports, so origin instances relink before mapped/replayed module code
  refreshes their methods.
- The method-operation log makes `compileImageCode:` capture the behavior current at
  save time, including removals and rollbacks.
- A normal compatible boot maps the compacted image unit as primary executable code.
- `--rebuild` replays the same ordered method operations.

Before code capture, image save asks `CompiledUnit` to reconcile the reflected active
unit with Codegen's ledger and the actual dictionary. A mismatch becomes `#drifted`;
save still captures the current portable operation stream when it is complete, but
it never publishes an image whose facts claim that a mismatching managed generation
is active. An unlogged/raw drift with no portable recovery path rejects the save
under the existing durable-state contract.

Raw rollback pointers, candidate roots, and dylib handles do not survive. Before any
user entry point/REPL send, pure-Birnam CompiledUnit reconciliation reads the mapped
composition identity, mints a reflected **composed executable generation**, relates
each saved-origin generation with `#composedInto`, marks those origins historical,
reserves `#active` for the composed unit that actually owns dispatch, and asks
Codegen to rebuild the transient ledger for every managed pair from the live direct
bindings. A rebuild similarly mints a recovered composition from the normalized
operation identity and reconstructs the same ledger after replay.
Entering image dispatch recovery first retires every pre-load transient active
ledger, including in-process loads; only the newly composed owner becomes active.

This boot-time reconciliation happens before the image append checkpoint, so it is
not journaled as a user mutation. Historical units always answer
`rollbackAvailable: false`; their immutable artifacts may be restaged as **new**
candidates against the composed live base, but reopening does not resurrect the old
unit's pointer snapshot. Preserving several rollbackable native generations inside
an image is a later format feature, not implied.

## 11. Source/binary equivalence

There is one candidate path:

```
source → Codegen candidate compile → candidate bundle ┐
                                                      ├→ inspect/open → explicit map → validate/activate
trusted prebuilt candidate bundle ────────────────────┘
```

Pure source evaluation remains the ordinary immediate live-edit path; asking for
candidate semantics necessarily compiles first. The semantic distinction is
immediate edit vs staged generation, never source vs dylib delivery.

## 12. Implementation slices (each ends green)

1. **MOP removal law.** Raw id removal + Store tracked removal + Reflect spelling;
   direct/inherited behavior, mirror honesty, history, backfill tests, and tracked
   user-method lowering for ordinary artifacts. No `IdApi` field is added.
2. **Portable-log cutover.** Freeze `__methods__`; route every supported ordinary
   and lifecycle mutation through sequenced `__methodops__`; bump the artifact ABI
   so pre-cutover writers are rejected; prove mixed ordinary/candidate ordering.
3. **Candidate envelope + immutable inspect.** Emit/read pointer-free metadata before
   `dlopen`, decide both ABI identifiers, copy-and-hash atomically to a content path,
   enforce the trusted-compiler/no-initializer contract, and reject unsafe source.
4. **Explicit dependency/map boundary.** Open reports missing dependencies without
   loading them; dependency preparation is a separate ordinary effect; mapping is
   explicit and checks envelope/export agreement.
5. **Ledger, roots, and activation.** Codegen's candidate-root provider and transient
   inherited binding ledger; stable-owned-target and arity validation; prepared
   one-shot commit plans; add/replace/remove in one candidate; facts/events publish
   inside the non-failing commit.
6. **Rollback and drift.** Symmetric post-state validation, exact inverse including
   removal of additions and restoration of inherited behavior, managed ordinary-edit
   drift notification, lazy raw-drift reconciliation, and true repeated rollback.
7. **Sequenced portable operations + images.** Save/boot and `--rebuild` after
   interleaved ordinary edits, activation, drift, and rollback reproduce current
   behavior; mapped composition identity mints the honest active composed generation
   and reconstructs its ledger.
8. **Module/Config integration.** Path-based candidate inspect plus explicit
   dependency preparation; no automatic reload or watcher. Existing ordinary
   `Module load:` stays unchanged.

## 13. Verification matrix

- id: remove direct binding; missing is no-op; inherited binding reappears; later
  parent redefinition is visible (proves no copied-parent tombstone).
- Store: tracked remove retracts object and `#primitive` mirrors; history retains
  prior binding; raw change leaves absent, never stale.
- Codegen: envelope inspection happens before `dlopen` and has zero dispatch/global
  effects; invalid candidate/runtime ABI, envelope/export disagreement, forbidden
  initializer/dependency load commands, mutable-path replacement/race, metadata disagreement,
  arity mismatch, missing/unstable target, duplicate binding, lifecycle-stale base,
  actual-pointer-stale base, unmanaged present base, and missing removal reject before
  mutation; GC preserves blocks belonging to active/superseded rollback-reachable
  units, an abandoned prepared token releases only its preparation roots, and only
  unreachable/forfeited candidate roots become releasable.
- Lifecycle: candidate adds + replaces + removes; activation changes all behavior;
  rollback restores all behavior and unit states/history; ordinary edit or target/
  vtable drift after activation makes rollback reject without clobbering the edit.
- Dependencies: inspect/open with missing dependencies changes no dispatch; explicit
  dependency preparation may have ordinary effects and is never represented as part
  of candidate rollback.
- Delivery: candidate compiled from source and the same prebuilt artifact have the
  same manifest and effects.
- Ordering: ordinary edit → candidate → ordinary edit and candidate → rollback →
  ordinary edit produce identical live, compatible-boot, and `--rebuild` behavior.
- Image: compatible boot and `--rebuild`, saved active, drifted, and rolled back,
  agree; unlogged non-portable drift rejects save.
- Image authority: mapped/recovered composition becomes `#active`; origin units are
  historical/`#composedInto`; no stale rollback capability survives boot.
- Regression: `zig build verify`; current-ABI normal module/source/artifact loading
  remains behavior-compatible, stale artifacts recover/recompile, and the benchmark
  confirms send cost unchanged.

## 14. Non-goals and deferrals

- No automatic file watching or recompilation daemon.
- No arbitrary-module transaction or external-effect compensation.
- No hostile-native-code sandbox; candidate bundles are trusted native input.
- No safe dylib unloading; old code remains mapped while anything may reach it.
- No cross-process rollback guarantee in the first image slice.
- No store-driven dispatch or generic trampoline on the send hot path.
- No new IdApi fields and no native unit-policy registry.
- No compiler self-hosting dependency.

## 15. Remaining open questions

1. Is the candidate-safe source subset useful enough with add/replace/delete and
   exact rollback, given the deliberate stable-owned-target requirement?
2. Should a later slice let a named kernel/module recovery operation make primitive
   baselines portable, or keep them categorically outside candidate replacement?
3. Should inactive candidate roots be explicitly releasable after the user forfeits
   rollback, or merely retained until process exit in the first slice?
4. Should a future single-file candidate container replace the first slice's
   detached pre-map manifest envelope?
