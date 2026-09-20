# 81BQ — Native stack word scanning

> **Status:** Production extraction complete after 81BP (`45c02878`).
> Debug validation passed; release validation remains waived.

[81D](81d-id-api-partition.md) places native-root integration above Id.
[81BO](81bo-production-native-stack-anchor.md) moved anchor ownership into
Native Stack; [81BP](81bp-production-native-mark-callback.md) moved native mark
scope state and emission policy. The next portion is the native word walker in
`gc::emit_native_root_range`, moving into the existing Native Stack module.
Register capture and native interval validation remain in Id for this step.

## Actual caller and ordering

`runtime/tracing.rs::roots` is the only caller. It emits the captured native
interval first, then explicit initial roots, invalidates dispatch caches, reads
floor roots and the root-provider snapshot, and finally invokes native providers.
The tracer retains immediate first-mark admission and LIFO traversal. The walker
owns none of those later steps, heap membership or marking state.

The ordinary collection path captures an interval after the collection-phase
guard, executable-retirement retry and anchor read. Source probes retain an
outer captured interval. The symbol-compaction mark path deliberately supplies
`(0, 0)`. All three paths must retain their existing meaning; the scanner must
not fetch the current anchor itself or treat every empty interval as an error.

## Exact walker behavior

The existing walker rounds the start upward to a machine-word boundary with
`next_multiple_of(size_of::<usize>())`. It reads only complete words whose
checked exclusive end is at or below the supplied bottom. It reads one word,
immediately emits that exact value, then advances by one word. Preserve:

- Low-to-high order, null values, duplicate values and arbitrary pointer bits.
  Heap membership and deduplication belong to the selected tracer.
- Empty `(0, 0)` and valid empty/partial-word ranges invoke no sink callbacks.
- Reads occur at their existing points, not through an eager copy of the range.
  A callback changing a later word must affect that later read.
- Emission uses the caller's sink/context and checks every result immediately.
  A non-OK result aborts with the exact historical diagnostic
  `birnam-id invariant failed: GC tracer rejected a candidate` before another
  word is read or emitted.
- No allocation, Id admission, collection, new roots, counters or anchor writes.
  This is a raw synchronous memory reader, not ordinary Id object access.

Do not expand the accepted input domain, add pointer validation to this walker,
replace checked arithmetic, change alignment rules or add a new recovery result.
Its caller must supply a live readable interval with a representable rounded
start. Production capture already establishes the applicable bounds. Array
fixtures may exercise the same walker using valid explicitly owned memory.

## Physical boundary stays in Id

Keep `RegisterSave`, its 192-byte size and 16-byte alignment, Darwin arm64
`_setjmp`, `with_native_root_range`, the downward-distance check, the 128 MiB
limit and the non-Darwin fallback exactly where they are. Capture must still
happen in the shallow frame above collection/tracing work. The relocated walker
runs below that captured start; it must not replace the start with a local
address from inside Native Stack.

Keep source-probe outer-range storage and its non-inlined body unchanged. Keep
ordinary and source-probe collection read points, retirement order and tracing
adapters unchanged apart from the one walker call substitution. Re-reading the
anchor during scanning would incorrectly make callback-time resets affect the
current collection. This remains the original copied interval.

## Native Stack contract migration

Extend Native Stack's immutable implementation factory to v2, adding
`scan(sink, start, bottom)` after create/destroy/canonical set/get. The new factory
is 56 bytes and is attached through versioned v2 provider/dependency exports.
The public setter capability and copied admission floor remain v1 and unchanged.
Create/destroy/set/get remain callback-free; scan has the explicit synchronous
sink callback contract above. Do not thread a Runtime, heap, Rust closure or
private representation through the new operation.

Migrate host attachment, Id's ownership adapter, Rust tests, standalone C
preparation helpers, the actual host preparation test and installed Native Stack
consumer together. Freeze the old v1 C header/contract rather than editing it
in place. Retain the v1 public capability provider; remove the superseded factory
and dependency exports from current implementation manifests. No compatibility
facade or extra initializer argument is needed.

IdApi remains v20, native initialization remains twelve arguments, Native Loader
remains v12, and image encoding remains v8. This deepens the existing native-root
module; it does not add another library solely for a word loop. Siphon startup
and installer library counts therefore remain unchanged.

## Independent proof and completion gates

