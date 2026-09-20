# 81BS — Per-runtime collection-phase ownership

> **Status:** Implemented and verified after completed 81BR (`57925cdc`).
> Dynamic Siphon startup and compiled/image consumers verified with debug builds.
> No new language behavior; public IdApi and image encoding unchanged.

[81BR](81br-production-allocation-budget.md) moved Heap allocation charges into
their own owner. The next small boundary is Runtime's `Cell<CollectionPhase>`.
Move its state and phase assignment into the existing `birnam-collect` module.
Keep Id's admission decisions and synchronous cleanup guard at their current
call sites. This adds an internal factory, not another library or language API.

## Current state and exact decision points

Every Runtime starts in `Idle`. The scoped collection source callback accepts
the existing Collect v1 phase codes: `IDLE = 0`, `MARKING = 1`, `FINALIZING = 2`.
It aborts with `birnam-id invariant failed: invalid collection phase` for any
other value before changing state. Valid assignment is unconditional, including
repeated assignment and any order of the three valid values.

| Reader | Existing behavior to retain |
|---|---|
| `require_allocation_allowed` | Non-idle allocation aborts with `id allocation attempted during collection`, before allocation or automatic collection |
| `gc_collect_active` | Non-idle returns zero before pending executable-retirement draining, anchor lookup or native capture |
| `attach_trace` | Non-idle rejects attachment before descriptor validation or function-identity admission |
| `attach_collect` | Same phase-first rejection, retaining the existing coordinator selection |
| `cached_default_lookup` | After direct dictionary lookup and epoch selection, non-idle bypasses inherited dispatch-cache probes and writes |
| `observe_storage` | Owner/observation checks occur first; non-idle returns `BUSY` before selector/request validation and permission creation |
| `with_storage` | Non-idle participates at its original position in the permission, observation and dispatch-depth checks; returns `BUSY` |

The actual coordinator still drives the same source callbacks in the same
order. It sets Marking before clearing marks and tracing, Finalizing before the
ordered detach/finalizer/reclaim loop, and Idle after accounting reset but before
Native Tier pruning. `CollectionGuard` is created at the same point in
`gc_collect_from_native_range`, and its Drop assigns Idle at the same point on
normal return or Rust unwind. Do not remove that reset because the coordinator
also normally writes Idle.

The source-probe route deliberately calls `gc_collect_from_native_range`
directly with its saved physical interval. Preserve that route and the ordinary
entry's different admission order. Mark-only calls to `run_mark_phase` do not
set the collection phase today; do not add phase changes around them.

## Ownership and internal factory

Collect owns one distinct opaque native phase context per Runtime, initialized
to Idle and destroyed with that Runtime. A moved Runtime retains its context.
Separate Runtime instances, floors and threads must not share phase state.
The owner retains no Runtime, Heap, Oop, callback, public IdApi or source pointer.

Use an immutable 48-byte `Collection Phase` factory descriptor with version,
byte size and four operations:

- `create() -> context`: allocate a native owner initialized to Idle.
- `destroy(context)`: free that owner exactly once.
- `read(context) -> u32`: copy its current phase code.
- `set(context, phase) -> int`: assign a valid phase and return 1; return 0 for
  any other code without changing state.

Read/set allocate nothing and call no foreign or Id code. Creation/destruction
only manage native storage. This permits use while a RuntimeState borrow is
live, notably in inherited dispatch lookup. Runtime's owner-thread discipline
continues to serialize access. Id retains its existing diagnostic when a source
phase assignment is rejected; this is not a new phase-transition policy.

Export `birnam_collection_phase_factory_v1` from Collect and
`birnam_collection_phase_dependency_v1` from Id. Validate version, exact size and
all four operations inertly. Accept repeated attachment of the same descriptor
and reject retargeting. Pin the descriptor and code for every owner's lifetime.

The host prepares this factory before Id initialization. Collect's library is
therefore mapped earlier, but its ordinary initializer and coordinator attachment
stay in their existing Siphon positions. Creating a Runtime must not implicitly
attach the collection driver. Standalone C/Rust fixture preparation must also
attach the factory before Id initialization; focused build lists must include
Collect where that was not already required.

Keep `Collect::ApiV1`, `SourceV1`, the function identity used by `attach_collect`,
the public IdApi v20, twelve-argument native initializer, Native Loader v12 and
image v8 unchanged. The installed library count remains 80.

## Adversarial boundary review

Collect already has a separate thread-local `ACTIVE` flag guarding reentry into
its driver. It is not Runtime's phase. Its lifetime includes validation/effects
and extends through pruning; Runtime is already Idle during pruning. It also
spans use of that loaded driver by different Runtime sources on one thread.
Do not merge the two states, change that scope or replace per-runtime phase
contexts with module-global/thread-local phase state.

Do not cache Idle across a callback, hoist its read ahead of direct lookup,
reorder storage admission, or turn the three values into a boolean. Do not add
a transition graph: valid assignments currently accept every ordering. A
factory setter must not invoke language behavior or re-enter Id while a caller
holds state. Do not retain a Source pointer or attempt to reconstruct Runtime
from it inside Collect.

This slice changes no lookup, mutation, object identity, allocation extent,
logical count, roots, finalizer order, code lifetime or durable state. Phase
state is transient native Runtime ownership; restored runtimes start Idle as
before. Actual collector order remains independently implemented and tested.

## Independent proof and completion gates

