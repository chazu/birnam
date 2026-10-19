# Store-independent core: completion plan

2026-10-19. Baseline `9e51daee` (qualified native-runtime setup committed).
**Status: Globals cleanup/early ownership and owner image payloads implemented and qualified. Task/resource fields, optional fact persistence and production composition remain Store-dependent.**
The next release is blocked on completing this milestone, per the user's decision.
This refines the [optional Store decision](optional-store-and-relational.md).

## Completion contract

A clean core must bootstrap from source, compile and run programs, reflect on and
replace definitions, collect memory, run tasks, and save/restore images without
loading Store or Relational. The ordinary Host, project, console, socket and worker
paths must not quietly reintroduce them. Source and compiled-image startup are
separate acceptance paths. Removing names after boot is not evidence of independence.

The distribution can ship optional libraries. Explicitly loading Store/Relational,
or a tool which declares those dependencies, must still work after core startup.
Preserve their self-describing bootstrap, explicit facts/history, queries and
opt-in code descriptions. Merely shipping the modules must not activate their
services or enroll ordinary definitions in fact storage.

The recommended default is core-first startup, with explicit optional-module
loading. No fake Store, permanently empty database, hidden fact service, new
universal runtime registry, or foreign layout access is an acceptable substitute.
All owner interoperability uses current Id messages. Preserve ordinary replacement,
identity/aliasing, GC roots, entered/suspended/recovery code ownership, and the
selected partial-failure contracts. No monkeypatch guards belong in authored code.

Store independence and full self-hosting are distinct milestones. Remaining Rust
artifact admission/mapping and optional certification still need eventual migration,
but the audit found no direct Store/Relational calls in Generation's production
sources. Prove those operations work without Store; do not require their wholesale
rewrite just because they are native. The Relational rewrite remains paused. New
image-buffer/dispatch optimizations and a generic live relational mirror are also
outside this completion sequence.

## Remaining dependency ledger

These are source-backed blockers or intentional optional consumers, not raw grep
counts. Comments, test assertions, TOML example keys named `db`, and CandidatePolicy's
ordinary `cellFor:` helper are not Store dependencies.

