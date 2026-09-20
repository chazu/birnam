# 81BJ — Native root-provider registry

> **Status:** Completed against 81BI (`868f62ed`), with focused debug validation.
> Root Registry owns registration and copied snapshots. Id, all native callers,
> early preparation and host loading use explicit capabilities. Installed startup,
> source-free compiled callers, saved native-Code images and baked executables
> pass. Release validation was explicitly waived.

[81D](81d-id-api-partition.md) places root-provider registration with collector
integration. [81O](81o-production-live-gc-traversal.md) and
[81P](81p-production-collection-coordinator.md) already moved traversal and
collection coordination above Id. This extraction moves the ordered native
root-provider list from `gc::MetadataShell` into `birnam-root-registry`.

## State and entry before extraction

`MetadataShell` owns a `Vec<RootProvider>` alongside three independent settings:
stack bottom, automatic collection requested and precise tracing. Registration
first performs Runtime owner/observation admission. It then checks the current
length against 32 and appends the callback. Duplicate addresses consume separate
entries; there is no deduplication, unregistration or provider invocation during
registration. Overflow fails with `gc: too many root providers` through the
existing Id invariant boundary. Providers are non-null C function pointers in
the declared contract, each taking the synchronous `MarkFn` callback.

The public setter selects the process-published Runtime. Id also registers
Machine's execution provider directly while installing execution objects. This
internal route must continue to work on independent Runtime instances without
requiring their publication through `PUBLIC_RUNTIME` or a new active entry.
Provider code stays live under the existing native library lifetime rules; the
registry does not own native code, add roots of its own, or pin object identities.

Only the list, append policy and snapshot belong to this candidate. Keep native
stack capture, precise/automatic flags, the 16 MiB trigger, collection phases,
mark-callback adaptation, core roots, trace hooks, finalizers, weak/executable
retirement and physical storage with their existing owners.

## Two distinct snapshot consumers

`runtime/tracing.rs::roots` emits native range candidates and initial roots,
clears dispatch-cache state, then snapshots both floor roots and the provider
list. It emits those floor roots and invokes the copied providers in order via
`invoke_with_mark_callback`. Do not move the snapshot before native admission or
replace it with live indexed traversal. A provider appended during a callback
belongs to a later snapshot; duplicates still execute separately. No RuntimeState
or registry borrow may span any provider or mark callback.

`runtime/storage.rs` also copies the list while constructing its owned diagnostic
view. It publishes the copied function addresses and count and sets
`LEGACY_ROOTS_UNOBSERVED` when nonempty. It does **not** invoke them. This happens
inside the restricted storage-observation phase: routing the internal snapshot
through ordinary public Id admission would incorrectly reject it. Preserve the
existing coverage gap, array lifetime and callback prohibition. The snapshot
must finish before the external observer runs.

## Native caller migration obligations

This is more widely used than Method Observation. Initial source inspection
finds early preparation calls in String, Float and Word, plus ordinary or lazy
registration in Integer, String, String Builder, Range, Vector, Map, Collection,
Store, Lang, PEG, Tcc, Codegen, Portable Code, Image, Artifact, Error, Relational,
Alien and Isolate. Complete a call-site table before changing the interface.

In particular:

- String preparation publishes its instance, registers storage roots, then
  populates it. Float/Word also register between instance publication and
  population. Their explicit preparation contracts need the matching capability;
  native module initialization alone is too late.
- Integer registers after publishing its Runtime/instance and before population.
  Repeated initialization checks its existing API and returns without another
  registration.
- Store uses the incoming API and sets its root-provider once flag before the
  callback. As demonstrated in 81BI, registration can reenter initialization
  with another incoming selection. Preserve the ordering relative to Store-root
  publication and overwrite-hook registration.
- Portable Code registers through its first saved execution API. Tcc, Codegen
  and Artifact also use saved execution selections at their registration points.
  Do not mechanically replace these with incoming selections.
