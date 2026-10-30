# Reflect without mandatory Store

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

2026-10-13; current API summary reviewed 2026-10-16. **Status: owner inspection,
ordinary slots, owned Globals and direct method edits implemented; source metadata
and full Store-independent bootstrap remain.** The implementation records below
are dated slices, not a cumulative list of remaining work.
This elaborates the reflection guarantee in
[Store and Relational as optional language facilities](../../design/optional-store-and-relational.md).
Existing selectors below were checked against current source; additional selector
names are explicitly proposals.

## Purpose and value

Reflect remains the language's core mirror and meta-object protocol: a place to
inspect the structure of the live environment and deliberately change it through
the responsible owners. It does not require a database copy of that environment.

The useful separation is between ordinary application messages and messages
about objects, bindings and definitions. An inspector can ask Reflect about an
object without requiring that object to implement every tooling selector. A live
editor can change a method through the supported publication path. A browser can
discover definitions without knowing native dictionary layouts. These remain
valuable independently of joins or history.

**Removing automatic facts must not reduce Birnam's inspectability or
malleability.** Intentional system state must remain discoverable and mutable
through meaningful language protocols before its public fact representation is
removed. This does not require reproducing incidental parser temporaries, inventing
source for native methods, or retaining history that was never requested.

Relational descriptions become an optional consumer of this protocol. They add
cross-domain queries, rules and selected history. They do not supply the authority
that makes ordinary reflection or mutation possible.

## What Reflect actually does today

There is no separate authored `reflect.birnam` module today. Codegen Runtime
publishes Reflect and installs its native methods; stdlib extends it. The public
receiver is already a useful boundary, even though its implementation placement
and dependencies need work.

Primary source: [reflection operations](../../../rust/birnam-codegen-runtime/src/reflection.rs),
[installation and mutation](../../../rust/birnam-codegen-runtime/src/lib.rs), and
[stdlib extensions](../../../modules/stdlib.birnam).

| Existing message | Current behavior | Store dependence and destination |
| --- | --- | --- |
| `[Reflect vtableOf: object]` | Sends Id `behaviorOf:` | Already a direct owner query; retain |
| `[Reflect nameOf: symbol]` | Converts a Symbol to a String; rejects other kinds | No fact access; retain. This is not reverse naming of arbitrary objects |
| `[Reflect globalNamed: name]` | Tries Namespace, then Globals when Namespace is absent | Globals now owns its bindings; lookup precedence is retained |
| `[Reflect allGlobalNames]` | Enumerates Namespace exports only | No fact access, but does not enumerate source-owned Globals; address the coverage gap explicitly |
| `[Reflect allMethodsOf: object]` | Walks actual vtable dictionaries from child to parent, deduplicating selectors | Names through owned Globals after Namespace reverse lookup fails; no fact lookup required |
| `[Reflect defineMethod: selector on: target as: block]` | Constructs a Method, obtains a private vtable when needed, then sends the dictionary owner `addMethod:as:` | No mandatory Store publisher or Generation veto; retain execution ownership/invalidation |
| `[Reflect removeMethod: selector on: target]` | Checks direct presence, then sends the dictionary owner `removeMethod:` | No mandatory Store publisher or Generation veto; preserve removal and inheritance behavior |
| `[Reflect factsOf: entity]` | Enumerates actual global `db` triples into Maps | Explicit Store extension; keep its fact meaning |
| `[Reflect isSymbol: object]`, `[Reflect isString: object]` | Authored vtable comparisons | No inherent fact requirement; retain |
| `[Reflect defineMethod: selector on: target as: block doc: text]`, `[Reflect docModule: name is: text]` | Install methods and conditionally publish documentation when Doc is present | Retain useful convenience; put catalog policy/access behind an optional Doc protocol |

`allMethodsOf:` answers a Vector of Maps with `#selector`, `#target` and `#method`.
The target is a discovered name or nil. The method is the implementation object
reported by Execution, or nil if unavailable; it is not a promise that every
binding has a fully reified, invocable Method in this row.

The dictionary walk is already independent of the relational evaluator. It omits
shadowed parent entries and does not model arbitrary custom lookup behavior. It
also sends multiple messages while walking; the returned Vector is not evidence
of an atomic snapshot of the whole environment.

Existing compiler integration tests exercise the basic four inspection selectors
and live definitions. They do not prove complete source-global enumeration or a
Store-free startup path. This investigation is source analysis, not new runtime
qualification.

## Proposed shape

Keep Reflect as a small, ordinary, replaceable facade over functional owners.
Move as much composition as practical into Birnam. Do not create a second universal
runtime service, permanent native metadata registry, or mandatory allocated mirror
object for every inspection.

The existing public spellings remain useful:

```birnam
[Reflect vtableOf: object]
[Reflect allMethodsOf: object]
[Reflect globalNamed: #Example]
[Reflect defineMethod: #answer on: object as: [:self | 42]]
[Reflect removeMethod: #answer on: object]
```

The calls inspect or mutate live owners without asserting descriptive facts.
Their existing target interpretation, success values and errors need regression
coverage during migration. In particular, defining on an instance can create a
private vtable; removal must delete the actual direct binding, allowing inherited
behavior to become visible. Installing a copied parent method is not removal.

An explicitly retained Method or mirror handle may keep its target/code alive.
That is ordinary reachability chosen by its caller, unlike a globally rooted
fact mirror retaining every described object. Start with the existing collection
results; add live mirror objects only where a concrete editing tool benefits.

### Inspection gaps to close

The first four interfaces below were implemented by the initial inspection slice.
Ordinary reference-slot discovery and access are now implemented by the
[slot and Module field slice](../../design/reflect-slots-and-module-state.md); that document
defines the index/name keys, snapshot rows and unsupported-storage boundary:

| Message | Required meaning |
| --- | --- |
| `[Reflect directMethodsOf: object]` | Direct dictionary entries on the object's vtable, including their owning vtable identity; no inherited deduplication |
| `[Reflect parentOf: vtable]` | Expose delegation structure through the owner, enabling traversal of shadowed definitions |
| `[Reflect bindingsIn: bindingOwner]` | Enumerate that owner's names and values with exact presence, including bound nil |
| `[Reflect namesOf: object in: bindingOwner]` | Return aliases by identity, without assuming an object has one canonical name |
| `[Reflect slotsOf: object]` | Enumerate ordinary storage through its owner, with readable/writable status and stable owner-defined slot identifiers |
| `[Reflect slot: slot in: object]`, `[Reflect slot: slot in: object put: value]` | Read/write through the storage owner; preserve aliases, bounds, tracing and mutation admission |

Id already provides dictionary, parent and binding messages.
[ObjectModel](../../../rust/birnam-object-model/README.md) already provides indexed
ordinary slot access and layout metadata. Those are starting points, not a claim
that named field discovery is complete. Moving named facts into unnamed positional
storage must not erase their useful names: the owner needs descriptive metadata
or an equivalent discoverable protocol. Native families expose their own logical
state through owner messages; Reflect must not decode foreign payloads.

Keep structural dictionary inspection distinct from actual message lookup.
Execution already supplies `methodFor:` on objects/vtables to capture current
lookup; the dictionary inventory must not pretend to predict every custom lookup
or does-not-understand path. Retained captures and later fresh lookups have
different meanings.

For globals, preserve the difference between Namespace exports and source-owned
Globals. The current `globalNamed:` is Namespace-first; source execution has a
different Globals-first, nil-triggered fallback contract. Do not accidentally
unify those rules while fixing `allGlobalNames`. Prefer explicit owner enumeration
first, then decide how a combined convenience handles collisions and order.

Source and compiler metadata also need a discoverable owner interface, with
identity-preserving associations and an honest indication when source is absent.
The current compiler metadata protocol is on Codegen, not Reflect. Reuse or
compose that interface rather than adding redundant source ownership to Reflect.
Metadata must survive image startup without reparsing or recompilation.

### Ordinary method edits without Store

The implemented path is:

```text
Reflect definition/removal
  -> Method construction and target/private-vtable resolution as required
  -> actual dictionary-owner addMethod:as: / removeMethod:
  -> existing dictionary invalidation and execution lifetime handling
```

Ordinary edits have no mandatory authorization, Generation phase check, observer
acknowledgement, fact mirror or rollback journal. Explicit Generation transactions
retain their own preparation, validation and selective compensation. This supersedes
this document's original proposal to transfer all publication machinery out of Store.
See [ordinary editing](ordinary-edit-and-snapshot.md) and
[coordination cleanup](coordination-cleanup.md).

Optional descriptions consume explicit snapshots or an explicitly enabled observer.
Core execution requires neither. A future observer must specify callback failure,
reentry, ordering and retention; it must not become an authority for ordinary edits.

Reflect itself remains inspectable and changeable through ordinary language
mechanisms. Reflection introduces no frozen dispatch or per-send defensive
monkeypatch checks. Optimized execution retains its existing admission and
deoptimization obligations.

## What the relational extension contributes

A tool can deliberately describe selected modules, methods, names and documents
in a Store and ask questions such as which directly defined selectors in a module
lack documentation, or which selected definitions changed since a retained basis.
Reflect supplies current structure and supported mutation; Store supplies the
deliberately retained facts; Relational supplies joins, rules and aggregates.

Start the evaluation tool with an explicit snapshot in a tool-owned Store and a
declared inventory. This recommendation avoids committing to automatic live
mirroring before a use case demonstrates its value. A snapshot means the data
actually captured, not a claimed atomic whole-world state: define a capture
boundary or expose its observation limits. Relational history starts when facts
are retained; it cannot recover previous live definitions retrospectively.

