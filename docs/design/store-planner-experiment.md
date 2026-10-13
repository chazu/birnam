# Store versus persistent snapshots: build-planning experiment

2026-10-13. Birnam 0.17.0, release `cae9aa0835f33de1c30b5485665c12187d4ca5cc`,
Apple M3, 24 GiB RAM, macOS 14.6. This is an application experiment, not a runtime
optimization or language change. See [the runnable application](../../examples/build-planner/README.md)
and [measurement receipt](../measurements/store-planner-20261013.json).

## Decision supported by the experiment

For this application, persistent Map/Vector state is simpler and cheaper than
Store facts. **History and restoration alone do not justify making Store the
foundation of ordinary application state.** Persistent roots provide both
capabilities without a temporal fact log.

Store does supply a uniform entity/attribute/relation model, prospective attribute
history, and general temporal fact enumeration. This experiment does not show
that those capabilities are useless. It shows that their value has to come from
applications needing that general model, rather than from assuming ordinary
immutable collections cannot support historical worlds.

Keep Store available while questioning which language facilities need to depend
on it. This does not justify removing Store-backed language metadata, changing
bootstrap, or starting the paused relational rewrite. A separate dependency audit
would be required before changing those responsibilities.

## What was implemented

Both backends run the same source-authored invalidation/topological planner. They
apply deterministic manifests and edits, maintain simulated successful artifacts,
and answer current and historical build plans. Independent Python `graphlib`
and copied dictionaries recompute the answers. Complete source/artifact state is
verified alongside the plans, preventing a superficially correct build list from
concealing incorrect history or publication.

The sequence includes initial indexing, no-op, leaf/root edits, module addition,
dependency changes, removal of a required module, cycles, and two restorations.
Missing dependencies and cycles publish no artifacts. Checkpoints precede build
publication, allowing the original plan to be reproduced. Restoration creates a
new source revision while retaining the latest successful artifacts and older
checkpoints.

Store represents dependencies as cardinality-many facts, with historied digests
and artifact records. Only changed facts are written. Current and historical
views use the public triple-enumeration messages. Map uses persistent maps and
adjacency vectors; its checkpoint is a retained map root, and restoration replaces
the source root directly. Neither backend receives an artificial implementation
restriction to force identical operation costs.

The planner rebuilds its dependency index per query. Its output is an incremental
build set; the graph algorithm itself is not incrementally maintained. Revisions
are sequential application-level groups, not atomic multi-fact Store transactions.
No actual compiler invocation or filesystem rollback is simulated as completed.

## Repeated CPU measurements

Three fresh processes per backend and size, alternating backend order. Each
process applies 12 revisions, then checks five historical states. The table is
**summed phase CPU for the 12 revisions**, including initial indexing, two
restorations, current-state reads, planning and artifact publication. It excludes
startup/input preparation, the five later history queries, explicit collection
between revisions, and verification output. Normal automatic GC remains enabled
inside measured phases.

| Module inventory | Map median (range), ms | Store median (range), ms | Store / Map |
| --- | ---: | ---: | ---: |
| 8 | 303.84 (302.90–306.12) | 466.91 (466.37–471.05) | 1.54× |
| 32 | 1,363.55 (1,345.56–1,363.75) | 2,008.46 (1,992.93–2,025.23) | 1.47× |
| 128 | 5,597.82 (5,579.98–5,735.27) | 8,437.04 (8,334.95–8,443.56) | 1.51× |

One inventory entry is initially absent and added during the scenario. The shared
shape combines a chain with additional shared dependencies.

At 128 modules, median sums by phase explain the difference:

| Phase across the revision sequence | Map, ms | Store, ms |
| --- | ---: | ---: |
| Read state before edits | 2.68 | 1,058.63 |
| Apply edits/restorations | 51.77 | 455.66 |
| Read updated state for planning | 2.40 | 1,113.01 |
| Shared planning algorithm | 5,425.51 | 5,515.30 |
| Publish successful artifact records | 113.31 | 105.21 |
| Retain checkpoints | 2.23 | 2.25 |

These are medians calculated separately, so they need not sum to the median
whole sequence. Historical reads used during restoration are an additional phase.
The roughly equal planning cost is important: most of Map's measured time, and
about two thirds of Store's, is common language/application work. Store indexing
alone cannot remove it.

The adapter reconstructs a graph view twice per revision for Store; Map already
holds that representation. These are measured implementation costs, not an
intrinsic lower bound for every possible Store planner. A cached projection or a
more suitable query interface could change the result, at a maintenance/memory
cost that should also be measured.

## History, restoration and retained state

At 128 modules, the median separately measured historical-view read was 0.234 ms
for Map and 97.277 ms for Store. The Map result is dominated by timer/message
overhead. Store reconstructs a graph from retained facts; this is not merely
looking up a checkpoint token. Automatic GC also occurred within some historical
reads, so the median is not a guarantee about latency tails.

Restoration's old-view read plus source-state update measured a median 0.394 ms
for Map and 231.301 ms for Store. This excludes subsequent full planning and
publication. Map switches a persistent source root; Store enumerates the old world
and applies the difference. Both preserve current artifact records.

After 12 revisions, median growth in process-wide post-GC live objects was:

| Module inventory | Map object growth | Store object growth | Store final native fact bytes |
| --- | ---: | ---: | ---: |
| 8 | 425 | 510 | 2,640 |
| 32 | 2,347 | 2,742 | 8,400 |
| 128 | 9,926 | 11,670 | 31,440 |

