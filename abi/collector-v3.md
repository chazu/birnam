# Collector v3

Production Collect publishes a 64-byte descriptor through the unchanged
128-byte [collection-state owner](collection-state-v2.md). It keeps
[v2's complete later-owner extension](collector-v2.md) and adds
`activate(context, bootstrap, roots)` and `active(context)`.

Activation uses completed Bootstrap v2, Root Registry, Lifetime, Object Model,
Trace, Native Stack v4 and collection controls. It needs no Runtime, Execution
or compiler. The host's `InitialFloor::activate_collection` runs it after those
owners exist; ordinary `finish` activates before constructing Runtime.

Activation is a single owner-thread transition:

1. Adopt Bootstrap's early roots into the same completed registry, or confirm
   their existing adoption. Enter Bootstrap's publication pause.
2. Validate all current Id allocation identities/edges and payload coverage.
   Every nonempty non-Id payload needs a layout or a native trace hook. Validation
   inspects hook declarations without invoking them or marking/reclaiming objects.
3. Reconcile weak records and accounting, and install Native Stack's lasting
   thread boundary. Existing allocations and identities stay in place.
4. Install `allocate`, `delegated` and `intern:` on Id's receiver and `clone`/`new`
   on Object's behavior, using ordinary Id messages. These wrappers remain
   non-collecting throughout installation.
5. Publish Active, then leave the publication pause. Bootstrap's roots already
   belong to the root registry; there is no blanket permanent marking of boot
   allocations or second heap.

A failed transition remains disabled, keeps bootstrap owners/roots and performs
no sweep. Repeated or nested activation returns BUSY; no automatic retry or
rollback is promised. The host reports a native startup error. `active` returns
one only after successful completion of every step. The [C mirror](fixtures/collector-v3/collector_v3.h)
fixes layouts. The temporary upper constructor is `birnam_runtime_init_v13`.

Allocation, behavior construction and cloning collect at a safe point **before**
invoking the retained non-collecting Id primitive. Their input remains explicitly
rooted throughout collection. Collection never sees a newly allocated instance
before its caller initializes the payload. Cloning copies the complete padded
payload and does not duplicate finalizer ownership. Interning first produces a
complete Symbol, accounts it and roots it across collection if newly allocated;
an existing Symbol does not cause collection. All new results update the same
weak records and allocation budget. Saved wrappers use the public collector and
therefore acquire the complete Runtime extension after its later installation.

During the remaining aggregate Runtime construction, temporary ordinary roots
protect its actual canonical objects and compatibility Symbols. Construction
runs with active Runtime access; those temporary roots are released only after
Execution objects and the full collector extension exist. Runtime still installs
its own completed construction methods and uses its old deferred-accounting
adapter. Removing that duplicate upper implementation belongs to the remaining
Execution/package migration; Id's primitive allocation stays non-collecting.

The independent native harness activates with only the twenty-two predecessor
libraries present. It forces collection through allocation, delegation, clone
and intern; checks existing-Symbol behavior and initialization-before-tracing;
rejects missing payload descriptions without sweeping; checks publication pause;
and exercises saved constructors after Runtime adoption. A separate full-prefix
case forces automatic collection during Runtime construction itself.
