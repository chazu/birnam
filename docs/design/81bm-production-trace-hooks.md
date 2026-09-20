# 81BM — Native trace-hook descriptions

> **Status:** Implemented after completed 81BL (`2cb614b2`) and the
> `d4a31a02` implementation checkpoint. Debug validation only.

[81D](81d-id-api-partition.md) places trace-provider integration above Id. The
next extraction is `birnam-trace-hooks`: registration of a native hook on an
exact vtable, and selection of the nearest inherited hook. The existing Trace
module still owns traversal; the live floor adapter still invokes the selected
hook after reading an object's payload. This introduces no new tracing mode,
callback kind, language method, inheritance rule or collection policy.

## Existing laws

`VtableRecord` stores an optional hook; a new record has none. The public
`setTraceHook` admits ordinary owner-thread/storage access before requiring
exact vtable membership. It overwrites the hook, synchronizes the raw vtable
mirror and does not bump the mutation epoch. A setter takes a nonnull function
pointer; clearing a hook is not an existing operation and will not be added.
Changing a hook during collection remains allowed. Calling it during restricted
storage observation retains the existing admission failure.

`Vtables::trace_hook` performs no public admission. It starts at the supplied
vtable, snapshots the registered-record count as a walk budget, and checks each
exact record before following its current parent. The first present hook wins.
A null vtable, missing record, null parent or exhausted budget returns no hook.
Cycles remain bounded by the same count; no new cycle diagnostic is introduced.
A parent mutation or an earlier object's native callback can change the next
query's result. No selection is cached across calls.

The 80-byte raw vtable mirror retains the hook at offset 40. Other record
mutations resynchronize the same pointer. Records remain weak, and a hook pointer
adds no object root. The existing requirement that callback code outlive its
registration remains unchanged; this extraction neither grants unload permission
nor invents a new ownership registry.

## Selected boundary and rejected shortcuts

Move the allocation-free optional-hook carrier and bounded selection algorithm
into a native service. Id retains exact record attachment, parent facts, registry
membership and retirement. Returning an owned Rust record or a Runtime pointer
would violate that boundary. A wrapper that simply called the old complete Id
setter/walker would leave the responsibility in Id. A second address-keyed
registry would duplicate weak identity and retirement authority. Neither is used.

Use an inline, eight-byte C slot with an optional hook. A canonical module
operation constructs a populated slot; Id attaches it to the exact record and
synchronizes its raw view. No per-record native allocation or destructor is
needed. An inert immutable factory is prepared and pinned before Id startup;
repeat attachment accepts the same pointer and rejects retargeting.

The public capability exposes only `set(context, vtable, hook)`. Its copied floor
descriptor has ordinary admission and exact slot replacement callbacks. The
module admits without holding a RuntimeState borrow, creates the slot and passes
it to the floor. The floor requires membership and replaces the attachment.
Internal setup uses the canonical slot operation at the existing call points.
Public descriptor instrumentation must not redirect internal construction or
hook selection.

Canonical selection consumes a synchronous C source containing a copied walk
budget, context, and exact-entry reader. An entry is a copied parent and slot;
the reader reports missing membership explicitly. The floor lends only this
read-only query source, never a Rust registry or retained state address. Source
callbacks are internal fact reads: they perform no language/native hook calls,
allocation, registry mutation or public admission. The module walks those facts
and returns an optional hook without invoking it. A temporary shared registry
borrow ends before the live adapter calls a hook or mark callback.

## Callers and timing

