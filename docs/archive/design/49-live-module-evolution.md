# 49 — Live Module Evolution: Source Revisions and Dispatch Generations

> **Status:** Withdrawn, 2026-07-17. The L1b/L2 policy layer was removed after
> implementation review. This document is retained as design history; the
> current runtime keeps the lower-level `CompiledUnit`/Codegen mechanism and
> ordinary blocking isolates, but does not ship `LiveModules`, file-backed live
> rollout, worker replacement, or cross-request evolution policy.
> Accepted with corrections by the
> [combined adversarial review](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/48a-live-evolution-isolate-combined-adversarial-review.md).
> Rev. 3, accepted by the second
> [adversarial review](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/49b-live-module-evolution-second-adversarial-review.md),
> replaced per-module active-generation state with one active
> dispatch generation per runtime, narrowed the first authoring workflow to
> file-backed source revisions and atomic full-image save, and made
> reactor/isolate safe points
> an explicit boundary. Rev. 4 splits that work around the blocking isolate
> substrate as L1a, L1b, and L2. Initialization, image-native source patching,
> migration, and append persistence are deferred.
>
> Builds on [40 — The Image](40-the-image.md),
> [43 — Modules and the Self-Hosted Layer](43-modules-and-config.md),
> [45 — Compiled Units Are Primary](../../design/45-compiled-units.md),
> [47 — CompiledUnit Lifecycle](../../design/47-compiled-unit-lifecycle.md), and
> [48 — Concurrency: Reactors, Isolates, and Structured Operations](48-concurrency-reactors-and-isolates.md).
> The first review is retained as
> [49a — Adversarial Review: Live Module Evolution](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/49a-live-module-evolution-adversarial-review.md).

## 1. The question

Birnam has an implemented `CompiledUnit` lifecycle for inspecting, mapping,
validating, activating, and rolling back a native dispatch delta. The next useful
step is smaller than arbitrary module reload:

```text
edit a candidate-safe module file
→ prepare an inert candidate against the current runtime generation
→ inspect it or request immediate activation
→ activate it at a runtime safe point
→ keep the running heap and application state
→ save a new image explicitly, or roll back while the capability remains valid
```

This design answers how source and dispatch authority move together. It does not
make initializer effects, migration, I/O, FFI, suspended work, or a pool of
isolates part of the dispatch transaction.

## 2. Governing simplification

**One Birnam runtime has one active dispatch generation.**

Modules remain the units of source ownership, dependencies, and candidate deltas.
They are not independently active executable owners inside one runtime. Activating
one module revision produces a successor generation for the whole runtime:

```text
generation G7
  sources: #ledger → revision 8
           #http   → revision 3
           #app    → revision 12

activate #ledger revision 9

generation G8
  predecessor: G7
  sources: #ledger → revision 9
           #http   → revision 3
           #app    → revision 12
  dispatch delta: ledger operations only
```

The successor need not copy every native binding. Its predecessor chain and
operation ledger may represent the effective dispatch mechanically. Its source map
is nevertheless the complete authority for the runtime generation.

This choice deliberately avoids per-module active-state facets. Such facets would
need independent lifecycle state, native ledgers, rollback chains, image
composition rules, and multi-module conflict handling. The current single-threaded
runtime and the reactor design both execute one effective dispatch generation at a
time, while isolates each own a separate runtime and generation.

This is an intentional revision to design 47's implemented per-name
`activeCompiledUnit` publication policy, not a claim about current behavior. The
candidate envelope, mapping, binding ledger, tracked-MOP commit, drift detection,
and exact inverse remain useful mechanism. `activeFor: M` may survive as a
compatibility query, but its answer must be derived from the one current generation
rather than stored as independently mutable lifecycle authority for `M`.

## 3. Three roles

The design has three conceptual roles. They may share implementation machinery,
but their authority must remain distinct.

### 3.1 SourceRevision

A `SourceRevision` is immutable authored input for one canonical module. It
records:

```text
canonical Module name
content-derived revision identity
optional parent revision identity
complete authored source text
parsed dependency and module metadata
origin, such as a file path
```

It does not record mutable activation state or a singular materializing
generation. One revision may be staged repeatedly and may appear in several saved
or process-local generations.

