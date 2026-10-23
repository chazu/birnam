# Simpler Oop-only object storage and call roots

Historical checkpoint. The fixed storage classification described below was
subsequently rejected and [removed](oop-payload-opacity-20261023.md). Keep these
measurements as evidence of that earlier implementation, not as design guidance.

2026-10-23. This changes the isolated Oop-only worktree, not the installed runtime.
See [the glossary](../../glossary.md) for terminology and
[the measurements](../measurements/oop-runtime-simplification-20261023.json) for evidence.

## Removed work

- Removed four tables holding Method data, Fault data, Name lengths and reference
  storage. These contents now live in their objects. Name length and the number
  of reference slots come from the existing allocation record.
- The allocation record identifies its storage layout using unused alignment bits.
  Its size remains 24 bytes, and allocation identity range is unchanged. Changing
  an object's vtable does not change its storage layout or allow another object
  to masquerade as a Name, Method or reference store.
- Method data is read when selecting the method. Ordinary dictionary bindings have
  already admitted the Method; custom lookup results and explicitly invoked
  Methods still undergo storage checks. Selection remains live on every send.
- Removed a repeated allocation check when reading a live vtable's own vtable.
  The checked path remains when detached storage exists during finalization.
- Replaced per-call copying of arguments into a root list with small Id-owned call
  records that borrow the caller's stable argument area. These records preserve
  receiver, selector, arguments, Method, context and code owner until return.
  GC copies the references when it needs a root snapshot, before calling visitors.
- Removed the duplicate allocation-list sort. The collector still orders objects
  by creation identity before freeing them and running finalizers.
- Removed the temporary managed object holding the dead-object list. GC passes
  those Oops directly to `removeDeadObjects`. It no longer allocates a reference
  store after marking that would subsequently contain freed references.

The storage layout discriminator is private allocation bookkeeping, not another
kind of Id message value. Id still receives and returns Oops. No validation/root
bypass from the diagnosis was incorporated.

## Checks

Normal and fresh-target qualification pass, including independent C, Rust and
TinyCC callers. There are 15 passing Id unit tests. Added checks cover:

- Object contents surviving removal from indexes and detachment, until actual
  storage release; rejected reads after release.
- Vtable replacement without losing storage identity, and rejection of ordinary
  objects disguised as Names, Methods, Faults or reference stores.
- Unchanged allocation extents, identity values and allocation-record size.
- Refusal to dispatch a detached object whose former vtable has been released.
- All 64 object arguments surviving collection while borrowed by an entered call.
- Mutation of source fields without changing an entered call's argument snapshot.
- Collection during custom method lookup, and finalization in creation order.

Existing checks still cover live replacement, entered Method/context survival,
reentry, malformed contexts, failed tracing, partial initialization, failed
finalization and owner-thread enforcement. A new GC-during-lookup check found a
fixture using a Node vtable on a zero-sized object; that fixture was corrected to
use an ordinary object vtable. Runtime shape checking was not weakened.

Call records borrow arguments only during the synchronous native call. They do
not preserve a language continuation on the native stack. Full stackless,
image and worker integration remains pending in this migration branch, so this
is not a claim of complete concurrency or language qualification.

## Measurements

Repeated ABBA processes compare the earlier Oop-only slice with this change.
Thread CPU medians; no confidence interval or application speedup is claimed.

| Workload | Before | After | Reduction |
| --- | ---: | ---: | ---: |
| Allocation/rooting | 59.36 ns/object | 52.92 ns/object | 10.8% |
| Interning an existing 64-byte Name | 53.62 ns/call | 47.26 ns/call | 11.9% |
| Producing/interning a new 64-byte Name | 318.91 ns/name | 290.90 ns/name | 8.8% |
| GC workload | 15.36 ms | 13.28 ms | 13.5% |

The initial storage-only experiment made hot calls slightly slower. It was not
accepted as the finished change; reading Method data once and removing argument
copies resolved that regression. Intermediate results are retained in the evidence.

The old v1 runtime is still faster in these native workloads: the remaining gaps
are approximately 21% allocation/rooting, 38% existing-name interning, 72% fresh-name
production and 27% GC. The strict interface and its semantics remain the target;
this change removes unnecessary mechanisms without pretending the full performance
or language migration is complete. No merge to main or installation was performed.