| Area | Current edge and evidence | Required destination / proof |
| --- | --- | --- |
| Bootstrap bindings | [authored Globals](../../modules/globals-owner.birnam) now constructs its own facade and state before SourceRegistry/named modules. Store's facade, seed conversion and fact cleanup are removed. | The real compiled initializer passes on a floor without Store/Relational, followed by explicit late loading without owner replacement. Slice qualification passes; full Bootstrap absence still awaits other owners. |
| Task and scheduler state | [stackless-control](../../modules/stackless-control.birnam) initializes Store schema and uses `db cellFor:` for TaskControl/TaskExecutor fields. Candidate policy and other ordinary modules use its transitions. | Ordinary per-object state with preserved field aliases, queue order, cancellation, shielding, transitions and suspension. Remove schema initialization and fact-backed field ownership. |
| Workers and outcomes | The same module stores Worker fields and RemoteFailure outcomes in facts; [isolate policy](../../modules/isolate.birnam) stores Concurrency fields in db and marks them transient. | Owned state and explicit resource lifecycle; worker preparation, copied messages, failure reporting, cancellation and cleanup run without Store. |
| REPL and sockets | [REPL](../../modules/repl.birnam) stores `cin`, `cerr`, `cbuf` plus transient declarations; [socket](../../modules/socket.birnam) stores connection descriptors/buffers in db. These are included in prepared startup. | Resource-bearing ordinary owners; preserve acquisition, close-once behavior, failed construction, repeated sessions and buffer safety. Define reconstruction/rejection before moving fields. |
| Image resource policy | [Graph::finish](../../rust/birnam-image/src/native.rs) now sends current `imagePayload`; Globals projection and task-handle rejection use this owner protocol. TaskControl's process-local marker is an ordinary slot. Task/resource field omission still uses Store schema until those fields move. | Owner-level image projection/admission through Id. Keep raw handles and captured process-local capabilities out of durable images, independently of facts. Ordinary persistent state must not be dropped to make an image save. |
| Alien wrappers | [Alien runtime](../../rust/birnam-alien/src/runtime.rs) keeps recipes under `__aliens__`, reuses code by reading those facts, and publishes wrapper Methods through Store. | Imageable owned recipe data with discoverable inspection/editing; native addresses/handles remain process-local. Construct/publish through the existing Method/dictionary owners. Preserve wrapper identity, signature distinctions, live replacement and Unit lifetime. |
| Alien restoration | [Image::foreign_publication](../../rust/birnam-image/src/native.rs) finds recipes by inspecting staged `__aliens__` facts. | Restore from the actual recipe owner via Id; resolve foreign addresses in the destination process and preserve partial-load failure reporting. |
| Image facts and Store Cells | Image unconditionally reads Store basis/retention metadata, exports facts, creates a staging Store, swaps Store state, and special-cases Store-owned CellVt. [ImageFormat](../../modules/image-format.birnam) admits that locator family. | Heap/code/source/Globals transport works with no fact participant. Store opts into transporting its own facts/history and Cells through a bounded owner protocol. Keep explicit facts and references intact; empty fact sections alone are not independence. |
| Standard-library optional hooks | [stdlib](../../modules/stdlib.birnam) directly writes Doc's facts from documented method definitions and provides `Reflect factsOf:` through global db. | Ordinary Reflect construction/descriptions remain independent. Put documentation writes behind the selected documentation owner and fact reflection behind explicit Store use. Decide and document behavior when those facilities are absent; do not fabricate an empty fact database. |
| Intentional documentation catalog | [Doc](../../modules/doc.birnam) and [NativeDoc](../../modules/nativedoc.birnam) deliberately store/query documentation. Doc declares only html; [Cli](../../modules/cli.birnam) requires Doc globally, and its LSP path populates the catalog. | Keep the useful relational catalog as an explicit dependency. Basic CLI commands and LSP static analysis must not acquire it accidentally. Loading documentation features may explicitly load Store/Relational. Do not convert the catalog merely to eliminate textual db references. |
| Composition and packaging | [Bootstrap](../../rust/birnam-bootstrap/src/lib.rs) loads store/relational unconditionally; [Host source startup](../../rust/birnam-host-entry/src/startup.rs) marks both as kernel modules. [Build](../../rust/birnam-build/src/main.rs) has one canonical native closure, builds startup with REPL/socket state, then loads all tools into a second image. | Core and optional dependency closures are explicit in build inputs, initialization, prepared images and receipts. A tooling image containing Doc is an explicitly optional composition, not the only way to run unrelated tools. |
| Late optional initialization | Store no longer publishes a Globals seed; explicit loading after the actual Globals initializer passes. Relational attaches its query behavior to Store. Full-world late optional activation still needs qualification. | Load after live globals/methods/tasks exist without replacing core authority, changing local Cell defaults or installing mandatory mutation hooks. Preserve self-description, repeat-load behavior and existing explicit data. |

The primary references above contain the live code at the baseline. Existing
[Cell](../../rust/birnam-siphon/tests/cell_without_store.rs) and
[Globals](../../rust/birnam-siphon/tests/globals_without_store.rs) floor tests remain
useful component evidence, but explicitly exclude the full bootstrap/Host path.

## Simplification review: delete before translating

2026-10-19 follow-up, against `5cbb91fa`. These candidates come from source
and call-site inspection. Row status and the progress entries below distinguish
completed work from remaining recommendations; deletion alone is not a speedup. The
migration should shrink the machinery rather than reproduce its fact-shaped APIs
in new private maps.