Revision identity covers the complete source and canonical metadata. The prepare
and commit paths revalidate the content hash; ordinary Store mutation must not be
able to change the meaning of an already named revision.

### 3.2 Candidate

A candidate is an inert proposed dispatch delta:

```text
candidate module
candidate SourceRevision
expected predecessor DispatchGeneration
artifact and source hashes
dependencies
ordered method operations
process-local mapping and rollback material
```

Opening and inspection do not map code, load dependencies, mutate dispatch, or
publish source authority. Dependency preparation remains an explicit ordinary
module-loading effect. Mapping remains an explicit trusted-native boundary.

### 3.3 DispatchGeneration

A `DispatchGeneration` is the effective executable owner for one runtime. It has:

```text
generation identity
predecessor generation, if any
complete module → SourceRevision map
dispatch delta and portable operation provenance
image-composition provenance, when recovered from an image
current process-local rollback capability, if any
```

The complete module-to-revision map is itself a sealed, content-addressed value.
Its digest is part of the generation's reproducible identity and is revalidated by
prepare and commit. Publishing a generation never exposes a mutable map whose
entries can later be replaced through ordinary Store operations. If a raw/private
mechanism nevertheless changes a published association, reconciliation marks the
generation drifted just as it would for an untracked binding change.

Generation object identity remains runtime-local. A generation also exposes a
value-like **base digest** covering its sealed source map and portable dispatch
provenance. Digests may be compared across runtimes; generation objects, numeric
allocation order, and rollback chains may not.

The digest is canonically computable. All integer lengths below are unsigned
big-endian; names are their UTF-8 spelling; hashes are raw 32-byte values. A source
revision hash is:

```text
SHA-256(
  "birnam.source-revision.v1\0"
  || u32(module-name length) || module-name
  || u32(sorted dependency count)
  || each dependency as u32(length) || spelling
  || u64(authored-source length) || exact authored-source bytes
)
```

Parent revision and origin path are lineage/diagnostic metadata and are excluded.
The base digest is `SHA-256("birnam.dispatch-base.v1\0" || u32(source-map count) ||
canonical source-map records || u32(operation count) || canonical
portable-operation records)`. Source-map records are sorted by module-name bytes
and encode the length-prefixed name plus its 32-byte revision hash. Portable
operations are ordered by dependency-topological module order with module-name
bytes as the tie-breaker, then by their explicit `operationSequence`. Each
operation encodes length-prefixed module, target, and selector spellings; one
action byte (`1` install, `2` remove); and either
`SHA-256("birnam.definition.v1\0" || exact portable definition-source bytes)` or 32
zero bytes for removal.

Native artifact checksums, image-composition checksums, local generation numbers,
pointers, mapping paths, predecessor/rollback chains, and Store entity IDs are
excluded. Mapped and rebuilt recovery must retain or reconstruct the same canonical
portable records. Digests render as lowercase hexadecimal only at text/wire
boundaries; hashing always uses the binary encoding above.

Exactly one Store cell identifies the current generation conceptually:

```text
#dispatch activeGeneration G8
```

`#active`, `#historical`, and rollback capability apply to whole dispatch
generations, never independently to each module name.

## 4. Source authority

Birnam separates three authorities:

- **Live dispatch** is the current runtime behavior.
- **The current generation's source map** is the reproducible source authority for
  that behavior.
- **A source file** is an editable representation of a revision. It may be current,
  ahead, stale, or divergent.

Active source is derived through the one current generation:

```text
active source for M
= [[DispatchGeneration current] sourceRevisionFor: M]
```

There is no separately written active-source cell and no revision-level active
flag. Before commit, the candidate already contains the verified replacement
revision and a complete successor source map. Commit changes dispatch and the
single current-generation cell at the same non-failing seam. Rollback restores the
predecessor generation and therefore its complete source map without another
source publication.

If live dispatch drifts outside the managed lifecycle, the generation is no longer
a trustworthy source authority. The system reports drift rather than continuing to
claim that its source map reproduces live behavior.

### 4.1 Canonical identity

Project-managed source requires a canonical `Module named:` declaration. Candidate
name, revision metadata, and embedded artifact identity must match it. File and
artifact paths are origins only. Renaming a file does not rename a module; module
rename is a later explicit identity transition.

Standalone scripts and ad hoc patches may keep looser naming rules outside this
workflow.