Preserve completed 81BR source and its installed debug prefix before migration.
Run the same new witness against the old Runtime implementation and the
extracted implementation. Cover distinct/moved Runtime owners, every phase code,
all seven reader decisions, early refusal before invalid descriptors/requests,
the direct-dictionary/cache distinction and guard reset during Rust unwind.
Exercise actual callbacks to prove marking/finalizing/idle timing, and preserve
all existing fatal diagnostics through subprocesses.

Use a real loaded factory and actual host admission, not only direct Rust
helpers. Verify missing/null/invalid factory rejection before Id initialization,
and capture native context creation to prove ownership belongs to the Runtime.
Cover nested collection in providers and finalizers, allowed Id reads, forbidden
allocation, trace/coordinator attachment rejection, and storage observation
refusal. Preserve the independent driver's busy behavior, including invalid
reentry requests, and its failure-before-sweep rules.

Run dynamic Siphon startup and source-free compiled consumers with GC activity,
mapped/rebuilt saved native Code images and baked execution. Exercise aliases,
module reload, native cleanup and actual image-backed workers. Retain source
probe and executable-retirement coverage. Check unchanged old/new consumer
transcripts, exports, linkage, admission contracts and final source/artifact
receipts. Add a repeatable focused gate, ABI reference, README links and changelog;
record completion evidence and commit before selecting another extraction.

## Completed implementation and evidence

Collect now owns the opaque phase context. Id holds its owner handle, preserves
the existing phase enum for callers, and retains assignment diagnostics and
`CollectionGuard`. Host and standalone fixture preparation attach the new
factory before Id initialization. Collect's normal initializer, coordinator
selection and Siphon position remain unchanged. The
[v1 contract](../../abi/collection-phase-v1.md), C mirror, focused gate, image
smoke, README links and changelog accompany the implementation.

Final review confirmed that the coordinator body and thread-local `ACTIVE`
guard are byte-for-byte unchanged. The storage, tracing, inherited lookup-cache
and source-probe consumer files are unchanged, as are Collect's v1 descriptors.
Runtime's allocation and nested-GC admission expressions and guard reset remain
at their previous sites. The phase callback delegates only phase validation and
assignment; its exact invariant diagnostic remains in Id.

Debug evidence:

- ABI, Collect and Id library tests passed: 297 tests, 15 ignored. Workspace
  build and all-target workspace check passed. The additional `ffi-test-hooks`
  source-probe test passed.
- The same new Runtime witness passed against preserved pre-extraction Runtime
  code, augmented only with test code invoking its original phase callback.
  All eleven transcript rows matched: moved/independent owners, every valid
  assignment, early GC/trace/coordinator/storage refusals, unwind reset, direct
  and inherited lookup-cache behavior, live default storage admission after
  permission creation, and surviving owner destruction. Both builds preserved
  the exact abort diagnostics for invalid phase and marking/finalizing allocation.
- The focused C gate captures the actual Runtime's native context at factory
  creation. Preparation creates no context; Id initialization creates one.
  Actual collection performs Marking, Finalizing, Idle, then the guard's Idle
  reset. Root and finalizer callbacks retain nested-GC, attachment and storage
  refusal while allowing ordinary reads. Runtime is Idle during pruning while
  the independent coordinator returns BUSY. Four failure subprocesses preserve
  exact diagnostics, including an unmounted coordinator: early phase preparation
  does not silently attach the collection driver.
- All twelve real host factory-admission cases passed, covering successful
  initialization, missing library/provider, null/version/size/four-operation
  rejection, absent floor dependency and rejected attachment.
- Six old/new actual runtime transcripts and exit statuses matched using the
  independent native-anchor and live-tracing fixtures. They preserve native and
  provider roots, automatic collection, callback/finalizer reset, owner-thread
  and observation admission, traversal order, live hooks, immutable attachment
  and failure before sweep.
- The Collection Phase image smoke passed dynamic Siphon startup, source-free
  compiled callers, mapped/rebuilt saved native Code images and baked execution.
  It verifies already-loaded operation ownership, independent contexts even
  during root callbacks, invalid/reordered assignment, nested collection and
  attachment refusal, arithmetic loops, automatic collection, roots, aliases
  and reload. The existing Finalizers image smoke passed the same execution
  paths for native cleanup and detachment.
- Old/new actual image-backed workers passed twelve exact wire-v1 value round
  trips and orderly stop; complete transcripts matched byte for byte. The
  external debug harness did not change production handshake deadlines.
- The Allocation Budget regression gate passed, including its actual bootstrap
  Heap, overflow diagnostic and fourteen host-admission cases after the earlier
  Collect preparation was added to fixture dependencies.
- Installation still contains 52 native libraries and 28 compiled modules.
  All 80 export sets match the baseline plus the two new factory/dependency
  exports. All 36 native manifests match; Id and Collect link only system
  libraries. Old/new installed admission contracts match exactly, including
  IdApi v20, runtime ABI hash and image v8. Native initialization remains twelve
  arguments and Native Loader remains v12.
- All 489 current source inputs and 527 installed artifacts match receipts.
  Source digest:
  `01023e683e98694ba3ecf9f66847a75be5ff9ef83009529639e6e9206c4e6864`.
  Artifact digest:
  `430b1b2349e5d96236c386069cec8ed6792f2142100b949c88d05ff8c6f306e1`.

Local logs and prefix markers are `.cargo-target/81bs-*`. Repeatable checks are
`scripts/collection-phase-test.sh [PREFIX]` and
`scripts/collection-phase-image-smoke.sh PREFIX`. No release validation was run.
