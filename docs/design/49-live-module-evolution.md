# 49 — Live Module Evolution: Source Revisions, Initialization, and Migration

> **Status:** Exploration, 2026-07-13, rev. 2 after
> [adversarial review](49a-live-module-evolution-adversarial-review.md). This
> document extends the implemented `CompiledUnit` lifecycle into a project-level
> development model. Rev. 2 corrects initializer ordering, source publication,
> canonical identity, image composition, migration rollback, persistence
> terminology, and implementation slicing. It records the current direction, not
> a final interface or implementation plan.
>
> Builds on [40 — The Image](40-the-image.md),
> [43 — Modules and the Self-Hosted Layer](43-modules-and-config.md),
> [45 — Compiled Units Are Primary](45-compiled-units.md), and
> [47 — CompiledUnit Lifecycle](47-compiled-unit-lifecycle.md).

## 1. The question

Birnam now has a first-class `CompiledUnit` lifecycle for compiling, inspecting,
mapping, validating, activating, superseding, and rolling back native dispatch
generations. The remaining question is how that mechanism becomes the normal
development workflow for a real project.

The intended experience is:

```text
edit module source
→ prepare a candidate generation
→ inspect or immediately activate it
→ keep the running objects and application state
→ test the new behavior
→ explicitly persist it, or roll back
```

This workflow must support ordinary source files without making file-based
development second-class. It must also preserve image-native editing: code may be
changed reflectively inside the running system and written to a file later.

The design must distinguish exact dispatch rollback from initialization,
migration, I/O, FFI, and other effects that cannot honestly participate in the
same transaction.

## 2. Existing mechanism

The hard runtime pieces already exist:

- candidate source compiles to a relocatable native artifact and pointer-free
  envelope;
- opening verifies the source, artifact, checksum, platform, ABI, dependencies,
  and declarative method operations without mapping code;
- dependencies are prepared explicitly through ordinary module loading;
- mapping is an explicit trusted-native step that does not publish dispatch;
- validation checks the exact live base;
- activation atomically publishes additions, replacements, and removals for one
  named generation;
- rollback restores the exact predecessor while the process-local capability
  remains valid;
- ordinary method edits produce observable drift and may be adopted when their
  source is portable; and
- images compose the active behavior and retain portable source for recovery.

The missing module is project-level policy: source revision ownership, file
import/export, initialization, candidate presentation, migration, and explicit
persistence.

## 3. Design direction

The leading direction rests on these decisions:

1. Files and image-native editing produce revisions of the same first-class module
   source. Neither silently overwrites the other.
2. Immediate and staged activation are two policies over the same `CompiledUnit`
   lifecycle.
3. One source module may contain both initialization and reloadable definitions;
   it does not need to be split into two physical modules.
4. Migration is optional and orthogonal. A separate migration module may be
   invoked from initialization or from the live-evolution workflow, but migration
   is not part of the dispatch transaction.
5. The declared `Module named:` symbol is the canonical identity for a
   project-managed module. File and artifact paths are origins only.
6. A prepared compiled unit carries immutable source revisions keyed by canonical
   module name. Active source is derived through `activeCompiledUnit`; it is not a
   separately published module cell.
7. Normal image-native editing is source-first. Immediate reflective mutation is
   an escape hatch that requires ownership-aware `adoptLive`.
8. Module-level programs execute dependency-first during fresh load, rebuild, and
   recovery.
9. Starting migration closes ordinary application rollback before effects begin,
   though native dispatch rollback remains an explicitly unsafe mechanism.
10. Activation controls live authority; `saveImage:` and `appendImage:` control
    durability. There is no first-class acceptance state in the initial design.

## 4. Source authority: revisions, not files versus images

Choosing either the filesystem or the image as the sole source of truth would
discard an important use case.

- If files are uniquely authoritative, reflective editing becomes a temporary
  mutation that is easy to lose and difficult to compose with images.
- If the image is uniquely authoritative, ordinary editors, version control,
  build tools, diffs, and source-oriented project habits become awkward.

Birnam should instead separate three authorities:

- **Live dispatch is authoritative for current behavior.**
- **The active source revision is authoritative for rebuilding that behavior.**
- **A source file is an editable representation of a revision.** It may be
  current, ahead, stale, or divergent.

### 4.1 Revision model

Each module source revision should record enough information to answer:

```text
module name
revision identity
parent revision
complete authored source text
definition provenance and spans
materializing compiled-unit generation or image composition
source/artifact identity
activation state
origin, such as file import or live edit
```

