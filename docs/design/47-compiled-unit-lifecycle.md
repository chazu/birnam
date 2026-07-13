# 47 — CompiledUnit Lifecycle: Staged Dispatch Generations

> **Status:** Proposed, 2026-07-12, rev. 2 after adversarial review. Rev. 2
> corrects the actual Store/id MOP seam, gives candidate blocks a Codegen-owned
> GC-root table, restricts targets to already-stable owned vtables, uses
> checksum-addressed immutable load paths, validates the actual binding base,
> gives recovery operations an explicit sequence, and reconciles image-composed
> executable generations honestly.
> Refines [45 — Compiled Units Are Primary](45-compiled-units.md) with an
> implementable live-reload boundary. Governed by
> [42 — Object Canonical, Relational Derived](42-object-canonical-relational-derived.md),
> especially the Store-arbitrated MOP and exact-removal codifications.

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
- `[Codegen moduleMetaOf:]` inspects dependency metadata without installation;
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
   dictionaries; an `#active` fact mirrors actual activation and never commands it.
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

## 3. Scope: a dispatch transaction, not a world transaction

### Candidate-safe operations

A candidate source is declarative. Rev. 1 accepts:

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

Normal module source/binary loading remains unchanged for these cases. A future
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
| `#sourceHash` | Deterministic source identity |
| `#artifactChecksum` | Integrity value of the dylib inspected |
| `#platform` / `#abi` | Compatibility metadata |
| `#contentPath` | Checksum-addressed immutable path opened in this process |
| `#requires` | Many-valued unit/module dependency names |
| `#provides` | Many-valued binding-operation entities |
| `#state` | `#candidate`, `#active`, `#superseded`, `#rolledBack`, `#historical` |
| `#supersedes` | Prior active unit, if any |
| `#rollbackAvailable` | Honest current-process capability flag |
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
Store history answers "what was active when?" without a second Zig registry. The
process-local dylib handle and rollback function slots are transient caches keyed by
artifact path; the facts remain meaningful after those caches disappear.

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

Ordinary artifacts retain their current symbols and semantics:

```
__birnam_module_meta
__birnam_unit_init
__ev
```

A candidate-capable artifact additionally exports:

```
__birnam_candidate_manifest   versioned, pointer-free bytes
__birnam_candidate_validate   mechanical target/binding/ABI checks only; no policy
__birnam_candidate_activate   prepare private code state + tracked MOP commit
__birnam_candidate_rollback   exact inverse while process-local pointers exist
```

The manifest contains only stable values: format version, name, source hash,
platform/ABI, dependencies, and ordered binding operations with target/selector,
operation kind, and portable source. It contains no Oops, vtable addresses,
function pointers, or dylib handles.

`__birnam_unit_init` may fill artifact-private intern/global-name slots because it
does not install methods or mutate user-visible state. Candidate validation runs
after fix-up and before candidate block roots or dispatch changes are created. The
native validator is deliberately mechanical: manifest/ABI integrity, named target
resolution, stable vtable ownership, duplicate/direct-binding checks. Dependency,
generation, and supersession policy remains Birnam.

The compiler emits candidate hooks only for candidate-safe source. Normal `__ev`
still exists so the same artifact may be delivered through ordinary load with the
ordinary semantics; the manifest marks ineligible source rather than silently
giving it partial candidate behavior.

## 7. Narrow Codegen mechanism

Codegen adds thin operations (names provisional):

```birnam
[Codegen candidateManifestOf: path]
[Codegen validateCandidate: path]
[Codegen activateCandidate: path]
[Codegen rollbackCandidate: path]
```

They are `dlopen`/`dlsym` adapters like `moduleMetaOf:`. They do not mint units,
choose generations, solve dependencies, change lifecycle facts, or decide which
unit may supersede another. They answer manifest values/status or kinded errors.

They also never open a mutable authoring path directly. Codegen reads and hashes the
completed dylib, copies it to an immutable checksum-addressed path, and `dlopen`s
that path. Process-local handles/root state are keyed by `(checksum, unit identity)`,
not the caller's path: macOS `dlopen` caches by path, so overwriting and reopening
`./candidate.dylib` could otherwise return the old mapping and fake a reload.

Generated candidate hooks reach two Codegen-owned runtime shims:

- tracked add/replace: call `Store addMethod:on:with:`;
- tracked remove: call `Store removeMethod:on:`.

Before the first dictionary mutation, activation validates the complete operation
set and captures for every binding: direct presence and old method pointer when
present. Raw pointers stay in artifact static rollback slots. Only then does it
perform tracked operations in manifest order.

### Candidate block roots

