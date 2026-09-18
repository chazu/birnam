# 81I — Ordinary storage reclamation above Id

> **Status:** Executed Rust experiment, 2026-09-18, macOS arm64.
> Builds on the 81H checkpoint `c4d49b23`. A separately loaded collector retires
> unreachable ordinary objects and their upper-module metadata, then Id releases
> their storage. The original native SeedApi carrier remains byte-for-byte unchanged.

## 1. Result and scope

**Allocation, tracing, metadata retirement, storage release and safe address
reuse now compose through the declared seed boundary.** The collector lives in
its own dylib and uses the existing tracer through a prepared entry. Neither
module imports seed implementation state or links a production Birnam runtime.

This first collection policy explicitly roots all seed-owned objects: Symbols
and default behaviors, including detached private behaviors. Their outgoing
edges participate in tracing normally. Merely declining to free a behavior would
be insufficient: its parent or attached behavior can be an ordinary object,
which must also survive. Module providers root factory/service objects and the
host supplies its application roots.

Only unreachable ordinary allocations are eligible. This is not a complete
production collector, behavior/binding retirement, weak Symbol collection,
finalization, native stack scanning, or a Birnam-authored implementation.

| Component | Responsibility |
| --- | --- |
| Id seed | Authoritative allocation membership, core edges, phase exclusion, validated detachment and backing storage release/reuse |
| Tracer dylib | Graph validation and reachability from declared roots/edges/payload descriptions |
| Collector dylib | Retained-kind policy, candidate selection and all-participant metadata retirement transaction |
| Integer dylib | Prepare/commit removal of initialized-instance provenance |
| Object-model dylib | Prepare/commit removal of weak private-owner associations |
| Pair fixture | Prepare/commit removal of representation descriptions |
| Host probe | Explicit roots, complete participant list, callback/code lifetimes and the independent acceptance oracle |

Sources: [collector](../../experiments/id-seed/collect/src/lib.rs),
[seed](../../experiments/id-seed/seed/src/lib.rs),
[carriers](../../experiments/id-seed/observation-abi/src/lib.rs), and
[probe](../../experiments/id-seed/collect-probe/src/main.rs).

## 2. Protected trace, retire and release

The Vtable root gains the ordinary replaceable message **`withReclamationView:`**.
Like `withStorageView:`, its default method starts only at a top-level,
owner-thread host safepoint. It supplies the same storage facts, plus a prepared
release Method and a phase epoch. The release Method is bound to that seed;
the epoch is valid only within that invocation. It cannot apply an old report
after intervening execution or during a later observation/collection phase.

The sequence is:

1. Id starts exclusion and snapshots its storage facts without retaining a
   registry borrow across any external callback.
2. The collector adds seed-owned objects to the actual root set, then invokes
   the tracer with the borrowed view, explicit roots and prepared providers.
3. The tracer finishes validation and reports unreachable objects. The collector
   copies their identities/counts; it does not release storage inside the trace
   report callback. Trace/provider failure ends the operation without retirement.
4. The collector invokes Id's release capability with that ordinary-object batch
   and a retirement callback. Id rejects wrong epochs, unknown/duplicate objects,
   seed-owned candidates and any surviving core edge into the batch. It reserves
   its detachment scratch before invoking retirement.
5. The collector prepares every participant's replacement metadata. Only after
   every preparation succeeds does it commit all plans.
6. Id removes all batch identities from its allocation registry, then releases
   their backing blocks. No fallible operation or user callback follows successful
   metadata retirement. The collector returns retired object and logical-byte counts.

Ordinary sends, reassociation and captured direct seed mutators remain blocked
through this entire sequence. Release cannot reenter itself; a successful release
consumes the capability for that phase. A failed validation/preparation can be
retried within the same phase because no state has committed. Returning an error
before commit restores normal operation without making any address reusable.

Successful release invalidates the view for further tracing or payload access.
Providers cannot retain borrowed arrays. Saved phase identifiers and Method
pairs are tested only while their seed/code remain alive; this is not permission
to use an API after seed destruction.

Id validates its own edges and storage ownership. It does not recompute payload
reachability or know Integer/Pair layouts. Correct roots, payload descriptions,
release selection and transactional callbacks remain trusted upper contracts.
An arbitrary native callback can violate those contracts by writing memory; the
phase is not an OS sandbox or protection against malicious replacement methods.

## 3. Metadata retirement without partial success

Each `CollectionParticipant` pairs its storage provider with a prepared retirement
entry. Integer publishes **`prepareStorageRetirement`** on its factory; the
object-model module publishes **`objectModelPrepareStorageRetirement`** on Vtable.
The host resolves these through ordinary messages before collection.

Preparation receives the full retiring identity batch and returns an opaque plan
with native `commit` and `abort` callbacks. The implementation copies its own
metadata and removes retiring entries from the copy. Existing state remains
unchanged. Failed preparation disposes its own scratch; the collector aborts
previous successful preparations in reverse order and reports the error.

After every preparation succeeds, commits swap in their prepared metadata.
Commit/abort consume the plan, cannot fail or unwind, and cannot invoke user
methods, allocate object storage, run finalizers or mutate payloads. These are
metadata operations, not general object destructors. Allocation failure/panic in
this bounded Rust witness is process-fatal rather than a recoverable transaction.