Revision identity must be stable and content-derived or otherwise unambiguous. A
file imported from revision 7 and a live edit made from revision 7 create sibling
revisions rather than silently pretending to be a linear sequence:

```text
revision 7 ── file edit ───── revision 8a
          └─ live edit ───── revision 8b
```

The first implementation need not merge these revisions automatically. It must
detect and present the divergence. Explicit replacement remains possible, but
silent clobbering does not.

### 4.2 File-oriented workflow

```text
fileOut active revision
→ edit with an ordinary editor
→ fileIn as a candidate revision
→ inspect and activate, or immediately activate
```

`fileOut` records or embeds enough base-revision information for a later `fileIn`
to detect whether the live module advanced independently. The exact representation
may be module metadata in the source, a sidecar, or project metadata; that remains
open.

### 4.3 Image-oriented workflow

```text
edit the stored module document
→ create a candidate source revision
→ inspect or immediately activate it
→ optionally fileOut the new active revision
```

This source-first path is the normal image-native editor behavior. It uses no
filesystem but creates the revision before dispatch changes.

Immediate `Reflect` mutation remains an escape hatch. It changes dispatch first,
marks the managed generation drifted, and must follow this recovery path:

```text
identify or explicitly choose the owning module/document
→ patch or reconstruct portable source
→ create a revision
→ adoptLive
→ associate the adopted baseline with that revision
```

Installed definitions therefore need ownership provenance: module revision,
document identity, span or node identity, target, selector, and operation kind.
Target and selector alone are insufficient because several modules may
intentionally patch the same binding. Raw native drift may remain observable
provenance but cannot become a reproducible source revision.

### 4.4 Preserve authored source

The image must retain the complete authored module document, not only independent
method strings. Reconstructing a file solely from method facts would discard:

- comments and prose;
- deliberate ordering;
- formatting;
- initialization and module metadata;
- helper declarations; and
- source that is not itself a method definition.

Definition provenance supplies the update mechanism:

- replacing a method patches its source span;
- removing a method removes its span;
- adding a method inserts it in a predictable place; and
- when no authored document exists, `fileOut` produces a canonical module.

Exact preservation under overlapping or stale edits is a later merge problem. The
first requirement is honest divergence detection and deterministic output.

### 4.5 Canonical identity and atomic source authority

For project-managed source, `Module named:` is required and canonical. The
candidate unit name and embedded artifact metadata must match it. Source filenames
and artifact paths are recorded as origins, and renaming a file does not rename the
module. A module rename is an explicit identity transition. Standalone scripts and
ad hoc patches may retain looser naming rules outside this workflow.

Before activation, a candidate carries an immutable source-revision association
whose content hash matches the candidate source hash. Active source is derived,
not separately written:

```text
active source for M
= [[CompiledUnit activeFor: M] sourceRevisionFor: M]
```

Activation already atomically changes `activeCompiledUnit`; source authority
therefore changes at the same commit seam. Rollback republishes the predecessor
unit and restores its revision association without a second Store write.

An ordinary candidate usually carries one module/revision entry. A composed image
generation carries a module-indexed map:

```text
image composition
  sourceRevisionFor: #ledger → ledger revision 8
  sourceRevisionFor: #http   → http revision 3
  sourceRevisionFor: #app    → app revision 12
```

Saved origins become historical as they do today; the composition copies their
active revision associations before taking executable ownership. A later module
generation may supersede the composition for only its name, while other module
names continue to resolve through the composition.

## 5. Immediate and staged activation

Birnam should support both a convenience path and an inspection path.

### 5.1 Immediate path

```text
source
→ compile
→ open and verify
→ prepare dependencies according to policy
→ map
→ validate
→ activate
```

The operation stops at the first error and leaves dispatch unchanged. It answers
the resulting `CompiledUnit` or a detailed error, so a successful generation can
still be inspected and rolled back afterward.

Immediate activation must not be a separate implementation with weaker checks. It
is a policy that drives the staged lifecycle to completion.

Dependency preparation propagates the original `Module load:` error and stops at
the failing dependency. Dependencies loaded earlier in the attempt remain loaded:
preparation is ordinary, non-transactional module loading. The candidate remains
inspectable, with already-loaded and still-missing dependencies visible.

### 5.2 Staged path

```text
compile and open
→ inspect source, operations, dependencies, and live base
→ optionally prepare dependencies
→ map
→ validate
→ activate at an explicit safe point
```

Inspection should expose at least:

- module and candidate revision identity;
- parent and current active revisions;
- additions, replacements, and removals;
- dependency changes and missing dependencies;
- the live generation the candidate expects;
- whether initialization or migration is implicated;
- whether rollback will be available after activation; and
- any incompatibility or divergence report.

### 5.3 Later policies

The same lifecycle may later support:

- activate immediately when valid;
- activate, run checks, and roll back on failure;
- prepare a candidate now and activate it at an application safe point; and
- activate several dependent units under an explicitly designed batch policy.

The current atomicity guarantee is one compiled unit. Multi-unit atomic activation
is a separate native and lifecycle design problem, not implied by this document.

## 6. Why arbitrary module reload is not transactional

A normal module may do much more than define methods:

```birnam
[Globals at: #Server put: [Object clone]];
[db assert: #config attr: #port value: 8080];
[Alien dlopen: "libthing.dylib"];
[Server startListening];

[Reflect defineMethod: #handle: on: Server as: ...];
```

Repeating this source on reload may:

- replace the stable `Server` identity while live objects still reference it;
- duplicate or overwrite state;
- acquire another foreign-library handle;
- open a second socket or fail because the first remains open; and
- perform an external effect that method rollback cannot undo.

The current candidate subset is deliberately declarative because method additions,
replacements, and removals admit an exact dispatch transaction. This does not mean
one user-facing module must become two modules. It means one source module has
different semantic phases.

## 7. Standard module initialization

A module declaration may carry a standard initializer hook:

```birnam
[Module named: #ledger
        requires: #[#file]
      initialize: [|
        [[Globals includes: #Ledger]
          ifFalse: [| [Globals at: #Ledger put: [Object clone]]]]]];
```

The syntax is illustrative. The important properties are explicitness,
inspectability before effects, and a durable record of initialization attempts.
Supporting the form is an explicit parser/Codegen slice: the current candidate
subset accepts only `Module named:requires:` plus method operations.

The hook is bootstrap-only. It may use floor protocol and already-initialized
dependencies to establish stable named targets and seed state. It must not call
methods supplied by its own candidate generation. If installation needs behavior
from the new generation, that behavior belongs to a later explicit hook with a
different failure contract, not to bootstrap initialization.

### 7.1 Initial installation

```text
read and verify the module declaration
→ compile and inspect the candidate without effects
→ load dependencies
→ if uninitialized, record an attempt and run the bootstrap initializer
→ record initialized or initializationFailed
→ map and validate
→ activate
```

Initialization is ordinary effectful code. It is not part of the dispatch
transaction and has at-least-once semantics for external effects. Its state is
independent of compiled-unit activity:

```text
#uninitialized
#initializing
#initialized
#initializationFailed
```

Each attempt records its source revision, identity, start basis, result, and
failure details. If the hook fails after producing effects, `Module` records the
failure and does not pretend those effects were rolled back. Retry is explicit;
hooks must be idempotent or resumable.

An initializer may succeed before mapping or validation later fails. The resulting
state—initialized stable targets with no active generation—is legitimate. A
corrected candidate may target those objects without rerunning initialization.

### 7.2 Reload

```text
read changed source
→ observe durable initialized marker
→ do not rerun initializer
→ compile the method candidate against stable targets
→ inspect or activate
```

Changing the initializer in a later source revision does not silently rerun it.
State evolution belongs to migration or another explicit effect.

### 7.3 Dependency order, images, and rebuild

The initialized marker is durable across the image lineage. Normal image boot and
`--rebuild` must not rerun initialization merely because compiled methods are being
mapped or replayed.

Every execution of module-level programs is dependency-ordered: fresh source load,
compiled module load, rebuild, appended-image reconstruction, and future recovery.
The module manager runs first, followed by dependencies and then dependents. Cycles
remain errors rather than acquiring special image semantics.

Conceptually, rebuilt boot does:

```text
bring up Module
→ restore durable facts and initialization markers
→ reconstruct the canonical module graph
→ topologically order module programs
→ evaluate declarations dependency-first, skipping completed initializers
→ replay or compile method generations
→ run explicit recovery hooks dependency-first
→ establish the composed executable generation
```

An entirely fresh installation has no marker and runs the initializer. Importing
an existing image restores the marker and established objects. A legacy image
missing a marker initializes dependency-first under an explicit compatibility
policy.

## 8. Migration as an orthogonal module

Migration should be optional and implemented as a separate module rather than as
hidden behavior inside `CompiledUnit`.

Illustratively:

```birnam
[Migration for: #ledger
           from: 1
             to: 2
             do: [|
               [Ledger migrateNames]]];

[Migration bring: #ledger to: 2]
```

