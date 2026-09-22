# 84 — Delete the parallel native object system

> **Continuation:** [Design 90](90-functional-modules.md) now governs functional
> ownership and the organization of the remaining work. The prohibition on
> parallel native APIs and the preserved semantic obligations below still apply;
> this document retains the deletion and migration history.

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

### 2. Replace remaining upper integration; preserve deoptimization and concurrency

Convert values, namespace, roots/lifetime, collection/tracing and Runtime services
with their actual consumers. Delete `runtime_values`, `runtime_support`, the old
Runtime `IdApi`, aggregate initializer types, provider tables and attachment
registries. Compiler-generated calls and host calls must use the same message
contract. The frozen primitive-entry bundle is also subject to removal; any
necessary metacircular operation belongs in Id, not in a distributed escape table.

Preserve the deoptimization work and concurrency machinery, as explicitly
required by the user. This includes the native optimization tier that exercises
recovery, recovery maps, materialization, continuation state, task scheduling,
workers, suspended execution and active-code lifetime handling. Their integration
must be rebuilt above Id through messages; foreign payload layouts, shared
storage codecs and captured cross-module service implementations are not exempt.
Do not delete these implementations or their semantic tests to simplify migration.
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
pages reflect this change. The subsequent cleanup removes the obsolete C table
fixture ecosystem; retained semantic harnesses still need migration.

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

### Preservation boundary for the next deletion pass

The user explicitly retained deoptimization and all concurrency machinery. Keep
the Runtime native tier, numeric/closure specialization and recovery implementations;
compiler lowering and recovery descriptions; scheduling, task/control state and
Isolate/worker implementations; image reconstruction; and their semantic tests.
These remain upper-language features, with no new knowledge added to Id or ABI.
Preservation of their source is not proof that they run during this broken-build
transition. Their old native integration still needs migration.

Delete the descriptor-based Bootstrap construction catalog and entry machinery,
Runtime's aggregate initializer and diagnostic factory catalog, Portable Code's
published function-table objects and Codegen's table lookup adapters. Retain
Bootstrap's actual result-retention algorithm and Runtime's execution/retirement
operations for reconstruction through objects. Delete the standalone Text View,
Error Value, Collection Value, GC Settings and Allocation Budget packages. String
conversion, equality/hash semantics, error construction, collection settings and
checked allocation accounting must be implemented by their actual owners, without
recreating those five service factories. Keep optimized execution and its guards;
any remaining descriptor-dependent guard is migration debt, not a valid exception.

Deletion checkpoint: Bootstrap's module catalog and descriptor initializer are
removed, retaining its load/result-rooting implementation. Runtime no longer
includes Id source or supports local allocation/symbol/dictionary backends; the
old diagnostic factory constructor is deleted. Its upper execution, deoptimization,
recovery and scheduling implementations remain. Portable Code retains its real
CodeObject/Block representations and constructors, while its four published API
objects and Codegen's three private API lookup routines are deleted. The five
helper crates and their Cargo/install/export-inventory entries are removed.

The earlier pending fixture cleanup is included, with deoptimization-related
`native-block-construction.c`, `closure-api-context.c`,
`native-numeric-api-context.c`, and the worker-finalization
fixture retained for migration. The recovery runner still contains its closure
admission checks. Old factory support includes are not restored: these harnesses
cannot run until they use the new object integration. Rebuilding test setup must
not recreate a second Id or the retired module APIs.

This is still a deletion checkpoint, not a completed restoration. Other upper
consumers, including optimization guards and native entry adapters, still refer
to removed interfaces. Their presence does not authorize restoring shared tables.

Validation for this deletion pass: 196 files covering Runtime recovery/native-tier,
execution, compiler lowering, artifact formats, Isolate, Image and Birnam modules
match the pre-deletion bytes. The one changed file in the wider preservation
inventory is Codegen Runtime's integration file: only private API imports/lookups
were removed. Ten focused Codegen recovery tests pass, including alias identity,
recovery maps, closure recovery and image recovery emission. The seven ABI/Id
tests pass. Cargo metadata/lock resolution, edited Rust parsing, Python/TOML syntax
and diff whitespace checks pass. Runtime concurrency and full native/image tests
cannot run until upper construction is rebuilt; preservation is not execution
validation. The host's Bootstrap descriptor wrapper is also deleted.


### Final catalog and factory deletion checkpoint

