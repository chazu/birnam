# Design 81 — Production extraction status and closeout

> **Status:** Preparatory Rust extraction phase closed at 81CA, 2026-09-20.
> Final runtime extraction: `ad9557fb`; reviewed source checkpoint: `e928780c`.
> 81CB is deferred. No further per-field extraction is queued.
> **Subsequent direction:** [Design 82](82-minimal-production-id.md) records the
> architectural failure in this phase and makes production Id decoupling the
> next project. The earlier recommendation to rewrite Integer first is withdrawn.
> [Design 83](83-production-id-construction-and-lifetimes.md) supplies the reviewed
> construction and lifetime protocols for that correction.

## Assessment

The work has produced useful replacement boundaries: Integer and the other value
families, Namespace, object-model helpers, storage observation/tracing, collection
coordination, native GC services, System methods and dispatch-cache storage are
now separate production Rust modules. IdApi shrank from the original 54 fields /
432 bytes to **v20's 18 fields / 144 bytes**. Current native initialization has
twelve arguments; Native Loader is v12 and images remain v8.

These are production modules loaded through the current Siphon startup, exercised
by source-free compiled callers and compiled images. The install contains **53
native libraries plus 28 compiled Birnam libraries**, or 81 `libbirnam_*.dylib`
files, with 38 native export manifests. Optional wrapped C libraries are separate.
The [bootstrap inventory](81a-current-bootstrap-and-runtime-layers.md) explains
the difference between preparation, native initialization and Birnam bootstrap.

This is substantial Rust modularization, but it does not establish a
Birnam-authored Id, allocator, collector, Machine or compiler. The installed
executable still uses the Rust host and compiler; image startup also brings up
that runtime. `RuntimeState` still coordinates heap, vtables, execution,
registration and retirement. Moving another counter or Vec behind C functions
would not resolve those ownership boundaries.

The last increments, 81BW–81CA, mostly moved backing storage while leaving policy
and lifecycle integration in Id. That is the point of diminishing returns for
this phase. **81CA is the stopping point**, with its completed checks and no
partially mounted production dependency.

## Closeout plan and scope

1. Restore the committed 81CA runtime and defer 81CB. Its uncommitted byte-buffer
   prototype adds three ABI operations without moving snapshot policy; remove it
   rather than expanding it into another extraction campaign.
2. Verify the current source against the retained debug install, rerun core tests
   and the existing startup/compiled-image regression harnesses, and record the
   scope of that evidence. No release validation or new language behavior.
3. Consolidate the current ownership map and correct stale bootstrap/ABI claims
   in the README and design entry points. Preserve historical proofs as history,
   rather than interpreting their old "next" sections as active work.
4. Remove superseded local caches and scratch copies after preserving useful
   evidence. Keep supported regression fixtures, independent ABI oracles and the
   latest installation/baseline. Commit the closeout without opening a successor.

## Subsystems and contracts

All extracted implementations below remain Rust. The design records describe
historical increments; current contracts and source define the installed result.