### 4.2 Module metadata follows active source

Dependencies and other declarative module metadata are parsed into the
`SourceRevision`. Queries about the current module graph derive from the current
generation's source map. Activation must not leave `Module`, `__modsrc__`, or image
rebuild ordering on predecessor metadata while the source map names a successor.

Image capture and rebuild therefore consume the current generation's canonical
source map rather than treating legacy `__modsrc__` enumeration as an independent
authority. Legacy facts may remain recovery input during a format transition, but
they do not compete with the current generation.

Whenever module programs are compiled or replayed, the canonical dependency graph
is ordered deterministically with the module manager first, then dependencies, then
dependents. Missing dependencies and cycles are errors. Neither Store enumeration
nor source-map iteration order has execution semantics.

## 5. File-backed revision workflow

L1b's first authoring workflow is deliberately file-backed and candidate-safe:

```text
fileOut current revision
→ edit with an ordinary editor
→ import as a SourceRevision
→ prepare a Candidate
→ inspect and activate, or request immediate activation
```

`fileOut` records or embeds a base-revision identity. Import compares that base
with the current revision for the canonical module:

- matching base: prepare normally;
- identical content: idempotent import;
- stale or sibling base: report divergence;
- missing base: require an explicit forced-replacement policy.

A file import never changes recorded active source merely because parsing or
evaluation began. Rejected source remains an inactive revision or an error, not a
partial `__modsrc__` update.

The accepted document subset is the implemented candidate-safe shape: one required
canonical `Module named:requires:` declaration plus declarative method additions,
replacements, and removals over already-established stable targets. Unsupported
top-level forms reject the revision. The evolution layer never extracts only the
methods from a more general module while publishing the complete document as
though its top-level effects had occurred.

Authored source is retained whole so comments, ordering, formatting, declarations,
and non-method text survive `fileOut`. Lossless mutation of that document through a
reflective editor is not part of this slice.

## 6. Preparation and activation

Immediate and staged use drive the same lifecycle.

### 6.1 Prepare

```text
parse and hash the SourceRevision
→ compile candidate-safe operations
→ open and verify the pointer-free envelope
→ record the expected current DispatchGeneration
→ inspect operations, dependencies, and divergence
→ optionally load dependencies explicitly
→ map and mechanically validate
→ produce a prepared one-shot capability
```

Preparation does not publish dispatch or source authority. It may have explicit
dependency-loading effects when the caller requests them; rejection and rollback
do not claim to undo those effects.

Inspection exposes at least:

- module and revision identity;
- expected and current dispatch generations;
- additions, replacements, and removals;
- dependency changes and missing dependencies;
- source divergence;
- whether mapping has occurred; and
- whether exact process-local rollback will be available.

### 6.2 Commit at a safe point

Commit is a small runtime operation:

1. Require the expected generation still to be current.
2. Revalidate revision/artifact hashes, target identities, vtables, direct
   bindings, and the complete publication plan.
3. Apply the dispatch delta through Store's tracked MOP.
4. Publish the successor as the one current dispatch generation.
5. Consume the prepared capability.

No ordinary send may observe a half-generation. Today the runtime is
single-threaded and the native commit path is non-failing after its final
validation. Future scheduling does not change that transaction; it controls when
the transaction may run.

L1a exposes a synchronous commit only at an explicit quiescent top-level boundary,
such as a tool or REPL command after the previous evaluation has returned. It does
not invent a queue, promise, scheduler, or general asynchronous submission API.
“Immediate activation” in L1a means prepare and commit within that top-level
operation after final validation.

This is enforced, not conventional. The native runtime owns a process-local phase
guard with at least `#running` and `#quiescentEvolution` states. Ordinary Birnam
code cannot mint or enter the latter phase, and the guard is never durable. A
shell/tool host enters it only after top-level evaluation returns; the isolate
driver enters it only between request-handler calls; a future reactor enters it
only between turns. Commit and rollback require the guard, reject nesting or
re-entry, and restore `#running` on every exit. The existing unrestricted
`CompiledUnit activate` and `rollback` entry points become bootstrap-private
mechanism or acquire the same guard as part of L1a.

Later execution substrates decide when they satisfy the same quiescence
precondition. I1a's blocking mailbox loop provides the first non-top-level example:
L2 processes an evolution control request between ordinary requests, commits before
reading the next request, and returns a blocking receipt. A future reactor may call
the same commit between turns. No operation claims atomic activation across several
isolates.

