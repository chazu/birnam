# 81BR — Allocation-byte budget ownership

> **Status:** Implemented and verified after completed 81BQ (`d833d6bb`).
> Dynamic Siphon startup and compiled/image consumers verified with debug builds.
> No new language behavior; public IdApi and image encoding unchanged.

[81D](81d-id-api-partition.md) separates collector policy from Id's storage
mechanisms. GC Settings already owns automatic/precise policy, Native Stack owns
the anchor and scanner, and Collect owns collection sequencing. The next small
state boundary is `Heap::bytes_since_collection`: the accumulated allocation
charge used by automatic-collection decisions and existing diagnostic probes.
Move this state and its arithmetic into `birnam-allocation-budget`.

This is not the live allocation count or total live byte extent. Those remain
derived from the heap's physical records and deferred ledgers. Do not move
`gcLiveCount`, change storage observation or infer reachability from this budget.
IdApi remains v20 for this extraction.

## State and concrete mutation points

Each Heap starts with a zero budget before bootstrap constructs any objects.
Permanent and collectable physical allocations both charge their full padded
allocation size, including the header. Ordinary freeing does not decrement the
budget. The following ordering is part of the contract:

| Path | Existing charge point and refusal behavior |
|---|---|
| `Heap::allocate` | Validate size/layout, obtain zeroed storage, advance allocation identity and register the block, then checked-add its total size before returning its payload |
| `record_dead_allocation` | Refuse a full dead-allocation ledger before charging; checked-add the saved total size before appending the snapshot |
| `record_numeric_result` | Reject invalid index/capacity or exhausted deferred identity first; checked-add the batch total before advancing the deferred identity and appending the entry |
| `record_dead_payload_indices` | Calculate byte multiplication and preview checked addition before validating membership, counts, indices and batch identity; commit the prepared total only after updating the ledger |
| `materialize_raw` | Save the budget, replace it with zero, realize previously charged storage and copy its payload, then restore the saved budget |
| `reset_allocation_bytes` | Replace the budget with zero at Collect's existing reset-accounting step |

The first three charging paths abort with the exact historical diagnostic
`birnam-id invariant failed: GC allocation byte counter overflow` on checked
addition overflow. The counted-index path instead returns false without mutation
when its preview overflows. Never replace checked arithmetic with saturation,
move its refusal after ledger mutation, or charge a preview before validation.

Materialization must not double-charge an already counted allocation. Temporary
zeroing specifically avoids an otherwise spurious overflow when the saved total
is near `usize::MAX`. Keep its save/zero/allocate/copy/restore order; do not add a
new unwind transaction, early restoration or callback around this sequence.
Keep allocation identity, deferred identity, ledger membership and payload
storage in their existing owners.

## Readers and collection behavior

`Runtime::maybe_collect_before_allocation` reads the current budget at its
existing point and asks GC Settings whether collection is due with the current
native-stack readiness. The Native Tier has additional projected-budget checks:
some deliberately use checked addition and others saturating addition. Preserve
those caller-specific rules and refusal points; this extraction does not
standardize optimizer policy or replace fresh reads with cached totals.

The budget is accumulated work since the last reset, not a remaining limit.
Disabling automatic GC or clearing the native anchor retains accumulated bytes.
Re-enabling policy or restoring the anchor does not itself collect; a subsequent
allocation reaches the existing decision point. Nested collection, finalizer
ordering, executable retirement and the timing of Collect's reset stay unchanged.
Existing source probes may observe exact differences across callbacks, including
a callback that collects and resets the budget.

## Ownership and native contract

The new module owns an opaque native allocation per Heap containing only the
budget scalar. It does not retain a Heap, Runtime, Oop, ledger, callback or public
IdApi table. Its constructor/destructor allocate/free only that native state;
its operations allocate nothing and invoke no foreign or Id callbacks. This is
required because the heap invokes them while its existing state borrow is live.
Heap moves and independent pre-Runtime heaps must retain separate stable owners.

Use an immutable 64-byte factory descriptor with version, byte size and six
operations: create, destroy, read, replace, checked charge and checked preview.
Checked charge changes the total only on success and returns a success flag.
Checked preview writes a projected total only on success and never mutates the
budget. Replace is exact scalar assignment, used for reset, materialization and
existing boundary tests. The output buffer of preview belongs to the synchronous
caller; no pointer is retained.

Id keeps the owner handle and existing diagnostic decisions. Failed ordinary
charges retain their current fail-stop message and ordering; a failed preview
retains the existing non-mutating refusal. Do not expose these internal raw
operations as language methods or add a public ability to edit GC statistics.

The host pins and attaches the factory before Id initialization, because initial
Heap construction already charges bootstrap allocations. Validate inertly,
accept repeated attachment of the same descriptor and reject retargeting.
Current standalone C/Rust fixture preparation and focused build dependencies must
include Allocation Budget. Siphon mounts the library without methods or objects.
IdApi v20, twelve-argument native initialization, Native Loader v12 and image v8
remain unchanged. Installation gains one native library.

## Independent proof and completion gates

Preserve completed 81BQ source and its installed debug prefix before migration.
Compare actual old/new runtime transcripts for zero-anchor accumulation,
automatic threshold crossings, suppression and re-enabling, callback-time reset,
materialization and native resource cleanup. Use the existing deferred-allocation
and Native Tier tests to preserve physical/logical distinctions, identities,
counted multiplicities, rejected plans and first-observation settlement.