Candidate trampolines do **not** use the ordinary artifact `__amblk_*` Globals.
That would make opening/rolling back a candidate mutate durable namespace state and
would cause image compaction to rebuild obsolete candidate closures. Instead
Codegen owns a transient candidate-root table keyed by checksum/unit/binding index,
registers its existing `gcRegisterRootProvider` callback once, and exports narrow
set/get shims used by candidate activation and trampolines. Activation creates each
method block and registers it there before changing dispatch; inactive/rolled-back
candidates retain roots while process-local rollback/reactivation is available.

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

## 8. Lifecycle algorithms

### Open candidate (pure Birnam policy)

1. Hash the finished artifact, copy/open it under its immutable content path, and
   read the manifest without running candidate code.
2. Reject unsupported manifest version, platform/ABI, integrity, or unsafe source.
3. Register/verify dependencies through `Module`; load dependencies normally.
4. Materialize the CompiledUnit and binding-operation facts as `#candidate`.
5. Do not change dispatch or `#active` state.

Opening the same `(name, artifactChecksum, platform, ABI)` is idempotent. Reusing an
authoring path with different bytes necessarily produces a different content path
and mapping.

### Validate

1. Every target name resolves in Globals.
2. Every target already resolves to a stable owned/family vtable; validation never
   calls `ensurePrivateVtable`.
3. Selectors and operation pairs are unique.
4. Explicit removals name direct bindings.
5. The current active generation fact names the expected base **and** every actual
   direct binding pointer/presence matches the expected base's process-local binding
   table. Facts alone are insufficient: ordinary/raw mechanisms or live edits can
   change dispatch without changing lifecycle state. A mismatch requires explicit
   adoption of the live bindings as a new baseline or rejection as stale.
6. The native mechanical validator agrees before any mutation.

### Activate

1. Revalidate both the lifecycle fact and actual direct binding base, plus the
   complete operation set.
2. Run candidate private preparation and capture old direct pointers/presence.
3. Apply each install/remove through Store's tracked MOP.
4. Record portable method operations in the `__methods__` recovery stream.
5. Only after the entire non-failing commit, set this unit `#active`, mark the prior
   one `#superseded`, assert `#supersedes`, and append an activation event.

The runtime is single-threaded today. After complete prevalidation, the ordinary
commit path uses the native Store MOP operations and contains no recoverable
candidate/user-code branch; allocation failure remains fatal as elsewhere. Thus no
*concurrent ordinary send* observes a half-generation. An adversarial override of
the MOP itself is an explicit escape from this guarantee, just like raw floor
mutation. A future concurrent runtime must put step 3 under its
single-transactor/dispatch mutation lock rather than treating the sequential loop
as intrinsically atomic.

### Roll back

For every operation in reverse order:

- candidate replaced a direct binding → tracked-replace the captured old pointer;
- candidate added a direct binding → tracked-remove it, revealing inheritance;
- candidate removed a direct binding → tracked-add the captured old pointer.

Then append the portable inverse operation to the recovery stream, mark the
candidate `#rolledBack`, restore its predecessor to `#active`, and record the event.
Rollback is idempotent only in its declared active→rolledBack transition; attempting
it from any other state is a kinded lifecycle error.

## 9. Portable method-operation history

Process-local pointer restoration is insufficient: an image saved after rollback
must rebuild the rolled-back behavior, not the last candidate definition it saw.
Legacy `__methods__` remains the pre-lifecycle base stream. New activity appends to
an explicit `__methodops__` **method-operation log**:

- install/replace appends the complete `Reflect defineMethod...` source;
- removal appends a complete `Reflect removeMethod...` source;
- rollback of a replacement/removal appends the predecessor's definition source;
- rollback of an addition appends removal source.

Each operation carries a monotonic sequence derived from the Store basis and uses a
unique sequence-bearing key. Replay and `compileImageCode:` evaluate the legacy
base first, then sort `__methodops__` by the explicit numeric sequence; they do not
rely on `attrAt:`'s current first-appearance implementation. Later operations win
exactly as live activation did. The sequence contract is tested across image
save/boot and is part of the format's recovery semantics.

Managed unit records retain the portable source for each binding; rollback is
rejected unless every inverse has portable source, even if a raw pointer happens to
remain callable. This prevents a live state that cannot honestly be saved/rebuilt.

Adopting a pre-lifecycle binding into a rollbackable baseline therefore requires a
known recorded definition source. Primitive bindings may be replaced live, but the
unit is not rollbackable across image/rebuild unless their prior implementation has
a portable recovery unit. The capability flag states this honestly.

## 10. Image behavior and composed authority

No image **format** change is required for the first slice, but a small image
introspection seam is: while loading `.code` records, Image computes the ordered
composition identity of their checksums and exposes it after code resolution. It
does not mint lifecycle objects or choose states.