### 6.3 Rollback

Rollback is another safe-point operation. While its process-local capability is
valid, it verifies that the candidate generation is still current and that every
touched binding still has the recorded post-activation state. It then applies the
inverse delta and republishes the predecessor generation.

Rollback restores dispatch and source authority. It does not undo dependency
loading, application state changes, I/O, FFI, messages already sent, or work
already completed by reactor turns or isolates.

## 7. Images and durability

### 7.1 Legacy-authority cutover

L1a includes one explicit upgrade from today's per-name `activeCompiledUnit` and
`__modsrc__` authority; it is not deferred representation work:

1. reconcile every legacy active unit against actual dispatch and reject drift;
2. normalize shipped durable modules to declared identities, then parse legacy
   module sources, require canonical identities and complete source, and validate
   the dependency graph;
3. deterministically order that graph and derive the complete sealed source map,
   base digest, and portable operation provenance;
4. construct one initial, non-rollbackable composed `DispatchGeneration`;
5. publish the single current-generation cell only after all prior steps succeed,
   and switch module queries, image compilation, and replay to that authority; and
6. thereafter treat `activeCompiledUnit` and `__modsrc__` only as compatibility
   mirrors derived from the current generation, never as inputs that compete with
   it.

The cutover and the code paths that consume the new authority ship in the same L1a
slice. Failure publishes no current-generation cell, mutates no legacy mirrors,
and refuses L1a activation or a new-format save; an image already marked as cut
over rejects boot if its sealed map cannot be validated. Compatibility mirrors
are updated at the generation commit seam or regenerated from it, and mirror drift
is reported rather than read back into authority.

Undeclared filed scripts are not silently promoted to modules by basename. They may
execute normally, but durable state requiring whole-program replay must acquire an
explicit module owner before cutover/save; a legacy undeclared `__modsrc__` entry is
a migration diagnostic, not guessed identity.

The first reliable persistence path is atomic full-image save.

Before save:

1. reconcile the current generation against actual dispatch;
2. reject unlogged non-portable drift;
3. capture the effective portable operation stream;
4. persist the complete canonical module-to-revision map; and
5. write through the existing temporary-file/rename path.

If full save reports failure, the running generation remains active and the prior
image remains intact through temporary-file replacement. This is not yet a claim
of power-loss durability: the current writer does not fsync both file and parent
directory before rename. The UI may report that live state is newer than the last
successful save; this is derived durability status, not another generation state.

On mapped or rebuilt boot, image code is the executable owner. Boot mints one
non-rollbackable composed `DispatchGeneration`, attaches the saved source map,
marks saved process-local origins historical, rebuilds the binding ledger, and
publishes that generation before user entry or a reactor turn.

The next module activation produces a whole-runtime successor of that composition.
All module queries therefore see the successor generation, even though its native
delta touched only one module. A second module can then activate against that same
successor without depending on a globally superseded shared base.

`appendImage:` is excluded from the first durability guarantee. The current append
path writes directly to the live image and does not have the full save's atomic
replacement semantics. It may be reconsidered with transactional framing or
tail-recovery rules.

## 8. Concurrency and isolation contract

This design does not implement concurrency, but it preserves the boundaries in
design 48.

### 8.1 One generation per runtime

A reactor preserves one heap, Store, and dispatch generation. Its queue may contain
an activation request, but a running turn completes under the generation with which
it began. Later turns resolve sends through the successor.

Long-lived scheduled behavior should be represented by receiver and selector where
possible. A retained native block may capture code from its creation generation and
is not silently upgraded.

### 8.2 Isolates evolve independently

Each isolate owns its heap, Store, compiler, current generation, and rollback
capability. Transferable rollout data is deliberately value-like:

- canonical module name;
- source-revision identity or source bytes;
- artifact checksum and content-addressed artifact location;
- expected base digest; and
- explicit health-check or rollout policy.

Mapped handles, Oops, blocks, native pointers, prepared tokens, and rollback
snapshots do not cross the isolate seam.

The receiving worker compares the digest with its own current generation, then
binds preparation to that runtime-local generation object. It returns a value-like
receipt containing the accepted source/base digest and outcome. A coordinator
never sends its own generation object or assumes that equal local generation
numbers mean equal runtime state.