- Alien and Relational have helper paths as well as initializer state.
  Preserve each caller's existing once mechanism and its reentry behavior;
  changing Once, OnceLock or atomic flags is outside this extraction.

Prelude/Native Loader must forward any new explicit initializer capability with
the same saved execution selection as the other capabilities. Early preparation,
C fixtures, compiled native consumers, export inventories and installation must
migrate together. No API-address registry, temporary callback-slot loan or
aggregate compatibility facade should substitute for explicit selection.

## Audited caller selections

The 22 production registration expressions preserve these selections. Existing
once and phase guards stay at their current call points.

| Caller | Selection and point |
|---|---|
| String storage | Incoming prepare API, after instance publication, before population |
| Float and Word | Incoming prepare API, after instance publication, before population |
| Integer | Incoming initial module API, after Runtime/instance publication, before population |
| String protocol | Incoming module API; Once after first storage publication, before Runtime selectors |
| String Builder | Incoming module API; Once before family construction/adoption |
| Range, Vector, Map | Incoming prepare API; Once before family construction/adoption |
| Collection | Incoming API; Once after first Runtime initialization, before layouts |
| Store | Incoming API; atomic flag after root publication, before overwrite registration |
| PEG | Incoming API, conditional on live namespace presence after method installation |
| Error | Incoming API; OnceLock after method installation, before publication |
| Image | Incoming API; existing bring-up guards and OnceLock after Runtime publication |
| Isolate | Incoming API; OnceLock after Runtime initialization, before layouts/methods |
| Alien | Incoming bring-up API in its helper; preserve Once and its current point |
| Relational | Incoming module API after bring-up; preserve helper's OnceLock |
| Lang | First saved execution API; OnceLock before grammar construction |
| Tcc | First saved execution API; OnceLock before layouts and method installation |
| Codegen | First saved execution API; OnceLock before method installation |
| Artifact | First saved execution API; OnceLock after method installation |
| Portable Code | First saved execution API; Once after Runtime publication |

The three collection-family prepare paths are additional early contracts, not
ordinary initializer calls. String preparation becomes v3; Float/Word preparation
and Range/Vector/Map construction become v5. Native Loader installation becomes
v8. All native initializers gain the eighth matching root-registration capability.
IdApi v15 removes the registration field and obsolete RootProvider layout entry:
25 fields, 200 bytes, six remaining type entries. Provider/MarkFn signatures,
traversal/storage contracts and image encoding stay unchanged.

## Selected ownership contract

A separately loaded Root Registry service owns each Runtime's ordered Vec and
its capacity law. Its mutable public 32-byte capability has version/extent,
context and register. Registration copies a 40-byte floor descriptor with owner
admission and nonreturning overflow reporting, invokes admission before append,
and preserves the old fatal diagnostic. The copied floor retains no movable
Runtime address. The immutable 48-byte factory creates/destroys the service and
provides canonical append and snapshot operations at its original owned address.

Public descriptor instrumentation cannot redirect Id's direct registration,
snapshot or destruction. Id performs its existing admission before internal
append; the factory reports capacity failure for Id's existing invariant path.
Public admission still selects the process Runtime (active Runtime in unit
fixtures); direct independent Runtime use does not acquire that requirement.
Canonical append and snapshot invoke no floor/provider callbacks.

A snapshot is an owned C-layout value: count plus 32 optional function-pointer
slots, 264 bytes. Only its first count entries are populated. This copies the
existing bounded list atomically with respect to owner-thread operations, without
passing Rust Vec ownership or a live borrowed buffer across the ABI. Id can copy
its populated entries into the same local scratch Vec used by existing consumers.
No state borrow crosses provider or mark invocation. Diagnostic snapshotting
bypasses ordinary admission exactly as its current internal clone does.

The host pins and attaches an inert factory before Id initialization. Ordinary
module mounting publishes no language objects or methods. Creation and factory
admission do not call the floor; same-pointer attachment repeats and retargeting
rejects. Destruction invokes no providers and does not release their code.
Stack capture, collection settings and physical root ownership remain unchanged.