The existing persistence pieces remain:

- CompiledUnit/binding/event facts serialize as ordinary durable data.
- The method-operation log makes `compileImageCode:` capture the behavior current at
  save time, including removals and rollbacks.
- A normal compatible boot maps the compacted image unit as primary executable code.
- `--rebuild` replays the same ordered method operations.

Raw rollback pointers, candidate roots, and dylib handles do not survive. Before any
user entry point/REPL send, pure-Birnam CompiledUnit reconciliation reads the mapped
composition identity, mints a reflected **composed executable generation**, relates
each saved-origin generation with `#composedInto`, marks those origins historical,
and reserves `#active` for the composed unit that actually owns dispatch. A rebuild
similarly mints a recovered composition from the normalized operation identity.

This boot-time reconciliation happens before the image append checkpoint, so it is
not journaled as a user mutation. Historical units answer
`rollbackAvailable: false` unless their immutable artifacts are explicitly reopened
and actual live bindings validate in this process. Preserving several rollbackable
native generations inside an image is a later format feature, not implied.

## 11. Source/binary equivalence

There is one candidate path:

```
source → Codegen candidate compile → candidate artifact ┐
                                                        ├→ manifest/open/activate
prebuilt candidate artifact ────────────────────────────┘
```

Pure source evaluation remains the ordinary immediate live-edit path; asking for
candidate semantics necessarily compiles first. The semantic distinction is
immediate edit vs staged generation, never source vs dylib delivery.

## 12. Implementation slices (each ends green)

1. **MOP removal law.** Raw id removal + Store tracked removal + Reflect spelling;
   direct/inherited behavior, mirror honesty, history, backfill tests, and tracked
   user-method lowering for ordinary artifacts.
2. **Candidate manifest + immutable open.** Codegen emits/reads pointer-free
   metadata, hashes/copies to a content path, and rejects unsafe source; ordinary
   artifact load remains behavior-compatible.
3. **Roots + activation.** Codegen's candidate-root provider; stable-owned-target
   validation; add/replace/remove in one candidate; broken/stale/actual-base
   validation changes nothing; facts/events become queryable.
4. **Rollback.** Exact inverse including removal of additions and restoration of
   inherited behavior; repeated/invalid transitions error visibly.
5. **Sequenced portable operations + images.** Save/boot and `--rebuild` after
   activation/rollback reproduce current behavior; mapped composition identity
   mints the honest active composed generation; rollback availability reconciles.
6. **Module/Config integration.** Path-based candidate open; no automatic reload or
   watcher. Existing ordinary `Module load:` stays unchanged.

## 13. Verification matrix

- id: remove direct binding; missing is no-op; inherited binding reappears; later
  parent redefinition is visible (proves no copied-parent tombstone).
- Store: tracked remove retracts object and `#primitive` mirrors; history retains
  prior binding; raw change leaves absent, never stale.
- Codegen: manifest inspection has zero dispatch/global effects; invalid ABI,
  mutable-path replacement, missing/unstable target, duplicate binding, lifecycle-
  stale base, actual-pointer-stale base, and missing removal reject before mutation;
  GC during active/rolled-back candidate states preserves exactly the rooted blocks.
- Lifecycle: candidate adds + replaces + removes; activation changes all behavior;
  rollback restores all behavior and unit states/history.
- Delivery: candidate compiled from source and the same prebuilt artifact have the
  same manifest and effects.
- Image: compatible boot and `--rebuild`, saved both active and rolled back, agree.
- Image authority: mapped/recovered composition becomes `#active`; origin units are
  historical/`#composedInto`; no stale rollback capability survives boot.
- Regression: `zig build verify`; existing normal module/source/artifact loading is
  unchanged; benchmark confirms send cost unchanged.

## 14. Non-goals and deferrals

- No automatic file watching or recompilation daemon.
- No arbitrary-module transaction or external-effect compensation.
- No safe dylib unloading; old code remains mapped while anything may reach it.
- No cross-process rollback guarantee in the first image slice.
- No store-driven dispatch or generic trampoline on the send hot path.
- No new IdApi fields and no native unit-policy registry.
- No compiler self-hosting dependency.

## 15. Remaining open questions

1. Is the candidate-safe source subset useful enough with add/replace/delete and
   exact rollback, given the deliberate stable-owned-target requirement?
2. Should primitive baselines be categorically non-portable for rollback, or can a
   named kernel/module generation provide their recovery identity without storing
   pointers?
3. What is the canonical compatibility identifier for the candidate ABI: the
   existing image runtime-shim ABI hash, a narrower candidate ABI version, or both?
4. Should inactive candidate roots be explicitly releasable after the user forfeits
   rollback, or merely retained until process exit in the first slice?