`factsOf:` remains an optional convenience for actual facts in `db`; querying a
different Store uses that Store's API. An object can have ordinary slots and
independently asserted facts, so the two inventories must remain distinguishable.
Explicitly enabling documentation can similarly install/use catalog extensions
without making every method definition depend on a database.

## Original first-slice boundary and continuing acceptance

The binding/dictionary inspection slice below is delivered, followed by ordinary
slots, owned Globals, and direct edits. Its original sequencing is retained as
context; use the current Store decision for the outstanding migration tasks.

Begin with owner-level binding enumeration/reverse naming and a precise direct
method inventory. Route Reflect, REPL global listing and Doc naming through those
interfaces while retaining the current storage backend. This establishes a usable
replacement for direct fact inspection before storage is removed; it is not yet
a Store-independent core.

Define the ordinary-slot discovery contract before migrating named public fields.
Keep direct ordinary edits separate from explicit Generation coordination, and
retain the latter's recovery tests during owner migration. Split
inspection from compiler-dependent definition construction so basic reflection
does not require loading a compiler merely because they share a source file today.

Acceptance must cover direct versus inherited and shadowed methods, private
vtables, custom lookup, primitive bindings, aliases and unnamed targets,
Namespace/Globals collisions, absent versus nil, slot identity and shared Cells,
source availability, collecting/reentrant calls and method replacement/removal.
Inspecting a live owner again must observe its current state; previously returned
descriptions must not masquerade as automatically updating views.

For each public fact family removed, record how its intentional information and
supported edits remain accessible. Run the final core tests without Store or
Relational loaded, and separate extension tests with them enabled. Model new
publication/recovery state transitions with Lean/TLA+ as appropriate, alongside
real active/suspended-code, callback and image acceptance. No speedup or complete
semantic equivalence is established by this design investigation alone.

## Historical first inspection slice implemented

The remaining-dependency statements in this subsection describe that initial slice.
Globals storage, local Cells and ordinary editing were migrated afterward.

The first slice adds these public messages:

| Receiver and message | Delivered contract |
| --- | --- |
| Namespace or Globals `liveBindingsDo: visitor` | Owner snapshot in existing order; sends `binding:value:` for each present binding, including bound nil |
| Globals `nameOf: object` | First identity alias or native Unit; preserves the former reflection naming path's replaceable Store reads |
| Reflect `bindingsIn: owner` | Authored facade returning a Vector of `#name`/`#value` Maps through the owner's current `liveBindingsDo:` |
| Reflect `namesOf: object in: owner` | Authored visitor selecting all identity aliases without constructing Maps for other bindings |
| Reflect `directMethodsOf: objectOrVtable` | Direct dictionary inventory only; accepts a vtable itself, or obtains an object's vtable; adds exact `#vtable` identity to the existing selector/target/method row shape |
| Reflect `parentOf: vtable` | Forwards to Id's current parent query; absent parent becomes language nil |

`allMethodsOf:` now asks Globals for reverse naming instead of naming Store's
internal `__globals__` entity. Its inherited/deduplicated result shape remains
unchanged. `allGlobalNames` and `globalNamed:` retain their existing semantics;
explicit owner enumeration supplies the missing source-binding inventory without
silently changing collision or source-resolution behavior.

REPL global listing uses Reflect's new inventory; Doc uses Globals' first-alias
query, avoiding full inventory construction when it needs just one name. REPL's
`#gshown` scratch fact is no longer needed. The collectors for `bindingsIn:` and
`namesOf:in:` use ordinary ObjectModel slots, with shared prototypes and per-call state, so
inspection itself asserts no facts. Public `asCell`, `factsOf:`, method publication,
compiler metadata and Globals' underlying fact representation remain unchanged.

Globals' snapshot reads its owned binding storage, not customized `at:` behavior.
This is a new inventory contract, distinct from evaluating a getter repeatedly.
Namespace's existing `bindingsDo:` still includes native Unit entries; the new
`liveBindingsDo:` excludes them. Both Namespace paths now explicitly retain the
copied pairs and visitor across callbacks. This closes a rooting gap where a
callback could overwrite a later export and collect its only remaining value.

A native regression also reproduced a return-handoff gap in reflection's temporary
root cleanup: collecting when the final inventory token was released reclaimed
the result before its caller received it. Inventory and binding-query operations now use
Roots' existing `releaseMany:protecting:batch:` protocol, preserving their forward
release order and individual replaceable `release:` sends. The result remains
covered through cleanup; failure drains the scope and propagates. `parentOf:`,
like `vtableOf:`, directly forwards with caller-covered inputs and no cleanup
callback after obtaining the result. No new monkeypatch admission check is added.