Object counts include driver/runtime state and prepared fixtures. They are not
byte footprints or exclusive backend ownership measurements. Fact bytes are a
separate native-storage metric and exclude spare capacity and allocator overhead;
seven initial schema facts are included. Prepared fixture rows remain live in
both implementations, affecting opportunities for sharing. Both backends retain
all revisions. No conclusion about bounded retention or full process memory
follows from this table.

## End-to-end and scaling limits

At 128 modules, median complete process elapsed times were 52.36 s (Map) and
57.07 s (Store), much larger than the measured planning phases. Complete state
serialization/verification, loading and explicit GC are outside phase timers.
The 1.51× phase-CPU ratio must not be presented as an end-to-end slowdown.

The 32-module topology probes each completed once per backend:

| Shape, 12 revisions | Map phase CPU, ms | Store phase CPU, ms | Store / Map |
| --- | ---: | ---: | ---: |
| Chain | 1,010.84 | 1,538.65 | 1.52× |
| Wide | 1,094.92 | 1,620.08 | 1.48× |

These are corroborating probes, not repeated timing distributions.

A separate 32-module shared graph retained 64 revisions. Both backends reproduced
all five requested historical worlds after all edits. Phase CPU was 7,241.03 ms
for Map and 10,356.49 ms for Store (1.43×), in one run each. Common planning was
7,090.32 / 7,100.49 ms. Median historical-view reads were 0.234 / 24.145 ms.
Store's read latency changed little from the shorter history at this size; merely
speeding the internal history scan is unlikely to remove the view-construction
cost measured here.

Within that longer run, retained-state diagnostics grew as follows:

| Revisions retained | Map object growth | Store object growth | Store native fact bytes |
| --- | ---: | ---: | ---: |
| 12 | 2,347 | 2,742 | 8,400 |
| 32 | 5,947 | 6,302 | 10,000 |
| 64 | 11,708 | 11,999 | 12,560 |

The same process-wide/fixture caveats apply. These observations do not isolate
all allocations to the backend, and do not establish a memory leak or an
asymptotic retention law.

The 512- and 10,000-module stress attempts each have a 60-second **whole-process**
deadline, including setup and verbose validation. They are separate from the
passing measurements. Both 512-module runs completed and independently verified
revisions 0–3 before timing out; no complete ten-revision or history-query result
is claimed. Their partial phase timings are preserved in the receipt but are not
mixed into the full-scenario comparisons.

Neither 10,000-module run printed the first post-setup memory checkpoint before
its deadline. That outcome cannot be attributed to Store operations. These
attempts establish a limit of the current harness/application combination, not
a valid large-world backend ranking. For a useful large-world comparison, first
separate compact timed output from full-state qualification and measure fixture
preparation explicitly. Raising the deadline alone would obscure that distinction.

## Acceptance evidence

All 24 complete processes passed: 392 current revision states and 120 historical
states, each checking full source/artifact state and build plans. That comprises
18 repeated shared-graph runs, four topology probes, and two longer-history runs.
Incomplete stress runs remain explicitly non-passing in their receipts.

Six Python tests pass, including hand-checked scenario outcomes, all 512 directed
three-node graphs checked against a separate transitive-closure calculation,
changed-dependency closure, malformed fixture rejection, and rejection of
corrupted/missing receipt output. The 512-graph enumeration validates the Python
oracle; it is not represented as 512 separate Birnam executions.

The saved receipt keeps all phase samples, memory checkpoints, plans,
full-state digests, runtime/source bindings and raw-log hashes. Full verbose logs
and original receipts remain under `/tmp/birnam-build-planner-*`; the runner can
regenerate them without human interaction. Application and runner hashes match
the delivered files; the checker tests were strengthened during measurement.
Python compilation and whitespace checks also passed. No runtime rebuild or full
release qualification was needed for these application-only additions.

## Implications and next experiments

1. **Use persistent snapshots for this planner.** They express the domain directly,
   make history cheap, and make restoring sources especially simple. The Store
   backend requires schema setup and graph reconstruction in addition to history
   bookkeeping. There was no application simplification compensating for its cost.
2. **If Store remains a general graph substrate, improve access to graph views.**
   Investigate current entity/attribute indexes, adjacency queries, and reusable
   temporal projections. Measure total view construction and callback costs, not
   only the internal fact scan. Store's snapshot grouping is already indexed
   within an invocation; this is not a claim that every snapshot path is quadratic.
3. **Keep the common planner as a language workload.** Its dominant cost survives
   replacing Store entirely. Profile traversal, transient collections, numeric
   loop/control work, frames and dispatch before selecting a language optimization.
   An incremental application index could also help both backends, but would be
   an application optimization, not evidence of a faster language.
4. **Test Store's distinctive value separately.** Heterogeneous world queries,
   generic change inspection and retention policies are stronger candidates than
   simple undo/history. This experiment uses Store's public fact API and an authored
   planner, not the Rust relational evaluator. It does not compare against an
   optimized relational execution engine or establish that temporal databases
   generally lose to maps.

No compiler/runtime shortcut, method freezing, GC threshold adjustment, native
layout dependency or deoptimization mechanism was introduced. Semantic checks
are the independent oracle, exhaustive small-graph checks of that oracle, and
actual Birnam executions. No new Lean/TLA+ proof of runtime behavior is claimed.

## Dependency audit, pass 1 — 2026-10-13