Preserve completed 81BP source and its installed debug prefix before coding.
Use independent baseline/new fixtures to compare actual native roots, root/hook
ordering, next-allocation triggering, callback-time anchor reset, nested
collection, native resource finalizers and failure before sweep. Preserve source
probe and symbol-compaction checks, including the intentional empty native range.

At the direct scanner boundary, use valid owned buffers to check aligned and
unaligned starts, partial trailing words, empty ranges, exact zero/duplicate/high
bit candidates and order. Mutate a later word from a sink callback to prove that
reads remain incremental. A rejection probe must prove that no later callback
occurs and compare the exact abort diagnostic. Validate v2 factory admission
inertly, including a missing scan operation, and rerun the actual host anchor
selection/precise-policy ordering test.

The physical proof must include an actual Oop held only in the live native
interval, using the independent 81BP runtime as the oracle; a synthetic buffer
alone cannot prove conservative-root preservation. Review capture/source-probe
code for exact agreement and preserve their current positions and signatures.

Run focused Native Stack, Native Mark, tracer and Id/ABI checks, followed by
installed dynamic startup, source-free compiled callers, mapped/rebuilt saved
native Code images and baked execution. Confirm actual callback/scan symbol
ownership, aliases, reload and the real image-backed worker comparison. Check
all declared export changes, linkage and source/artifact receipts. Record final
evidence and commit the extraction before selecting another responsibility.

## Completed implementation and evidence

Native Stack factory v2 now owns the word walker. Id's existing
`emit_native_root_range` selects that immutable scan operation at the original
call site. The v1 setter capability and copied floor are unchanged, and their
historical C declarations/contract remain frozen. Current factory/dependency
exports, host attachment, Id ownership, Rust/C fixtures and native image
consumers now use v2. No library or native initializer argument was added.

The completed 81BP source at `45c02878` and its installed debug prefix were
preserved before migration. Final debug evidence:

- ABI, Id, Native Mark and Native Stack library tests: 296 passed, with 15
  existing Id tests ignored. Workspace build and all-target checks passed.
  Scanner tests cover every byte boundary in a valid buffer, partial trailing
  words, empty intervals, exact zero/duplicate/high-bit candidates, incremental
  reads after callback mutation, synchronous reentry and immediate rejection.
- Five actual old/new runtime anchor transcripts match exit status, stdout and
  stderr. They exercise an Oop held only in a live native interval, automatic
  collection thresholds, provider roots, callback-time anchor reset, native
  finalization, nested collection and owner/observation failures.
- Two direct old/new scanner transcripts also match exactly. For this separate
  buffer-level comparison, the private old walker and failure policy were
  compiled verbatim from preserved 81BP source behind a test-only C entry;
  this supplements the actual-runtime native-root comparison above.
- The feature-gated source-probe test passed: a nested probe retains its outer
  interval after resetting the current anchor, preserves an outer native-only
  object and reclaims deeper scratch that must not become a native root.
  Existing empty-range symbol-compaction and weak-dictionary tests passed in
  the Id suite.
- The focused Native Stack and Native Mark gates passed. The actual installed
  host test confirms live anchor callback/context selection and ordering before
  optional precise-GC policy. All 18 host factory/capability admission cases
  passed, including missing scan and unchanged v1 public-capability validation.
- Live-tracing and Finalizers gates passed, including native roots, the tracer's
  admission/LIFO oracle, failure before sweep, ordered native cleanup, reentry
  and all seven native finalizer callers. The v20 C client also passed.
- Installed startup, source-free compiled callers, mapped/rebuilt saved native
  Code images and baked execution passed. The native consumer checks scan
  symbol ownership, incremental reads, a live native-only object, anchor resets,
  aliases and module reload. Both old/new image-backed workers completed 12
  exact wire round trips and orderly shutdown with matching transcripts.
- All 79 installed library export inventories and 35 native manifests match
  the declared v1-to-v2 factory/dependency changes. Id and Native Stack link only
  system libraries. All 478 current build inputs and 515 published artifact
  hashes match the debug receipt; counts remain 51 native libraries and 28
  compiled modules.

The final review confirms exact agreement for capture, collection, tracing,
Runtime, the Native Mark adapter and Native Loader. Source-probe production
code is unchanged; only the feature-gated regression test was appended. IdApi
remains v20 and image encoding remains v8. Current C fixtures pass strict syntax
checks. Evidence is in `.cargo-target/81bq-*.log`; repeatable entry points remain
`scripts/native-stack-test.sh PREFIX` and
`scripts/native-stack-image-smoke.sh PREFIX`.