The regression exercises direct and inherited inventories and global-name results,
plus collecting cleanup failure and subsequent reclamation. Existing rooting and
handoff formal models apply to this ownership pattern; they do not prove the
Rust implementation. The checked `BorrowedSnapshots` TLC model likewise captures
why copied references must survive removal of the original owner's edges.

The [code inspector](../../../examples/code-inspector/README.md) exercises a controlled
three-stage world and deliberately exports selected descriptions into a separate
Store. A join finds undocumented direct methods; explicitly retained value
history reproduces 20, 42 and the inherited 10 after removal. This demonstrates
that direct reflection and optional relational use can compose without automatic
publication by the new inspection calls. It does not establish a Store-free
bootstrap, automatic live mirroring, whole-world atomicity or a speedup.

The separate [owner regression](../../../tests/fixtures/reflect-owner.birnam) covers
replaced values during collection, nested snapshots, bound nil, identity aliases,
an alternate replaceable binding owner, owner Error propagation and repeated
inspection without fact growth. Namespace's native integration adds raw-absence
and failure/unwind checks.

## Acceptance and workload observations

[Source-bound receipt](../../measurements/reflect-owner-inspection-20261013.json).
The final prefix was rebuilt with the development Cargo cache; authored bootstrap
modules, tools and startup images were regenerated. This is not clean-cache release
qualification, and the normal installed version was not changed.

- 718 language checks passed, followed by the private-scratch gate, the new
  collecting/reentrant owner fixture and the inspector application.
- All 26 REPL golden checks passed, including global listing and mapped-image
  behavior.
- An earlier broad native run passed all 66 integration binaries. After the final
  handoff fix, eight affected binaries were rerun successfully: compiler, modules,
  global resolution, method observation, roots, root batches, root reuse and
  result cleanup. The final native test prefix overlays the unchanged TCC module's
  resource-metrics build onto the rebuilt prefix; its provenance and hash are in
  the receipt. It is separate from the application timing prefix.
- 13 Store/Namespace unit tests passed (three existing manual tests ignored), and
  three Python checker tests passed, including rejection of corrupt observations.
- The existing formal gate passed 218 Lean theorems and axiom auditing, rejected
  47 Lean mutations, and obtained the expected outcomes from 123 TLC configurations.
  No new formal model or production implementation proof is claimed.
- The final cleanup regression fails against the pre-fix native prefix with
  `Reflect returned an inventory collected during cleanup`, then passes against
  the fixed implementation, including failure propagation and reclamation.

Three additional fresh application processes ran sequentially, without concurrent
qualification or builds. Every scenario passed its independent Python checks.
Median inspection CPU was 167.48 ms initially, 167.71 ms after replacement and
168.02 ms after removal. Each measured phase includes direct/inherited inventories
and alias discovery; it excludes startup, export, query evaluation and validation.
All phases created zero global Store facts. These are measurements of one controlled
workload, not a before/after speedup or a claim that relational export is free.

The application supports keeping both capabilities: direct inspection is useful
on its own, and a deliberately selected relational description supports joins
and retained observations. The next storage migration can use these interfaces
and this workload as acceptance; slot discovery, publication ownership and a
Store-independent bootstrap remain unfinished.

## Ordinary reference slots and Module directory

This is the slot slice's historical implementation record. Module registry and
Globals ownership, direct ordinary edits, and the five ordinary-object field
migrations followed; their status is tracked in the Store decision.

The [next slice](../../design/reflect-slots-and-module-state.md) adds `slotsOf:`, `slot:in:`
and `slot:in:put:` in Birnam over ObjectModel's owned reference inventory. Optional
`slotNames` metadata is an ordinary method; anonymous slots retain index access.
Module's directory moves to an ordinary slot while keeping a named generic edit
path. Registry facts, binding/publication ownership and native-family logical
inspection remain separate work. Existing images retain their saved old methods.


## Qualified optional extensions (2026-10-20)

`factsOf:` is now installed by `storeSupport`, alongside explicit image fact
participation, rather than core stdlib. Its current-db/live-snapshot meaning,
identity retention, current method replacement and error propagation remain.
Core Reflect has no fact-query binding until the optional integration is loaded;
slot/method/source inspection remains independent.

`defineMethod:on:as:doc:` retains its method-installation and Reflect-return
contract. If Doc is present it sends that current receiver
`recordMethod:on:text:`; otherwise it drops documentation as before. Doc owns the
entry layout and ordered catalog. Its hooks are constructed before the new owner
is published, in a lexical scope that introduces no extra global naming alias.
Doc records the optional reflection description without replaying Store's
initializer or changing an existing image-participation choice.

All 853 language checks and actual Store-absent documentation/image checks pass.
See the [receipt](../../measurements/optional-store-hooks-20261020.json). This is not
completion of Store-free Host/tool composition or late optional loading.
