# Store-independent core: completion plan

2026-10-19. Baseline `9e51daee` (qualified native-runtime setup committed).
**Status: Store-independent core completion is implemented and qualified. Core-absence, optional-provider, Host/worker, format, formal, cold-archive and timing checks pass. Clean-candidate release qualification and publication remain separate.**
This closes the implementation milestone selected as a prerequisite for the next release.
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

These are source-backed completed boundaries or intentional optional consumers, not raw grep
counts. Comments, test assertions, TOML example keys named `db`, and CandidatePolicy's
ordinary `cellFor:` helper are not Store dependencies.

| Area | Current edge and evidence | Required destination / proof |
| --- | --- | --- |
| Bootstrap bindings | [authored Globals](../../modules/globals-owner.birnam) constructs its own facade and state before SourceRegistry/named modules. Store's facade, seed conversion and fact cleanup are removed. | Real source and compiled core bootstrap pass without Store/Relational; late loading preserves the existing owner. |
| Task and scheduler state | Completed: [stackless-control](../../modules/stackless-control.birnam) owns ordinary slots, with explicit aliased TaskField views. | Actual initializer, task transitions/cancellation, GC and full provider-absent Host/worker execution pass. |
| Workers and outcomes | Completed owned fields for Worker, RemoteFailure and [Concurrency](../../modules/isolate.birnam), with explicit image projection/refusal. | Aliases, reset, images and full provider-absent worker/service execution pass. Optional worker images activate their declared providers without loading them in the parent. |
| REPL and sockets | Socket and REPL ownership are qualified; REPL uses ordinary session/readiness Cells and borrowed process stdio. | Conditional retirement, aliases, failed setup, image reset/refusal and real provider-absent console/TCP/project execution pass. |
| Image resource policy | [Graph::finish](../../rust/birnam-image/src/native.rs) now sends current `imagePayload`; Globals projection and task-handle rejection use this owner protocol. TaskControl's process-local marker is an ordinary slot. Task/resource projection now lives with the ordinary owners; no Store transient schema is needed for those fields. | Owner-level image projection/admission through Id. Keep raw handles and captured process-local capabilities out of durable images, independently of facts. Ordinary persistent state must not be dropped to make an image save. |
| Alien wrappers | Completed in `9aa4843d`: [AlienRecipes](../../modules/alien-recipes.birnam) owns indexed cache/registration data; native Alien publishes through current Method/dictionary messages. No Store calls or facts remain. | Store-absent wrapping, identity/signature distinctions, live replacement, reentry and exact Unit reclamation pass. The accepted startup cost remains recorded below. |
| Alien restoration | Completed in `9aa4843d`: Image retains the explicit recipe-owner root, restores foreign methods before Map rehash, then builds ordinary registration indexes. | Current-definition selection, hash-dependent recovery, reentrant publication, independent cache aliases and mapped/rebuilt/resaved images pass. Partial-load failure reporting is preserved. |
| Image facts and Store Cells | Image selects its fact owner through current `System imageStore`; nil omits facts and all Store metadata/staging/exchange. Fact Cells use their recorded selected owner. Only explicit Store activation selects a participant. | Core mapped/rebuilt/resaved images pass with providers physically absent. Explicit facts/history/Cells and fresh optional-provider workers also pass. |
| Standard-library optional hooks | stdlib forwards documentation to current `Doc recordMethod:on:text:` and has no db accesses. Authored `store` installs `Reflect factsOf:` and fact-Cell conveniences. The intermediate `storeSupport` module is removed. | Ordinary Reflect construction/descriptions remain independent. Missing Doc means no documentation recording; fact queries require explicit Store. No empty database is fabricated. |
| Intentional documentation catalog | [Doc](../../modules/doc.birnam) and [NativeDoc](../../modules/nativedoc.birnam) deliberately store/query documentation. Doc declares html, Store and Relational. CLI doc/docgen explicitly select that composition. | Basic CLI commands and LSP use the core; documentation retains its useful relational catalog. Direct Doc loading propagates dependency failures before construction. |
| Composition and packaging | [Bootstrap](../../rust/birnam-bootstrap/src/lib.rs) and [Host source startup](../../rust/birnam-host-entry/src/startup.rs) omit Store/Relational. [Build](../../rust/birnam-build/src/main.rs) prepares and probes the core before optional providers are built/installed; raw providers live under `lib/native/`. | Explicit core/optional closures are recorded in receipts. Installed/relocated and two independent fixture-free cold archive builds pass. Release-candidate acceptance is separate. |
| Late optional initialization | Authored `store`/`relational` entries use the current native loader and ordinary Module registry. Store no longer publishes a Globals seed. | Full-world late activation, self-description, queries/history, repeated loading, existing aliases and user replacements pass. Ordinary edits create no facts before or after activation. |