| Area | Production result | Current reference |
|---|---|---|
| Id and Object Model | Id keeps canonical object identity, interning, physical allocation and dispatch. Object Model owns private-behavior helpers, enumeration selection, epoch/key-index storage. | [81J](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81j-private-behavior-extraction.md), [81K](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81k-owner-and-parent-protocols.md), [81BG](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bg-production-vtable-enumeration.md), [81BH](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bh-production-selector-enumeration.md), [81BW](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bw-production-vtable-mutation-epoch.md), [81BX](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bx-production-vtable-key-index.md); [IdApi v20](../../../abi/idapi-v20.json) |
| Numeric and singleton values | Integer, Float and Word own representation and native behavior. Boolean and Nil own methods; singleton identity remains in Id. | [Integer](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81l-production-integer-extraction.md), [Float](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81r-production-float-storage.md), [Word](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81t-production-word-storage.md), [Boolean](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81u-production-boolean-methods.md), [Nil](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81v-production-nil-methods.md) |
| Collections and text | String, StringBuilder, Vector, Range and Map own their native storage and methods; shared comparison/hash and String-to-Vector adapters have explicit owners. Collection retains integration. | [String](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81x-production-string-storage.md), [StringBuilder](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81y-production-string-builder.md), [Vector](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81ae-production-vector-methods.md), [Range](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81ac-production-range-vector-methods.md), [Map](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81ag-production-map-methods.md), [Collection Value](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81ad-production-collection-value.md), [String Collection](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81ah-production-string-collection.md) |
| System and error policy | Printing, text views, IO, environment, GC controls, Block loops, source/native loading and lazy Error handling have separate native implementations installed by Prelude. | [Printing](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81ai-production-printing-methods.md), [System GC](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81aj-production-system-gc.md), [Text View](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81ak-production-text-view.md), [Output](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81al-production-system-output.md), [Input](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81aq-production-system-input.md), [Environment](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81ar-production-environment.md), [File IO](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81as-production-file-io.md), [Block Loop](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81at-production-block-loop.md), [Native Loader v12](https://github.com/chazu/birnam/blob/cffba722/abi/native-loader-v12.md), [Source Loader](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81av-production-source-loader.md), [Error Value](https://github.com/chazu/birnam/blob/cffba722/abi/error-value-v3.md) |
| Namespace | Binding storage, names, enumeration, lookup, presence, publication, import and export live in Namespace. Initializers receive matching explicit capabilities. Globals remains separate. | [Namespace v8](https://github.com/chazu/birnam/blob/cffba722/abi/namespace-v8.md), [Names](https://github.com/chazu/birnam/blob/cffba722/abi/namespace-names-v1.md), [Presence](https://github.com/chazu/birnam/blob/cffba722/abi/namespace-presence-v1.md), [Import](https://github.com/chazu/birnam/blob/cffba722/abi/namespace-import-v1.md), [Export](https://github.com/chazu/birnam/blob/cffba722/abi/namespace-export-v1.md); 81AW–81BF |
| Storage, tracing and collection | Storage validates protected facts and owns observation state, identity backing cells and the deferred index. Trace owns traversal; Collect owns coordination and phase state. Id keeps physical storage, source adapters and retirement/reclamation mechanisms. | [Storage](https://github.com/chazu/birnam/blob/cffba722/abi/storage-v1.md), [Trace](https://github.com/chazu/birnam/blob/cffba722/abi/trace-live-v1.md), [Collect](https://github.com/chazu/birnam/blob/cffba722/abi/collect-v1.md), [Observation](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bt-production-storage-observation-scopes.md), [Physical Identity](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81by-production-allocation-identity.md), [Deferred Identity](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bz-production-deferred-identity.md), [Deferred Index](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81ca-production-deferred-index.md) |
| GC and observation services | Explicit owners for method observation, roots, GC settings, payload layouts, trace hooks, finalizers, native stack/mark scopes and allocation budgets. Existing invocation order and lifetime rules remain. | [Method Observation](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bi-production-method-observation.md), [Roots](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bj-production-root-provider-registry.md), [Settings](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bk-production-gc-settings.md), [Layouts](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bl-production-payload-layout.md), [Hooks](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bm-production-trace-hooks.md), [Finalizers](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bn-production-finalizers.md), [Stack](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bq-production-native-stack-scanning.md), [Mark](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bp-production-native-mark-callback.md), [Budget](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81br-production-allocation-budget.md) |
| Dispatch caches | Dispatch Cache owns the inherited lookup table and guard-verdict cells. Id keeps fresh lookup, guard evaluation, invalidation and recovery decisions. | [Dispatch Cache](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bu-production-inherited-lookup-cache.md), [Guard Verdicts](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81bv-production-guard-verdict-caches.md) |

The [81F–81I seed experiments](81f-isolated-id-seed-proof.md) remain independent
Rust witnesses, not a second production runtime to merge wholesale. Production
private behavior and Integer were integrated in 81J–81L; storage observation,
tracing and coordination followed in 81M–81P. Seed-only construction, teardown
and reclamation experiments are evidence for future design, not unfinished
production features implicitly promised by this closeout.

## Remaining work, at useful granularity

**First decouple production Id.** [Design 82](82-minimal-production-id.md) defines
the required dependency direction, genuine bootstrap seams, production migration
and completion tests. Id must initialize without upper modules, then load them
through its message protocol. The extracted services currently remain Id's
construction dependencies; a smaller IdApi did not fix that inversion.

The broader workstreams remain below. Their ownership corrections belong within
Design 82; their Birnam implementations follow it. They are not competing
next-project choices. Small commits are useful within a complete responsibility;
each field does not need its own module, design document and full installation
campaign.

1. **First complete production Birnam replacement.** Use one already extracted
   value family, starting with Integer, to audit which existing authoring/entry
   facilities can express its implementation. The deliverable is the complete
   Integer implementation through its existing boundary, with Rust Integer
   removed from normal execution and retained only for independent bootstrap.
   If required facilities
   are missing, identify those explicitly; do not count forwarding wrappers as
   replacement. This follows production Id decoupling; it is not the next project.
2. **Allocation and materialization ownership.** As part of Design 82,
   address physical allocation, snapshot/journal lifecycle, charging,
   GC/retirement and recovery as one ownership design. Reconsider 81CB's buffer
   inside that work, together with the remaining numeric and counted-index
   storage. Success means an independently usable owner, not more raw containers.
3. **Execution and executable lifetime.** Separate registration/publication,
   retained code owners, Frame storage and Machine transitions with explicit
   lifecycle and mutation contracts. Keep fresh lookup, alias identity, shared
   Cells, finalizers and live replacement intact.
4. **Compiler and construction loop.** Move canonical parsing/lowering/emission
   into Birnam and prove self-compilation, shipped-module builds and image/source
   recovery on the intended substrate. Siphon decoupling belongs to Design 82;
   removal of C / libtcc remains a later decision. Preserve a cold bootstrap
   throughout.

Do not resume the old infinite extraction sequence from an earlier document's
"next candidate" paragraph. Before another native boundary, name the Birnam
consumer it enables or the complete responsibility it removes from Id, identify
its end-to-end acceptance, and account for ABI and fixture maintenance cost.

## Validation and evidence

The closeout uses the unchanged 81CA runtime. Debug validation is sufficient for
this phase; it makes no release or performance claim. The durable
[closeout receipt](../../measurements/81-production-closeout.json) records actual
commands, result summaries, source/artifact digests and cleanup disposition.
81CA's [implementation evidence](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/81ca-production-deferred-index.md#implementation-and-evidence)
retains the original differential tests and worker comparison.

Fresh closeout checks passed: 428 core tests (16 ignored), the focused native
suite including 16 host admission cases, seven loaded source/compiled cases,
and twelve mapped/rebuilt/baked image executions. All source and installed
artifact hashes match. Normal and forced recovery retain the same clone-index
counts across image carriers; numeric work appends no clone rows. The earlier
image-backed worker comparison remains recorded in 81CA and was not rerun for
this documentation-only closeout.

The reusable checks are:

```sh
mise exec -- cargo test --offline --locked \
  -p birnam-abi -p birnam-dispatch-cache -p birnam-host \
  -p birnam-id -p birnam-object-model -p birnam-storage --lib
scripts/deferred-index-test.sh "$prefix"
scripts/deferred-index-loaded-smoke.sh "$prefix"
scripts/deferred-index-image-smoke.sh "$prefix"
```

Here `prefix` is an existing debug installation whose receipt matches the
runtime source. For a fresh installation, use the documented
[installer](81a-current-bootstrap-and-runtime-layers.md#2-build-time-sequence).
The image harness also builds its Rust host fixture from the checkout. It checks
dynamic startup, source-free compiled callers, mapped/rebuilt/baked execution,
real deferred closure/numeric work, aliasing, live method changes, GC,
finalization, and the existing BUSY observation boundary. Earlier narrow fixtures
remain available for individual contract regressions; no copied 81CB suite is
introduced.

ABI versions and seed fixtures are intentional independent oracles. Historical
design proofs stay in Git and remain searchable. Disposable build outputs,
failed prototype files and superseded source copies are not project deliverables;
the receipt distinguishes their cleanup from retained test coverage.

At closeout, 29 superseded baseline source directories (47,214 files) were
archived and verified byte for byte before removal. Removing obsolete build
caches, loose baseline copies and the discarded prototype reduced their local
footprint by approximately 7.7 GiB after accounting for the archive. The active
build cache, installer cache, latest installation and latest comparison baseline
remain available. No supported regression test or ABI oracle was removed.