A later pool policy may use rolling replacement: start and check a new pool, route
new work, drain the old pool, then stop it. I1b proves only the single-worker
version of that handoff. A stateful worker may instead activate at a mailbox safe
point. The coordinator records per-worker results; it does not claim one atomic
generation across the pool.

### 8.3 Durable descriptions are not in-flight work

Images may retain desired worker generations, rollout intent, durable job IDs, and
recovery descriptions. They do not retain running turns, mailbox receives, pending
replies, open channels, or partially executed callbacks. Live evolution must not
turn process-local execution into apparently durable state.

## 9. Explicit deferrals

The following are useful directions, but are not prerequisites for the first
file-backed workflow.

### 9.1 Standard initialization

The initial slice requires stable targets to have been established already through
ordinary module loading or bootstrap. It adds no initializer declaration, durable
attempt state machine, or exactly-once claim.

A later initialization design may provide an at-least-once, idempotent/resumable
hook with explicit persistence and recovery rules. It should be informed by the
same durable-operation model used for reactor jobs and isolate recovery rather than
inventing a second partial task system inside `CompiledUnit`.

### 9.2 Image-native source editing and `adoptLive`

Source-first editing without files remains a goal, but lossless document patching,
definition ownership, spans, ambiguous patch ownership, and source-associated
`adoptLive` are deferred together.

The existing low-level `adoptLive` may remain a dispatch-ledger escape hatch, but
the project workflow must not claim source authority for its result. A later
source-aware adoption must verify and attach the complete successor source map
before publishing the adopted generation; it must not publish first and associate
source afterward.

### 9.3 Migration

Migration remains optional and orthogonal. The first evolution module does not run
migrations, gate application rollback, or claim that old methods remain compatible
with changed state.

A later migration design belongs with durable operation identity, recovery mode,
idempotence/resumption, and per-isolate rollout policy. Native dispatch rollback
will remain mechanically narrower than application rollback.

### 9.4 Multi-module candidates

One candidate changes one module revision in the first slice. A later batch may
produce one successor generation containing several validated deltas, but no such
atomicity is implied now.

### 9.5 Module rename

Canonical module rename must eventually define revision lineage, dependency edges,
initialization/migration identity, and image membership. File rename alone remains
an origin change and is not module rename.

## 10. Project-level evolution module

One deep Birnam module should coordinate policy over the existing mechanisms. It
owns:

- source-revision import and export;
- content identity and base/divergence checks;
- complete successor source-map construction;
- candidate retention and inspection;
- immediate versus staged top-level activation;
- proof that synchronous commit is entered at a quiescent runtime boundary;
- explicit full-image save; and
- cleanup of abandoned candidate artifacts and transient records.

It does not duplicate:

- source parsing or native compilation from Codegen;
- Store-arbitrated dispatch mutation;
- dependency loading from `Module`;
- image encoding from Image/System;
- reactor scheduling or isolate transport; or
- migration policy.

The module depends on a narrow synchronous commit boundary rather than on a
particular scheduler. L1a invokes it only at a top-level quiescent point. L2 later
invokes it from the worker's blocking control-message path between requests; a
future reactor may invoke it between turns. Queueing and asynchronous completion
belong to those execution substrates, not to the L1a transaction API.

## 11. Live-evolution bodies of work

### 11.1 L1a — generation authority and image coherence

L1a proves the executable authority model before adding file UX or scheduling:

```text
inert candidate against expected generation
→ synchronous commit at a quiescent top-level boundary
→ whole-runtime successor generation
→ exact process-local rollback
→ full-image save and mapped/rebuilt recovery
```

It includes:

1. immutable, content-addressed `SourceRevision` values and sealed source-map/base
   digests;
2. the atomic legacy-authority cutover and one current-generation cell per runtime;
3. candidate preparation and stale-base rejection against that whole-runtime base;
4. one non-failing commit that publishes dispatch and successor authority together;
5. exact rollback to the predecessor while the local capability remains valid;
6. module graph and deterministic dependency order derived from the source map;
7. mapped/rebuilt recovery into one composed generation; and
8. atomic full-image persistence only.