The primary references above point to the current implementation. Existing
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
| Completed; qualified 2026-10-20 | Duplicate local-Cell implementation inside Store | Removed `Store localCell:` / `localCell:prototype:`, their native opcodes and tagged restoration. Remaining examples, scratch fixtures and the image-format generator use authored Cell; obsolete compatibility runner options are removed. | Keep actual fact-backed Store Cells and their explicit history. Preserve local value/alias/clone behavior through the authored owner; do not keep a second native representation solely for old-image compatibility. |
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

## Current next step

Commit the qualified production cutover, then qualify and publish a clean release
candidate through the existing release workflow. `scripts/core-without-store-test.py PREFIX
--services` physically removes both optional native libraries and authored entry
artifacts before exercising real startup, images, console, LSP, projects, sockets
and workers. The companion optional-provider tests cover late activation and
fresh-process optional images. Earlier progress entries below are chronological
slice records; their “next” statements describe that point in the migration.


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

## Qualified task/worker field ownership (2026-10-19)

TaskControl, TaskExecutor, Worker and RemoteFailure now own ordinary reference
slots. Internal state operations use replaceable owner accessors; explicit
`field:` requests retain ordinary aliased owner/index views. There are no Store/db
calls, field schemas or attribute-prefix construction in stackless-control.
Worker compare-and-clear, task transitions, cancellation/shielding, handle aliasing
and owner image admission/reset behavior are retained. Fresh-task preparation
explicitly resets every state field, including request fields copied by `clone`.

The initial owned-view prototype regressed task CPU 3.14× and was held for user
approval. The approved direct-accessor remediation now uses **80.2% less task CPU**
than the committed Store baseline and adds zero task facts versus 562. Current-task
reads use 36.7% less CPU. Explicit view operations have smaller mixed changes;
startup is about 0.1 s slower (3.8% wall / 4.2% CPU), and Logue quiet turns are
within 0.2% of baseline. This is not an across-the-board speedup claim.

Acceptance includes all 847 language checks, 177 task checks, installed task
images, Concurrency/multicore/closure gates, retained-field aliases and image
refusal, fresh-task reset and actual initializer operation with Store/Relational
physically absent. GC checks prove a retained view keeps its owner/payload alive
and releasing it makes them collectible. Existing formal contracts pass 396 Lean
proofs, 125 mutation rejections and 272 expected TLC outcomes. See the
[report](task-field-migration-performance.md) and
[receipt](../measurements/task-owned-fields-20261019.json).

This completes the task-field implementation and the Worker/RemoteFailure part
of the resource slice. **Store-free Image and complete Host/worker composition
are not yet proven:** quiescent-image owner semantics pass in the existing
composition; actual Store-free image acceptance remains with the Image slice.
Concurrency program ownership, REPL/socket resources, Alien, optional persistence
and tools/composition still remain. The original release objective is unchanged.

## Removed obsolete Machine preparation (2026-10-19)

The `Machine prepareBlockControl` no-op, native operation/selector and authored
initialization call are removed. Execution loading remains the single adapter
installation owner. The Machine fixture now reloads Execution, creates Machines,
collects and actually dispatches user-replaced `park`/`protectedBlock:` methods;
those replacements survive without any preparation hook.

A fresh installed build, the Machine lifecycle fixture, all 177 task checks and
actual Store-absent task initialization pass. Three paired timing runs show task
CPU +0.6%, reads -1.1% and writes -1.2%: no material regression or speedup claim.
The task absence fixture is now part of the regular native-message gate. See the
[receipt](../measurements/remove-machine-preparation-20261019.json).

Next is the remaining resource ownership: Concurrency preparation objects, then
REPL and socket connection state. Worker and RemoteFailure ownership was already
included in `9e7c91ed` so the task initializer could be genuinely independent.