| Priority | Candidate | Evidence and recommended reduction | Boundary to retain |
| --- | --- | --- | --- |
| Completed `b1fe7200` | Globals compensation banks and image recovery checkpoints | `reserveBindingCompensationFor:count:`, credit queries/releases and conditional restoration remain in authored Globals and its native seed. Current candidate policy no longer calls them; a Generation regression explicitly checks that activation makes zero reservation calls. `imageBindingCheckpoint` / `restoreImageBindings:` have no current production callers in the repository. Image still calls `releaseImageReservations` to clear otherwise obsolete banks. Remove this capability family, its state slot/order snapshots, and its cleanup hook unless a concrete current requirement is selected. Update tests and the old ownership documentation accordingly. | Ordinary binding coherence, absent versus nil, alias order and identity; explicit candidate undo already has its own selected-write protocol. This deliberately removes public APIs, not their names alone. |
| Completed; see qualification below | Dual Globals implementations and seed conversion | Store creates a native Globals facade; authored Globals imports its state, changes behavior, retracts automatic facts and compacts. Aim for one authored binding owner through bootstrap. Delete the Store-owned facade and transfer/cleanup path once any genuinely necessary minimal bootstrap interface is established. This also removes the late-Store-load clobber hazard. | Globals and Namespace have distinct responsibilities; do not merge them merely to remove a seed. Keep public binding behavior and early compilation working. |
| With Cell/image cleanup | Duplicate local-Cell implementation inside Store | `Store localCell:` and `localCell:prototype:` still maintain tagged native local values alongside the authored Cell owner. No core Birnam module calls them; remaining uses are examples, fixtures and the image-format test generator. Convert those local scratch callers to authored Cell, then remove the native local variant and its legacy image branch. | Keep actual fact-backed Store Cells and their explicit history. Preserve local value/alias/clone behavior through the authored owner; do not keep a second native representation solely for old-image compatibility. |
| With task/resource migration | Database-shaped field access | Task/Worker/Concurrency fields repeatedly manufacture prefixed attribute Symbols, obtain fact-backed Cells, then read/write their values. They also maintain schema/history/transient declarations for ordinary private state. Replace that layer with ordinary owned fields/collections; retain actual Cells where field handles escape and aliases require shared mutation. | Queue order, transitions, cancellation/shielding, field alias semantics and resource cleanup. A field migration is not permission to simplify the scheduler's semantics. |
| With Alien migration | Fact encoding and Store-mediated method installation | Recipes are encoded as `__aliens__` attributes with synthetic `__alien_N` keys, enumerated back into vectors, searched, and converted again for Image. Wrapper installation also detours through Store's method-publication API. Use one owned recipe collection and the established Method/dictionary publication protocols. | Function/signature code reuse and target/selector registration are distinct matching purposes. Preserve both, current reads across reentrant callbacks, native-handle ownership and Method/Unit lifetimes. |
| With Image migration | Mandatory database staging and global transient schema | Even a fact-free image currently queries Store metadata, creates a staging Store and swaps Store state. Every heap object gets a Store transient-marker query. Remove these from the core path; integrate projection/admission into actual owner snapshot protocols and keep facts/history as an explicit optional contribution. | Reject unsupported resources and captured transient handles; preserve explicit facts/history and the selected partial-application contract. Staging optional fact data can still be useful—only its mandatory role is redundant. |
| With tool split | Optional catalog knowledge embedded in core code | stdlib's documented-method helper knows Doc's fact layout, while Cli requires Doc for unrelated commands. Forward documentation to its owner and load the catalog only for features requiring it. Use existing module dependencies/precompiled artifacts; avoid a second dependency planner. | Doc's relational catalog is intentional useful data. Keep that facility, its explicit APIs and self-description rather than rewriting it to erase every `db` occurrence. |

In particular, do not replace these paths with a universal persistence registry,
new compensation engine, mandatory live mirror, or a permanent native owner for
what can be ordinary Birnam data. Select the smallest owner-message protocol that
covers the actual resource and optional-data cases. Automatic rollback and policing
of arbitrary replaced setters are outside the chosen contract.

The strongest first deletion is the Globals bank/checkpoint family, paired with
the seed work in slice 1. Keep `imageBindingProjection` and its transient-binding
policy: unlike the abandoned rollback checkpoints, Image actively uses that
projection to exclude process-local values without changing live bindings.
Likewise, necessary ownership, failure propagation and code retirement must survive
all these removals. Their cost is not evidence that they are redundant.

