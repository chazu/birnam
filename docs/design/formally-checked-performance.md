# Formally checked GC and performance changes

This slice implements three bounded changes. It preserves logical root tokens,
object identities, owner messages, collector traversal/retirement order,
callback dispatch, live replacement, native ABI/image formats and the Rust cold
bootstrap. It does not enable persistent duplicate-root coalescing: the proposed
lease protocol is not yet implemented and public root multiplicity is observable.

## Changes and proofs

| Change | Implementation | Formal contract |
| --- | --- | --- |
| Inventory-index mark flags | Collect stores one byte per inventory allocation; address admission still uses the validated immutable index | `GC.emitAll_preserves` and `dead_inventory_equivalent`: same admission, LIFO pending work and creation-ordered dead list |
| Guarded cleanup batches | Generated scopes with at least eight tokens ask Roots `tryReleaseMany:`; accepted batches apply exact reverse-order releases | `BatchCleanup.tla` and `GC.bulk_cleanup_equivalent`: exact token state, swap-remove order, and unskipped custom callbacks on fallback |
| Borrowed shape inspection | Execution reads the current canonical bytes through the same CodeObject/Bytes messages; a pure callback-free parser returns owned counts | `BorrowInspection.tla` and `Borrow.pure_inspection_equivalent`: same bytes, shape and errors, with a counterexample if callbacks intervene |

The collector still invokes current root/edge visitors and trace/finalizer owners.
No direct foreign object payload or context access is introduced. The batch
message marshals only caller-owned native token Words, copies its ABI input
before guard sends, and mutates only Roots-owned storage. Opaque NativeEntry
receipts are compared for guard eligibility, never called to dispatch a foreign
method or dereferenced as a foreign context.

## Cleanup batch protocol

Roots `tryReleaseMany:` takes native bytes containing native-endian usize tokens
in creation order. Word 1 means the complete reverse-order sequence was handled;
Word 0 means declined with no release performed. Malformed ABI input is rejected
before mutation. Unknown and repeated tokens have the ordinary release's no-op
effect; generated cleanup already ignores the per-token 0/1 return.

Eligibility requires the actual Roots receiver, default lookup, the current
owned `release:` meaning and a matching non-saturated behavior epoch. Changed
proof-service meanings decline before invoking them. The final guard is followed
by a callback-free owner-thread mutation of the registry; there is no persistent
omission lease to revoke. A changed release implementation, alias receiver or
failed guard takes individual late-bound sends in the original reverse order.
Those sends can see further replacements between calls.

Generated cleanup resolves its selectors before capturing the final token count.
Selector-cache admission can create temporary scope roots: resolving `release:`
inside an already-counted cleanup loop previously left those first-use temporary
tokens behind. The executable C test verifies that every scope token, including
selector temporaries, is released, and that an original body failure takes
precedence over an optional batch/cleanup failure.

NativeBackend options now include bit 8 for batching; its default is mask 15.
Masks 0–7 retain the prior individually dispatched scope cleanup for paired
comparison. Typed adapter units without explicit options use the default.
Existing machine-code images remain executable; this changes future emission
and requires no default startup recompilation or new persistent image format.

## Borrowed inspection and rejected cache

The ABI byte buffer is consumed immediately after the current `bytes` send, with
its owning Bytes object protected by Execution's existing scope. The parser is a
provider-free Rust helper and makes no Id send. The borrowed Program view ends
before subsequent capture/constructor sends; only parameter/capture counts
survive. No parsed view or foreign pointer is cached across callbacks.

Execution `programShapeBorrowing:` accepts native Word 0/1. The default 1 enables
borrowed inspection; 0 restores the former temporary-copy path for diagnostics.
Both modes keep CodeObject/Bytes messages, allocations, capture validation and
real Block construction. This policy changes only pure native work.

An exact-byte memoization cache was also proved and tested, then removed:
Integer showed no meaningful improvement (ratio 1.003), and large Block
construction regressed about 6.1% (interval 1.053–1.065). It offered no reason to
retain extra runtime cache state. `Memo.lean` preserves its reusable correctness
proof; the measurements preserve this rejected candidate's evidence.

## Measurements

All timings are short thread-CPU measurements on an actively usable machine.
No quiet-host requirement or startup/tool-image rebuild was used for timing.
Raw data and source/library hashes are in the measurement receipt listed below.
The paired tool's interval captures within-run uncertainty, not machine drift.

- Guarded cleanup: the finalized four-second public Integer comparison is in
  `root-batches-delivery-timing.json`. Masks 7 and 15 have identical allocations.
- Byte-copy elimination: the four-second `block-shape-large` comparison is in
  `shape-borrow-timing.json`; both variants use mask 15 and differ only in
  Execution's borrowing policy, set outside timed batches.
- Indexed marks: `gc-indexed-marks.json` contains two runs of each compatible
  collector in ABBA order. Each uses 512 explicitly rooted objects with four
  registrations apiece. Separate-process comparisons are descriptive; they do
  not receive the paired reporter's confidence verdict or establish whole-app
  speedups. The old collector dylib was preserved before editing it.

| Short measurement | Result |
| --- | --- |
| Public Integer cleanup, 23 paired batches | Candidate/baseline CPU 0.956; roughly 4.4% lower, within-run interval 0.954–0.959 |
| Large Block borrowed inspection, 42 pairs | Ratio 0.960; roughly 4.0% lower, interval 0.957–0.963 |
| Precise collector, separate-process ABBA | Median 17.688 ms to 16.567 ms, about 6.3% lower; descriptive only |
| Public Integer token/message census | Individual releases 738.5 to 466.25 per addition; 9.0625 batch messages; retains unchanged at 738.5; zero observed tokens remaining |

The permanent summary is
`docs/measurements/formal-performance-20261006.json`; raw logs and cases are under
`/tmp/birnam-formal-perf/` and `.cargo-target/formal/results/`. These measurements
are not new startup timings, and they do not close the native/authored Integer
performance gap.

## Verification and remaining boundaries

Run `mise run formal-rooting` for bounded TLC checks, fresh Lean kernel checking,
axiom audits and deliberately broken-proof rejection. Rust tests compare indexed
marks against an independent hash-set oracle and exercise the production C
prelude's ordering, refusal, failures and first-use rooting. Production Id tests
cover token/snapshot equivalence, live release replacement and changed proof
services. Existing tracing/finalizer/reentrant visitor, materialization, ABI,
Integer and Word tests provide their independent owner/language oracles.

The formal results establish abstract transformation rules with explicit
premises; they do not constitute a verified Rust/C compiler or the entire
runtime. In particular, inventory-index injectivity is checked in Collect,
callback-freedom is enforced by the pure-helper/owned-mutation implementation
boundaries, and Id introspection must report current facts under its protocol.
Formal verification does not justify suppressing arbitrary visitor effects,
reading another owner's payload, caching a selected executable for dispatch,
eliminating observable Frames or bypassing signed Integer policy.

Larger possibilities remain candidates: owner-admitted CodeObject shape
messages with current allocation/lookup permissions; a true scoped-root lease
with exact observer recovery; and branch construction recovery. Each needs its
own contract, proof-to-implementation checks and short performance evidence.