## Required evidence

Archive the completed 81BI source/headers and debug prefix as independent oracles.
Compare append order, duplicate entries, exact capacity/overflow diagnostics,
owner and observation admission, callback-time append and next-collection
visibility, nested collection behavior, roots/weak identities and independent
moved Runtime destruction. Exercise the real diagnostic view without invoking
providers and without changing its coverage flags.

For every native caller, preserve registration timing, incoming versus saved
selection, root publication and once/reentry behavior. Verify the new module's
actual state/operation ownership, startup preparation before the first root
registration, ordinary siphon loading, source-free compiled callers, saved
native-Code images and baked executables. Keep image encoding unchanged. Use
focused debug validation; no release validation is required.

## Completed evidence

The independent oracle is the archived source at `cfacc90f`, which retains the
completed 81BI implementation and v14 headers, plus its separately installed
debug prefix. Current fixtures compile against independent v15 C headers;
historical headers remain unchanged. The current installation contains 45 native
libraries and 28 authored modules. All 1,953 recorded source files remained
unchanged during installation and through the installed checks.

- Workspace all-target checking and the debug workspace build pass. Affected
  ABI, Id, Root Registry, host and native consumer crates also pass all-target,
  all-feature checking. Focused ABI/Id/registry tests pass **272 tests**, with
  15 existing Id timing tests ignored. All 69 current C fixtures pass syntax
  checking.
- Four root-registry comparisons match old/new output and exit status: copied
  ordered duplicates, callback-time append visible only to the next collection,
  nested GC, exact capacity overflow, owner rejection and observation rejection.
  The current C probe also checks the factory's by-value snapshot layout, public
  descriptor independence, destruction at the original address, ordinary module
  mounting and dynamic operation ownership.
- Sixteen existing native consumer transcripts and 32 registration/callback
  transcripts match the oracle exactly. These cover early preparation,
  adoption/presence branches, incoming versus saved selections, Store root/hook
  reentry and String Collection installation reentry. Error-presence admission
  and numeric copied-API context checks also pass.
- Seventeen host admission cases pass, including missing or malformed factories,
  rejected dependencies, missing or malformed public capabilities and successful
  preparation. The installed prefix repeats the native consumer and host checks
  and passes the independent v15 client/module fixture.
- Export inspection covers all 73 installed libraries and 29 current native
  manifests. Changes are the new Root Registry exports, two Id attachment/provider
  exports and the declared preparation/dependency version changes. Id and Root
  Registry retain only system linkage; the new library has its expected install
  name.
- Installed stdlib execution passes **145 assertions**. Root Registry, Method
  Observation, Native Loader, Namespace, Selector Enumeration and Block Loop
  each pass startup, source-free compilation, saved v8 native-Code recovery and
  baked execution. The Root Registry fixture verifies loaded library ownership,
  duplicate provider order, callback-time append, nested GC, kept roots, aliases
  and repeated loading.
- All nine floor-discovery cases pass. An actual image-backed host worker
  completes 12 exact wire-value round trips and orderly shutdown in both old and
  new installations, with byte-identical transcripts. The external harness
  accommodates debug boot time; the production handshake deadline is unchanged.

The durable focused entry points are `mise run root-registry` and
`scripts/root-registry-image-smoke.sh PREFIX`. Local evidence is under
`.cargo-target/81bj-`: `rust-check4.log`, `all-features-check.log`,
`debug-build.log`, `integrated-tests3.log`, `c-syntax.log`, `root-oracle.json`,
`native-audit.log`, `registration-consumers.json`, `host-validation.log`,
`install.log`, `source-hashes.json`, `installed-idapi.log`,
`installed-native-audit.log`, `installed-host-validation.log`,
`installed-stdlib.log`, the six `*-image.log` files, `floor-discovery.json` and
`worker-wire.log`. `prefix-path` records the independently installed current
debug prefix. These are scoped development checks, not release validation.