| Caller | Required selection and timing |
|---|---|
| Id Machine/Continuation family setup | Canonical registration before public Runtime publication; preserve current family creation and cleanup hooks |
| PEG Pattern/Grammar/CST registration | Saved first Runtime API and matching saved trace capability, after each existing layout assignment |
| Store and Cell registration | Saved first Runtime API/capability; preserve method installation and layout-before-hook order |
| Portable Code node/statement/binding/CodeObject families | Capability saved with the first execution API; preserve lazy construction, roots and finalizers |
| TCC Unit registration | Saved first Runtime API/capability on repeated initialization; ForeignAddress family preparation still uses its incoming layout capability |
| Native Loader | Save and forward the trace capability with the first execution API on every repeated initializer invocation |
| Live GC | Query at the current post-payload point for each reached ordinary object; invoke only after dropping the source/record borrow |
| Storage observation | Copy the inherited hook at the current allocation-description point without invoking it or ordinary admission |
| Recovery and optimizers | Retain current hook-presence refusals and short-circuit/read order in graph recovery, closure elision, discarded clones, virtual aggregates and unboxed numerics |

Remove only `setTraceHook` from IdApi: v18 has 20 fields / 160 bytes. Native
initialization gains an eleventh explicit capability; Native Loader becomes v11.
Existing early value preparation does not register hooks and needs no new
argument. Migrate native initializers and independent fixtures together while
preserving historical headers. Keep MarkFn/TraceHook signatures, raw mirror,
image v8 encoding and native callback lifetime unchanged. Native consumers must
rebuild. There is no API-address lookup or aggregate compatibility facade.

## Adversarial audit

- **Fresh selection:** inspect the parent at each query, keep the existing count
  bound, and prove a parent change or earlier callback changes a later result.
  Do not precompute all allocation hooks for live GC.
- **Borrow safety:** internal fact callbacks cannot reenter a language send; the
  source expires before invoking the selected hook. Public admission holds no
  state borrow across a callback.
- **Identity and retirement:** attach inline metadata to existing records; neither
  a function pointer nor its carrier roots a behavior or resurrects a retired
  address. Test independent/moved Runtimes and weak record reclamation.
- **Diagnostics:** preserve admission before exact membership. Invalid internal
  queries return no hook; the public setter still aborts on invalid membership.
- **Dispatch and image:** layout and trace metadata remain separate, including
  inherited-hook refusals for aggregate recovery. The image codec and callback
  execution order are unchanged.

## Evidence required before completion

Archive the completed 81BL source and installed debug prefix before coding.
Compare old/new independent C transcripts for registration, replacement, parent
selection, observation and callback execution. Cover unset, direct, inherited,
nearest override, parent reassignment, missing records, self-cycles, multi-record
cycles, count bounds, raw mirror stability and unchanged mutation epochs.
Include owner-thread/observation admission failures and invalid membership.

Test the canonical factory and source independently, including exact entry-read
order and no hook invocation. Mutate the public descriptor and confirm canonical
queries stay unchanged. Keep the existing live-walk transcript while replacing a
future hook through the new public capability. Check registry retirement and
independent/moved Runtime state. Use distinct incoming/saved capabilities for
all four production registration consumers and repeat initialization.

Verify installed symbol ownership and source-free compiled startup, saved native
Code image recovery and baked execution. Exercise real Store/Cell, PEG, Portable
Code and TCC paths, storage observation, recovery/optimizer refusal, adjacent
GC/image checks and image-backed worker replies. Check the final source/artifact
relationship, exports and system-only link dependencies. Commit the completed
extraction before selecting the next responsibility. Do not run release gates.

## Implementation and evidence

`birnam-trace-hooks` owns the inline slot and inherited selection algorithm.
Id supplies exact record facts and attachment primitives. The host prepares and
pins the factory before `id_init`, includes the library in startup/install
inventories, and supplies the matching public capability. PEG, Store, Portable
Code and TCC retain the first execution selection and consult its live descriptor
at each original registration point. Prelude and Native Loader preserve that
selection during repeated initialization.