## Ordered implementation slices

Each slice should have its own implementation, focused negative/positive tests,
updated ledger and commit. Keep the existing supported composition working while
removing dependencies. The final release gate cannot pass on partial progress.

1. **Establish the end-to-end absence gate and free bootstrap Globals.** Add a
   reproducible current failure using a genuinely excluded Store/Relational input
   closure. Specify how the available core objects establish initial Globals before
   early compiled artifacts need it; prefer authored construction over a new native
   service. Remove the seed-import fact cleanup requirement. Prepare safe late
   Store initialization so the seed cannot be reintroduced when Store is loaded.
   Exit: core early initialization and ordinary binding operations need neither
   library; existing Globals identity/reflection and explicit facts tests still pass.
   The full absence harness remains a failing diagnostic until the later owners
   are cut over; passing this early checkpoint is only component evidence.
2. **Separate persistence policy from Store, then migrate task state.** Specify the
   smallest owner-level projection/admission contract needed for process-local
   capabilities. Reuse existing Image/Globals/owner protocols where possible;
   do not build a general transactional framework. Move TaskControl/TaskExecutor
   fields to owned objects, preserving exposed Cell/field aliases deliberately.
   Exit: tasks, transitions, wait/resume, cancellation/shielding, error propagation,
   precise GC and quiescent-image behavior pass without Store. A retained transient
   handle must not become a seemingly valid empty restored object.
3. **Migrate the rest of the resource owners.** Move Worker, Concurrency and
   RemoteFailure state, then REPL/connection fields. Apply the persistence contract
   chosen in slice 2. Exit: fresh/closed/failed resources, repeated console/socket
   sessions, subprocess workers, cancellation, failed startup and shutdown pass;
   local state is collectible and saved images do not contain process addresses.
4. **Give Alien recipes an ordinary owner.** Separate durable recipe/code/target
   references from native library handles and addresses. Replace Store-mediated
   publication and Image's fact-table recipe discovery with owner messages.
   Exit: compile/load/reuse/replacement, cloned/retained recipes, fresh-process
   restoration, missing libraries/symbols, callback failures and code reclamation
   preserve the chosen behavior. No opaque native recipe registry replaces db.
5. **Complete Store-free Image and optional Store persistence.** Retire Store's
   duplicate native local-Cell variants after moving the remaining examples and
   fixtures to the authored Cell owner; keep real fact-backed Cells. Remove mandatory
   Store stamps, staging and swaps from the heap/code path. Keep fact/history/Cell
   transport as an explicit Store contribution. Choose the concrete protocol and
   any image version/rebuild transition before editing the format; reject unsupported
   data rather than silently omit it. Images using optional data must declare their
   required providers and load them explicitly or reject a missing provider; a core
   image must not discover a database by probing ambient global bindings.
   Exit: mapped/rebuilt/resaved core images work
   with no Store/Relational; optional explicit facts/history and Store Cells still
   roundtrip. Failed live application retains effects and reports partial load;
   recovery remains an explicit user snapshot, without resurrected capabilities.
6. **Finish optional loading and tool boundaries.** Complete late Store/Relational
   initialization, move incidental stdlib documentation writes to Doc, locate
   explicit fact reflection with its provider, and declare real catalog dependencies.
   Split CLI/tool compositions so basic commands do not load Doc just to reach
   unrelated commands. Exit: explicit optional load works in an already-used core,
   self-description and queries work, and ordinary operations never auto-create facts
   before or after that load. LSP analysis remains usable without its fact catalog.
7. **Cut over production composition and release acceptance.** Remove mandatory
   loads/kernel registrations and update installer inventories, bootstrap/tool
   images, install manifests, relocation/archive checks, test setup and release
   receipts together. Promote the absence gate from diagnostic failure to a required
   positive release gate; qualify the optional path as well. Update current design
   status and consolidate release notes around the final contracts. Only after these
   proofs pass should a release candidate be prepared and qualified once through
   the existing cold, resumable release workflow.