## Qualified Concurrency preparation ownership (2026-10-19)

Concurrency now owns five ordinary reference slots. Its current accessors and
explicit `field:` views use those locations; the schema declarations and prefixed
fact attributes are gone. Image projection preserves platform setup and resets
workspace, workers, image path and closed bookkeeping without modifying live
state. Fresh preparation resets every field, including the per-object platform
flag that ordinary cloning would otherwise copy from its factory.

The migration also exposed an unnecessary `fieldIndex:` forwarding method added
during the prior task slice. Its sole caller was `field:`. It is removed from all
four field owners; `field:` directly sends `at:` to its captured Map. There is no
new replacement check, fixed method pointer or field-index service.

Actual initializer operation with Store/Relational physically absent, owner/field
aliases, mapped and rebuilt images, source-free workers, 19 service checks and
177 task checks pass. The native absence fixture now runs the actual Concurrency
source initializer, matching its production source-bootstrap entry. Worker and
image workflows themselves still depend on the existing Alien/Image composition;
this is not yet full Store-free worker acceptance.

Three paired observations show 20 preparations/cleanups at 187.3 to 191.1 ms
(+2.0%) and 500 explicit field requests/reads at 195.1 to 207.1 ms (+6.1%). The
batch adds **zero facts instead of 80**. These small costs are recorded rather
than presented as a speedup. See the
[receipt](../measurements/concurrency-owned-fields-20261019.json).

### Resource teardown checks for the next slice

Before completing REPL/socket ownership, resolve these source-review concerns
with actual resource probes and tests:

- `SockConn close` currently retains its numeric descriptor after closing it.
  Check repeated/reentrant close and descriptor reuse so an old connection cannot
  close a different connection that later receives the same descriptor number.
- Console setup calls `fdopen` for stdin/stderr each session; teardown frees the
  read buffer but does not close those FILE objects. Establish ownership explicitly
  while preserving the process's standard descriptors, and test repeated sessions
  plus failed initialization.

These concerns come from existing code, not a demonstrated regression in the
Concurrency migration. Keep resource retirement local to its owner, preserving
intervening replacements and actual cleanup errors; do not add a general resource
registry, rollback system or method-identity policing.


## Initial socket ownership regression (resolved below)

The socket draft removes Store-owned fd/buffer fields and uses one ordinary shared
Cell per connection. It fixes the now-reproduced repeated-close/descriptor-reuse
bug, preserves clone aliases and cleanup replacements, and adds owner image
admission. Its runtime, GC/image, interaction and expanded formal checks pass.
However, 100 wraps/closes rose from 57.0 to 71.2 ms (**25% more CPU**). The runtime
change was held uncommitted. The user approved the Cell remediation; its completed
qualification is recorded in the following section.

The measured extra work includes existing Cell accessors repeatedly resolving
ObjectModel. The proposed next step is captured receiver/index values inside Cell,
keeping current clone/set and method dispatch, plus actual-initializer Cell absence
coverage instead of its old zero-capture export assumption. See the
[decision report](socket-owner-migration-performance.md) and
[receipt](../measurements/socket-owner-regression-20261019.json). The user has approved this remediation. REPL resource ownership, Alien,
Image, optional composition and release remain pending.


## Qualified socket ownership and Cell accessors (2026-10-19)

The approved remediation is complete. SockConn holds a shared ordinary Cell with
one descriptor/buffer pair. Clones alias that owner; new connections get fresh
owners. Close conditionally retires the observed pair before invoking cleanup,
so reentry/aliases cannot close it twice and cleanup replacements survive. This
fixes the reproduced descriptor-reuse defect. Live connections refuse image
saving; closed connections and factory clones restore with their owner aliases.

`Cell replace:with:` is an identity-based conditional write through its owner's
ObjectModel protocol. Socket uses that message, not Cell's private slot layout.
Cell's accessors capture ObjectModel/index values while retaining current method
dispatch; clone/set dispatch in construction remains unchanged. The actual Cell
initializer now replaces the old zero-capture export assumption in its absence
fixture, preserving all ancillary record/registry/reflection/harness checks.

