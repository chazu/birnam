# 84 — Delete the parallel native object system

> **Status:** Accepted implementation direction, 2026-09-21. Replaces the
> cross-module table architecture retained by Designs 81–83. Implementation is
> incomplete until the completion criteria below hold in production.
> Design 83's language safety and active-code lifetime requirements remain;
> its module-specific native descriptors are migration debt, not an approved
> destination. The user explicitly requested deletion before further extraction.

## The mistake and the decision

Birnam objects are opaque references. Their behavior is accessed through messages.
We instead built another object system from shared Rust structs, context pointers,
function tables, dependency attachment registries and factory aggregates. Loading
those structures through an Id message did not make subsequent direct calls into
message sends. Moving them out of Id, or renaming them capabilities, did not fix it.

Delete that second system. Do not relocate it to another crate, encode its pointers
as Words, replace it with generated tables, or preserve it behind compatibility
adapters. Keep useful implementations of language behavior; replace their public
integration. This is a production replacement, not another seed or experiment.

At the start of this work, `birnam-abi/src` has 69 files and 6,899 lines; Id's source
has 14 files and 2,460 lines (both counts include tests). These are a baseline,
not a deletion quota. Smaller files, more crates and fewer individual API fields
are not acceptance criteria.

## What survives

- Siphon's single ad-hoc loading request and handoff to the real Id carrier.
- Id's metacircular object graph, opaque identity, method dictionaries and message
  dispatch, with the finite lookup termination needed to implement itself.
- Language implementations: value operations, collections, compiler, execution,
  images and workers. Retain their algorithms where they implement real behavior.
- Safety under method replacement, collection, suspended execution and native-code
  retirement. Delete redundant machinery, not the guarantees it was meant to serve.
- Tests of language behavior, independent format readers and semantic oracles.

Native implementation code may call its own functions and access its own storage.
A consumer cannot interpret another module's object payload or call a published
implementation table. An implementation helper does not need a separate library,
factory, versioned descriptor or initialization stage merely because it exists.

## The only common native contract

The shared ABI describes opaque Oops, message operands/results, native method
entries, the generic message carrier and the module entry used by the loader.
Generic argument-buffer helpers are allowed. It has no knowledge of Integer,
Namespace, GC, Runtime, compilers or application services, and no per-module state.

A native method entry is necessary to install/invoke a method. It is not permission
to capture a set of implementations at startup and publish it as another API.
Ordinary service calls send messages and observe replacement. Cache selector
identities if useful; do not substitute cached implementations for fresh dispatch.
Execution's already-selected active call remains callable under Design 83's
lifetime rule; that does not authorize bypassing lookup on later calls.

Native Bytes and Words carry bounded native data required at the bootstrap/FFI
boundary. They must not smuggle a dependency descriptor or foreign object layout.
A module initializer installs methods and returns an Object, or Unit for a completed
extension. It receives the Id carrier and already available objects/data, never
future-filled callbacks or an aggregate of all predecessor APIs.

Examples of intended production use:

| Consumer | Receiver and message | Remove |
|---|---|---|
| String splitting | Vector prototype `new`, result `add:` with each element | Vector empty/push table |
| Map iteration | Temporary Vector `do:` or `inject:into:` with caller's blocks | Direct Vector method callbacks |
| Range concatenation | Constructed Vector `,` with the other collection | Captured Vector comma implementation |
| Numeric consumers | Integer object/family messages for construction and native conversion | IntegerApi and shared storage codec |
| Runtime/bootstrap | Root/lifetime/namespace objects receiving operations | Owned/factory descriptor aggregates |
| System extensions | Methods installed on System; initialization returns Unit | Install-only tables and host attachment fields |

The collection selector spellings above must be checked against the existing
installed methods while changing the callers. Prefer an existing selector. Any
new native conversion message must document its receiver, arguments, result and
borrow lifetime alongside its implementation; do not create a central API catalog.

## Deletion and replacement order

