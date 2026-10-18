# Store and Relational as optional language facilities

2026-10-13; implementation status reviewed 2026-10-16.
**Status: agreed direction; migration in progress.** The direction is:
retain Store and Relational, including their self-describing bootstrap, and
remove their mandatory role in core language operations. The migration contracts
and sequence below guide the migration; the current implementation has completed
several slices, but not the whole transition.

The delivered slices provide owner-based reflection, local Cells, owned Module,
LSP and Globals state, and [ordinary record fields](ordinary-record-owners.md).
[Ordinary mutation and explicit coordination](coordination-cleanup.md) now have
separate contracts: ordinary edits need no Store observer or Generation handshake;
fresh startup and advanced live loading are distinct. Live loading now leaves
applied changes in place on failure; the user owns the recovery snapshot. Retained compiler metadata now uses compilation records;
recipes, lifecycle/task state, remaining tool state and bootstrap dependencies
still require owner migrations. Backwards compatibility is not a requirement;
useful reflection, late binding, owner lifetimes and explicit user facts remain
required.

## Decision

Birnam's canonical environment is its live, malleable objects. Ordinary state,
bindings, method publication, compilation and image recovery must have owners
whose basic operation does not require a fact database or relational evaluator.

Store remains the facility for deliberately asserted facts, schema, retained
history and temporal access. Relational remains the facility for queries, rules
and aggregates over such data. Programs and tools may opt into using these
facilities to describe code and inspect relationships in the live environment.
Ordinary execution does not implicitly opt every object, field or definition in.

Keep the self-describing bootstrap of Store and Relational. Their own schema,
rule definitions and bootstrap descriptions remain inspectable language data.
Removing their role as core language instruments does not justify replacing
their self-description with hidden native tables or deleting their ability to
describe themselves.

Availability and participation are separate choices. A standard distribution may
ship these modules, and a tool may explicitly depend on them. Merely making them
available must not enroll the rest of the language in fact storage, mirroring or
history. Exact default loading and command-line policy are deferred; the target
dependency graph must permit core execution without loading them.

## Why this boundary

The [Store audit and planner experiment](store-planner-experiment.md) distinguish
three needs that the current integration often combines: owning state, exposing
it to generic queries, and coordinating publication or recovery.

Private scanner state stored in global `db` remained reachable after its operation
finished. Disabling history and compacting did not remove current facts. Converting
Markdown, AlienCompiler and NativeDoc scratch to local Cells removed accumulating
scratch facts in the tested cases. This is an ownership correction as well as a
measured CPU improvement.

The planner obtained history and restoration from persistent Map/Vector roots.
Its Store backend used 47–54% more measured phase CPU, largely to reconstruct
graph views. That result supports using owned collections for this application;
it does not establish that relational queries or temporal databases are generally
inferior. Cross-domain queries and deliberate fact history remain useful reasons
to select Store.

The separate [private runtime index optimization](private-runtime-indexes.md)
improved application CPU without removing any public Store dependency. Runtime
cost and architectural value must continue to be evaluated separately.

## Relationship to existing decisions