The migration module may own:

- current state/schema version by module;
- ordered migration discovery;
- applied migration records;
- source and generation identity for each migration;
- failure reports; and
- retry or resumption policy.

### 8.1 Invocation from initialization

A module initializer may ask the migration module to establish the expected state:

```birnam
[Module named: #ledger
        requires: #[#migration]
      initialize: [|
        [[Globals includes: #Ledger]
          ifFalse: [| [Globals at: #Ledger put: [Object clone]]]];
        [Migration bring: #ledger to: 2]]];
```

This lets a fresh installation and an old installation use the same versioned
state-evolution logic where appropriate. Alternatively, an initializer may create
the latest state directly and record its version. The migration module should not
force one construction strategy. Any migration invoked before first activation
must live in already-loaded dependency protocol and must not depend on the pending
module generation.

### 8.2 Invocation from live evolution

The project-level evolution module may invoke migration after trial activation:

```text
activate method generation
→ run non-migrating checks while application rollback is safe
→ explicitly choose migration
→ record migration intent and persist it
→ mark application rollback unsafe
→ run optional migration
→ record and persist success or failure
```

`Module` need not understand migration internals. It supplies the initialization
hook and module identity; `Migration` owns version transitions.

### 8.3 Transactional honesty

The migration module must not claim more atomicity than its effects possess. A
failed migration may have partially changed facts or the outside world. Migrations
should normally be:

- idempotent;
- resumable;
- small;
- explicit about external effects; and
- recorded as complete only after their bodies succeed.

If the Store later grows a suitable transaction for a migration's fact changes,
that adapter may strengthen the guarantee. It still cannot automatically undo
files, sockets, foreign processes, or arbitrary FFI effects.

### 8.4 Migration and rollback

Dispatch rollback and application rollback are different capabilities:

```text
activate new methods
→ migrate live state
→ restore old methods
```

The old methods may not understand the migrated state. `CompiledUnit` can still
mechanically restore their bindings, but doing so may not restore a valid
application.

Before migration begins, both native dispatch rollback and application rollback may
be available. Before invoking any migration body, the evolution module durably
records `#migrationInProgress` and marks ordinary application rollback unsafe. The
native dispatch capability remains observable only as an explicitly unsafe expert
mechanism.

If persistence of the intent fails, migration does not start. If migration fails,
the new methods remain active and boot enters migration recovery or maintenance
mode rather than the normal application entry point. Recovery may resume, retry,
repair, or invoke an explicit reverse migration.

Application rollback becomes safe again only after a suitable reverse migration
succeeds, or under a separately justified compatibility guarantee. No automatic
migration runs during activation.

## 9. Recovery is not initialization

Initialization establishes durable module identities and state. It does not cover
runtime-only resources that must be recreated in every process, such as:

- listening sockets;
- child processes;
- reactor registrations;
- open files;
- foreign handles; and
- callbacks into external libraries.

Eventually `Module` may need a separate recovery hook:

```text
initialize — once per image lineage
recover    — once per process boot
migrate    — explicitly between state versions
```

Recovery should be added only when a concrete module requires it. The initial
design must nevertheless avoid defining `initialize` so broadly that it secretly
means both durable construction and per-process resource recovery.

## 10. Project-level evolution module

One deep birnam module should coordinate the complete workflow while leaving the
existing mechanisms in their present owners. Its implementation is expected to be
almost entirely birnam over `CompiledUnit`, `Module`, Store, File, and System.

It owns policy for:

- importing and exporting module source revisions;
- detecting stale or divergent bases;
- associating source documents with compiled generations;
- creating and retaining candidates;
- presenting operation and dependency summaries;
- choosing immediate or staged activation;
- running optional project checks;
- coordinating explicit migration;
- exposing whether active state is newer than durable image state;
- coordinating explicit `saveImage:` or `appendImage:` operations; and
- cleaning abandoned candidate artifacts and transient records.

It does not duplicate:

- candidate validation or native mapping from Codegen;
- dispatch generation policy from `CompiledUnit`;
- dependency loading from `Module`;
- migration policy from `Migration`; or
- image encoding from Image/System.

If this module were deleted, the lifecycle would remain usable at its lower level,
but callers would again need to reproduce the ordering, divergence, durability,
and failure policy. That is the complexity the module exists to hide.

### 10.1 Activation is not persistence

