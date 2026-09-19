# 81P — Production collection coordinator

> **Status:** Implemented. Follows the live tracer in `320e470c`.

## Interface decision

Three interfaces were compared before implementation.

1. A synchronous `collect(source, out_count)` driver uses scoped operations from
   the current storage and metadata owners. The loaded module controls phase
   transitions, optional deferred settlement, retirement before detachment,
   ordered finalizer invocation, reclamation and completion. The source keeps
   private heap records and the exact String/Machine settlement exemptions.
2. An `open/advance/close` action machine returns each requested effect and
   accepts a sequenced completion. It hides the same ordering and makes each
   transition inspectable, but introduces session, action and completion state
   for an operation which must remain synchronous. No yielding or cancellation
   semantics are wanted.
3. A `sweep(addresses, detach, reclaim)` driver owns only the per-object loop.
   It is small and efficient but leaves almost all collection coordination in
   Id. It would be a valid narrower extraction, with less architectural depth.

The synchronous driver is selected. Its source has more individual operations
than the sweep-only proposal, but exposes effects rather than duplicating the
collection algorithm in the floor. The driver hides the complete ordering,
branch and loop. There is no new object registry, persistent session, aggregate
IdApi extension, or link to the Id implementation.

## Preserved boundaries

Native stack/register capture and pending-generation retirement remain outside
the coordinator's frames. Mark-only callers still use the loaded tracer directly.
The coordinator enters Marking, clears marks and invokes that tracer. A failed
trace cannot reach metadata retirement or sweep.

The floor snapshots dead executable bindings and evaluates the existing deferred
settlement predicate. The coordinator conditionally settles, then retires those
same bindings and weak indexes before requesting the dead-address snapshot.
Binding classification, weak-index policy and String/Machine exemptions remain
with their current owners, for a later extraction.

The coordinator enters Finalizing even for an empty snapshot. It detaches,
finalizes and reclaims one allocation at a time, in the original snapshot order.
Only addresses are snapshotted: each current finalizer is fetched at detachment.
An earlier finalizer may still inspect later registered dead objects or replace
their finalizers. No RuntimeState, Heap or detached-owner borrow crosses the
foreign finalizer call. Allocation stays forbidden and nested GC answers zero.

After the loop it reclaims deferred allocations, clears marks, resets allocation
accounting, restores Idle and prunes native metadata, in that order. Executable
and weak metadata therefore retire before addresses can be reused. The floor
retains its physical allocator and one invocation-local detached block owner.

The host mounts the immutable native driver after the tracer and before Integer.
Normal Id has no implementation link or fallback; unit tests use the real driver
through a dev-dependency. IdApi, Method, Frame, image bytes and ordinary dispatch
are unchanged. This is a Rust module extraction, not a Birnam rewrite or a claim
that the current Rust host is already the thin siphon of 81B.

## Validation

The focused debug checks pass:

- 246 Id tests, no failures, 16 ignored, including deferred numeric/clone/index
  settlement, observing finalizers, weak indexes, Frame/Continuation roots and
  executable generation lifetime. Three coordinator tests cover exact effect
  order, conditional settlement, malformed carriers and fail-stop propagation;
  the four ABI tests and 32 host startup tests also pass.
- The independent C probe loads the actual Id, tracer and coordinator libraries.
  An interposed source checks phase, retirement, snapshot, detach and reclaim
  order for explicit and allocation-triggered collection. Three dead victims
  prove that the first finalizer can still inspect later objects and replace
  their finalizers. Nested collection answers zero; mounting during collection
  and dependency replacement fail. Child processes abort on missing dependency
  or trace failure, with a pipe proving no victim was finalized on failure.
- The existing live-tracer and current independent IdApi v4 clients pass with
  explicit coordinator mounting. Coordinator/ABI Clippy passes with warnings
  denied. The normal standalone floor has its exact 29 exports; the coordinator
  has its exact four exports, a relocatable install name, only system linkage,
  and only the no-state ABI crate as a normal Rust dependency.
- The debug prefix publishes 20 native libraries and 28 compiled Birnam modules.
  `collection-image-smoke.sh` verifies ordinary startup, a compiled module after
  removal of its source file, a saved v8 image containing native Code, and its
  baked executable. The image application forces GC before and after method
  replacement and checks retained aliases, current dispatch and Integer math.
  Loader output confirms the collector library in startup and both image modes.
  Saved-image execution uses the existing mapped-code/recovery path unchanged.

Local logs are `.cargo-target/81p-{floor,unit,host-tests,native,live-native,image}.log`;
`.cargo-target/81p-prefix-final.log` records the debug prefix build. No release
validation or performance claim is part of this extraction.

## Next boundaries

Storage, native-root policy and metadata retirement implementations remain with
Id and their current owners. Further movement must preserve those owners' rules;
this does not import the seed experiment's simpler pinned-object policy.
The remaining primitive families are independent prerequisites for later owner
splits. Start with Float's existing methods as a small separately loaded module,
then move Float representation and its actual consumers in a subsequent commit,
following the two-checkpoint Integer precedent. Word and owned String storage
follow separately. Symbol identity and interning remain in the Id seed.
