# Store and Relational as optional language facilities

2026-10-13. **Status: draft decision document.** The direction is agreed:
retain Store and Relational, including their self-describing bootstrap, and
remove their mandatory role in core language operations. The migration contracts
and sequence below are proposed; the current implementation has not made this
transition.

The first delivered slices provide owner-based reflection and local default Cells.
Ordinary slot reflection and the first Module field migration now follow those
slices. Globals storage, the remaining Module registry, publication and several
bootstrap dependencies still use Store. See [the slot contract](reflect-slots-and-module-state.md).

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
Store-arbitrated and that a system-wide fact mirror be mandatory. Those are
current integration contracts to migrate deliberately, not requirements for the
destination architecture.

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
| Methods and live mutation | Object dictionaries and the supported publication protocol | Selected method descriptions and change history |
| Compiler metadata and Alien recipes | Imageable source and recipe owners | Queryable metadata and recipe descriptions |
| Generation and image recovery | Publication and persistence protocols spanning the affected owners | Inspection or audit data requested by a tool |
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
uses the publication protocol with notifications, deoptimization and code lifetime
preserved. Binding enumeration and reverse naming use the binding owners. Slot,
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
facts with history. The initial mechanism is not selected here. Each exposed
description must specify:

- Which objects and properties it covers, and how identities correspond.
- Whether it is a snapshot or live, and what consistency a query observes.
- Whether writes are supported and, if so, which owner protocol receives them.
- Which history is retained, from when, and what unavailable history means.
- Which references it retains and how participation ends or data is released.

A read-only description cannot silently replace today's writable `db` alias for
Globals. Historical values cannot be reconstructed from a current-only view.
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

The present bootstrap loads Store and Relational before Execution/compiler.
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

Publication failure must still compensate only the operation's own changes and
retain unrelated callback writes. Generation's preparation observations,
authorization, notifications and recovery behavior must move with the protocol.
Replacing them with a whole-world snapshot restore is not equivalent.

Old images and programs can depend on current fact names, writable aliases and
history. Select an explicit compatibility or format transition before removing
those contracts. Do not silently discard their state during loading or leave an
unbounded compatibility mirror in the normal execution path.

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
5. **Move binding and publication responsibilities.** Transition Globals and
   method publication to their owners, including notifications and exact
   presence/value behavior. Provide only the explicitly selected compatibility
   and optional description paths.
6. **Move durable and coordinated state.** Migrate metadata, recipes, Generation,
   Image and process-local state with their full lifetime and recovery contracts.
   Then remove mandatory bootstrap edges and demonstrate core operation with
   Store and Relational absent.

Small migrations should implement this common design. They should not invent
different answers about authority, history or inspection in each module.

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
uses explicit snapshots. Remaining decisions include live code-description
mechanisms, compatibility for existing writable fact interfaces,
image/version policy, and default module-loading policy. These are bounded API
and delivery decisions; the destination is fixed: **relational facilities remain
available and self-describing, while ordinary language operation is independent
of them.**