There is no first-class `#accepted` transition in the initial design. Activation
makes a revision authoritative for current behavior. `saveImage:` or
`appendImage:` makes current behavior durable. `imageCheckpoint` remains internal
reconstruction machinery and is not a persistence operation.

If persistence fails after activation:

- the generation and its source revision remain active;
- rollback availability is unchanged;
- the prior image remains intact under atomic-save rules; and
- the workflow reports that live state is newer than durable state.

A convenience “activate and persist” policy is a two-step operation whose result
must represent partial success. It never reports a persistence failure as though
activation had not occurred, and it does not automatically roll back successful
activation.

## 11. Implementation slices

The work is split so each slice has one dominant failure model. Every slice remains
explicit and file-watcher free.

### 11.1 Slice A — file-based revision lifecycle

Use a module whose stable targets are already initialized. Implement:

- canonical declared module identity;
- complete source-revision entities;
- immutable revision association on prepared candidates;
- exact file import/export of whole authored documents;
- base-revision and divergence detection;
- immediate and staged activation;
- dependency-error propagation;
- rollback restoring source authority;
- module-indexed source associations in image compositions; and
- mapped and rebuilt image survival.

Do not include initialization, reflective editing, source-span patching, or
migration. This slice proves:

```text
file revision ↔ candidate ↔ live generation ↔ image
```

### 11.2 Slice B — module initialization

Add initializer declaration parsing, compile-before-effect ordering, the
bootstrap-only contract, initialization attempt states, explicit retry,
dependency-ordered module programs, and image persistence of initialization
status.

This slice proves that one module file can establish stable targets once per image
lineage and then activate replaceable generations repeatedly.

### 11.3 Slice C — image-native source editing

Add definition ownership, persistent source spans or node identities, source-first
image editing, lossless document patching, ownership-aware `adoptLive`, ambiguous
owner handling, and canonical output where no authored document exists.

This slice proves that files and the image are peer editing media without making
raw reflective drift the normal path.

### 11.4 Slice D — migration

Add versioned migration registration, durable intent and completion records,
application rollback gating, recovery/maintenance boot behavior, optional reverse
migrations, and explicit persistence semantics.

## 12. Verification requirements

Tests should exercise the interfaces callers actually use:

- an invalid immediate load leaves behavior and active revision unchanged;
- staged inspection maps no code before the explicit map step and publishes no
  methods before activation;
- candidate name, declared module name, and embedded artifact identity must match;
- duplicate import of identical content is idempotent;
- an imported file based on a stale revision reports divergence;
- a forced replacement of a stale or unknown base is explicit;
- a rejected import never changes recorded module source;
- file output preserves untouched source text and comments;
- activation and rollback atomically select the source revision through the active
  compiled unit;
- composed-image boot reassociates every active canonical module with its source
  revision;
- dependency preparation returns the original module-load error while leaving the
  candidate inspectable;
- initialization is at-least-once and idempotent across every body/marker failure
  window;
- initialization does not call candidate methods and may remain complete after
  later candidate failure;
- a completed initializer does not rerun on reload, mapped boot, rebuild, or
  appended-image reconstruction;
- an initialization failure is visible and never reported as rolled back;
- module programs execute dependency-first even when Store enumeration is reversed;
- activation and exact rollback preserve application objects and facts;
- active behavior and source survive mapped and rebuilt image boot;
- source-first image edits activate through the candidate lifecycle;
- immediate reflective edits require an unambiguous owner and `adoptLive`;
- migration intent persists before the body and disables application rollback;
- migration interruption at every intent/body/completion window enters recovery;
- method rollback after migration requires explicit compatibility or a reverse
  migration;
- persistence failure after activation leaves live authority and rollback intact;
  and
- changed initializer source never reruns an already initialized module.

## 13. Open decisions

1. The exact representation of source-revision identity in ordinary files.
2. Whether the complete source document is stored directly as a revision fact or
   through a content-addressed source object.
3. The initial policy for patching live method edits into authored source while
   preserving formatting.
4. The exact `Module` initializer declaration and how its source is inspected
   before candidate target validation.
5. Whether migration definitions live beside their module source, in separate
   files, or merely register with a generic `Migration` module regardless of file
   layout.
6. When a separate recovery hook becomes necessary and how it composes with image
   boot.
7. Whether active revisions form a strictly linear history while inactive
   candidates may branch.
8. How dependent multi-unit revisions are prepared and eventually activated
   without overstating the current single-unit atomicity guarantee.
9. Whether source-revision maps on composed generations are Store facts directly
   or reflected through per-module facet objects.
10. The exact durable representation and recovery interface for migration intent.