L1a's public commit is synchronous and top-level. It has no queue, promise,
reactor, mailbox, or asynchronous completion abstraction. Tests may construct
candidate-safe revisions directly; file import/export is L1b.

### 11.2 L1b — file-backed revision and image-baseline workflow

After I1a has proven the actual process/message boundary, L1b adds the authoring
workflow:

- required canonical `Module named:requires:` identity;
- candidate-safe whole documents and rejection of unsupported top-level forms;
- exact file import/export of authored source;
- revision/base identity, idempotent import, and divergence detection;
- original dependency-error propagation;
- staged inspection and immediate top-level activation over the L1a transaction;
  and
- production of full saved image baselines suitable for I1b replacement.

L1b does not add source-aware `adoptLive`, initialization, migration, file watching,
or asynchronous activation.

### 11.3 L2 — blocking mailbox-safe live rollout

L2 bridges the proven live-generation and blocking-isolate substrates without
changing either into an asynchronous framework. The coordinator sends one blocking
control request containing:

- canonical module and source-revision identity or source bytes;
- artifact checksum and content-addressed location;
- expected value-like base digest; and
- the requested operation: prepare-and-activate or rollback.

The worker handles that control frame between ordinary requests. It compares the
digest with its local current generation, prepares and validates locally, commits
synchronously before reading another request, and returns a value-like receipt with
the prior/current digest and success or kinded failure. Optional rollback is a later
blocking control request while that worker's local capability remains valid.

L2 assigns wire-v1's reserved frame kinds 9 and 10 to `evolutionRequest` and
`evolutionReply`. It registers three positional record schemas:

- `birnam.evolution-activate.v1`: module Symbol, revision digest String, source
  String-or-nil, artifact checksum String, artifact location String, expected base
  digest String;
- `birnam.evolution-rollback.v1`: expected current digest String; and
- `birnam.evolution-receipt.v1`: operation Symbol, module Symbol-or-nil, accepted
  base digest String, prior digest String, current digest String, outcome Symbol,
  error kind Symbol-or-nil, and message String.

Digests use canonical lowercase hexadecimal text. Rollback means “roll back the
locally retained current generation if its digest equals this expected digest”; it
does not export a capability token or assume that local generation numbers match.

Prepared tokens, generation objects, Oops, mapped handles, and rollback snapshots
never cross the isolate seam. L2 remains single-worker and single-flight. It does
not claim pool-wide atomicity, concurrent rollout, automatic retry, or asynchronous
completion.

All three bodies explicitly exclude initialization, reflective document editing,
project-level source-aware `adoptLive`, migration, append persistence, file
watching, multi-module atomic activation, and cross-isolate atomicity.

## 12. Interleaved roadmap with isolates

The architecture work alternates at the dependency boundaries:

```text
L1a — generation authority and image coherence
→ I1a — blocking single-worker isolate substrate
→ L1b — file-backed revision and image-baseline workflow
→ I1b — single-worker rolling replacement
→ L2  — blocking mailbox-safe live rollout
→ choose later features from evidence
```

I1a owns clean worker boot, bounded wire-v1 encode/decode copying, opaque
process-local ownership, one blocking request at a time, crash/EOF observation, and
bounded stop/terminate/reap. It boots a static image and has no replacement or live
candidate policy.

I1b starts B while idle A remains selected, waits for transport-ready, performs one
bounded application health request, switches one coordinator-held worker reference
between calls, then stops and reaps A. Failure leaves A selected. Pools and busy
draining remain later policy.

Only after L2 should the project choose among standard initialization,
image-native source editing, migration, pools, richer supervision, or structured
operations. The reactor remains orthogonal: it may be implemented earlier for an
I/O-driven product, but neither I1a nor L2 depends on it. A future reactor invokes
the same L1a commit between turns rather than changing the generation transaction.

The executable breakdown is
[50 — Five-Phase Implementation Plan](50-five-phase-live-modules-and-isolates-implementation-plan.md).
It fixes the first representations, current code seams, test matrix, and exit gate
for each arrow above.

## 13. Verification requirements

### 13.1 L1a — generation authority and image coherence

Tests must prove:

- an invalid immediate load leaves dispatch and the current generation unchanged;
- opening and inspection map no code and publish no source;
- revision content is rehashed at prepare and commit;
- published source maps are sealed, their digest participates in generation
  identity, and raw association drift is detected;