The 25% wrap/close regression is reduced to 6.5% (60.31 versus 56.63 ms per 100),
with zero facts added versus 200. Reads are 5.9% faster; startup wall is 13.7%
faster and Logue quiet turns differ by about -1%. All 847 language checks, 177
task checks, Cell/socket GC/image/absence checks and interaction/TCP checks pass.
The expanded formal contracts pass 405 Lean proofs, 128 mutation rejections and
277 expected TLC outcomes. Details and limits are in the
[report](socket-owner-migration-performance.md) and
[receipt](../measurements/socket-owned-resources-20261019.json).

The performance hold is resolved. REPL stdio ownership is next. Alien's recipe
storage still prevents complete Store-free socket initialization; Alien, Image,
optional composition and release remain unfinished.


## Initial REPL ownership performance hold (resolved below)

The REPL draft removes its Store fields and transient schema. An ordinary shared
Cell owns session bindings and the read buffer, with alias-preserving image reset
and conditional teardown before callbacks. It borrows the process's current stdio
streams instead of allocating fresh FILE objects with `fdopen` each session.
Actual initializer absence, cleanup/alias/image tests, installed interaction and
no-compilation startup checks pass.

Repeated session setup/teardown nevertheless rises from 4.902 to 7.824 s per 20
operations (+59.6%). Per-session `wrapStdio` reinstalls seven wrappers instead of
four. Installing once in an isolated diagnostic reduces those totals to 15.30 ms
and 31.91 ms respectively, identifying repeated setup as the dominant work.

The runtime draft was held uncommitted. The approved remediation is ordinary
shared one-time setup state, with explicit `wrapStdio` still available and no
method-identity checking or automatic repair of user replacements. Source-first,
image and cold/warm behavior must be qualified. Per the user's regression gate,
the user approved this initialization-policy change; final qualification is recorded below. See the
[report](repl-owner-migration-performance.md) and
[receipt](../measurements/repl-owner-regression-20261019.json).


### Alien recovery audit added by REPL qualification

The REPL image test reproduced older Alien recipes overwriting a saved authored
method replacement. Recovery now excludes a recipe when the same saved target and
selector has an authored binding. Source replacement, explicit foreign reinstall,
untouched wrappers and mapped/rebuilt/baked recovery have dedicated coverage.

During the upcoming recipe migration, also investigate different foreign functions
registered to the same target/selector, and removed wrappers. Recipe code reuse is
not method authority; inspect the actual current bindings during explicit snapshot
construction. Do not add guards to ordinary execution or preserve dead recipes as
live definitions simply because the cache retains their code.


## Qualified REPL ownership and one-time setup (2026-10-19)

REPL no longer creates session facts or transient schema. Its ordinary owner Cell
holds borrowed process stdio and an owned buffer; teardown retires the observed
state before callbacks. Images reset session capabilities while retaining aliases.
Shared ordinary readiness initializes wrappers once, retains explicit `wrapStdio`
and allows failure/retry. Sessions use current methods without checking their
identities or reinstalling user replacements. This also removes leaked per-session
`fdopen` FILE allocations while preserving unread libc input across sessions.

The repeated-session regression is resolved: 20 sessions take 33.84 ms versus
4,984.07 ms CPU (147x faster), adding zero facts instead of three. Fresh console
startup is 11.0% faster; ordinary source-file startup has a small observed 2.9%
(71 ms) overhead. Setup, actual-initializer absence, resource/GC/image/replacement,
installed interaction and no-compilation startup checks pass. Image round trips,
a 20-generation resave chain, partial-load conflicts and snapshot mutation pass.
The recovery fix and remaining recipe-authority audit are described above.

See the [report](repl-owner-migration-performance.md) and
[receipt](../measurements/repl-owned-resources-20261019.json). Alien recipes and
publication are next, followed by optional Image/Store contributions, duplicate
Store local-Cell removal, optional tool composition, full absence gates and release.


## Qualified direct Alien publication (2026-10-19)

Alien now installs its constructed Method through the behavior's current
`addMethod:as:` protocol, matching direct Image/Reflect publication. It no longer
routes the installation through `Store addMethod:on:with:` or creates a second
method-description fact. Result/error propagation and the target-return contract
are preserved. Recipe recording still uses Store; this is a completed deletion
within the Alien slice, not completion of recipe ownership.