This direction preserves [Design 42's object authority](42-object-canonical-relational-derived.md)
and [Design 90's functional ownership](90-functional-modules.md). It changes
Design 42's requirement that supported post-bootstrap method mutation be
Store-arbitrated and that a system-wide fact mirror be mandatory. Those mandatory contracts have been removed from ordinary edits by the
[coordination cleanup](coordination-cleanup.md). Explicit coordinated operations
still own their admission and recovery contracts.

Where a relational description is enabled, the honesty requirement remains:
it must not silently claim a stale method is current. Its coverage and observation
basis must be explicit. A historical description must be identifiable as such.

This decision does not itself change query equality, arithmetic, ordering or
history semantics, and does not resume the paused Relational rewrite. That work
must follow the agreed late-bound language direction in its own migration.

## Ownership in the destination architecture

These are responsibilities, not a requirement to create a crate or service for
every row.

| State or operation | Authoritative home | Optional relational use |
| --- | --- | --- |
| Ordinary fields and local mutation | Objects, slots, Cells and collections | Explicitly published descriptions or application facts |
| Globals and module registry | Binding and module owners | Names, dependencies and selected binding history |
| Methods and live mutation | Object dictionaries, with Method construction and lifetime owned by their existing protocols | Selected method descriptions and change history |
| Compiler metadata and Alien recipes | Imageable source and recipe owners | Queryable metadata and recipe descriptions |
| Generation and image recovery | Explicit coordination and persistence operations spanning the affected owners | Inspection or audit data requested by a tool |
| Tasks, workers and I/O handles | Their lifecycle owners | Explicit diagnostics with bounded retention |
| Asserted facts, schema and retained transitions | Store | Its primary purpose |
| Rules and query evaluation | Relational using Store data and per-query state | Its primary purpose |
| Documentation catalog | May deliberately use Store and Relational | A valid opt-in consumer, separate from its private scanner state |

Ordinary mutable state follows object reachability. Publishing facts deliberately
adds Store ownership and may extend the lifetime of referenced objects. History
adds further retention according to its selected policy. Image-transient markers
must not be mistaken for weak references or cleanup rules.

Local Cells must be usable independently of Store. The target default mutable-cell
abstraction is local; a fact-backed Cell is an explicit database choice. The
previous public `asCell` created facts and advertised history. The implementation
now selects local Cells; this is the deliberate API transition described below.

The everyday spelling remains `[value asCell]`, followed by `value`
and `set:` on the resulting Cell. It creates no facts and retains no implicit
history. Explicit database-backed mutation remains available through
`[db cellFor: entity attr: attribute]`, with history enabled deliberately. This
transition does not rewrite existing fact-backed Cell objects or remove their
facts. The implementation and validation scope are recorded in
[local Cells by default](local-cells-by-default.md). Local Cell construction
now uses the [authored Cell owner](standalone-cells.md) over ordinary reference
slots. Legacy Store Cells remain supported for explicit callers and old images.

## Reflection and code descriptions

**Key guarantee: removing automatic facts must not reduce Birnam's inspectability
or malleability.** Reflect remains a core API independent of Store and Relational.
Before removing public system facts, provide direct owner-based inspection and
supported mutation for the intentional language state they expose. Preserving
named getters while losing the ability to discover or change that state does not
satisfy this guarantee. Historical queries remain an explicit retained-history
facility; incidental private scratch facts need not be recreated.

Core reflection must inspect the actual owners of objects, slots, bindings,
methods and source metadata. Core mutation must use their supported protocols.
Neither requires those owners to publish a second authoritative fact copy.
Moving data must preserve useful inspection and malleability, not hide it behind
opaque native state.

Keep existing Reflect spellings and behavior where they express these operations.
Method inspection reads authoritative dictionaries; method definition/removal
uses actual dictionary-owner messages, with dictionary invalidation, deoptimization
and code lifetime preserved. Ordinary edits require neither Generation admission
nor a Store observer; optional observation is explicitly enabled. Binding enumeration and reverse naming use the binding owners. Slot,
identity, vtable and source inspection remain possible without creating facts.
This does not claim that every such operation already has a complete Reflect API.

`Reflect factsOf:` specifically means asserted database facts. Retain that meaning
as an explicit Store-dependent extension or equivalent database-facing API; do
not silently reinterpret it as ordinary slots. Documentation helpers may likewise
compose with a deliberately enabled catalog. The core reflection operations must
work when neither extension is installed.

Optional relational code descriptions consume Reflect and other owner protocols;
Reflect does not depend on those descriptions. See
[Reflect without mandatory Store](reflect-without-store.md) for the current API
inventory, gaps and proposed shape of this meta-object protocol.

An opted-in code description may be a snapshot, a live derived view, or retained
facts with history. The implemented initial mechanism is explicit snapshots in
the code-inspector experiment; a generic live projection is not required. Each exposed
description must specify:

- Which objects and properties it covers, and how identities correspond.
- Whether it is a snapshot or live, and what consistency a query observes.
- Whether writes are supported and, if so, which owner protocol receives them.
- Which history is retained, from when, and what unavailable history means.
- Which references it retains and how participation ends or data is released.

Globals now owns its bindings; explicit facts at the former keys are independent
data. Direct owner mutation replaces the former writable fact alias. No legacy
alias compatibility is required. Historical values cannot be reconstructed from
a current-only view.
An implementation that always mirrors the entire world but merely hides the
query interface until requested does not satisfy opt-in participation.

Code descriptions may be incomplete by design; their coverage must be honest.
Snapshot extraction and live queries must account for collecting and reentrant
callbacks. A live projection needs a consistency protocol before implementation,
not an assumption that two independently mutable copies will stay synchronized.

Ordinary object operations remain late-bound. Authored algorithms do not acquire
defensive checks around their sends to anticipate replacement. Any compiler or
runtime optimization that bypasses those sends must retain the applicable
admission, recovery and deoptimization guarantees. Optional description must not
freeze definitions or change dispatch authority.

## Bootstrap and persistence

The desired construction order has the core objects and protocols needed by
Store available independently, then constructs Store and its self-description,
then Relational and its self-description. Tools can subsequently request code
descriptions. Keep whatever bounded initialization is needed to establish the
modules' own descriptive data; it must not make unrelated core facilities depend
on a global fact world.

The present bootstrap loads Execution before Store, but still loads Store and
Relational unconditionally before the compiler and Generation.
Removing isolated callers is insufficient to change that order. Inventory and
remove the actual dependencies before claiming either module is optional.

Preserve clean-checkout cold bootstrap and the eventual ability to build Birnam
entirely in Birnam. Necessary Rust seeds may remain during migration; this
decision does not authorize a permanent Rust binding, metadata or publication
service as their replacement.

Image must preserve the authoritative objects and any deliberately retained Store
state through owner protocols. Compiler metadata must remain available without
source reparsing or recompilation at image/artifact startup. Preserve source
identity, shared Cells, aliases and code lifetimes. Moving facts into a Map keyed
by value-equal Strings is not automatically identity-preserving.

Operations that explicitly promise compensation must undo only their own changes and
retain unrelated callback writes. Generation's preparation observations,
authorization, notifications and recovery behavior must move with the protocol.
Replacing them with a whole-world snapshot restore is not equivalent.

Backwards compatibility is not required. Specify a rebuild/version transition
when a storage or image change needs one; do not add indefinite adapters or a
compatibility mirror. Explicit user facts must not be silently discarded or
reinterpreted as ordinary fields during loading.

## Migration sequence

1. **Establish the contract.** Record the default Cell API transition and define
   owner-level inspection and mutation requirements. Inventory public fact
   consumers separately from private scratch. Record changes to Design 42 and
   user documentation alongside the corresponding implementation slices.
2. **Finish private state cleanup.** Convert remaining private accumulators and
   traversal state using existing authored protocols. Separate local Cells from
   Store packaging when their rooting and image contracts are ready.
3. **Exercise opt-in code description.** Build a small unattended tool joining
   module, definition, method and documentation information, with one useful
   change query. Compare explicit facts with owner inspection or a derived view.
   This selects a useful integration mechanism; it does not reopen the decision
   to remove mandatory core dependence.
4. **Migrate ordinary records and registries.** Move fields, LSP state and Module
   bookkeeping in bounded slices. Replace intentional public inspection paths
   before removing their facts; verify bootstrap after each affected slice.
5. **Separate ordinary mutation from explicit coordination.** Globals owns its
   bindings. Reflect composes Method construction and actual dictionary owners;
   ordinary edits need no mandatory publisher, Generation authorizer or fact mirror.
   Keep explicit change-set and optional description contracts separate.
6. **Move durable and coordinated state.** Migrate metadata, recipes, Generation,
   Image and process-local state with their full lifetime and recovery contracts.
   Then remove mandatory bootstrap edges and demonstrate core operation with
   Store and Relational absent.

Small migrations should implement this common design. They should not invent
different answers about authority, history or inspection in each module.

## Progress as of 2026-10-17

The five ordinary-object field migrations are committed as `e41c4f35`; Bench/Test
ownership is committed as `5c70fe4c`. The basic
live-loading simplification is committed as `3869bd65`. Coordination cleanup is committed through
`bd8a532e`; generation lifecycle simplification is committed through `c4bf4451`.
Implementation status does not imply release or local installation.

| Plan area | Delivered | Remaining |
| --- | --- | --- |
| Contracts, private scratch and reflection | Ownership decision, scanner scratch cleanup, local Cells, binding/dictionary/slot inventories | Apply the same discoverable-owner contract to remaining public state. |
| Opt-in description experiment | Code inspector with explicit snapshots, joins and retained observations | A generic live projection is not selected or required for the first optional core. Keep coverage, history and retained references explicit. |
| Ordinary records and registries | Module registry, LSP, Globals, Logger, Config, HttpRequest, ProcResult and AppConfig fields; Bench/Test fields, local scratch and owned suite discovery implemented and qualified | REPL handles/buffers belong with socket/task resource-lifecycle work. |
| Step 5: construction and publication | Independent Method construction/templates; direct Reflect edits; no default authorizer/mirror; direct Generation writes; descriptive snapshots; separate startup/live Image contracts | Basic live loading now reports partial application without compensation; stronger recovery is an optional future feature, not a migration prerequisite. |
| Step 6: durable/coordinated and process-local state | Globals participates in explicit owner recovery; non-publishing inspections and retained compilation records; lifecycle fields, pure status reads and non-destructive record retirement implemented and qualified; authored source-registry ownership implemented and qualified | Alien recipes, remaining Image staging, task/worker/I/O state and remaining seeds. |
| Final independence | Individual Store-absent floor tests exist | Core cold bootstrap, compile/run, reflection/editing and image roundtrip without Store/Relational; companion optional-module/self-description acceptance. |

The [supplied-template report](supplied-method-templates.md) records constructor
qualification. The earlier universal publication-owner proposal was superseded by
the [minimal-mutation decision](minimal-mutation-and-coordination.md); the
[coordination cleanup](coordination-cleanup.md) implements that boundary. Broader
representation-independent error normalization remains separate work. The [retained compilation record cutover](retained-compilation-records.md) implements
compiler metadata ownership with its compiled-startup, lifetime and image contracts.
Source-registry ownership is implemented in [the authored-owner slice](authored-source-registry.md). Lifecycle field ownership is
implemented in [the record-owner slice](lifecycle-record-owners.md); the approved
coordination simplifications are implemented and qualified.

The [source-registry Id boundary check](source-registry-id-boundary.md)
(2026-10-18) supports an ordinary authored owner with Id-only consumers. It is
design evidence, not an implemented port. Before cutover, resolve candidate
commit's newly callback-capable source write and the owner's early bootstrap
installation. Remove unused image-source rollback rather than preserving it by
translation; settle counter semantics and image-owner identity explicitly.
The follow-up [one-way application slice](one-way-image-application.md) removes
recoverable capability backups and image-source rollback and passes its formal,
Host, native/image, language and compiled-startup qualification. The implementation
is committed with its qualification report. The subsequent
[authored source registry](authored-source-registry.md) implements the owner
migration and records passing formal, native/image, Host, language and cold/compiled
bootstrap qualification.

The [Generation lifecycle review](generation-lifecycle-review.md) sequence is
complete: code ownership, module-scoped batch edits/undo and optional whole-program
certification are separate. The [post-simplification review](generation-post-simplification-review.md)
records the resolutions of two reproduced API inconsistencies and explicit
certificate release, with regression and native ownership evidence.

Backwards compatibility is not required for the remaining migration. A deliberate
artifact/image format transition may require rebuilding instead of retaining old
adapters. This does not authorize silently discarding explicit user facts, weakening
current-world recovery, or preserving a permanent native service under a new name.

### Superseded investigations and current implementation

The [concerns investigation](store-refactor-concerns.md) recorded callback-write
loss on its historical baseline and explored a result-aware publication pipeline.
The [minimal-mutation decision](minimal-mutation-and-coordination.md) superseded
that universal pipeline: ordinary edits need no publisher, mandatory description
step or managed observer. Broad error normalization and startup-memory proposals
remain separate work, not prerequisites for optional Store.

The [ordinary editing/snapshot cutover](ordinary-edit-and-snapshot.md) implements
direct Reflect edits and snapshots of edited worlds. Drifted managed metadata stays
descriptive and cannot reactivate old capabilities. The
[coordination cleanup](coordination-cleanup.md) removes default mirror registration
and Generation's observer handshake, separates startup from live loading, and fixed
callback-write erasure with conditional recovery. The later basic-live-loading
decision below replaces compensation with explicit partial application. These are delivered changes,
not pending publication work. Explicit Generation transactions retain their own
checks and journals.

### Selected live-load failure contract (2026-10-16)

The user selected basic live loading without automatic compensation. Loading an
image into a running world is an explicit, potentially partial mutation. Validate
and stage it before applying it where possible; if application fails, stop and
report the original failure with `System imageSaveReport` status `#partialLoad`.
Do not undo applied changes or callback writes at the failure boundary, replay
callbacks, or run inverse writes. Later ordinary writes during loading may still
supersede earlier values. Clear composition identity and abandon stale prepared/adoption capabilities.
Cleanup still releases staging resources; Execution still owns entered/captured code.
Early admission failures retain the ordinary rejection report, but replaceable
validation/staging callbacks are not promised side-effect-free.

It is the user's responsibility to save an image before loading another if recovery
is desired, and to check that saving succeeded. No automatic checkpoint, “last
known-good” image, restart, or retry is introduced. A failed live load is not a
usable restored snapshot by guarantee. The user can discard that world and start
a fresh runtime from their saved image; trying another live load remains their
explicit choice and can fail again. Unsaved work and external effects are outside
snapshot recovery. The loader does not automatically rerun main.

Fresh startup still requires discarding its failed world; application-phase failure
reports `#startupFailed`. Both modes retain format/native-code validation, rooting,
source-free mapped restoration and explicit rebuild. Successful load behavior and
image format are unchanged. Explicit Generation rollback remains a separate
requested operation with its own contract; ordinary edits have no rollback journal.

This replaces the preceding conditional live-compensation implementation. The
[coordination cleanup](coordination-cleanup.md) remains historical evidence for why
unconditional snapshot rollback was wrong; its inverse pass and conservative
conflict admission are no longer the basic loader contract. The
[basic live-loading report](basic-live-image-loading.md) records the simplification.

Future robust loading could add an explicitly selected operation with owner receipts,
isolated staging or checkpoints. It must state its limits and retain the basic
loader; stronger recovery must not become a requirement for ordinary mutation or
Store independence. No extra framework is needed now to reserve that possibility.

### Terminology: ordinary objects and facts

“Ordinary records” means objects carrying fields, such as Logger, HttpRequest and
ProcResult. It does not mean database facts or a new language-level record type.
Their fields previously lived in triples keyed by object identity and attribute;
the migrated objects own reference slots instead. Explicit facts about those same
objects remain separate data, even when attribute names match slot names.

Cloning these objects now copies field references into independent slots. Replacing
a clone's slot does not change the original; mutating a referenced object can be
visible through both. Previously a raw clone had a fresh identity with no field
facts at that identity. This is a deliberate semantic transition, not merely an
unobservable representation optimization, and does not copy or delete user facts.

## Actionable migration safeguards

- [ ] Maintain a dependency ledger for each remaining state family: authority,
  readers/writers and query consumers, inspection/mutation API, retention, image
  policy, and bootstrap edge. Track native seeds, generated artifacts and implicit
  module initialization as well as direct `db` calls. Close an entry with evidence.
- [x] Qualify Bench/Test ordinary fields, scratch and discovery. Test now owns its
  suite registry and explicitly describes suites into a caller-selected Store;
  intentional discovery remains available without facts. See
  [the implementation report](test-and-bench-owners.md).
- [ ] For each field migration, document and test clone/reference sharing,
  identity versus value equality, aliases, absent versus nil, reflective edits,
  reentry/collection and explicit same-name facts. Preserve explicit facts and
  intentional discovery; do not add method-identity guards around ordinary sends.
- [ ] Audit REPL input/error handles and buffers with socket/task/worker state.
  Replace the current `imageTransient` fact policy explicitly; specify cleanup,
  ownership and recreation/rejection on load before moving resources into slots.
  Test failed construction, shutdown, image restoration and retention.
- [x] Qualify non-publishing `moduleMetadataOf:` / `embeddedFilesOf:` queries.
  Leave revision preparation and compiled admission separate; do not replace Store
  with a permanent source-keyed cache. Record the durable dependency ledger in
  [compiler metadata ownership](compiler-metadata-ownership.md).
- [x] Qualify retained compilation records and their registry/image integration.
  Metadata is now ordinary data with explicit inspection/replacement APIs. Preserve source identities and entered/captured code lifetimes; exercise
  source-free mapped startup with parsing/compilation unavailable and the explicit
  rebuild path separately. Reject missing metadata rather than falling back to ASTs.
- [x] Move lifecycle fields to ordinary owners, keep explicit facts independent,
  remove implicit status reconciliation and destructive record retirement, and
  qualify exact rollback, retained code and images. See
  [lifecycle record owners](lifecycle-record-owners.md).
- [x] Remove token-long global dictionary/Globals counter checks and the
  exactly-one setter mutation rule; retain observed-value checks and conditional
  inverses. See [prepared operation admission](prepared-operation-admission.md).
- [x] Narrow transaction admission, postconditions and compensation to selected
  bindings/Globals and lifecycle observations. Preserve outside managed edits and
  allow retry after an affected binding is restored. See
  [scoped transactions](scoped-generation-transactions.md).
- [x] Restore/resave images without automatic generation adoption, replacement
  component units or lifecycle reconciliation. Keep explicit baseline adoption and
  source-free mapped/rebuilt restoration; see [explicit image adoption](explicit-image-adoption.md).
- [x] Remove ordinary console/TCP/worker adoption and all source-registry sealing;
  make explicit validation observational. See [opt-in baselines](opt-in-generation-baselines.md).
- [x] Finish separating optional whole-program certification from ordinary
  compilation/loading. Preserve meaningful conditional
  undo and source-free restoration; do not relocate the old policy unchanged.
- [x] Complete generation simplification before resuming Store migration: module-scoped
  candidates, renewable optional certificates, ordinary activation, and logical
  discard separate from files/diagnostics. See [generation review status](generation-lifecycle-review.md).
- [x] Close the [post-simplification review](generation-post-simplification-review.md)
  findings: dependency comparison without certification, discarded-predecessor
  status on successor rollback, and explicit release of optional certificate roots.
  Keep exact code ownership and undo; do not reintroduce whole-world admission.
- [ ] Before migrating each coordinated owner, record which operations actually
  promise rollback and why. Keep basic live Image loading free of inverse machinery; add stronger recovery
  only as an explicitly selected feature with a demonstrated requirement. Preserve
  applied/callback writes, truthful partial-load reports and stale-capability
  invalidation, including collecting/reentrant failure paths.
- [ ] Specify Alien recipe durability separately from process-local addresses and
  resources. Preserve reconstruction, reflection and code ownership without a
  permanent opaque native registry or a replacement global fact service.
- [ ] Audit persistence of independent Store instances as an optional-facility
  follow-up. The Bench/Test qualification reproduced rejection of an image capturing
  `Store new` on the pre-migration runtime; canonical `db` descriptions persist.
  Document that boundary rather than implying every runtime snapshot is imageable.
- [ ] Define any artifact/image version or rebuild transition before changing its
  representation. Keep explicit facts and retained history independent; reject an
  unsupported format rather than silently dropping data. Compatibility adapters
  are not required.
- [ ] Establish the final Store/Relational-absent core acceptance path and companion
  optional-module/self-description path. Isolated exported-method floor tests are
  useful evidence but do not prove cold bootstrap, compile/run or complete loading.
- [ ] Keep this status table authoritative. Mark historical probes and superseded
  proposals at their source; distinguish implemented/qualified, committed, released
  and installed state. Review owner boundaries before accepting a new registry,
  mirror, transaction framework or native service as a dependency replacement.

Bench/Test is committed and non-publishing project inspection is qualified. The
retained compiler metadata ownership is now qualified; the remaining structural
work includes Alien recipes and remaining image/candidate coordination; keep resource-bearing REPL state with
lifecycle work. The selected live-load change is stated above; these tasks do not authorize changing
GC policy or resuming the paused Relational rewrite.

## Acceptance and remaining decisions

For each migration, check output and errors, live replacement, aliasing,
collecting/reentrant callbacks and retained-state behavior. Measure CPU and memory
where savings are claimed. Explicit fact consumers must retain their selected
query and history behavior, including Store and Relational self-description after
bootstrap and image restoration.

Use Lean for the concrete identity, aliasing and state-equivalence claims being
made. Use TLA+ where new publication, projection or recovery transitions require
temporal reasoning. Models must cover the proposed contract; the existing private
scratch and index proofs do not establish that this architecture is implemented
or that all public fact removals are safe.

Completion requires an automated path that boots the core, compiles/runs code,
reflects on and changes definitions, and saves/restores a core image without
Store or Relational. A companion path must load the optional modules, exercise
their self-description, opt into code description and verify its declared
mutation/history/lifetime behavior. Exact prefix composition and test commands
will follow the dependency migration.

The local Cell default is implemented and the first code-description experiment
uses explicit snapshots. Remaining decisions include whether to add live code-description
mechanisms, explicit opt-in description/write contracts, image/version transitions,
and default module-loading policy. Legacy writable fact aliases need not be preserved. These are bounded API
and delivery decisions; the destination is fixed: **relational facilities remain
available and self-describing, while ordinary language operation is independent
of them.**