Slices 2 and 5 share one persistence contract: design that boundary before migrating
resource fields, then finish its integration when the participating owners are
ready. Removing the two native load entries first would only hide missing owners.

## Required acceptance matrix

The test runner must use the actual production paths. It must not substitute a
mock Store, export selected methods into a special test VM and call that full
startup, or load the libraries and merely erase their public names afterwards.

| Path | Required evidence |
| --- | --- |
| Cold core build | Build from source without prebuilt Birnam artifacts/images and without Store/Relational in the bootstrap input closure. Building optional distributable modules separately is allowed. |
| Source bootstrap | Ordinary core bindings, local Cells, SourceRegistry, compiler, task transitions and method construction work with both optional libraries unavailable. |
| Compiled startup | Restore a core image with the parser and compiler disabled; show Store/Relational were not loaded. Missing required core artifacts fail honestly. |
| Live language | Compile/run, Reflect enumerate/read/edit, custom lookup and replacement, aliasing, nil versus absence, failures, GC, suspension and recovery preserve their contracts without Store. |
| Host and tools | Source/project check-build-run, console, socket and workers run without facts; basic LSP analysis works. Documentation commands explicitly activate their declared optional dependencies. |
| Core persistence | Save in one process and restore in a fresh core; cover mapped/rebuilt/resaved images, aliases, code lifetime and rejection/projection of process-local resources. |
| Optional facilities | Load Store, then Relational, after core state exists. Verify their self-description, queries, deliberate history, code-description snapshots, repeat loading and persistence of supported explicit data. |
| Independence after opt-in | Existing Globals/local Cells/Methods remain the same owners. Ordinary method edits require no Store authorization or mandatory observer; explicit facts do not become writable aliases for core fields. |
| Failure and retention | Early failures preserve pre-application authority, later failures retain applied effects, callbacks are not replayed, and unused state/code is reclaimed after actual owners release it. |
| Distribution | Both core-only and opt-in compositions pass clean-source, installed, relocated and archive acceptance. No ambient installed prefix supplies the missing libraries behind the test. |

Use Lean for concrete aliasing/identity/state-equivalence claims and TLA+ for the
new resource/persistence transitions where needed. Include negative models and
runtime fault/GC/reentry tests. Existing formal receipts are not proof of this new
composition. Record startup/steady-state CPU and retained/peak memory when claiming
savings, without changing the GC threshold or weakening ownership to improve a score.

## Decisions to close during the relevant slice

- **Early Globals:** establish the concrete cold construction order and minimal
  interface, preserving the current Globals/Namespace distinction. No default
  fact-backed adapter may remain merely for bootstrap convenience.
- **Image ownership:** settle contribution selection, transient object/field
  behavior, graph references, failure boundary and format transition. The contract
  must remain ordinary owner messaging; a permanent Rust authority table is not
  an acceptable stand-in for Store.
- **Independent Stores:** the current runtime rejects an image capturing `Store new`,
  while a control world saves successfully (rechecked 2026-10-19). This is a separate
  optional persistence capability, not evidence that core independence requires
  implementing every Store feature. Preserve and regression-test truthful rejection
  unless per-instance persistence is deliberately included and qualified in slice 5;
  do not silently discard the instance's facts or claim every Store is imageable.
- **Optional tools:** distinguish packaged availability from activation. The default
  core and unrelated commands stay independent; catalog-dependent features explicitly
  select Store/Relational. Update declarations and saved-image contents, not only docs.

## Start here

The next design check is Image's Store-owned transient policy, so task/resource
field moves cannot accidentally make native capabilities durable. The early
Globals owner and initializer floor are implemented; qualification is recorded
below. `scripts/core-without-store-test.py PREFIX` copies a real installed prefix
with Store/Relational excluded and runs early bootstrap and full source startup.
It deliberately returns failure while the production composition still requires
those libraries. It does not stand in for the separate cold-build, image and Host
acceptance matrix.