Eighteen direct-publication checks cover Store-authorizer independence, explicit
Store authorizer behavior, snapshots that remain unchanged without subscription,
explicit Id observation/invalidation, current replaced publishers, collecting
callbacks, publisher failure and a reentrant replacement that survives publication.
The initial added test wrongly expected automatic mirror invalidation; it was
corrected against the existing opt-in-observation decision, not by adding a hook.
Native FFI marshalling, callbacks, collection and exact wrapper-Unit reclamation
pass, as do mapped/rebuilt/baked wrapper images and no-compilation console startup.

Three paired normal-release observations show 20 repeated wraps at 654.5 to
656.3 ms CPU (+0.3%) and source-file startup at 2.538 to 2.500 s wall (-1.5%).
This is an architectural simplification, not a qualified runtime speedup. See the
[receipt](../measurements/alien-direct-publication-20261019.json).

### Next: ordinary recipe ownership and snapshot authority

Keep function/signature object-code reuse separate from target/selector registration.
The existing recipe facts cannot serve as method authority: authored replacement
was already reproduced and fixed; repeated foreign replacement and removal need
explicit coverage. Normal dispatch must not check recipes or repair user edits.

Use ordinary authored data and current owner messages. Do not retain native Methods
solely to recognize registrations: that would keep retired executable Units alive.
Id already provides non-reused allocation identities, and Execution exposes direct
Method observations; an ordinary process-local identity stamp can recognize a
registration during explicit snapshot construction without retaining its Method.
Projection must omit obsolete registrations and strip those process-local stamps;
restoration must establish new stamps for newly created destination Methods.

Image should consume the restored recipe owner after ordinary bindings are applied,
when restored Globals is authoritative. Investigate this existing path before
adding another format section, native registry or cross-owner layout decoder.
Failures in that phase must follow the selected partial-load contract. Source and
compiled bootstrap, no-Store wrapping, code reuse, reentrant publication, removal,
aliasing, GC and successive mapped/rebuilt images are the acceptance boundaries.

## Alien recipe migration: approved performance remediation

The ordinary recipe-owner draft removes Alien's remaining Store calls and Image's
fact-table recipe discovery. Actual Store-absent wrapping, current replacement/
removal, reentry, native reclamation and successive images pass. The expanded
formal gate passes 413 Lean theorems, 132 mutation rejections and 282 expected TLC
outcomes. Runtime work remains uncommitted: repeated wrapping regressed 74% and
startup CPU 22% with the initial linear-vector implementation.

The user approved ordinary Map indexes, avoiding unchanged cache rewrites and
reconstructing process-local registration keys on restoration. Preserve independently
edited cache data and finish the owner-discovery/Map-rehash ordering probe before
qualification. See the [report](alien-recipes-migration-performance.md) and
[receipt](../measurements/alien-recipes-regression-20261019.json).

## Qualified indexed Alien owner (accepted 2026-10-20)

The approved indexed owner passes functional/formal qualification, including real
Store-absent wrapping, current-definition image restoration, callback reentry, code
reclamation and 847 language checks. Explicit image-owner roots resolve the proven
hash-bootstrap dependency; no new Globals lookup mode, native recipe registry or
Map-entry workaround remains. The formal gate passes 417 theorems, 134 rejected
mutations and 287 expected TLC outcomes.

Repeated wrapping is back to baseline (648.2 versus 653.9 ms per 20). Fresh startup
still uses 0.241 s more CPU (+9.8%). Removing the temporary current-method report
reduced image allocation work but did not erase that overall difference. The user
accepted that remaining cost and authorized continuation on 2026-10-20. Proceed
to optional Image/Store contributions and tool composition; do not claim later
work has already recovered it. See the
[decision report](alien-recipes-migration-performance.md) and
[receipt](../measurements/alien-recipes-indexed-20261019.json).


## Removed duplicate native local Cells (2026-10-20)

Store's `localCell:` and `localCell:prototype:` constructors, native operation
variants, tagged value/history branches and tagged restoration are removed.
Only explicit fact-backed handles retain the native `CellVt` representation.
Remaining planner/profile examples, scratch fixtures and the ImageFormat test
generator construct ordinary `Cell with:` values. The legacy-image compatibility
runner options are removed; images containing native local Cells require rebuilding.

