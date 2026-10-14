# Reflect without mandatory Store

2026-10-13. **Status: investigation and proposed API direction.** Implements no
runtime change. This elaborates the reflection guarantee in
[Store and Relational as optional language facilities](optional-store-and-relational.md).
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

Primary source: [reflection operations](../../rust/birnam-codegen-runtime/src/reflection.rs),
[installation and mutation](../../rust/birnam-codegen-runtime/src/lib.rs), and
[stdlib extensions](../../modules/stdlib.birnam).

| Existing message | Current behavior | Store dependence and destination |
| --- | --- | --- |
| `[Reflect vtableOf: object]` | Sends Id `behaviorOf:` | Already a direct owner query; retain |
| `[Reflect nameOf: symbol]` | Converts a Symbol to a String; rejects other kinds | No fact access; retain. This is not reverse naming of arbitrary objects |
| `[Reflect globalNamed: name]` | Tries Namespace, then Globals when Namespace is absent | Globals is fact-backed; move its storage and preserve lookup precedence |
| `[Reflect allGlobalNames]` | Enumerates Namespace exports only | No fact access, but does not enumerate source-owned Globals; address the coverage gap explicitly |
| `[Reflect allMethodsOf: object]` | Walks actual vtable dictionaries from child to parent, deduplicating selectors | Uses Store to find target names after Namespace reverse lookup fails; replace this naming dependency |
| `[Reflect defineMethod: selector on: target as: block]` | Checks Generation policy, constructs a Method, obtains a private vtable when needed, then publishes through Store | Move publication responsibilities intact; keep the public operation |
| `[Reflect removeMethod: selector on: target]` | Checks Generation policy and requests direct-entry removal through Store | Move publication responsibilities intact; preserve exact removal and inheritance behavior |
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

The following are proposed capabilities and illustrative spellings, not existing
Reflect APIs or finalized schemas:

| Proposed message | Required meaning |
| --- | --- |
| `[Reflect directMethodsOf: object]` | Direct dictionary entries on the object's vtable, including their owning vtable identity; no inherited deduplication |
| `[Reflect parentOf: vtable]` | Expose delegation structure through the owner, enabling traversal of shadowed definitions |
| `[Reflect bindingsIn: bindingOwner]` | Enumerate that owner's names and values with exact presence, including bound nil |
| `[Reflect namesOf: object in: bindingOwner]` | Return aliases by identity, without assuming an object has one canonical name |
| `[Reflect slotsOf: object]` | Enumerate ordinary storage through its owner, with readable/writable status and stable owner-defined slot identifiers |
| `[Reflect slot: slot in: object]`, `[Reflect slot: slot in: object put: value]` | Read/write through the storage owner; preserve aliases, bounds, tracing and mutation admission |

Id already provides dictionary, parent and binding messages.
[ObjectModel](../../rust/birnam-object-model/README.md) already provides indexed
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

### Method publication without Store

The target path is:

```text
Reflect definition/removal
  -> compiler and object-owner preparation as required
  -> supported publication protocol
  -> authoritative dictionary change and execution recovery/lifetime handling
```

The publication protocol must retain authorization, phase checks, notification
ordering, failure behavior and Generation's selective compensation. It is a
responsibility to place with the existing mutation/generation owners, not a new
service name chosen here. Reflect should not simply replace a Store send with a
raw Id mutation.

Optional code descriptions consume explicit observations or an explicitly enabled
publication observer. Core execution must work with no such consumer. A future
observer contract must specify callback failures, reentry and ordering; it must
not displace Generation's required notifications or silently change when a method
change is considered committed.

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

## First implementation boundary and acceptance

Begin with owner-level binding enumeration/reverse naming and a precise direct
method inventory. Route Reflect, REPL global listing and Doc naming through those
interfaces while retaining the current storage backend. This establishes a usable
replacement for direct fact inspection before storage is removed; it is not yet
a Store-independent core.

Define the ordinary-slot discovery contract before migrating named public fields.
Move publication only with its Generation consumers and recovery tests. Split
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