## Progress: Globals recovery cleanup (2026-10-19)

The first deletion slice is implemented and qualified. Authored Globals now keeps
four state fields; compensation banks, saved inverse ordering and image checkpoint/
rollback selectors are removed, together with the native seed adapters and Image's
reservation-cleanup call. Binding identity/alias order, nil/absence, coherent state
publication and transient projection remain. Explicit candidate undo and Store's
own optional fact-compensation API still pass their tests.

The Globals-specific Lean/TLA+ model now describes writes/removal and image
preservation without banks. Qualification passes 388 Lean theorems, 122 rejected
mutations, 265 expected TLC outcomes, 847 language assertions, native binding/image/
Store/candidate tests, the authored Globals absent-Store floor, and mapped/rebuilt
owner images. An old Globals image fixture still expected automatic rollback;
it now exercises the selected partial-load contract and preservation of callback
writes. See the [receipt](../measurements/globals-recovery-removal-20261019.json).

No significant performance regression was found: interleaved Logue quiet-turn
process medians differ by +0.54%; generation is effectively unchanged against the
repeated baseline. Initial application timings drifted with machine load and are
not a speedup claim. Three paired Globals loops measured reads -0.8%/-1.8% and
writes +3.7%. Startup medians were 2.816 → 2.755 seconds; the bootstrap image is
about 6 MiB smaller. Remaining seed ownership and the complete absence gate are
next; this slice does not establish Store-independent startup.

## Progress: early authored Globals (2026-10-19)

Globals now constructs itself through the existing Object/Map/ObjectModel owners,
before SourceRegistry and named Module admission. Store's native facade, seed
transfer and automatic-fact cleanup are deleted. The named `globals` module is
only a registration wrapper; policy lives once in `globals-owner.birnam`. Existing
owner/state identities and non-Symbol identity keys survive explicit policy reload.
Namespace permits its normal export fallback before Globals exists; a present
owner's failure or malformed result still propagates.

The initializer test loads the actual production artifact with Store and Relational
unavailable, exercises binding identity, nil/absence, snapshots, GC and reload,
then explicitly loads/reloads those optional libraries and checks independent facts
and unchanged core owners. Native binding, Store/Relational, source-registry,
candidate and image failure/lifetime checks pass. Also passing: 847 language
assertions, mapped/rebuilt Globals images, installed/relocated Host acceptance,
391 Lean theorems, 122 rejected mutations and 265 expected TLC outcomes. The build
creates fresh authored artifacts using a Rust Cargo cache; final cold-source
archive acceptance is still required for the whole milestone.

Paired steady-state measurements are near baseline: Globals reads +1.30%, source
reads +0.75%, writes -0.11%, and Logue quiet turns +0.75%. Three paired startup
runs measure 2.712 → 2.782 seconds (+69 ms, +2.56%). The bootstrap image shrinks by
43,611 bytes. This is an ownership simplification, not a demonstrated performance
improvement. See the [qualification receipt](../measurements/globals-bootstrap-owner-20261019.json),
including the instrumentation distinction between native lifetime tests and the
standard installed prefix.

The whole-core absence diagnostic still fails at Bootstrap's unconditional
Store/Relational loads. Removing those loads prematurely would only hide the task,
resource, Alien and Image dependencies. Next: define the minimal owner image
projection/admission contract, then migrate task/resource fields; remove the
remaining duplicate local-Cell variant alongside Cell/image cleanup.

## Owner image payload contract (2026-10-19, implemented)

For each identity in the image heap graph, Image sends current `imagePayload`
once while retaining the receiver and result across callbacks. Canonical
nil/Boolean/Symbol values and Namespace exports keep their existing reference
encoding. The ordinary Object default
returns `self`. `false` rejects the save without replacing its destination; an
Error result other than `self` propagates as an owner failure. Returning `self`
keeps ordinary Error objects imageable. No hook result or method is cached across
objects, and a failed callback is not replayed.