This extends the planner experiment with source inspection of the current
bootstrap, authored modules, compiler/Generation, Image, reflection and Alien
consumers. It changes recommendations, not implementations. The only additional
executions are the bounded retention probes below; the planner's 47–54% CPU
penalty is **not** extrapolated to these consumers. Probe source, source/runtime
hashes and both runs are in the [dependency-audit receipt](../measurements/store-dependency-audit-20261013.json).

The important distinction is between **needing an owning state representation**,
**needing a public fact/query interface**, and **depending on publication or image
protocols that currently happen to live on Store**. Moving a protocol without
reproducing its semantics would not be a storage refactor.

### Decision summary: where Store should remain

**Implementation update:** the private Markdown, AlienCompiler and NativeDoc
conversions are now implemented and validated; see [the implementation record](#implemented-private-scratch-removal--2026-10-13). Other migration recommendations
remain proposals.

Keep Store where **facts, relationships, and temporal queries are the feature**,
and remove it where it serves as ordinary object storage. These are migration
recommendations; the conditions below remain part of each proposed change.

| Area | Recommendation | Reason / conditions |
| --- | --- | --- |
| Explicit databases and relational queries | **Keep** | This is Store's intended capability. |
| User-requested fact history and temporal queries | **Keep** | Make these deliberate choices rather than default storage behavior. |
| Documentation catalog | **Keep provisionally** | It actually queries documented entities and classifications. Separate its temporary scanner state. |
| Build planner state and history | **Remove** | Persistent Map/Vector snapshots were simpler and faster. |
| Markdown, AlienCompiler, NativeDoc scratch state | **Remove first** | Temporary state becomes permanently reachable through facts; reproduced for Markdown and AlienCompiler. |
| Logger, configuration, HTTP requests, process results | **Remove** | Ordinary per-instance records need ownership and fields, not a database. |
| LSP caches and buffers | **Remove** | Already represented primarily as Maps and Strings; Store adds indirection. |
| Module registry and dependency bookkeeping | **Remove** | A registry fits the existing access patterns. Preserve cold-bootstrap and image behavior. |
| Local mutable cells | **Keep independent of facts** | `localCell:` already bypasses facts. Eventually separate its implementation from Store's packaging. |
| Public `asCell` | **Redesign deliberately** | Prefer local mutation by default, but explicitly address its existing history contract. |
| Globals | **Remove as primary storage** | Introduce binding-owner interfaces; decide how existing direct `db` writes and history remain accessible. |
| Method mirror | **Remove as mandatory machinery** | Id already owns authoritative dictionaries. An optional query view could remain. |
| Method publication | **Move out of Store** | Preserve authorization, notifications, errors, and code lifetime. |
| Compiler metadata and Alien recipes | **Remove as primary storage** | Use imageable owned records; retain metadata inspection and startup without reparsing. |
| Generation lifecycle records | **Remove eventually** | Ordinary records fit, but coordinate with publication and rollback. |
| Generation compensation and Image staging | **Retain for now** | Replace only alongside an equivalent cross-owner recovery protocol. |
| Task, worker, socket, and REPL live state | **Remove eventually** | Move state together with cleanup and image-exclusion rules. |

**First implementation target: Markdown and AlienCompiler scratch state.** They
have the clearest demonstrated problem and a relatively contained replacement path.

### Cross-impact: removing storage can also remove a public capability

The decision table above recommends a primary representation. It is **not a claim
that deleting those facts leaves Store and its dependent APIs equally useful**.
The planner experiment tested one application's needs; it did not measure the
value of a uniformly queryable language world. Removing system data can leave
the database engine intact while substantially reducing what programs can ask it.

| Candidate group | What could be lost elsewhere | Migration condition |
| --- | --- | --- |
| Private parser/scanner scratch | Incidental visibility of temporary entities and fields in global fact enumeration | Lowest-impact starting point. Keep intended outputs and published records; identify any intentional scratch-inspection API. |
| Ordinary object fields, LSP and Module state | `Reflect factsOf:`, generic queries and joins over those fields, mutation through `db` or saved Store Cells, and any explicitly enabled field history | Preserving named getters alone is insufficient. Decide which generic state interfaces remain supported and provide their replacement before removing public data. |
| Globals, compiler metadata and method facts | Queries joining definitions, names, modules and metadata; existing global enumeration/reverse naming; direct global mutation through facts | Preserve or deliberately redesign the world-inspection and mutation contract. Live method reflection can remain authoritative in Id even when method facts move. |
| Generation and Image records | Coherent preparation checks, selective compensation, durable state and coordinated recovery | Move the protocol and its observations together, not just the data. |
| Task/worker/I/O state and image markers | Handle inspection and cleanup, serialization rejection, and scheduling/lifetime behavior | Owner state must retain these capabilities, including explicit process-local image policy. |

Concrete current dependencies include:

- [Reflect `factsOf:`](../../modules/stdlib.birnam) enumerates actual Store triples.
  It will not discover ordinary ObjectModel slots automatically.
- [Doc `globalNameOf:`](../../modules/doc.birnam) first queries `#__globals__` facts;
  moving Globals while leaving this consumer unchanged loses names.
- [Relational evaluation](../../rust/birnam-relational/src/native.rs) obtains
  current/historical triples through Store's enumeration messages. It does not
  automatically join arbitrary owner-held Maps or records. Rules depending on
  removed system facts can therefore return different answers even if the query
  API itself is unchanged.
- [Globals tests](../../rust/birnam-siphon/tests/store.rs) require direct Store
  assertions to affect Globals and support opted-in global history. A read-only
  view cannot preserve that writable alias or supply historical values by itself.

Externally retained fact-backed Cells need particular care: replacing an object's
accessors with slot reads does not redirect an existing Cell to the new storage.
The Cell can become a stale second view unless its contract is migrated too.

For public world state, investigate owner-backed query views or explicit snapshots
alongside direct storage changes. That could preserve useful relational access
without eagerly recording every private temporary. It is a design option, not an
existing compatibility mechanism or a demonstrated free optimization. A complete
replacement must specify identity, nil/absence, ordering where observable,
cross-owner snapshot consistency, write routing, and historical revision meaning.
A current-state projection alone does not preserve mutation or temporal queries.

**Revised implementation boundary:** proceed first with genuinely private scratch
state. Treat removal of public/system facts as an API and language-tooling decision,
not automatically as a semantics-preserving refactor. Keeping Store useful for
world queries may be a reason to retain selected records or expose selected owner
state through it; the measured planner result does not settle that tradeoff.

### Newly reproduced: temporary state survives its operation

The [Store owner](../../rust/birnam-store/src/native.rs) roots the global Store
receiver. Its `references` visitor traces every retained fact's entity, attribute
and value. A separate unreachable Store can be collected; the global `db` remains
reachable. Consequently, facts written for fresh anonymous scratch entities keep
those entities and their current values alive after callers discard their results.
`#history = false` limits past versions; it does not remove current facts for an
otherwise dead entity. Image-transient markers affect image export, not this GC
reference graph.

Two fresh installed 0.17.0 processes each warmed an operation, checked every
result, discarded the results of 50 further calls, collected, compacted `db`, and
collected again. Both produced the same retention counts:

| Operation, 50 calls | Additional facts after GC | Additional facts after compaction + GC | Process live-object increase after first GC |
| --- | ---: | ---: | ---: |
| Return the existing Integer 7 | 0 | 0 | 3 |
| Create/read/discard `db localCell: 7` | 0 | 0 | 3 |
| Create/read/discard `7 asCell` | 50 | 50 | 103 |
| `Markdown toHtml: "hello"` | 200 | 200 | 353 |
| `AlienCompiler parse: "i i"` | 500 | 500 | 1,503 |

The three objects in the control are common measurement overhead. Counts are
process-wide, not byte measurements. Markdown results were checked against
`<p>hello</p>`; signature results against the expected scalar-result/scalar-argument
structure. CPU is recorded incidentally, not offered as a before/after speedup.
The receipt contains the complete runnable probe so these observations can be
repeated without relying on the temporary files.

This is **retention caused by the chosen state representation**, not evidence of
incorrect GC. The collector correctly follows the facts. Changing the GC threshold
or compacting current facts cannot solve the demonstrated problem.

### Consumer inventory and disposition

| Consumer | What it uses Store for | Assessment / direction |
| --- | --- | --- |
| Markdown, AlienCompiler, NativeDoc scratch | Scanner fields, cursors, temporary cells | Highest-priority removal candidates. Use invocation-owned state; Markdown and AlienCompiler retention reproduced. NativeDoc is source-inspected only. |
| `Object asCell` | A fresh entity's `#value` fact | Retention reproduced. Review its public history/fact contract separately; convert private scratch callers first. |
| `db localCell:` | Direct value in Store-owned Cell records | Already bypasses fact history/scans. Keep behavior; owner placement can be separated later if Store is to become optional. |
| Logger, AppConfig, HttpRequest, ProcResult | Ordinary per-instance fields | Prefer ordinary records/slots. No temporal-query use found in their accessors. Preserve instance/clone/accessor semantics. |
| LSP caches and buffers | Persistent Maps stored as single field values | Maps already provide the data structure. Store acts as an extra field-access layer; explicit LSP-owned state is a better fit. |
| Module manager | Registration/kind/load flags, dependency Vectors, library directory, traversal scratch | Strong follow-on to the planner experiment. Prefer a module registry and local traversal state; cold bootstrap is a required migration gate. |
| Doc catalog | Documented entities, kinds, method documentation and relational selection | Real use of generic fact queries. Keep provisionally; separate scanner/control scratch from the catalog. |
| Globals and tools that enumerate globals | Live source bindings, nil/presence, fact visibility and optional history | High-value separation candidate, but the two-way Store relationship is currently an explicit tested behavior. Needs a public-interface transition. |
| Method publication and mirror | Authorization, actual Id edits, fact mirror, post-mutation notification | Separate publication authority from an optional query projection. Preserve mutation/lifetime protocols; do not simply bypass Store today. |
| Compiler metadata | An immutable metadata Vector keyed by source object | Candidate for an imageable source record/metadata owner. Keep metadata available without parsing; preserve deliberate relational inspectability. |
| CompiledUnit / Generation | Lifecycle fields, projections, preparation stamps, exact inverse journals, reservation-backed compensation | Not a snapshot-only problem. Defer storage substitution until a cross-owner publication contract is specified and tested. |
| Alien wrapper recipes | Imageable native-object recipes and reuse inventory | Owner-held recipe records are plausible. Coordinate with Image and tracked method publication. |
| Tasks, workers, sockets, REPL and Concurrency handles | Ephemeral scheduling/I/O fields and image-exclusion declarations | Prefer owner-held live state, but handle/image/cleanup semantics must move together. Retention path identified in source; no task/socket stress test in this pass. |
| Image | Fact serialization, whole-Store staging/exchange, transient markers and publication compensation | Keep current integration until replacement owners participate in the same recovery transaction. |
| Explicit database applications, `Reflect factsOf:`, relational facade | Entities, attributes, history and generic queries as requested functionality | Keep Store as the database facility. These are not redundant incidental field accesses. |

### 1. Remove private scratch dependence first

[Markdown](../../modules/markdown.birnam) allocates a scanner per conversion and
writes `#mode`, `#buf`, `#items`, and `#shift` into `db`. The four retained facts per
call match the source. [AlienCompiler](../../modules/alien-compiler.birnam) uses
`state:at:`/`state:at:put:` for parser state and a `cell:` helper that allocates a
fresh fact-backed `#value` entity. The scalar signature probe retains ten facts
per parse. [NativeDoc](../../modules/nativedoc.birnam) similarly puts its temporary
`#pending` and `#summary` scanner fields in `db`; its resulting documentation
records are a separate, intentional catalog concern.

Use ordinary scanner records whose slots point to local Cells or directly hold
the mutable fields. There is already an appropriate public owner interface:
[ObjectModel](../../rust/birnam-object-model/README.md#ordinary-storage-and-slot-metadata)
provides `newSlots:parent:`, `slot:in:` and `slot:in:put:`, with tracing and image
support. [Trace](../../modules/trace.birnam) already uses this pattern in authored
Birnam. No new Rust state service or foreign-layout access is needed.

The [public `asCell` definition](../../modules/stdlib.birnam) explicitly creates a
Store Cell, and its documentation advertises history. Changing it globally could
change fact visibility and explicitly enabled `#value` history. First replace
private scratch constructors such as AlienCompiler's helper; then make a separate
API decision about the default mutable-cell abstraction. In contrast,
[`localCell:`](../../rust/birnam-store/src/native.rs) stores a tagged direct value,
returns zero history, and already has the desired non-fact retention behavior.
Its being implemented by the Store module is a packaging dependency, not evidence
that local mutation requires a database.

Acceptance for a scratch conversion: unchanged output/error cases, shared-cell
aliasing where required, collecting callbacks, no accumulating scratch facts,
post-GC retained-state plateau, and the relevant existing module tests. Include
cold wrapper compilation for AlienCompiler. Source-authored algorithm changes do
not need guards to emulate their old internal Store sends. Any intentional public
fact/query contract must be identified explicitly rather than silently erased.

### 2. Module state is a particularly relevant next audit target

[Module](../../modules/modules.birnam) reads/writes `#registered`, `#kind`,
`#loaded`, and `#requires`. Despite the introductory description of relational
edges, `#requires` is stored as one dependency Vector. `requiresOf:` is a point
read; `loadOrder:` performs its own DFS. It does not ask the relational evaluator
to solve the graph. Its traversal scratch is a fixed `#topoOrder` Cell on Module.

A registry of ordinary records and invocation-local traversal state fits this
consumer. Its library-directory rebasing and loaded flags still need persistence
and deliberate restoration behavior. Module is loaded before stdlib, so a rewrite
must use only the available bootstrap vocabulary; it cannot acquire a circular
requirement on a later collection helper. Preserve declared order, cycle Errors,
native versus authored module distinctions, idempotent runtime loading, artifact
identity/requirements checks, and relocated image behavior. The planner experiment
supports this investigation, not a predicted percentage speedup or proof that the
migration is trivial.

[LSP](../../modules/lsp.birnam) is another strong candidate: documents, versions,
project caches, completion caches and buffers are largely Maps/Strings reached
through repeated `cellFor:value` operations. Store is not computing their indexes.
Each `cellFor:` call constructs a fresh Cell handle in the Store owner, including
read-only getters. Removing this indirection can avoid allocation/rooting work as
well as fact lookup; no speedup is claimed without measuring the replacement.
These are primarily representation/lifetime changes, with LSP protocol and cache
invalidation tests as acceptance. Logger, AppConfig, HTTP requests and process
results have the same field-access pattern at smaller scale. Moving to slots must
preserve how cloning and per-instance initialization work; a default shallow clone
is not automatically equivalent to previously absent Store fields.

### 3. Globals require an explicit compatibility boundary

[Globals' owner implementation](../../rust/birnam-store/src/native.rs) forwards
`at:`, `at:put:`, `at:ifAbsent:` and `includes:` to the global Store under
`#__globals__`. This is more than an internal representation choice today.
[`globals_are_facts`](../../rust/birnam-siphon/tests/store.rs) tests that:

- Store reads and historical queries observe Globals writes.
- A direct Store assertion changes a subsequent Globals read.
- Bound nil is present and differs from an absent binding.
- Replacing Store's reader is observed by Globals.

[REPL enumeration](../../modules/repl.birnam),
[Doc reverse naming](../../modules/doc.birnam),
[reflection](../../rust/birnam-codegen-runtime/src/reflection.rs), and
[Generation's exact global capture](../../rust/birnam-generation/src/candidate_preparation.rs)
read the underlying facts directly. Moving only the Globals getter/setter would
leave those clients observing another authority.

First define owner-level enumeration, reverse naming and exact presence/value
operations, then route these consumers through the binding owner. Source execution
must retain [Globals-first, nil-triggered Namespace fallback](../../rust/birnam-namespace/src/native.rs);
reflection currently uses Namespace-first resolution for some operations, so
those distinct contracts must not be accidentally unified. Preserve alias naming
and observable enumeration behavior where relied upon.

Then decide whether direct `db` writes remain a supported way to mutate globals,
and how users request global history/query views. A read-only projection cannot
silently replace today's writable alias. A permanent second authoritative map plus
fact mirror would reproduce the synchronization problem being audited. Keep live
`Globals` sends and replaceability; do not replace the dependency with compiled-in
bindings or a native service table.

### 4. Method facts are derived; method publication is substantive

The [Id dictionaries](../../rust/birnam-id/src/id/native_id.rs) are authoritative
for dispatch. Store's `addMethod:on:with:` / `removeMethod:on:` wrap dictionary edits
with authorization, mirror maintenance and post-mutation observation. Raw Id edits
also notify Store, which removes stale mirror facts. The mirror is a derived and
potentially partial view, not a second authoritative method dictionary.

[Reflection's method inventory](../../rust/birnam-codegen-runtime/src/reflection.rs)
already enumerates actual Id dictionaries. Its remaining Store dependency includes
reverse naming through global facts. This supports moving reflection toward owner
interfaces and making method-fact projection optional.

However, [Generation's running-phase policy](../../rust/birnam-generation/src/candidate_phase.rs)
is installed through Store's authorizer/observer slots. Reflect, Codegen, Alien
and Image all participate in that publication route. Retain current admission,
post-commit notification ordering, errors, reentrant callback handling and Method/
code ownership if the route moves. Separating these responsibilities does not
justify deleting the existing callbacks or changing running-phase policy.

### 5. Compiler metadata and Alien recipes need persistence, not fact history

[`source_metadata.rs`](../../rust/birnam-codegen-runtime/src/source_metadata.rs)
reads one `#compiledSourceMetadata` value by source identity, validates its digest,
and persists a Vector if absent. A differing already-present value is rejected.
This path does not use temporal or relational query evaluation.
[Generation already has a separate source registry](../../rust/birnam-generation/src/registry.rs)
with sealed state, roots and staged exchange. An explicit imageable source record
could associate original source and compiler metadata without putting the mapping
in the global fact table.

The earlier requirement that compiler metadata remain available as relationally
inspectable data should remain explicit in that design. The non-negotiable
behavior is compilation-produced metadata, preserved source identity, validation,
and image/artifact startup without reparsing or recompiling. Missing/malformed
metadata must still fail admission, as checked by
[the startup tests](../../rust/birnam-host/src/startup.rs). Relocating data is not
permission to recreate it lazily by parsing source.

Identity needs care: Store compares entities by identity, while
[Map](../../rust/birnam-map/src/lib.rs) may call `collectionEquals:` on keys.
A naive Map keyed by source Strings can change association semantics. Use explicit
source records or an identity-preserving owner contract, with image remapping and
collection behavior specified. Do not use permanent raw addresses as language IDs.

[Alien's recipe inventory](../../rust/birnam-alien/src/runtime.rs) likewise uses
`#__aliens__` as an enumerated record collection. It stores function/signature,
selector, target and relocatable object bytes, then scans recipes for reuse.
Owner-held imageable records are plausible. Preserve wrapper code lifetimes,
current foreign-address rebinding, tracked method publication, and image recovery
without compilation. This change should follow the relevant persistence interfaces,
not become another Rust-only cache that the self-hosted language cannot express.

### 6. Generation rollback cannot be replaced by wholesale snapshot restore

Generation holds a transient binding ledger already, while
[CompiledUnit's authored policy](../../modules/compiled-unit.birnam) keeps lifecycle
fields and immutable source/selection projections in Store. Much of that data
could eventually become owned records. The transaction mechanisms need separate
analysis:

- [Prepared tokens](../../rust/birnam-generation/src/candidate_preparation.rs) bind
  Store's numeric basis, Id's binding epoch, precise direct bindings, and exact
  global absence/value. Change-and-restore (ABA) is detectable. A saved Map root or
  equality comparison alone is not a replacement for all these observations.
- [Inverse journals](../../rust/birnam-generation/src/candidate_transaction.rs)
  restore only candidate-owned writes, conditionally on expected current values.
  They reserve capacity for forward publication and compensation. Unrelated
  callback facts must survive failure; restoring an entire previous world would
  erase them.
- [Ledger reconciliation](../../rust/birnam-generation/src/candidate_ledger.rs)
  distinguishes unrelated facts from managed-binding drift. Unrelated facts can
  invalidate a prepared token without permanently invalidating an active ledger.
  Preserve that distinction until a narrower invalidation contract is deliberately
  designed; do not remove checks as an incidental storage optimization.

The immediate opportunity is separating lifecycle records from publication
coordination, not eliminating the coordinator. Any replacement needs a cross-owner
state machine and appropriate Lean/TLA+ work for its actual new contract, followed
by [real compensation tests](../../rust/birnam-siphon/tests/generation_transaction.rs)
with collecting/reentrant/failing callbacks, exact bindings and unrelated writes.
The current snapshot-planner experiment proves none of that substitution safe.

### 7. Image and process-local state must move together

[Image](../../rust/birnam-image/src/native.rs) serializes facts, stages a separate
Store, swaps its state, publishes methods and exchanges source registries. Failure
compensates bindings, sources, Store and Generation state. This is a coordinated
recovery boundary, not just a serializer for a Map. New state owners need rooted,
staged image exchange and compensation before their Store facts can disappear.

Image also queries `#imageTransientObject` for reached objects, while Store filters
facts whose attributes are marked `#imageTransient`. Removing facts without moving
these declarations can accidentally make process-local handles appear serializable.
[TaskControl/TaskExecutor/Worker](../../modules/stackless-control.birnam),
[Concurrency](../../modules/isolate.birnam),
[REPL](../../modules/repl.birnam), and [Socket](../../modules/socket.birnam) need
explicit treatment of live Machines, workers, file handles, buffers and cleanup.
Task fields explicitly disable history and mark themselves image-transient, yet
remain ordinary GC edges through global Store facts. That is a concrete retention
route to investigate; this pass did not execute a completed-task retention test.

Preserve rejected-capability behavior, shared identities, original-source recovery,
Map rehashing after image restoration, native code lifetimes, and no source
reevaluation. Existing [image roundtrip/compensation tests](../../rust/birnam-siphon/tests/image.rs)
and repeated resaves are necessary acceptance; the format/compatibility decision
must accompany a new owner representation.

### Recommended order after this audit

1. Convert **Markdown and AlienCompiler private scratch state**, then NativeDoc
   scratch. Re-run the retained-fact probes and the modules' correctness/cold-build
   gates. This is the smallest scope with a reproduced lifetime problem.
2. Move **ordinary per-instance records and LSP state**, and investigate **Module's
   registry** with an explicit cold-bootstrap acceptance path. These should be
   authored Birnam over existing record/slot/Cell protocols.
3. Introduce **binding-owner/tooling interfaces**, then decide the Globals and
   method-fact compatibility boundary. Keep one authority for each kind of state.
4. Move **metadata and recipe records** with their persistence interfaces. Keep
   the earlier metadata/no-reparse guarantees.
5. Revisit **Generation, Image and process capabilities** as coordinated protocol
   work, with focused formal models and failure/recovery acceptance. Do not turn
   these into superficial Map substitutions.

Keep the explicit Store/relational database API and provisionally the documentation
catalog. Bootstrap currently loads `vector`, `map`, `store`, `relational` before
the later compiler/authored layers ([bootstrap source](../../rust/birnam-bootstrap/src/lib.rs)).
Removing a consumer's fact access does not yet make the Store Rust crate removable
from cold bootstrap. Track those edges until they actually disappear; replacing
them with another permanently privileged Rust subsystem would miss the language's
self-hosting goal.


## Implemented: private scratch removal — 2026-10-13

[Validation receipt](../measurements/private-scratch-20261013.json).

### Evaluation and chosen boundary

Repository-wide searches of scanner/helper selectors and state-field accesses
found their consumers within the owning modules. Markdown's scanner never becomes
the returned HTML. AlienCompiler returns parsed value trees, ABI classifications
or C/Errors, not its parser/classifier records or loop Cells. NativeDoc's scanner
is separate from the intentionally published documentation catalog.

Those three implementations were converted. Public `asCell`, Globals, Module,
Doc's catalog records, lifecycle state, image policy and the database/relational
APIs were left in place. Same-named fields on unrelated entities are not touched.
This is not a claim about arbitrary external programs inspecting incidental scratch
facts or constructing the old internal scanner representation: those helpers now
consume local records, and private scratch facts intentionally stop appearing in
`db`. No existing facts are deleted, so previously saved worlds can still contain
old scratch entities; identifying and purging them safely is separate work.

### Implementation

- [Markdown](../../modules/markdown.birnam): one invocation-owned Map containing
  local Cells for mode, buffer, list items and heading shift.
- [AlienCompiler](../../modules/alien-compiler.birnam): `cell:` constructs a local
  Cell; parser and classification records hold named local Cells. This covers
  signature parsing, error construction, HFA classification and C generation.
- [NativeDoc](../../modules/nativedoc.birnam): local pending-doc/summary Cells.
  Its existing `Doc for:in:says:`, doc-kind and summary publication remain intact.

Maps contain shared mutable Cells, so copying/capturing the scanner reference
preserves aliasing. Helpers, collection operations and Cell access remain ordinary
late-bound sends. No native fast path, method cache, compiler guard or alternative
Rust state service was added. `localCell:` is still provided by the existing Store
owner, but it creates no facts: this removes use of the fact database, not the
module's entire packaging dependency. Moving that Cell owner is not part of this
change.

### Acceptance and measured retention

A fresh validation prefix was built at `/tmp/birnam-scratch-state/prefix`. Cargo
native compilation reused the development cache; authored bootstrap modules,
authored tools and both startup images were regenerated from source. Installer
compiled-image console/tool checks passed. This is source-bootstrap acceptance,
not a claim of a clean-cache Rust release qualification. The user's installed
prefix was not replaced.

The [new regression runner](../../scripts/scratch-state-test.py) exercises compiled
installed modules, with [the fixture](../../tests/fixtures/scratch-state.birnam)
running three batches of 20 calls for each case:

| Operation | Additional facts in each 20-call batch | Post-GC live-object increase |
| --- | --- | --- |
| Mixed Markdown paragraphs/list/fence | 0 / 0 / 0 | 3 / 3 / 3 |
| Alien signature parsing | 0 / 0 / 0 | 3 / 3 / 3 |
| Invalid Alien signature | 0 / 0 / 0 | 3 / 3 / 3 |
| Nested aggregate / HFA classification | 0 / 0 / 0 | 3 / 3 / 6 |
| Complete Alien C generation | 0 / 0 / 0 | 3 / 3 / 3 |
| NativeDoc summary-only scan | 0 / 0 / 0 | 3 / 3 / 3 |

Each batch checks results, explicitly collects, then compacts `db` and collects
again. Small process-wide object deltas include measurement/temporary-root
overhead; the test permits fewer than 16, not a language-level allocation-count
invariant. Prior probes retained four facts per Markdown call and ten per simple
Alien parse. This acceptance run made no CPU-speed claim; a subsequent paired
CPU comparison is recorded below.

The runner additionally verifies:

- NativeDoc still publishes queryable kind, summary and declaration records.
  The zero-growth NativeDoc fixture has no declarations; it does not pretend
  intentional new documentation entries should create no facts.
- Fourteen same-named fields on an unrelated Store entity retain their values.
- Public `asCell` still creates a fact-backed Cell and returns the original value.
- Replaced Markdown, AlienCompiler and NativeDoc helpers collect and reenter their
  owning operation without corrupting outer or nested state. This exercises live
  dispatch and actual GC, rather than adding guards against replacement.

The runner fails against the original 0.17.0 installation with
`markdown published scratch facts`, then passes against the rebuilt artifacts.
It is now included in [the normal module test command](../../scripts/birnam-test.sh).
The full existing language suite passed **718 checks, zero failures**; that suite
and the dedicated scratch check were run independently before connecting the gate.

The [existing frozen Rust oracle](../../scripts/alien-compiler-test.py) passed
**64 byte-identical generated-C/diagnostic cases and eight ABI cases**, running
without stdlib or source loading. This covers the bootstrap-sensitive compiler
consumer rather than testing only the source overlay.

### Formal verification and limits

[PrivateScratch.lean](../../formal/semantics/lean/PrivateScratch.lean) proves a
value-level simulation with disjoint public keys and private Cell identities:
private writes preserve public values, aliases observe updates, other Cells are
unchanged, and interleaved public publication is preserved over every finite
modeled action prefix. This explicitly retains NativeDoc's catalog writes.

The model intentionally excludes public queries of removed private facts. It does
not prove compiler correctness, native rooting, constructor/callback effects,
allocation/Store-basis equality or image behavior. Those limitations prevent
mistaking the model for a proof that arbitrary Store consumers are unaffected.
Actual module, oracle, retention and collecting/reentrant tests complement it.

The standard formal gate passed **213 Lean theorems**, kernel axiom auditing,
**45 rejected Lean mutations**, and **123 existing TLC cases** with their expected
safe results/counterexamples. New mutations specifically try to overwrite the
public world from a private write and to drop intended catalog publication;
both are rejected. No new TLA+ publication protocol was needed for these authored
local-state changes.

Reproduction:

```sh
python3 scripts/scratch-state-test.py /path/to/rebuilt-prefix
mise exec -- python3 scripts/alien-compiler-test.py /path/to/rebuilt-prefix
BIRNAM_TEST_PREFIX=/path/to/rebuilt-prefix bash scripts/birnam-test.sh
python3 scripts/check-rooting-model.py
```


## CPU follow-up: private scratch changes — 2026-10-13

[Paired measurement receipt](../measurements/private-scratch-performance-20261013.json).
Compared the original installed 0.17.0 prefix with the rebuilt prefix above.
Five fresh processes per variant and workload, alternating before/after order;
40 sequential runs in total. Each operation was warmed once, and every timed
result was checked. C generation was checked byte-for-byte against the frozen
Rust oracle output for `sample` / `l l`.

Normal automatic GC remained enabled. Startup, module loading and result JSON
were outside the operation timer. One explicit post-batch collection was timed
separately. This compares the complete changed prefixes, including their different
startup fact inventories; it does not isolate a single accessor's implementation.

| Workload | Calls per batch | Before median CPU, ms | After median CPU, ms | CPU reduction |
| --- | ---: | ---: | ---: | ---: |
| Mixed Markdown paragraph/list/fence | 100 | 1,206.63 | 1,156.61 | 4.1% |
| Parse `i i` | 100 | 1,877.77 | 1,784.51 | 5.0% |
| Generate C for `sample` / `l l` | 20 | 645.00 | 583.31 | 9.6% |
| NativeDoc summary-only scan | 50 | 103.05 | 94.63 | 8.2% |

Before/after CPU ranges in milliseconds were respectively:
Markdown 1,132.83–1,214.35 / 1,131.19–1,173.13;
parsing 1,836.39–1,912.92 / 1,778.19–1,786.37;
C generation 644.17–651.80 / 579.79–591.28;
NativeDoc 99.13–105.83 / 94.17–96.49.
Markdown's small improvement is less conclusive because ranges overlap; the other
three workloads have separated ranges in these five samples. These are bounded,
warmed workload observations, not an estimate of a whole-language speedup.

Post-batch explicit GC median CPU also fell: 312.90→297.97 ms for Markdown,
334.07→321.38 ms for parsing, 186.56→171.30 ms for C generation, and 88.56→80.16 ms
for NativeDoc. Those timings include collection of the process's startup heap and
are not pure costs attributable to the operation's newly created objects.

All after runs added zero scratch facts. Corresponding before batches retained
400, 1,000, 620 and 100 facts. Thus the change has a modest measured CPU benefit
on these workloads, alongside the more substantial lifetime improvement: temporary
working state no longer accumulates as reachable database records. Longer-running
performance has not been quantified by these short batches.