The follow-up deletion pass removes the five remaining scaffolding categories:

- Host `LoadedFloor`, `InitialFloor` and `LoadedStack` no longer hold or expose
  upper-module descriptor catalogs. Id loading still uses Siphon. The host no
  longer creates the retired Bootstrap descriptor wrapper or runs its staged
  factory construction path. Later startup callers await migration.
- Runtime's `build_bootstrap_with` and `new_with_owners`, alternate Heap constructors,
  and local Nil/Boolean construction are gone. Heap adoption/accounting and upper
  method installation remain as operations on already constructed state. Execution,
  scheduling, compiler admission, lease and retirement implementations remain;
  the replacement construction path must initialize that state in its owning layer.
- Factory-only Runtime wrappers and their publishers in the owning crates are
  deleted. Actual root registration/retention, payload release, finalizer eligibility,
  inherited trace-hook lookup, native register capture/scanning, collection phases,
  observation, deferred-allocation indexing, lookup caches and guard-state algorithms
  are retained. No new service bundle or alternative API table replaces the old ones.
- Compiler retains its native-tier and numeric registries and initialization logic.
  Its service descriptor, string-to-function resolver and test-only implicit
  initialization path are deleted. Recovery registration and generation leases remain.
- Twenty IdApi manifests, 111 module-specific C headers, the construction manifest
  and its install-time graph validator, the ABI diagnostic export/fingerprint receipt
  fields, and the foreign heap-layout probe are deleted. The generic Id header,
  registration-envelope header, artifact/image formats and independent format
  fixtures remain. Recovery runners no longer enforce retired IdApi definitions;
  their behavioral assertions are preserved.

This completes the identified deletion pass, not caller migration. Mixed owner
implementations still reference removed carriers or admission callbacks, and
native fixtures still contain old includes. Replace those integrations with Id
messages while preserving their semantics; do not recreate the deleted headers,
factory constructors, foreign-layout assumptions or cached module APIs. The next
work is production object integration, not another round of extraction scaffolding.

Validation: all seven ABI/Id tests and ten focused Codegen recovery tests pass.
Of 196 files inventoried around execution, recovery, native optimization, compiler,
Isolate, Image, Birnam modules and native capture, 191 are byte-identical. Five
changed files remove the deferred-index wrapper/declaration, compiler resolver,
dispatch-cache wrapper and storage-observation factory selection. Unique observation
identity allocation remains in the private state constructor. Their owner
algorithms and semantic tests remain. This is source-preservation evidence, not a
Runtime concurrency test result. Full-language and native harness validation remain
unavailable until callers are migrated. No release validation is required here.

Cargo metadata/lock resolution, edited-source syntax and whitespace checks pass.
All eight validation-tool tests pass, as do both independent stackless/recovery
and Portable Code binary-fixture verifiers. The verifiers retain their malformed
input and truncation assertions.


### Siphon and CLI boundary

After the deletion pass, separate the executable shell from native construction:

- `birnam-siphon` depends only on `birnam-abi`. Its 77-line implementation sends
  one ad-hoc load request for Id, validates the returned protocol, and returns it.
  It knows no upper modules, CLI, compiler, collection policy or host state.
- `birnam-cli` owns the `birnam` executable, option parsing, command dispatch
  and CLI-specific policy selection. Birnam-authored CLI behavior stays in
  `modules/cli.birnam`.
- `birnam-build` is separate repository build/install tooling. It invokes Cargo
  and the staged Birnam executable, but has no Rust dependency on the CLI, host,
  Siphon or runtime. Its executable name and install command remain unchanged.
- `birnam-host` supplies runtime and project operations. It calls Siphon as a
  dependency and no longer contains or re-exports argument parsing. `RunOptions`
  and `ReplMode` remain host operation data, supplied by the CLI parser.

The dependency direction is CLI → host → Siphon → ABI; Id also depends only on
ABI. No native language module depends on the CLI. These are ordinary executable
and bootstrap boundaries, not additional language-module APIs or service tables.

`mise run siphon` builds the actual production Id dylib in debug mode and checks
that, after Siphon returns, the real Id performs symbol interning and native
loading. It also checks invalid/missing paths. No upper module or installed prefix
is needed. Siphon's implementation moved unchanged; only its Cargo ownership and
call sites changed. Existing parser and installer tests remain with their respective crates;
CLI/host execution remains blocked by the deliberate upper-ABI deletion.