There is no new Cell policy or constructor shortcut: the existing authored owner
retains current clone/set sends, ordinary reference slots, shallow-clone behavior,
shared aliases and suspension. The native regression checks restored fact handles
sharing their Store/entity/attribute, GC, writes through aliases, absent removed
constructors and rejection of the former local tag without changing the handle.

All 847 language checks pass, as do native Store/compensation/Relational checks,
Cell replacement/suspension and actual-initializer absence, exact post-return Cell
reclamation, ordinary/fact Cell mapped and rebuilt images, scratch reentry, and
132 ImageFormat admission cases with 16 byte-identical oracle round trips.
Both planner backends pass all twelve revisions on chain/wide/shared fixtures;
all log-analysis profiling phases match the independent oracle. Existing formal
contracts pass 417 Lean theorems, 134 mutation rejections and 287 expected TLC
outcomes. These models are not a proof of the production compiler or image codec.

Three interleaved startup observations show 2.725 to 2.764 seconds CPU (+1.4%)
and 2.742 to 2.806 seconds wall (+2.3%). The previously unmigrated planner example
pays for ordinary authored Cell dispatch: measured phases rise 9–14% across two
three-pair observations, or 27–62 ms over twelve revisions. The follow-up
whole-program CPU medians rise 2.5–2.9% (about 180–205 ms); wall medians rise
about 1.2%. These are modest additional caller-migration costs, not a runtime
speedup. The authored Cell implementation and GC configuration are unchanged.
See the [source-bound receipt](../measurements/remove-store-local-cells-20261020.json).

Next is explicit optional Image fact participation: remove mandatory Store stamps,
staging and swaps while preserving deliberately retained facts/history and Store
Cells, declared provider requirements, and the existing partial-load boundary.
Choose that concrete protocol before changing the format. Optional tool loading,
production core composition, full absence gates and release qualification remain.


### Qualified optional Image participation (2026-10-20)

The [concrete contract](optional-store-image.md) selects one optional owner through
current `System imageStore`, with nil as the core default and an authored
`storeImage` integration as the explicit opt-in. Image no longer resolves a default
Store. Facts, snapshot revision, Cell shells, staging and exchange all use the
selected/recorded owner. The `imageOwnersV2` payload declares that provider; old
envelopes require rebuilding. No new outer wire section or general registry is
introduced. The current distribution still selects the integration until the
production-composition cutover; this alone does not close the full absence gate.

Source review also found `Store swapImageStateWith:atRevision:stagedRevision:`
remaining only in an old recovery-owner test and documentation. Basic Image uses
ordinary optional state exchange and retains partial effects. Remove that unused
conditional exchange in a separate qualified deletion after this slice. Keep
`imageStateRevision`: optional save consistency now needs it to detect retention
and compaction edits that do not advance the fact basis.


Optional Image qualification passes actual Store-absent mapped/rebuilt/resaved
images and a controlled missing-provider test with Store as the only differing
library. Explicit history/Cells, foreign hashes, reentry, partial failure, mutation
(including retention changes), code lifetime and all 847 language checks pass.
Formal gates pass 424 theorems, 135 mutation rejections and 291 expected TLC
outcomes. Three paired observations show image-save CPU +0.4% and startup CPU
+1.5%; no material regression or speedup is claimed. See the
[contract and evidence](optional-store-image.md).

Next: delete the unused conditional Store exchange, then finish optional Doc and
fact-reflection hooks, late loading, CLI/LSP boundaries and production composition.
The actual full-core absence gate and release qualification are still open.


## Removed unused conditional Store exchange (2026-10-20)

`swapImageStateWith:atRevision:stagedRevision:` and its native implementation,
registration and interned name are removed. It had no production caller after
the basic live-loader simplification. The old recovery-owner fixture now retains
only the active Id binding-undo checks, under `binding_undo.rs`; explicit undo
still protects against ABA, changed owners and intervening notification writes.

Store's `imageStateRevision` remains the selected optional snapshot observation.
`swapImageStateWith:` remains the forward exchange for explicitly included facts.
Core images use neither. Store tests now assert the removed selector is unbound.
Binding undo, Store/history/compensation, snapshot mutation and mapped/rebuilt
Cell/optional-participation checks pass. Three paired startup observations differ
by +0.07% CPU and +0.25% wall, effectively unchanged. See the
[receipt](../measurements/remove-conditional-store-exchange-20261020.json).