Work in three substantial packages. The user subsequently required deleting all
superfluous ABI code first, before repairing its callers. That deletion checkpoint
is deliberately not workspace-buildable. Do not restore or relocate the retired
interfaces to make callers compile; update them to messages. Do not turn this
work into another endless series of one-field extraction designs.

### 1. Remove dead interfaces; convert the live collection boundary

Delete the eleven installer/construction-only ABI modules: Printing, Block Loop,
String Collection, System GC, System Output, System Input, Environment, File IO,
Source Loader, Range and Map. Their native entries already perform installation.
Delete the corresponding provider tables, attachment state, Bootstrap casts and
host bookkeeping together. Keep initialization idempotent and report Unit after
successful extension installation.

Then delete Vector's cross-module table and Range's captured comma callback.
Consumers send the existing Vector messages. The Vector constructor returns its
opaque prototype; no consumer reads its layout. Remove obsolete construction
exports and descriptor fixtures rather than preserving them for tests.

### 2. Replace remaining upper integration and remove obstructing optimizations

Convert values, namespace, roots/lifetime, collection/tracing and Runtime services
with their actual consumers. Delete `runtime_values`, `runtime_support`, the old
Runtime `IdApi`, aggregate initializer types, provider tables and attachment
registries. Compiler-generated calls and host calls must use the same message
contract. The frozen primitive-entry bundle is also subject to removal; any
necessary metacircular operation belongs in Id, not in a distributed escape table.

Remove optimizations that require foreign payload layouts, shared storage codecs,
or captured service implementations. Use the correct message path first; record
measured regressions rather than retaining a competing interface to avoid them.
Do not remove language features or active-call/root safety as an optimization
shortcut. Rebuild native artifacts after the ABI change; no compatibility table
is required. Preserve image semantics and the existing explicit source recovery
path when a compiled artifact is incompatible.

Collapse separately packaged implementation containers into their actual owners
where they have no independently useful object protocol. Do not replace dozens
of tiny libraries with a new universal runtime-service catalog.

### 3. Remove the abandoned scaffolding

Delete tests whose only purpose is preserving retired table layouts, factories,
setter exports or attachment order. Keep or migrate their independent semantic
assertions to production messages. Delete redundant seed implementations and their
runners once their useful assertions are accounted for in production coverage.
Remove dead build/install entries at the same time as their modules.

Update current guides, construction documentation and README. Mark old designs
historical/superseded; do not treat their implementation-specific ABI promises as
requirements. Git retains deleted code. Do not add archive copies or a second
implementation to ease the transition.

## Necessary seams, not blanket exceptions

Siphon enters native loading before real Id exists. Id implements its terminating
lookup and owns its allocation records. Native modules call OS/FFI functions and
register native methods. Execution retains selected active code until it can be
safely released. These are concrete mechanisms to preserve.

Collection needs to discover roots and edges without losing live objects during
its own work. Specify the collection phase and callback/message rooting rules
when converting it; do not assume arbitrary allocation/reentry is safe, and do
not use that concern to retain all current GC tables. Representation-specific
edge enumeration stays with the representation's owner.

Borrowed native buffers have explicit synchronous lifetimes. A caller copies or
retains their owner before a later message can invalidate them. Trusted native
code remains trusted; no arbitrary-pointer sandbox or speculative security system
is part of this work.

## Completion criteria

All of these are required; the first deletion commit is not completion:

1. ABI source contains only the universal native calling/loading contract and
   generic helpers. No module catalog, per-module structs, state or registries.
2. Production native modules exchange opaque objects through Id messages. No
   descriptor pointers disguised as Word, private symbol lookup for service APIs,
   foreign payload codecs, or replacement tables in another crate/header.
3. Siphon loads Id with all upper libraries absent. The same production path builds
   useful prefixes and then the complete language, without forward declarations.
4. Replacing a dependency method affects its next use. Representative collection,
   value and service tests observe this through real production consumers.
5. Targeted tests retain arithmetic/error behavior, collections, GC/root safety,
   method/code lifetime, source execution, compiled-image startup and workers.
   Normal compiled startup must not unconditionally recompile embedded source.