A distinct payload is supported for ordinary objects whose representation family
is ObjectVt. The payload must be readable through ObjectModel's typed-slot protocol;
returning it does not transport any separate native context it may own. Image reads
those slots through ObjectModel but keeps
the original identity and behavior. Thus aliases of the original remain aliases;
references inside the payload still name their actual original identities. This
is payload projection, not arbitrary graph identity substitution. Native families
retain their existing owner-specific codecs and reject replacement payloads, so
an arbitrary slot layout cannot be restored as native collection/code storage.
Native process handles continue to fail their existing representation admission.

Globals installs the hook on its actual storage owner and delegates to its existing
binding projection. Cloned ordinary snapshots are not silently filtered. TaskControl
keeps its existing process-local marker in an ordinary named reference slot.
Preparation sets it, completion preserves it, and ordinary cloning copies it.
Thus factory clones remain imageable, while prepared/completed handles remain
rejected even after their Machine is released. Explicit task outcomes
remain ordinary durable data. Deliberately replacing these methods changes policy;
there is no mutation observer, protection against user replacement, or registry.

This removes Image's Globals-only projection table and the per-object Store
`imageTransientObject` read. It does not yet remove Store fact transport or task
field storage. Subsequent task/resource owners use this same contract for rejection
or an explicit reset payload, with retained field handles tested through their
actual owner references. Existing image stamps still reject an intervening owner
or method change; live-load failure continues to preserve applied effects.

### Qualification of owner payloads

The native snapshot test verifies a fresh payload and child survive collection in
an outgoing ObjectModel call, then become collectible when the save scope ends.
The source fixture verifies original identity/alias/cycle preservation, unchanged
live fields, a retained unfiltered clone of Globals storage, ordinary Error values,
current replacement/rejection, native-layout refusal, and mapped/rebuilt/resaved
images. The rejection fixture releases its own retained result before expecting
an authored rejecting capability to disappear. No production retention is weakened.

The existing 21-generation image-resave chain passes with a fixed live set. Native
roundtrip/recovery tests, all 177 task-control checks, and completed/waiting-handle
rejection plus imageable task-factory clones pass. Formal qualification passes
396 Lean theorems, 125 rejected mutations and 272 expected TLC outcomes. A formal
run exhausted local scratch space; only the successful rerun is acceptance evidence.

Three paired image-save CPU medians are 31.397 → 31.328 seconds (-0.22%). Repeated
Logue quiet-turn medians are 26.912 → 26.739 ms (-0.64%). The first application run
showed machine-load drift and is retained as such. Startup wall medians are
2.827 → 2.906 seconds while CPU medians differ by +0.9%; paired results vary in
both directions. No material steady-state regression or general speedup is claimed.
The saved benchmark image grows about 0.56 MiB (0.23%). See the
[receipt](../measurements/image-owner-policy-20261019.json).

Next is ordinary TaskControl/TaskExecutor field ownership using this image
contract, with actual field aliases, scheduling transitions and task retention
preserved. Store fact transport and production optional composition remain pending.

## Current hold: task/worker field prototype (2026-10-19)

The working prototype moves TaskControl, TaskExecutor, Worker and RemoteFailure
state out of Store, eliminating all db calls from stackless-control initialization.
It builds, but task completion is 3.14× slower and field loops are about 39% slower.
The runtime change is **uncommitted and unqualified**. Per the user's explicit
performance condition, implementation is held for approval. The investigation
identifies disposable field views and their helper dispatch as the main cause;
ordinary direct owner accessors are the proposed next step. See the
[report](task-field-migration-performance.md) for measurements, protocol tradeoffs
and the acceptance requirements. The overall core-independence/release goal is
not complete or reduced by this hold.

A further source-backed cleanup found during this investigation is
`Machine prepareBlockControl`: its native `Op::Prepare` now only returns the
receiver, and adapter installation already occurs during Execution loading.
Remove the obsolete call/selector/operation after the performance hold is resolved;
retain meaningful adapter/repeated-loading coverage instead of testing a no-op.