This closes the extra deletion identified during Image separation. The next slice
moves stdlib's conditional documentation writes into Doc and explicit fact
reflection into its Store integration. Documentation already defaults to no-op
when Doc is absent; preserve that behavior and current receiver dispatch. Then
finish optional late loading, CLI/LSP dependencies, production core composition,
full absence and release qualification.


## Qualified optional documentation and fact-reflection hooks (2026-10-20)

Core stdlib no longer reads/writes db or knows Doc's catalog fields. Documented
method definitions retain their existing installation, current-Doc selection,
absent-Doc no-op and Reflect-return behavior, but recording is delegated to
`Doc recordMethod:on:text:`. The new hook is built before Doc publication.
Constructor variables are lexical, preserving Doc's public reverse name and
avoiding incidental Globals aliases.

The Store integration is now `storeSupport`: it installs image participation and
`Reflect factsOf:`. The fact-query body is unchanged, including current db sends,
collecting callbacks and error propagation. Its tests now belong to a separate
StoreSupport suite, not core stdlib. Doc registers that optional API's description
without replaying the integration or reselecting an image participant.

All 853 language checks, mapped/rebuilt Cell and optional-fact images, no-compilation
startup and CLI documentation probes pass. An actual Store-absent floor verifies
both the missing fact-query binding and schema-free, rebound documentation receivers
with collection. Final floor libraries match that qualified run byte-for-byte.
Three paired observations show absent-Doc definitions -0.3% CPU, recording +1.5%,
and startup -1.3%; no material regression or general speedup is claimed. See the
[receipt](../measurements/optional-store-hooks-20261020.json).

Next is CLI/LSP catalog separation, then actual late native Store/Relational
activation and the production input-closure/packaging cutover. Merely installing
these optional boundaries in today's Store-loaded bootstrap does not close the
full absence or release gates.


## Qualified CLI catalog boundary (2026-10-20)

Cli has no static Doc dependency. `docNamed:sel:` and `docgen:rustWorkspace:guides:`
load Doc explicitly; `lsp:` loads only LSP and its declared dependencies. An already
enabled catalog remains available to LSP hover, while semantic/project navigation
works without one. Starting a language server no longer loads every library merely
to populate documentation.

All 854 language checks pass. The Store-absent floor loads the actual compiled Cli
initializer and verifies its dependency selection. Separate real LSP project tests
pass in compiled-image and source-bootstrap modes; source bootstrap exercises the
Doc-absent path. Explicit source-bootstrap doc and full HTML docgen also pass.
Three paired initialize/shutdown/exit exchanges improve from 4.073 to 3.639 seconds
CPU (-10.7%) and 4.184 to 3.648 seconds wall (-12.8%). This is a measured LSP
composition saving, not a language-wide speedup. See the
[receipt](../measurements/cli-optional-catalog-20261020.json).

The remaining production boundary is explicit in the
[provider-loading/cutover contract](optional-provider-loading.md): authored public
Store/Relational entries over the existing native loader, separate optional native
library paths, core-first build/image preparation, explicit catalog composition,
late-load preservation tests, and full Host/worker/absence/release gates. These
changes are planned, not delivered by the CLI dependency edit.

## Qualified production cutover (2026-10-20)

The working implementation now builds/probes the core before installing optional
providers. Store/Relational are authored `Module load:` entries over native
libraries in `lib/native/`; they are absent from core bootstrap and ordinary
CLI/LSP composition. Native image requirements are described by Module and carried
in v8 record 12, so fresh images and workers activate their declared local providers
before heap reconstruction. Core declarations are empty. See the refined
[provider contract](optional-provider-loading.md).

The installed Host absence gate passes early, source, compiled, console, LSP,
mapped/rebuilt/resaved image paths with provider libraries and entry artifacts
physically excluded. Late Store/Relational activation preserves live bindings,
Cells, methods and data; repeated loading preserves user replacements. Fresh
optional-data images and workers preserve history/query behavior without loading
providers in the parent. The independent format gate passes 142 admission cases
and 18 byte-identical round trips. The expanded formal gate passes 424 theorems,
135 mutation rejections and 295 expected TLC outcomes.