6. Dead compatibility code, experiments, fixtures, build entries and current
   documentation claims are removed or corrected. No parallel runtime survives.

Use targeted debug builds and meaningful installed behavior tests. Release
validation and forced-source worker readiness timing are not required. Report
which criteria remain open; do not claim success based on crate counts or a
passing suite that only validates the architecture being removed.

## Implementation checkpoint — 2026-09-21

The first production change deletes all twelve tables in package 1, their
attachment code and host fields. Vector returns its opaque prototype. Range,
Map and String Collection use live `new`, `add:`, `,`, `do:` and `inject:into:`
sends; the special comma initializer and captured native entry are gone.
Installation-only results are Unit. The construction manifest and current ABI
pages reflect this change. Legacy C table fixtures still await deletion/migration.

Validation: workspace `cargo check --offline --locked --workspace`; five targeted
Vector/System Output unit tests; debug installation at
`/tmp/birnam-84-message-modules`; `scripts/id-native-loading-smoke.py` on that
installation. The native suite includes 43 Runtime service cases, construction
prefixes, full native application initialization and a new test replacing all
five Vector methods through actual Map/Range/String consumers, with GC inside
the replacements. Logs: `/tmp/birnam84-check.log`, `/tmp/birnam84-unit.log`,
`/tmp/birnam84-install.log`, `/tmp/birnam84-native.log`.

This completes the first package's production edits, not Design 84. The old
Runtime message bridge and value, collection-policy, lifetime, collector and
compiler tables remain; subsequent work must remove them with their consumers.

### Retired seed coverage

The separate `experiments/id-seed` workspace and `id-seed-experiment` runner/task
are deleted. They tested a second implementation, never the installed language.
The following production checks replace their role, not every experimental API:

| Useful obligation | Production coverage / remaining obligation |
|---|---|
| Id construction, identity, binding, replacement and native loading | `id-native-module.c`, `id-native-host.rs`, Id dispatch unit tests |
| Integer arithmetic, overflow, invalid operands and replacement | `early-integer-native.c`: 605 signed cases before and after Runtime adoption |
| Private behaviors, ownership and graph traversal | `early-object-model-native.c`, `early-trace-native.c` |
| Roots, collection phase, finalization and Runtime adoption | `early-lifetime-native.c`, `early-collection-native.c` and the installed native suite |
| Exact address reuse, stale metadata and collection failure atomicity | Still require the final production lifetime audit inherited from Design 83; deletion of the seed is not evidence of closure |

Seed-specific factory layouts, multiple seed-universe destruction and its native
library unload sequence are not production contracts and are deliberately retired.
Its negative controls validated the discarded implementation, not production.
No test result from that workspace counts toward Design 84 acceptance.

### ABI deletion first — user-directed checkpoint

All 54 remaining module-specific ABI source files are deleted, together with the
upper Runtime `IdApi`, its diagnostic descriptor and old entrypoint signatures.
The crate now has three source files / 428 lines: opaque Oops and generic borrowed
buffers, the typed Id message/module protocol, and the stateless OS loader shared
by Siphon and Id. It has no dependencies or module catalog. This is deletion, not
relocation; Git contains the former interfaces if needed to understand callers.

The two existing ABI unit tests pass with the minimal contract. **The workspace
is intentionally not buildable at this checkpoint**: production callers still
refer to deleted interfaces. Repair begins at Id and proceeds upward through the
production dependency graph. Preserve real implementations inside their owners;
remove cross-module descriptors, foreign layouts and cached implementation bundles
instead of recreating them elsewhere. Full native, image and language validation
can resume when those callers are converted. Prior installed-prefix results do
not validate this changed workspace.

Id is the first repaired caller: its allocation geometry and registry indexes are
private implementation details, its enumeration methods expose the existing
borrowed result through Id messages, and its unused test-only primitive bundle
is removed. It uses no retired ABI interface. All five Id unit tests pass,
including metacircular dispatch and the independent C typed-operand round trip.
The remaining upper callers are still unconverted; these tests do not establish
that the complete language builds or runs.