IdApi v18 removes only `setTraceHook`: 20 fields / 160 bytes. All current Rust and
independent C initializers receive eleven arguments. Native Loader v11 installs
with thirteen arguments. Early value preparation contracts and image v8 encoding
are unchanged; historical headers and manifests remain frozen. The v18 manifest
fingerprint is
`9015df2ad6d27a354ee9bcd48a8e959dddd9494ab40c2dfdd5a883e6b9b94224`.
`MarkFn` and `TraceHook` remain shared ABI types, not IdApi slots.

Validation uses the untouched `2cb614b2` source archive and its installed debug
prefix as the independent pre-extraction baseline:

- Locked offline workspace/all-target checks and debug builds pass. Focused
  tests pass: 14 ABI, 264 Id and four Trace Hooks tests (282 total); 15 existing
  Id tests remain ignored. The selection suite compares 18,000 graph/budget/root
  cases against an independent all-pairs shortest-path oracle. Id tests cover
  raw mirrors, unchanged epochs, missing records, cycles, live parents, weak
  retirement, moved/independent Runtimes and public descriptor instrumentation.
- All 78 current C fixtures pass strict syntax checks. The new independent
  old/new registration probe matches exactly for unset/inherited/nearest hooks,
  replacement, raw mirror resynchronization, parent changes and observation
  without invocation. Owner-thread, restricted-observation and invalid-membership
  failures match both exit status and exact diagnostic. C factory checks confirm
  symbol ownership, slot/source layout, exact read count and budget behavior.
- Separate PEG, Store/Cell, Portable Code and TCC processes produce identical
  old/new transcripts after two reinitializations with different incoming APIs
  and capabilities. Replacement of the saved descriptor's function and context
  is observed; the later incoming hook setter is never selected. Native Loader's
  independent oracle checks matching forwarded capability identity, including
  registration reentry. The TCC layout oracle still distinguishes incoming
  ForeignAddress preparation from saved Unit registration.
- The existing live-tracing/current C client, storage-observation,
  storage-tracing, native-loader and payload-layout debug scripts pass. The live
  traversal fixture now replaces future hooks through the explicit capability.
  Nineteen prior native method transcripts and 27 value/storage consumer
  transcripts match the pre-extraction build. All 145 stdlib assertions pass.
- Seventeen installed-host preparation cases cover missing images/exports,
  null/malformed factories, each missing canonical operation, missing/rejected
  floor attachment, missing/malformed public capabilities and successful startup.
- A debug prefix contains 48 native libraries and 28 compiled Birnam modules.
  All 76 library export sets match the baseline plus the declared extraction;
  all 32 applicable native manifests match. Id and Trace Hooks link only system
  libraries. All 2,038 source hashes were unchanged through installation;
  subsequent edits are fixture corrections and documentation, with production
  build inputs unchanged.
- `scripts/trace-hooks-image-smoke.sh` passes dynamic startup, source-free
  compiled-module execution, v8 native Code image reload and baked execution.
  Actual hooks execute across collections and repeated module loads while roots
  and aliases survive. Dynamic-library traces and `dladdr` establish ownership.
  The first smoke draft incorrectly expected immediate reclamation after clearing
  explicit roots while C locals were still conservatively scanned; only that
  fixture assertion changed, not collection policy or production code.
- Portable image v8 passes ordinary and forced-rebuild entry, source-free nested
  meaning, CodeBinding recovery, baked entry, resave and boundary refusal. Actual
  old/new saved-image workers return identical ready, twelve wire-value replies
  and orderly-stop bytes. The external harness accommodates debug startup without
  changing the production handshake deadline.
- Installed Native Loader, Method Observation, Root Registry, GC Settings and
  Payload Layout image suites also pass. They cover Store mirror behavior,
  duplicate roots and callback-time mutation, precise/automatic settings,
  reload, baked execution and live image-encoding reads. Floor discovery passes
  against the same installed prefix.

Run `mise run trace-hooks` for focused registration/selection/consumer checks and
`scripts/trace-hooks-image-smoke.sh PREFIX` for installed startup and image proof.
No release validation was run.