Broad native qualification found and addressed two leftover core couplings:
stdlib still installed fact-Cell `inc`/`dec` on an unavailable `CellVt`, and the
optional `imageStore` override conflicted with ImageFormat's declarative source
certificate. Fact-Cell conveniences now belong to Store; the fallback image hook
is initialization policy, still inspectable through source descriptions. Source
descriptions now include nested literal method declarations without changing
revision recognition, method lookup or inferring that the declarations executed.
Generation adoption/ledger/retirement, compiler and source-loader checks pass with
these changes. Candidate tests now preserve the selected ordinary-token and
mutable-description contracts instead of demanding obsolete integer tokens or
forgery guards on descriptive fields.

The native loader's collecting-hook stress test passes unchanged in 288 seconds:
its deliberately collecting Integer hooks perform thousands of full collections.
No hook or assertion was weakened. The final language run passes 854 checks;
ancillary ownership/retention, full provider-absent services, installed relocation
and two independent fixture-free cold archive builds pass. Host lifecycle checks
pass when run in their required isolated processes. Disk-pressure failures were
rerun after reclaiming superseded test artifacts, with passing results.

The final image CLI check exposed a genuine stale bootstrap assumption: Module
now exists in the minimal core, so its presence cannot imply that stdlib is loaded.
Host now consults current `Module isLoaded: #stdlib`, retaining the missing-Module
case, rather than policing any method identity. Minimal images again report the
useful bootstrap diagnostic; explicit source recovery preserves saved state.
The complete image CLI suite passes, including deleted source/assets, main
dispatch, recovery, invalid versions and corrupt images.

The release native gate's timeout is 45 minutes: measured native tests already
exceed the previous 15-minute limit before build and owner-image checks. This is
a qualification budget change, not a runtime optimization or removed coverage.
Three serial, interleaved normal-GC pairs show startup CPU 2.695 → 2.686 s
(-0.3%) and LSP CPU 3.536 → 3.151 s (-10.9%). Image-save CPU is 32.728 →
33.218 s (+1.5%); the benchmark image grows 3,916,987 bytes (1.45%). No material
new runtime regression is identified, and the previously accepted Alien startup
cost is not claimed recovered. This is a composition improvement for LSP, not a
general VM speedup. GC policy is unchanged.

The [consolidated receipt](../measurements/store-independent-core-20261020.json)
records source hashes, the passing union of all 85 native targets, corrected
failure reruns, 854 language checks, 424 Lean theorems/135 rejected mutations,
295 expected TLC outcomes, actual provider-absent services, optional images and
worker behavior, installation/archive checks and raw timing samples. These are
implementation qualification; only the clean release workflow establishes
release acceptance and publication. The remaining artifact/certification ports,
generic live mirrors and per-instance Store persistence are separate follow-ups,
not unfinished parts of this core-independence milestone.

### Release fixture follow-up

Clean release qualification passed all 85 native targets and the owner/absence
image scripts. It exposed stale fixtures outside that matrix: the candidate
ABI rejection test wrote the now-supported value 5, and the Host task-control
smoke test still kept seven private handle Cells in db. The ABI test now rejects
both 4 and 6; task scratch uses ordinary Cells. All 177 task-control assertions
pass at scheduling quanta 128 and 1, with native barrier checks intact.

The REPL golden suite also expected the removed G0 module-introduction restriction
and implicit Store startup. Its transcripts now verify ordinary fresh module
loading after image restoration and explicit Store activation; all 26 cases pass.
The small stackless smoke fixture uses ordinary counter Cells as well, retaining
its exact 100,000-iteration loop/search, collection, GC and error-boundary outputs.

Fact-backed examples now explicitly load Store or Relational instead of assuming
bootstrap activated them. Their existing application data policy is preserved.
Fact/query output matches the previous composition, GC preserves the same live
values, Logue passes all 34 headless checks, and Minesweeper accepts a scripted
quit. See the [follow-up receipt](../measurements/store-independent-core-release-fixtures-20261020.json).
These are fixture/application dependency corrections, not runtime policy changes.
The renewed release candidate must finish its affected Host and archive gates.