Validation for this separation: both production Siphon tests and all eight
validation-tool tests pass; Cargo dependency/target checks and edited-source
syntax checks pass. Building CLI tests stops in `birnam-artifact` on its existing
references to removed `IdApi` and Namespace tables, before reaching CLI code.
No CLI execution or release-validation claim is made at this checkpoint.

The build/install tool was subsequently separated from `birnam-cli`: its unchanged
implementation is now `rust/birnam-build/src/main.rs`, with only `hex`,
`serde_json` and `sha2` dependencies. This lets the tool build and its installation
safety tests run independently of the broken upper-language integration. Full
installation still requires the workspace and staged Birnam executable to work.

All four build-tool tests pass after this move, covering development-cache options,
staging cleanup, mixed-prefix rejection and protection of unowned files.

### Message integration: Roots and Namespace

Siphon loads Id unchanged. Id now loads Roots with no arguments, returning an
Object, then Namespace with that Roots Object, returning an Object. Namespace
publishes `Roots` and `Namespace`. Their implementations document the native
messages locally; neither library exports a table or foreign payload layout.
Namespace retains ordered bindings and reverse lookup. Roots retains ordered
providers, duplicate registration, exact retention tokens and its existing limit.
Enumeration snapshots private storage before invoking visitors or sending provider
messages. The former namespace lookup-admission export is now an ordinary
`isCanonicalLookup:` message about a selected entry.

The ABI gains stateless conveniences for sending and interning through the same
Id carrier, with no module-specific selectors or cached implementations.
`scripts/native-message-test.sh` builds actual production dylibs and tests loading,
publication, replacement and root-enumeration reentry through Siphon and Id.
ABI, Roots and Namespace unit tests pass (7 tests; one timing probe ignored),
and the production integration test passes. This establishes the first repaired
upper prefix. Collection is not active here; GC phase safety and the complete
language remain pending the higher-module conversion.

### Message integration: Boolean and Nil

Boolean takes Namespace, resolves Roots by message, creates and retains the
canonical nil/true/false identities, publishes them, and returns true. Nil takes
that completed Namespace and installs Object/nil predicates, returning Unit.
All identities remain private references, not a shared base-values struct.
Boolean control is installed immediately: it sends `value` to the selected
argument at call time, so it needs no Block table, forward declaration or later
control initializer. Both printing extensions take a completed String Object.
The printing installation remains pending until String is converted.

The production message check now covers Boolean truth tables, default identity,
nil predicates, all five conditional/short-circuit selectors, unchosen arguments
without a `value` method, replacement of the chosen block's method, malformed
operands and repeat loading without overwriting a replaced Boolean method.
Boolean and Nil build independently with only birnam-abi.

### Production Bootstrap sequence and result retention

Bootstrap now takes a native-library directory (Bytes) through Id's loader,
constructs Roots then Namespace, publishes its own completed receiver, and loads
Boolean and Nil through its `loadNative:` message. It returns Namespace. The
host's `InitialFloor::load_bootstrap` sends this request after Siphon returns Id;
Siphon remains unchanged. The loader sequence lives only in Bootstrap.

Bootstrap now starts after Roots, eliminating its former early-root table,
managed-root adoption, factory ownership and callback pause/destroy interface.
It still retains each Object input across initialization, retains each Object
result before releasing its inputs, and supports nested loads without borrowing
its records across sends. `release:` removes one retained result occurrence.
Roots and module owners are process-lived, as are installed native contexts.

The production integration test enters this Bootstrap initializer, then checks
Roots/Namespace and Boolean/Nil behavior through the returned Namespace. It also
checks duplicate result retention, load failure, and replacement of Id's loader
with a nested initializer returning its input. Only result roots survive those
calls, and explicit release removes them. This validates actual Bootstrap/Roots
implementations without a second Id or bootstrap model.

The production prefix test, 12 ABI/Id/Roots/Namespace unit tests, and both
Siphon handoff tests pass (one existing Namespace timing probe remains ignored).
The new host handoff is syntax-checked;
building the whole host is still blocked by unconverted Artifact/Runtime callers.
Next is the value/lifetime/collection boundary, followed by Runtime, compilers,
images and workers. None of those retained algorithms have been deleted in this
pass, and no full language, GC, image or worker acceptance is claimed yet.