Unit checks must cover zero defaults, independent/moved owners, exact replacement,
checked-charge success and overflow without mutation, and preview success and
refusal without state/output changes. Keep near-maximum materialization tests,
empty counted-index acceptance, all ledger refusal cases and original fatal
messages. An arithmetic-only module test cannot prove that callers charge at
the original points, so inspect their before/after ordering and exercise the
actual heap paths as well.

Verify factory admission and actual host startup. Test dynamic siphon loading,
source-free compiled callers, mapped/rebuilt saved native Code images and baked
execution with GC activity and deferred/ordinary allocation. Verify aliases,
reload and the actual image-backed worker comparison. Check that allocation
count, storage identities, header layout, payload extent, roots and image data
are unchanged. Check manifests, exports, linkage and final source/artifact
receipts; record completed evidence before committing the extraction.

## Implementation checkpoint — 2026-09-20

Committed at the user's request to save and push current progress. This is an
implementation checkpoint, not a declaration that the extraction's completion
gates above have passed.

The new module owns each Heap's budget through the factory described above.
Physical, deferred and counted charges, reset and materialization now use that
owner. Host preparation attaches the dependency before Id initialization; the
Siphon module list, installer inventory and standalone fixture preparation include
the library. Public IdApi, native initialization and image versions are unchanged.

Debug checkpoint evidence:

- `cargo check --offline --locked -p birnam-allocation-budget -p birnam-id
  -p birnam-host` passed.
- `cargo test --offline --locked -p birnam-allocation-budget -p birnam-abi
  -p birnam-id --lib -- --nocapture` passed: 292 tests, 15 ignored.
- The same new heap witness ran against the preserved `d833d6bb` implementation,
  with only its test budget-seeding helper adapted to the old scalar field.
  All twelve transcript rows matched, including materialization near
  `usize::MAX`, reset, refusal and independent owners. All three actual heap
  overflow paths retained their abort signal and exact diagnostic in both builds.
  The witness's initial expected totals were corrected to account for the
  existing eight-byte header; no production layout was changed.

At that checkpoint, still pending: the repeatable Allocation Budget gate and ABI reference,
dynamic runtime and host-admission checks, installed compiled/image/worker
comparisons, final manifests and artifact receipts, and completion documentation.
Release validation remains waived.

## Completed implementation and evidence

The checkpoint's remaining work is complete. The
[v1 contract](../../abi/allocation-budget-v1.md), independent C declaration,
`mise run allocation-budget`, installed image smoke, README links and changelog
now accompany the implementation. Final production review confirms that only
the counter owner and its operations moved: caller charge/refusal order,
materialization and reset placement are unchanged. Collect's production
coordinator and source-probe root handling are byte-for-byte unchanged from
`d833d6bb`.

Debug evidence:

- The 292 core tests and twelve-row independent old/new Heap witness above
  passed. Workspace build and `cargo check --workspace --all-targets --offline
  --locked` passed. The additional `ffi-test-hooks` nested-source-probe test
  passed, preserving the outer native interval after an inner anchor reset.
- The focused gate passed module, ABI and Heap checks, plus a C fixture that
  captures the actual bootstrap Heap owner at factory creation. Admission
  creates no owner; Id initialization creates exactly one. Its loaded charge
  operation accounts for padded allocation bytes. Suppressed collection retains
  charges; restoring readiness waits for the next allocation, which collects
  before charging. Root callbacks observe the previous charge, and collection
  resets at its original completion point. Actual public allocation overflow
  retains the exact abort diagnostic. The observer adds no production getter or
  callback and never re-enters Id from a budget operation.
- All fourteen actual host-admission cases passed: valid startup, missing
  library/provider, null/version/size/six-operation rejection, missing floor
  dependency and rejected attachment. Invalid descriptors are rejected before
  Id initialization or operation invocation.
- Five old/new actual runtime transcripts and exit statuses matched, using the
  archived pre-extraction C fixture and its installed stack independently.
  These cover zero-anchor accumulation, automatic threshold crossing, callback
  and finalizer-time reset, nested suppression, roots, owner-thread rejection
  and storage-observation rejection.
- The new Allocation Budget image smoke passed dynamic startup, source-free
  compiled callers, mapped and rebuilt saved native Code images, and baked
  execution. It checks already-loaded operation ownership, independent native
  budgets, overflow/preview refusal, arithmetic loops, automatic collection,
  roots, object aliases and native-module reload. The existing Finalizers image
  smoke passed all of those execution paths for native cleanup and detachment.
- Old/new actual image-backed workers passed twelve exact wire-v1 value round
  trips and orderly stop; the complete transcripts matched byte for byte.
  The external harness allowed debug startup time without changing production
  handshake deadlines.
- Debug installation contains 52 native libraries plus 28 compiled modules.
  All 80 installed export sets match the old stack plus Allocation Budget and
  Id's one new dependency export. All 36 native export manifests match. Id and
  Allocation Budget link only system libraries; the new install name is
  `@rpath/libbirnam_allocation_budget.dylib`.
- All 485 final source inputs and 523 installed artifacts match the installer
  receipts. Source digest:
  `5902f38291df49602cd06152f403a12c0458a4ce08e5e7a85523b921007ac1ff`.
  Artifact digest:
  `46dbcc291764eaa0117e584b29d446a950c8d724fb1a0525cb68e624e1095254`.
  Old/new installed admission contracts are identical, including IdApi v20,
  its fingerprint, runtime ABI hash and image v8. Native initialization remains
  twelve arguments and Native Loader remains v12.

Local evidence is recorded in `.cargo-target/81br-*` logs and prefix markers.
The repeatable gates are `scripts/allocation-budget-test.sh [PREFIX]` and
`scripts/allocation-budget-image-smoke.sh PREFIX`. No release validation was run.