- activation rejects a stale expected whole-runtime generation;
- one commit atomically publishes dispatch and the successor source map;
- rollback restores the exact predecessor dispatch and source map;
- commit and rollback reject ordinary `#running` execution, nested evolution, and
  forged or durable phase state;
- failed legacy cutover leaves the old authority untouched and cannot publish or
  save a partial new contract;
- successful cutover makes module queries, compilation, replay, and compatibility
  mirrors derive from the one sealed generation;
- boot creates one composed generation for several modules;
- activating module A over a composition leaves module B represented by the new
  whole-runtime successor;
- module B can then activate against that successor;
- two successive full-image save/boot cycles preserve the complete source map;
- image rebuild derives module order and source from the current generation rather
  than Store enumeration;
- mapped and rebuilt module programs run in deterministic dependency-topological
  order with the module manager first, and reject missing/cyclic graphs;
- commit is callable only from an explicit quiescent top-level boundary and L1a
  exposes no queue, promise, callback, or asynchronous submission surface;
- persistence failure leaves live authority unchanged and the prior full image
  intact; and
- `appendImage:` is not advertised through the reliable persistence workflow.

### 13.2 L1b — file-backed revision and image-baseline workflow

Tests must prove:

- candidate name, declared module name, revision metadata, and artifact identity
  match;
- a document containing unsupported top-level forms is rejected rather than
  partially activated;
- identical import is idempotent and stale/sibling import reports divergence;
- rejected import changes neither current source nor legacy module-source facts;
- authored file output preserves untouched text and comments;
- dependency preparation returns the original `Module load:` error; and
- a saved baseline boots with the same complete source-map and generation digests
  under both mapped and rebuilt recovery.

### 13.3 L2 — blocking mailbox-safe live rollout

Implemented integration coverage runs under `zig build test-isolate-rollout` and
the complete `zig build verify` gate. It composes the codec tests, candidate
validation suite, generation/image tests, and repeated real-process rollout path.

Tests must prove:

- rollout transfers only wire-v1 values and rejects Oops, generation objects,
  mapped handles, prepared tokens, and rollback snapshots;
- the worker compares the value-like base digest and binds preparation to its own
  runtime-local current generation;
- a control request is processed only between ordinary requests, and the next
  ordinary request observes the successor generation;
- the receipt contains the accepted base, prior/current digest, and a kinded
  success or failure result;
- a later blocking rollback request restores that worker's predecessor while its
  local capability remains valid;
- two workers may accept different generations without either claiming global
  atomicity; and
- invalid source, missing artifact, checksum mismatch, map/ABI failure, stale or
  intervening base, second rollback, and lost post-boot capability leave the
  worker's dispatch deterministic;
- a queued evolution frame is read only after an executing application request
  replies, malformed evolution records terminate as protocol failures, and a
  prepared candidate whose binding drifts is rejected before publication; and
- L2 exposes no promise, multi-flight, pool-wide commit, retry, or concurrent
  rollout semantics.

The named matrix is implemented by `src/id/isolate_test.zig` (wire records,
receipt validation, malformed frame, and mailbox order), `src/id/codegen_test.zig`
(candidate integrity and commit-time drift), and
`scripts/isolate-rollout-smoke.sh` (real worker failures, independence,
persistence, rollback lifetime, and intervening generation). The documented
recipe is itself gated by `zig build test-live-feature-examples`.

## 14. Implementation choices

[Plan 50](50-five-phase-live-modules-and-isolates-implementation-plan.md#3-representation-choices-fixed-by-this-plan)
fixes the first implementation choices:

1. `#dispatch/#activeGeneration` is the one current-generation cell.
2. `DispatchGeneration` is a small distinct policy object; `CompiledUnit` remains
   candidate delta and provenance.
3. `SourceRevision` and the sorted source map are immutable Vector records whose
   canonical hashes are revalidated at every authority boundary.
4. A file-backed revision carries its base digest in the first-line
   `// birnam-live-base sha256:...` envelope comment; the remaining authored bytes
   are exact source.
5. `activeCompiledUnit` and `__modsrc__` are refreshed eagerly as compatibility
   mirrors, while all new readers derive from the generation.

These are implementation choices, not new invariants. They may be replaced later
without reopening one-active-generation-per-runtime or the copied isolate seam.
