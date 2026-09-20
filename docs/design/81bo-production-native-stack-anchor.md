# 81BO — Native stack-anchor ownership

> **Status:** Audited next extraction after completed 81BN (`6608e7fb`).
> No production migration yet. Preserve behavior and validate in debug only;
> release validation remains waived.

[81D](81d-id-api-partition.md) assigns `gcSetStackBottom` to native-root
integration. After Root Registry, GC Settings and Finalizers, `gc::MetadataShell`
still owns one native address alongside its GC Settings service. Move that
address and its assignment into `birnam-native-stack`. This gives the
native-root boundary its own state without moving physical register capture or
stack scanning in the same step.

## Existing state and laws

Each Runtime starts with a zero stack anchor. `Runtime::gc_set_stack_bottom`
performs ordinary owner-thread/storage-observation admission, then assigns the
supplied `usize` verbatim. It accepts zero, repeated values, unaligned values
and arbitrary addresses without reading memory, checking the collection phase,
collecting, registering roots or changing allocation accounting. This is a raw
host execution boundary, not a language object or a scoped stack of anchors.

Zero suppresses ordinary collection because no native interval can be captured.
A nonzero address makes the automatic-collection predicate eligible, but does
not prove the address describes a valid stack. The existing capture function
later checks the downward interval and the 128 MiB maximum. Do not introduce
setter validation or merge the readiness and capture predicates.

The anchor has three read sites:

| Reader | Required point and meaning |
|---|---|
| `MetadataShell::automatic_collection_due` | Read nonzero readiness before invoking GC Settings' existing predicate with the caller's already-computed allocation count |
| `Runtime::gc_collect_active` | Return early for a non-idle collection, retry pending executable retirement, then copy the anchor; zero returns without capturing |
| `with_source_probe_native_roots` | Copy the anchor before creating probe scratch; nested probes for the same Runtime retain their original outer interval |

Ordinary allocation and all existing native-tier budget consumers reach the
first reader. Their checked/saturating arithmetic and ordering stay in place.
Changing the anchor from a root provider or finalizer affects later collection
attempts; the current collection retains its already-copied start/end range.
Nested collection still returns zero through the existing phase guard.

Storage observation does not expose the anchor. It still rejects the public
setter, including a request to set zero, through ordinary admission. Internal
readiness queries remain callback-free under their existing RuntimeState
borrows. The service must not retain a movable Runtime address.

## Physical boundaries stay in place

Keep `with_native_root_range`, `RegisterSave`, Darwin arm64 `_setjmp`, the
downward-distance validation, `emit_native_root_range`, word alignment and
low-to-high emission in their current functions. Moving capture behind a new
callback could change the physical frame that bounds conservative roots.
Likewise, keep source-probe outer-range storage and its non-inlined body in Id.
Their future extraction needs its own physical-root proof.

The new canonical anchor getter returns before capture starts. It neither
captures nor scans, invokes no floor callbacks and cannot collect. It does not
retain or manufacture Oops. No new heap roots, dispatch behavior, allocation
policy, executable-retirement order or image data is introduced.

## Selected ownership contract

Native Stack owns an independent allocation per Runtime containing the anchor,
initially zero. A copied 32-byte floor descriptor supplies only owner/observation
admission. Its 32-byte mutable public capability contains version, byte size,
context and `set(context, address)`. The setter copies the floor descriptor,
admits without holding a service-state borrow, then assigns the address.

An immutable 48-byte factory supplies create, destroy, canonical set and
canonical get. Canonical operations address the original allocation directly;
instrumenting the public callback/context cannot redirect internal assignment,
readiness queries or destruction. Create/destroy and canonical access invoke no
floor callbacks. Id's internal setter retains its existing admission. Runtime
moves and independent test Runtimes retain separate anchors.

The host pins and attaches the factory before Id initialization. Attachment is
inert, repeats the same pointer and rejects retargeting. Id publishes the
Runtime's capability through a dedicated provider. Ordinary siphon mounting
adds no methods or language objects. The factory and library code outlive the
owning service and every retaining host reference.

## Consumer and ABI migration

`HostRuntime::prepare_execution` is the sole production external setter caller.
It uses the loaded floor's explicit capability at the existing point, before
applying optional precise-GC CLI policy. Each host operation keeps its supplied
anchor alive for the full synchronous evaluation. Preserve the numeric address
exactly; some independent C fixtures deliberately use one-past their local word.

No ordinary native module consumes this setter. Therefore native initialization
keeps its twelve arguments and Native Loader stays v12. Do not append an unused
capability to every initializer. Host loading obtains the capability explicitly;
standalone current fixtures do the same through their existing preparation
helpers. No API-address map or callback-slot compatibility loan is needed.

IdApi v20 removes only `gcSetStackBottom`: 18 fields / 144 bytes, with
`gcCollect` at 120, `gcLiveCount` at 128 and `objectPayloadSize` at 136.
Migrate the host, current Rust test tables, C mirrors, scripts, ABI manifests and
installer inventory together. Frozen v19 and earlier contracts stay unchanged.
Native consumers must rebuild; finalizer, tracing, collection, storage and image
v8 contracts remain unchanged.

## Adversarial checks and completion evidence

Preserve the completed 81BN source and installed debug prefix as independent
oracles before coding. Compare exact return/stdout/stderr behavior for absent,
present and reset anchors, next-allocation triggering, unchanged byte accounting,
and wrong-thread/storage-observation rejection for zero as well as nonzero
assignments. Do not pass an arbitrary unreadable interval to real collection;
setter acceptance and capture-boundary validation are separate tests.

Test zero initialization, exact scalar round trips, inert factory admission,
floor-copy lifetime, admission reentry, live public callback/context selection,
independent/moved Runtime ownership and canonical operation independence.
Prove real automatic collection still waits for both an anchor and the old
budget threshold, and that assignment itself never collects.

Exercise a root callback that changes the anchor to zero: that collection must
finish with its original interval, the next ordinary collection must skip, and
re-establishing the anchor must enable a later collection. Retain actual native
root, weak-object, finalizer and source-probe tests, including nested collection
and the existing retirement-before-anchor ordering. Review the capture/scanner
diff to ensure no physical boundary moved.

Validate the host's actual capability selection and preparation ordering, not
only a mocked setter. Verify cached-capability admission directly so the
provider getter cannot mask a missing check. Run startup through siphon,
source-free compiled calling, saved mapped/rebuilt images, baked execution and
the real image-backed worker comparison. Check dynamic symbol ownership,
system-only links, exports, ABI fingerprints and installation receipts against
the final build inputs. Commit only after this extraction is complete, then
select the next responsibility.
