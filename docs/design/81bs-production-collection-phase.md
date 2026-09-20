# 81BS — Per-runtime collection-phase ownership

> **Status:** Audited next extraction after completed 81BR (`57925cdc`).
> Production migration pending. Preserve behavior; debug validation only.

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