Integer removes retiring identities from its provenance set. Object-model removes
entries whose owner or private behavior is in the batch (this slice only retires
ordinary owners). Pair removes its retired payload descriptions. These removals
happen **before Id detaches or makes storage reusable**.

The host must include every metadata owner, including weak caches with no payload
description. Missing descriptions of nonempty payloads are detectable, but a
forgotten cache-only participant is not discoverable from allocation facts.
Participant completeness is an explicit mounting contract, not a proved automatic
service registry. Returning success with an invalid/uninitialized plan or changing
live state during preparation violates the trusted callback ABI.

## 4. Storage reuse and the native boundary

Id keeps at most **one reusable backing block**. Every other retired backing block
is dropped; a mismatched spare is also dropped on the next allocation. Reuse
clears the complete block, installs the new behavior, and records the newly
requested logical extent/kind. No retired identity remains in the allocation
registry. This bounded allocator policy makes the exact-address witness
repeatable without relying on the system allocator's address choices.

The spare is allocator capacity, not a rooted object. Retired logical bytes are
not an RSS reduction metric: one block can remain reusable and the system
allocator may retain freed pages. This experiment makes no performance claim.

Two scoped native interfaces complement the ordinary messages:

- `birnam_trace_view_v1` calculates from an already protected view, using the same
  tracing implementation as 81H. The existing `birnam_trace_v1` still starts a
  read-only observation phase. Starting another ordinary send from within the
  collection phase would violate exclusion.
- The release Method and prepared participant callbacks run directly inside the
  phase, after message lookup is complete. `birnam_collect_v1` starts collection
  from the explicit host boundary and sends `withReclamationView:`.

These are explicit integration exceptions, not new native SeedApi fields or a
permanent decision about the eventual language-level collector interface. The
collector depends only on the two experimental carrier crates; the tracer is
passed as a pinned Method. Id contains no collector handle, module index layout,
mark set, or transitive reachability algorithm. The seed retains storage/phase
mechanics, including batch validation and its bounded spare-block policy.

## 5. Executed evidence

Run the complete isolated suite:

```sh
mise run id-seed-experiment
```

| Collection case | Observation |
| --- | --- |
| Cycles and survivors | Exact dead cycle/Integer/empty-object counts; seed membership disappears; surviving Integers still send and compute correctly; address-shaped raw words do not root objects |
| Root scopes and retained behavior edges | Explicit roots retain then release values; retained behavior parents keep ordinary targets alive; Pair representation survives reassociation |
| Failure atomicity | Description failure, invalid roots and missing providers release nothing; a late preparation failure aborts earlier plans and preserves Integer/private-owner state |
| Forced Integer/private reuse | An unrelated object occupies the exact retired Integer address; old provenance is rejected and old private-owner state cannot be reused; normal reconstruction later succeeds at the same address |
| Forced description reuse | Reused Pair storage is zeroed and has no inherited layout claim; collection rejects it until its new representation owner supplies a description |
| Replaceable phase message | Wrapping `withReclamationView:` remains observable |
| Release validation and guards | Duplicate/unknown/seed-owned/referenced candidates fail before retirement; reentrant release, sends, direct mutators and destruction are blocked |
| Expired and foreign capability | Saved release fails outside its phase, with another seed, and with an old epoch in a later phase |
| Repeated reuse | 256 collect/reconstruct cycles reuse the exact address; final root removal collects the remaining live cycle and values; empty subsequent collection succeeds |
| Ordered shutdown | Seed is destroyed before module state and loaded code |

The suite runs 19 original semantic, 11 Integer-only, nine tracing and ten
collection cases in **both debug and release**. Rustfmt, Clippy with warnings
denied, original SeedApi hashing, package/dependency/export checks and exact dyld
image inventories are included. The collection process maps only its probe,
seed, Integer, object-model, tracer and collector as non-system images.

Nine deliberately broken variants must be detected by unchanged probes: the
five earlier controls, plus retained Integer provenance, retained private-owner
entries, committing during preparation, and failing to detach seed identities.
These are disposable source snapshots, never mutations to production code.

The successful local receipt is
`.cargo-target/id-seed-experiment/20260918-113314-m21efbv0/receipt.json`.
It records checkpoint `c4d49b23` plus exact source/binary hashes. Production
exhaustive/release gates are outside this isolated workspace; no production
runtime, installer or IdApi source changes are part of this proof.

## 6. What remains

This supplies a bounded ordinary-storage part of I3 reclamation/index retirement
and I4 collection-phase integration. It does not settle the full seed sufficiency
audit or make the installed stage 1 a siphon.

Default behaviors, Symbols, callable code and module state remain pinned. Their
retirement requires binding/entry ownership, weak-index rules and lifetime
accounting. Native execution roots, automatic allocation safepoints, Frame/Machine
roots, external resource cleanup, image recovery and Birnam primitive authoring
remain separate acceptance obligations. In particular, reclaiming a behavior
must retire its method references before code/data can be unloaded.
