# 81BJ — Native root-provider registry

> **Status:** Initial boundary audit against completed 81BI (`868f62ed`).
> No implementation or new ABI is selected yet.

[81D](81d-id-api-partition.md) places root-provider registration with collector
integration. [81O](81o-production-live-gc-traversal.md) and
[81P](81p-production-collection-coordinator.md) already moved traversal and
collection coordination above Id. The next bounded state owner is the ordered
native root-provider list still held by `gc::MetadataShell`.

## Existing state and entry

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
- Alien and Relational have helper/lazy paths as well as initializer state.
  Preserve each caller's existing once mechanism and its reentry behavior;
  changing Once, OnceLock or atomic flags is outside this extraction.

Prelude/Native Loader must forward any new explicit initializer capability with
the same saved execution selection as the other capabilities. Early preparation,
C fixtures, compiled native consumers, export inventories and installation must
migrate together. No API-address registry, temporary callback-slot loan or
aggregate compatibility facade should substitute for explicit selection.

## Boundary still to resolve

A separately loaded service can own each Runtime's list and provide public
registration plus immutable internal append/snapshot operations. Public
registration must retain the existing admission order, while direct Runtime
registration and storage-observation snapshots must preserve their different
admission contexts. Owned internal operations must not be redirected by mutable
public capability instrumentation. Do not use a movable Runtime address as the
service context.

Specify an inert pre-Id factory, failure/overflow propagation, synchronous
snapshot output ownership and teardown before selecting layouts. Do not expose
Rust Vec ownership across the ABI, borrow the live vector through a callback,
add a new provider lifetime manager, or make the registry part of the tracer's
mutable worklist. Its canonical factory and native code remain pinned.

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
